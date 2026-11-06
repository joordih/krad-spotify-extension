use super::names::App;
use super::value::{boolean, float, integer, text};
use crate::media::{
    Artwork, Capabilities, MediaError, PlaybackState, Position, TrackState, Volume,
};
use std::collections::HashMap;
use std::time::Duration;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub(super) const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

const IDENTITY: &str = "Identity";
pub(super) const SHUFFLE: &str = "Shuffle";
pub(super) const LOOP_STATUS: &str = "LoopStatus";
pub(super) const VOLUME: &str = "Volume";
const CAN_CONTROL: &str = "CanControl";
const CAN_SEEK: &str = "CanSeek";
const CAN_RAISE: &str = "CanRaise";
const PLAYBACK_STATUS: &str = "PlaybackStatus";
const POSITION: &str = "Position";
const METADATA: &str = "Metadata";

const TITLE: &str = "xesam:title";
const ARTIST: &str = "xesam:artist";
const ALBUM: &str = "xesam:album";
const ALBUM_ARTIST: &str = "xesam:albumArtist";
const TRACK_NUMBER: &str = "xesam:trackNumber";
const ART_URL: &str = "mpris:artUrl";
const TRACK_ID: &str = "mpris:trackid";
const LENGTH: &str = "mpris:length";

const PLAYING: &str = "Playing";
const PAUSED: &str = "Paused";
const LOOP_NONE: &str = "None";
const LOOP_PLAYLIST: &str = "Playlist";
const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";
const SPOTIFY_TRACK_PATH_PREFIX: &str = "/com/spotify/track/";
const SPOTIFY_TRACK_URI_PREFIX: &str = "spotify:track:";

const MICROS_PER_SEC: i64 = 1_000_000;
const FULL_VOLUME: f64 = 1.0;
const PERCENT_PER_FULL_VOLUME: f64 = 100.0;

pub(super) type Properties = HashMap<String, OwnedValue>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Interface {
    Root,
    Player,
}

impl Interface {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Root => ROOT_INTERFACE,
            Self::Player => PLAYER_INTERFACE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoopStatus {
    Off,
    Repeating,
}

impl LoopStatus {
    pub(super) fn of(value: &Value<'_>) -> Option<Self> {
        text(value).map(|status| {
            if status == LOOP_NONE {
                Self::Off
            } else {
                Self::Repeating
            }
        })
    }

    pub(super) const fn toggled(self) -> &'static str {
        match self {
            Self::Off => LOOP_PLAYLIST,
            Self::Repeating => LOOP_NONE,
        }
    }
}

#[must_use]
pub(super) fn capabilities(
    app: App,
    props: &Properties,
    root: &Properties,
    volume_writable: bool,
    track: &TrackState,
) -> Capabilities {
    if read(props, CAN_CONTROL, boolean) == Some(false) {
        return Capabilities::default();
    }
    Capabilities {
        seek: flag(props, CAN_SEEK) && track.duration_secs > 0,
        shuffle: read(props, SHUFFLE, boolean).is_some(),
        repeat: read(props, LOOP_STATUS, LoopStatus::of).is_some(),
        volume: volume_writable && app != App::Browser && read(props, VOLUME, float).is_some(),
        open: flag(root, CAN_RAISE) || (app == App::Spotify && track.track_url.is_some()),
    }
}

#[must_use]
pub(super) fn track_state(props: &Properties, app: App) -> TrackState {
    let metadata = metadata(props);
    let field = |key: &str| read(&metadata, key, text);
    let number = |key: &str| read(&metadata, key, integer);
    TrackState {
        title: field(TITLE).unwrap_or_default(),
        artist: field(ARTIST).unwrap_or_default(),
        album: field(ALBUM).unwrap_or_default(),
        album_artist: field(ALBUM_ARTIST).unwrap_or_default(),
        track_number: number(TRACK_NUMBER)
            .and_then(|number| u32::try_from(number).ok())
            .filter(|number| *number > 0),
        artwork: field(ART_URL).and_then(|url| Artwork::from_url(&url)),
        track_url: match app {
            App::Spotify => field(TRACK_ID).and_then(|id| spotify_track_uri(&id)),
            App::Browser | App::Other => None,
        },
        state: read(props, PLAYBACK_STATUS, text)
            .map_or(PlaybackState::Stopped, |status| playback_state(&status)),
        position_secs: position_micros(props).map_or(0, secs),
        duration_secs: number(LENGTH).map_or(0, secs),
        shuffling: flag(props, SHUFFLE),
        repeating: read(props, LOOP_STATUS, LoopStatus::of) == Some(LoopStatus::Repeating),
        volume: read(props, VOLUME, float).map_or(0, volume_percent),
    }
}

