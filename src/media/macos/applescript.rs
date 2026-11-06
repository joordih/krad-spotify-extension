mod commands;
mod fields;
mod music_artwork;
mod script;

pub(super) use commands::control;

use super::process::{self, output_within, SCRIPT_TIMEOUT};
use crate::media::{Capabilities, MediaError, PlaybackState, SourceId, SourceState};
use fields::{Field, Fields};
use music_artwork::MusicArtwork;
use script::Record;
use std::process::Command as Process;
use std::time::Duration;

const APP_TIMEOUT: Duration = Duration::from_secs(1);
const LISTING_MARGIN: Duration = Duration::from_millis(500);
const PGREP: &str = "/usr/bin/pgrep";
const PGREP_FLAGS: &str = "-lx";
const PGREP_ALTERNATIVE: &str = "|";
const PGREP_MATCHED: i32 = 0;
const PGREP_NO_MATCH: i32 = 1;
const PGREP_COLUMN: char = ' ';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum App {
    Spotify,
    Music,
}

impl App {
    const ALL: [Self; 2] = [Self::Spotify, Self::Music];

    pub(super) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|app| app.bundle_id() == id)
    }

    pub(super) fn bundle_id(self) -> &'static str {
        match self {
            Self::Spotify => "com.spotify.client",
            Self::Music => "com.apple.Music",
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Spotify => "Spotify",
            Self::Music => "Music",
        }
    }

    fn process_name(self) -> &'static str {
        match self {
            Self::Spotify => "Spotify",
            Self::Music => "Music",
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Listing {
    pub(super) sources: Vec<SourceState>,
    pub(super) answered: Vec<App>,
    pub(super) failure: Option<MediaError>,
}

impl Listing {
    pub(super) fn failed(error: MediaError) -> Self {
        Self {
            failure: Some(error),
            ..Self::default()
        }
    }
}

#[derive(Default)]
pub(super) struct Scripted {
    music_artwork: MusicArtwork,
}

impl Scripted {
    pub(super) fn list(&self) -> Result<Listing, MediaError> {
        let running = running_apps()?;
        if running.is_empty() {
            return Ok(Listing::default());
        }
        let timeout = running.iter().map(|_| APP_TIMEOUT).sum::<Duration>() + LISTING_MARGIN;
        let stdout = process::osascript(&script::listing(&running, APP_TIMEOUT), timeout)?;
        Ok(self.listing(&String::from_utf8_lossy(&stdout)))
    }

    fn listing(&self, output: &str) -> Listing {
        let mut listing = Listing::default();
        for record in script::records(output) {
            match record {
                Record::Answered(app, fields) => {
                    listing.answered.push(app);
                    listing.sources.extend(self.source(app, &fields));
                }
                Record::Failed(_, error) => {
                    listing.failure.get_or_insert(error);
                }
            }
        }
        listing
    }

    fn source(&self, app: App, fields: &Fields<'_>) -> Option<SourceState> {
        let mut track = fields.track();
        let track_id = fields.get(Field::TrackId);
        match app {
            App::Spotify => {
                track.track_url = Some(track_id.to_owned()).filter(|url| !url.is_empty());
            }
            App::Music => {
                if track.title.is_empty() && track.state == PlaybackState::Stopped {
                    return None;
                }
                track.artwork = self.music_artwork.for_track(track_id);
            }
        }
        Some(SourceState {
            id: SourceId::new(app.bundle_id()),
            name: app.name().to_owned(),
            capabilities: Capabilities::ALL,
            track,
        })
    }
}

fn running_apps() -> Result<Vec<App>, MediaError> {
    let pattern = App::ALL
        .iter()
        .map(|app| app.process_name())
        .collect::<Vec<_>>()
        .join(PGREP_ALTERNATIVE);
    let mut pgrep = Process::new(PGREP);
    pgrep.args([PGREP_FLAGS, &pattern]);
    let output = output_within(pgrep, SCRIPT_TIMEOUT)?;
    match output.status.code() {
        Some(PGREP_MATCHED) => {}
        Some(PGREP_NO_MATCH) => return Ok(Vec::new()),
        _ => return Err(process::failure(PGREP, &output)),
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let names: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split_once(PGREP_COLUMN).map(|(_, name)| name.trim()))
        .collect();
    Ok(App::ALL
        .into_iter()
        .filter(|app| names.contains(&app.process_name()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::Artwork;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn source(app: App, values: &[&str]) -> Result<Option<SourceState>, &'static str> {
        let fields = Fields::parse(values).ok_or("one value per field")?;
        Ok(Scripted::default().source(app, &fields))
    }

    #[test]
    fn music_records_become_sources() -> TestResult {
        let values = [
            "fast forwarding",
            "Song",
            "Artist",
            "Album",
            "Band",
            "5",
            "",
            "",
            "295",
            "59",
            "true",
            "all",
            "80",
        ];
        let source = source(App::Music, &values)?.ok_or("no source")?;
        assert_eq!(source.id.as_str(), "com.apple.Music");
        assert_eq!(source.name, "Music");
        assert_eq!(source.capabilities, Capabilities::ALL);
        let track = source.track;
        assert_eq!(track.album_artist, "Band");
        assert_eq!(track.track_number, Some(5));
        assert_eq!(track.state, PlaybackState::Playing);
        assert_eq!((track.position_secs, track.duration_secs), (59, 295));
        assert!(track.shuffling && track.repeating);
        assert_eq!(track.volume, 80);
        assert_eq!(track.track_url, None);
        Ok(())
    }

    #[test]
    fn music_without_a_track_is_not_a_source() -> TestResult {
        let values = [
            "stopped", "", "", "", "", "", "", "", "", "0", "false", "off", "100",
        ];
        assert_eq!(source(App::Music, &values)?, None);
        Ok(())
    }

    #[test]
    fn spotify_records_keep_their_url_and_artwork() -> TestResult {
        let values = [
            "paused",
            "Song",
            "Artist",
            "Album",
            "",
            "0",
            "https://i.scdn.co/image/x",
            "spotify:track:1",
            "200",
            "3",
            "false",
            "false",
            "150",
        ];
        let track = source(App::Spotify, &values)?.ok_or("no source")?.track;
        assert_eq!(track.track_number, None);
        assert_eq!(
            track.artwork,
            Artwork::from_url("https://i.scdn.co/image/x")
        );
        assert_eq!(track.track_url.as_deref(), Some("spotify:track:1"));
        assert_eq!(track.state, PlaybackState::Paused);
        assert!(!track.repeating);
        assert_eq!(track.volume, 100, "volume stays within 0-100");
        Ok(())
    }
}
