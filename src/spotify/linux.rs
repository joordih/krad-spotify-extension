//! Linux backend: the MPRIS D-Bus interface that the Spotify desktop client
//! registers on the session bus as `org.mpris.MediaPlayer2.spotify`.
//!
//! Spotify's MPRIS support is partial: some versions report a position of 0
//! or ignore writes to `Shuffle`, `LoopStatus` and `Volume`. The calls follow
//! the MPRIS specification and Spotify decides whether to honour them.

use super::{stepped_volume, Artwork, Capabilities, TrackState, NOT_RUNNING};
use std::collections::HashMap;
use std::process::Command;
use zbus::blocking::fdo::DBusProxy;
use zbus::blocking::{Connection, Proxy};
use zbus::names::BusName;
use zbus::zvariant::{OwnedValue, Value};

pub const CAPABILITIES: Capabilities = Capabilities {
    shuffle: true,
    repeat: true,
    volume: true,
};

const BUS_NAME: &str = "org.mpris.MediaPlayer2.spotify";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const MICROS_PER_SEC: i64 = 1_000_000;
/// Older clients use `/com/spotify/track/<id>` object paths as track ids.
const TRACK_PATH_PREFIX: &str = "/com/spotify/track/";
const TRACK_URI_PREFIX: &str = "spotify:track:";

fn failure(error: impl std::fmt::Display) -> String {
    format!("D-Bus error: {error}")
}

/// A proxy for one of Spotify's MPRIS interfaces, or [`NOT_RUNNING`] when
/// nobody owns Spotify's bus name.
fn proxy(interface: &'static str) -> Result<Proxy<'static>, String> {
    let connection = Connection::session()
        .map_err(|error| format!("Could not connect to the D-Bus session bus: {error}"))?;
    let name = BusName::try_from(BUS_NAME).map_err(failure)?;
    let running = DBusProxy::new(&connection)
        .and_then(|bus| bus.name_has_owner(name).map_err(Into::into))
        .map_err(failure)?;
    if !running {
        return Err(NOT_RUNNING.to_owned());
    }
    Proxy::new(&connection, BUS_NAME, OBJECT_PATH, interface).map_err(failure)
}

fn player() -> Result<Proxy<'static>, String> {
    proxy(PLAYER_INTERFACE)
}

fn call(method: &str) -> Result<(), String> {
    player()?
        .call_method(method, &())
        .map(drop)
        .map_err(failure)
}

/// Text of a metadata value; for string lists (such as `xesam:artist`) the
/// entries are joined with commas.
fn text(value: &Value<'_>) -> Option<String> {
    match value {
        Value::Str(text) => Some(text.as_str().to_owned()),
        Value::ObjectPath(path) => Some(path.as_str().to_owned()),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().filter_map(text).collect();
            (!parts.is_empty()).then(|| parts.join(", "))
        }
        Value::Value(inner) => text(inner),
        _ => None,
    }
}

fn integer(value: &Value<'_>) -> Option<i64> {
    match value {
        Value::I64(number) => Some(*number),
        Value::U64(number) => i64::try_from(*number).ok(),
        Value::I32(number) => Some(i64::from(*number)),
        Value::U32(number) => Some(i64::from(*number)),
        Value::F64(number) if number.is_finite() => Some(*number as i64),
        Value::Value(inner) => integer(inner),
        _ => None,
    }
}

fn secs(micros: i64) -> u32 {
    u32::try_from(micros.max(0) / MICROS_PER_SEC).unwrap_or(u32::MAX)
}

/// `spotify:track:<id>` from the MPRIS track id, whichever form the client uses.
fn track_uri(track_id: &str) -> Option<String> {
    if track_id.starts_with(TRACK_URI_PREFIX) {
        Some(track_id.to_owned())
    } else {
        track_id
            .strip_prefix(TRACK_PATH_PREFIX)
            .map(|id| format!("{TRACK_URI_PREFIX}{id}"))
    }
}