pub(super) fn identity(root: &Properties) -> Option<String> {
    read(root, IDENTITY, text)
}

pub(super) fn publishes_volume(props: &Properties) -> bool {
    props.contains_key(VOLUME)
}

pub(super) fn position_micros(props: &Properties) -> Option<i64> {
    read(props, POSITION, integer)
}

pub(super) fn seekable_track_id(props: &Properties) -> Option<OwnedObjectPath> {
    match &**metadata(props).get(TRACK_ID)? {
        Value::ObjectPath(path) if path.as_str() != NO_TRACK => {
            Some(OwnedObjectPath::from(path.to_owned()))
        }
        _ => None,
    }
}

pub(super) fn micros(position: Position) -> Result<i64, MediaError> {
    let duration =
        Duration::try_from_secs_f64(position.secs()).map_err(|_| MediaError::InvalidValue)?;
    i64::try_from(duration.as_micros()).map_err(|_| MediaError::InvalidValue)
}

pub(super) fn volume_fraction(volume: Volume) -> f64 {
    f64::from(volume.percent()) / PERCENT_PER_FULL_VOLUME
}

fn read<T>(props: &Properties, key: &str, decode: fn(&Value<'_>) -> Option<T>) -> Option<T> {
    props.get(key).and_then(|value| decode(value))
}

fn flag(props: &Properties, key: &str) -> bool {
    read(props, key, boolean).unwrap_or(false)
}

fn metadata(props: &Properties) -> Properties {
    props
        .get(METADATA)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Properties::try_from(value).ok())
        .unwrap_or_default()
}

fn playback_state(status: &str) -> PlaybackState {
    match status {
        PLAYING => PlaybackState::Playing,
        PAUSED => PlaybackState::Paused,
        _ => PlaybackState::Stopped,
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is finite, clamped to 0..=1, scaled to 0..=100 and rounded first"
)]
fn volume_percent(volume: f64) -> u8 {
    if !volume.is_finite() {
        return 0;
    }
    (volume.clamp(0.0, FULL_VOLUME) * PERCENT_PER_FULL_VOLUME).round() as u8
}

fn secs(micros: i64) -> u32 {
    u32::try_from(micros.max(0) / MICROS_PER_SEC).unwrap_or(u32::MAX)
}

