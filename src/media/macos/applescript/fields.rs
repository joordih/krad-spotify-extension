use super::App;
use crate::media::{self, Artwork, PlaybackState, TrackState};

const VALUE: &str = "v";
const TRACK: &str = "t";
const TRUE: &str = "true";
const REPEAT_OFF: [&str; 3] = ["", "off", "false"];
const PLAYING: [&str; 3] = ["playing", "fast forwarding", "rewinding"];
const PAUSED: &str = "paused";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Field {
    PlayerState,
    Title,
    Artist,
    Album,
    AlbumArtist,
    TrackNumber,
    ArtworkUrl,
    TrackId,
    Duration,
    Position,
    Shuffle,
    Repeat,
    Volume,
}

impl Field {
    const ALL: [Self; 13] = [
        Self::PlayerState,
        Self::Title,
        Self::Artist,
        Self::Album,
        Self::AlbumArtist,
        Self::TrackNumber,
        Self::ArtworkUrl,
        Self::TrackId,
        Self::Duration,
        Self::Position,
        Self::Shuffle,
        Self::Repeat,
        Self::Volume,
    ];
    pub(super) const COUNT: usize = Self::ALL.len();

    fn index(self) -> usize {
        self as usize
    }

    fn scope(self) -> Scope {
        match self {
            Self::PlayerState => Scope::Required,
            Self::Title
            | Self::Artist
            | Self::Album
            | Self::AlbumArtist
            | Self::TrackNumber
            | Self::ArtworkUrl
            | Self::TrackId
            | Self::Duration => Scope::Track,
            Self::Position | Self::Shuffle | Self::Repeat | Self::Volume => Scope::Player,
        }
    }
}

#[derive(Clone, Copy)]
enum Scope {
    Required,
    Track,
    Player,
}

#[derive(Clone, Copy)]
enum Conversion {
    Text,
    Rounded,
    Milliseconds,
}

impl Conversion {
    fn of(self, value: &str) -> String {
        match self {
            Self::Text => value.to_owned(),
            Self::Rounded => format!("(round {value})"),
            Self::Milliseconds => format!("(round ({value} / 1000))"),
        }
    }
}

fn property(app: App, field: Field) -> Option<(&'static str, Conversion)> {
    use Conversion::{Milliseconds, Rounded, Text};
    Some(match (field, app) {
        (Field::PlayerState, _) => ("player state", Text),
        (Field::Title, _) => ("name", Text),
        (Field::Artist, _) => ("artist", Text),
        (Field::Album, _) => ("album", Text),
        (Field::AlbumArtist, _) => ("album artist", Text),
        (Field::TrackNumber, _) => ("track number", Text),
        (Field::ArtworkUrl, App::Spotify) => ("artwork url", Text),
        (Field::ArtworkUrl, App::Music) => return None,
        (Field::TrackId, App::Spotify) => ("spotify url", Text),
        (Field::TrackId, App::Music) => ("persistent ID", Text),
        (Field::Duration, App::Spotify) => ("duration", Milliseconds),
        (Field::Duration, App::Music) => ("duration", Rounded),
        (Field::Position, _) => ("player position", Rounded),
        (Field::Shuffle, App::Spotify) => ("shuffling", Text),
        (Field::Shuffle, App::Music) => ("shuffle enabled", Text),
        (Field::Repeat, App::Spotify) => ("repeating", Text),
        (Field::Repeat, App::Music) => ("song repeat", Text),
        (Field::Volume, _) => ("sound volume", Text),
    })
}

pub(super) fn empty_list() -> String {
    format!("{{{}}}", vec!["\"\""; Field::COUNT].join(", "))
}

pub(super) fn read_block(app: App, list: &str) -> String {
    let mut required = String::new();
    let mut track = String::new();
    let mut player = String::new();
    for field in Field::ALL {
        let Some((name, conversion)) = property(app, field) else {
            continue;
        };
        let assign = |source: &str| {
            format!(
                "set {VALUE} to {source}\nset item {item} of {list} to {converted} as text\n",
                item = field.index() + 1,
                converted = conversion.of(VALUE),
            )
        };
        match field.scope() {
            Scope::Required => required.push_str(&assign(name)),
            Scope::Track => track.push_str(&guarded(&assign(&format!("{name} of {TRACK}")))),
            Scope::Player => player.push_str(&guarded(&assign(name))),
        }
    }
    format!("{required}try\nset {TRACK} to current track\n{track}end try\n{player}")
}

fn guarded(body: &str) -> String {
    format!("try\n{body}end try\n")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Fields<'a>([&'a str; Field::COUNT]);

impl<'a> Fields<'a> {
    pub(super) fn parse(values: &[&'a str]) -> Option<Self> {
        <[&str; Field::COUNT]>::try_from(values).ok().map(Self)
    }

    pub(super) fn get(&self, field: Field) -> &'a str {
        self.0.get(field.index()).copied().unwrap_or_default()
    }

    pub(super) fn track(&self) -> TrackState {
        TrackState {
            title: self.get(Field::Title).to_owned(),
            artist: self.get(Field::Artist).to_owned(),
            album: self.get(Field::Album).to_owned(),
            album_artist: self.get(Field::AlbumArtist).to_owned(),
            track_number: self
                .get(Field::TrackNumber)
                .parse()
                .ok()
                .filter(|number| *number > 0),
            artwork: Artwork::from_url(self.get(Field::ArtworkUrl)),
            track_url: None,
            state: playback_state(self.get(Field::PlayerState)),
            position_secs: self.get(Field::Position).parse().unwrap_or_default(),
            duration_secs: self.get(Field::Duration).parse().unwrap_or_default(),
            shuffling: self.get(Field::Shuffle) == TRUE,
            repeating: !REPEAT_OFF.contains(&self.get(Field::Repeat)),
            volume: media::Volume::new(self.get(Field::Volume).parse().unwrap_or_default())
                .percent(),
        }
    }
}

fn playback_state(text: &str) -> PlaybackState {
    if PLAYING.contains(&text) {
        PlaybackState::Playing
    } else if text == PAUSED {
        PlaybackState::Paused
    } else {
        PlaybackState::Stopped
    }
}