pub fn current_track() -> Result<TrackState, String> {
    let player = player()?;
    let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata").map_err(failure)?;
    let field = |key: &str| metadata.get(key).and_then(|value| text(value));
    let number = |key: &str| metadata.get(key).and_then(|value| integer(value));

    let status: String = player
        .get_property("PlaybackStatus")
        .unwrap_or_else(|_| "Stopped".to_owned());
    // Optional properties: a client that omits one should not hide the track.
    let position: i64 = player.get_property("Position").unwrap_or(0);
    let shuffling: bool = player.get_property("Shuffle").unwrap_or(false);
    let loop_status: String = player
        .get_property("LoopStatus")
        .unwrap_or_else(|_| "None".to_owned());
    let volume: f64 = player.get_property("Volume").unwrap_or(0.0);

    Ok(TrackState {
        title: field("xesam:title").unwrap_or_default(),
        artist: field("xesam:artist").unwrap_or_default(),
        album: field("xesam:album").unwrap_or_default(),
        album_artist: field("xesam:albumArtist").unwrap_or_default(),
        track_number: number("xesam:trackNumber")
            .and_then(|number| u32::try_from(number).ok())
            .filter(|number| *number > 0),
        artwork: field("mpris:artUrl")
            .filter(|url| url.starts_with("https://"))
            .map(Artwork::Url),
        spotify_url: field("mpris:trackid").and_then(|id| track_uri(&id)),
        player_state: status.to_lowercase(),
        position_secs: secs(position),
        duration_secs: number("mpris:length").map_or(0, secs),
        shuffling,
        repeating: loop_status != "None",
        volume: if volume.is_finite() {
            (volume.clamp(0.0, 1.0) * 100.0).round() as u8
        } else {
            0
        },
    })
}

pub fn previous() -> Result<(), String> {
    call("Previous")
}

pub fn play_pause() -> Result<(), String> {
    call("PlayPause")
}

pub fn next() -> Result<(), String> {
    call("Next")
}

pub fn toggle_shuffle() -> Result<(), String> {
    let player = player()?;
    let shuffling: bool = player.get_property("Shuffle").map_err(failure)?;
    player.set_property("Shuffle", !shuffling).map_err(failure)
}

/// Switches between no repeat and repeating the playlist, like the macOS toggle.
pub fn toggle_repeat() -> Result<(), String> {
    let player = player()?;
    let status: String = player.get_property("LoopStatus").map_err(failure)?;
    let next = if status == "None" { "Playlist" } else { "None" };
    player.set_property("LoopStatus", next).map_err(failure)
}

pub fn change_volume(delta: i16) -> Result<(), String> {
    let player = player()?;
    let volume: f64 = player.get_property("Volume").map_err(failure)?;
    let current = (volume.clamp(0.0, 1.0) * 100.0).round() as u8;
    let target = f64::from(stepped_volume(current, delta)) / 100.0;
    player.set_property("Volume", target).map_err(failure)
}

/// Opens the current track with the `spotify:` URI handler, or raises the
/// window when the track id is unknown.
pub fn open() -> Result<(), String> {
    let uri = current_track()?.spotify_url;
    match uri {
        Some(uri) => {
            let status = Command::new("xdg-open")
                .arg(&uri)
                .status()
                .map_err(|error| format!("Could not start xdg-open: {error}"))?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("xdg-open failed with status {status}"))
            }
        }
        None => proxy(ROOT_INTERFACE)?
            .call_method("Raise", &())
            .map(drop)
            .map_err(failure),
    }
}

/// Linux exposes an artwork URL instead.
pub fn thumbnail() -> Option<Vec<u8>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_ids_become_spotify_uris() {
        assert_eq!(
            track_uri("spotify:track:abc").as_deref(),
            Some("spotify:track:abc")
        );
        assert_eq!(
            track_uri("/com/spotify/track/abc").as_deref(),
            Some("spotify:track:abc")
        );
        assert_eq!(track_uri("/org/mpris/MediaPlayer2/TrackList/NoTrack"), None);
    }

    #[test]
    fn metadata_values_are_read_in_any_integer_or_text_form() {
        assert_eq!(integer(&Value::U64(180_000_000)), Some(180_000_000));
        assert_eq!(integer(&Value::I32(3)), Some(3));
        assert_eq!(text(&Value::from("Song")).as_deref(), Some("Song"));
        assert_eq!(text(&Value::from(vec!["A", "B"])).as_deref(), Some("A, B"));
        assert_eq!(secs(180_000_000), 180);
    }
}