fn spotify_track_uri(track_id: &str) -> Option<String> {
    if track_id.starts_with(SPOTIFY_TRACK_URI_PREFIX) {
        return Some(track_id.to_owned());
    }
    track_id
        .strip_prefix(SPOTIFY_TRACK_PATH_PREFIX)
        .map(|id| format!("{SPOTIFY_TRACK_URI_PREFIX}{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use zbus::zvariant::{self, ObjectPath};

    type TestResult = Result<(), Box<dyn Error>>;

    fn props(entries: Vec<(&str, Value<'_>)>) -> zvariant::Result<Properties> {
        entries
            .into_iter()
            .map(|(key, value)| Ok((key.to_owned(), value.try_into_owned()?)))
            .collect()
    }

    fn metadata_value(entries: Vec<(&str, Value<'static>)>) -> Value<'static> {
        let map: HashMap<String, Value<'static>> = entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        Value::from(map)
    }

    fn track_id(path: &'static str) -> Value<'static> {
        Value::from(ObjectPath::from_static_str_unchecked(path))
    }

    fn full_player(length: i64) -> zvariant::Result<Properties> {
        props(vec![
            (CAN_CONTROL, Value::from(true)),
            (CAN_SEEK, Value::from(true)),
            (SHUFFLE, Value::from(false)),
            (LOOP_STATUS, Value::from("Track")),
            (VOLUME, Value::from(0.42)),
            (PLAYBACK_STATUS, Value::from("Paused")),
            (POSITION, Value::from(65_000_000_i64)),
            (
                METADATA,
                metadata_value(vec![
                    (TITLE, Value::from("Song")),
                    (ARTIST, Value::from(vec!["A", "B"])),
                    (LENGTH, Value::from(length)),
                    (ART_URL, Value::from("https://i.scdn.co/image/x")),
                    (TRACK_ID, track_id("/com/spotify/track/abc")),
                ]),
            ),
        ])
    }

    #[test]
    fn capabilities_follow_the_published_properties() -> TestResult {
        let raise = props(vec![(CAN_RAISE, Value::from(true))])?;
        let player = full_player(180_000_000)?;
        let track = track_state(&player, App::Other);
        assert_eq!(
            capabilities(App::Other, &player, &raise, true, &track),
            Capabilities::ALL
        );
        assert_eq!(
            capabilities(App::Other, &player, &Properties::new(), false, &track),
            Capabilities {
                volume: false,
                open: false,
                ..Capabilities::ALL
            }
        );
        assert!(!capabilities(App::Browser, &player, &raise, true, &track).volume);
        let live = full_player(0)?;
        let live_track = track_state(&live, App::Other);
        assert!(!capabilities(App::Other, &live, &raise, true, &live_track).seek);
        let bare = props(vec![
            (CAN_SEEK, Value::from(false)),
            (PLAYBACK_STATUS, Value::from("Playing")),
        ])?;
        let bare_track = track_state(&bare, App::Browser);
        assert_eq!(
            capabilities(App::Browser, &bare, &raise, false, &bare_track),
            Capabilities {
                open: true,
                ..Capabilities::default()
            }
        );
        let mut locked = full_player(180_000_000)?;
        locked.insert(CAN_CONTROL.to_owned(), Value::from(false).try_into_owned()?);
        assert_eq!(
            capabilities(App::Other, &locked, &raise, true, &track),
            Capabilities::default()
        );
        Ok(())
    }

    #[test]
    fn spotify_opens_its_track_without_can_raise() -> TestResult {
        let player = full_player(180_000_000)?;
        let track = track_state(&player, App::Spotify);
        assert_eq!(track.track_url.as_deref(), Some("spotify:track:abc"));
        assert!(capabilities(App::Spotify, &player, &Properties::new(), true, &track).open);
        assert_eq!(track_state(&player, App::Other).track_url, None);
        Ok(())
    }

    #[test]
    fn track_state_reads_the_player_properties() -> TestResult {
        let track = track_state(&full_player(180_000_000)?, App::Other);
        assert_eq!(track.title, "Song");
        assert_eq!(track.artist, "A, B");
        assert_eq!(track.state, PlaybackState::Paused);
        assert_eq!(track.position_secs, 65);
        assert_eq!(track.duration_secs, 180);
        assert_eq!(track.volume, 42);
        assert!(!track.shuffling);
        assert!(track.repeating);
        assert_eq!(
            track.artwork,
            Artwork::from_url("https://i.scdn.co/image/x")
        );
        let empty = track_state(&Properties::new(), App::Other);
        assert_eq!(empty.state, PlaybackState::Stopped);
        assert_eq!(empty.title, "");
        Ok(())
    }

    #[test]
    fn seeking_uses_set_position_only_with_a_real_track_id() -> TestResult {
        assert_eq!(
            seekable_track_id(&full_player(1)?)
                .as_ref()
                .map(|path| path.as_str()),
            Some("/com/spotify/track/abc")
        );
        let no_track = props(vec![(
            METADATA,
            metadata_value(vec![(TRACK_ID, track_id(NO_TRACK))]),
        )])?;
        assert_eq!(seekable_track_id(&no_track), None);
        let string_id = props(vec![(
            METADATA,
            metadata_value(vec![(TRACK_ID, Value::from("spotify:track:abc"))]),
        )])?;
        assert_eq!(seekable_track_id(&string_id), None);
        Ok(())
    }

    #[test]
    fn positions_and_volumes_convert_to_mpris_units() -> TestResult {
        assert_eq!(micros(Position::try_from(1.5)?)?, 1_500_000);
        assert_eq!(
            micros(Position::try_from(f64::MAX)?),
            Err(MediaError::InvalidValue)
        );
        assert!((volume_fraction(Volume::new(42)) - 0.42).abs() < f64::EPSILON);
        assert_eq!(volume_percent(1.25), 100);
        assert_eq!(volume_percent(f64::NAN), 0);
        Ok(())
    }

    #[test]
    fn track_ids_become_spotify_uris() {
        assert_eq!(
            spotify_track_uri("spotify:track:abc").as_deref(),
            Some("spotify:track:abc")
        );
        assert_eq!(
            spotify_track_uri("/com/spotify/track/abc").as_deref(),
            Some("spotify:track:abc")
        );
        assert_eq!(spotify_track_uri(NO_TRACK), None);
    }

    #[test]
    fn repeat_toggles_between_off_and_the_playlist() {
        assert_eq!(LoopStatus::of(&Value::from("None")), Some(LoopStatus::Off));
        assert_eq!(
            LoopStatus::of(&Value::from("Track")),
            Some(LoopStatus::Repeating)
        );
        assert_eq!(LoopStatus::Off.toggled(), "Playlist");
        assert_eq!(LoopStatus::Repeating.toggled(), "None");
    }
}
