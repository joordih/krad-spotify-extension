mod artwork;
mod control;
mod ignore;
pub mod select;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod unsupported;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux::Platform;
#[cfg(target_os = "macos")]
pub(crate) use macos::spawn;
#[cfg(target_os = "macos")]
use macos::Platform;
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
use unsupported::Platform;
#[cfg(windows)]
use windows::Platform;

pub use artwork::{content_key, Artwork, ArtworkBytes, HttpsUrl};
pub use control::{perform, Command, Control, Position, Volume};
pub use ignore::IgnoredApps;

use std::collections::HashSet;
use std::fmt;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MediaError {
    #[error("Nothing is playing")]
    NothingPlaying,
    #[error("That media source is no longer available")]
    SourceGone,
    #[error("Not supported by this source")]
    Unsupported,
    #[error("macOS does not allow controlling {app}")]
    AutomationDenied { app: String },
    #[error("The media app didn't respond in time")]
    TimedOut,
    #[error("Invalid value")]
    InvalidValue,
    #[error("Media control is not available on this platform")]
    Unavailable,
    #[error("{0}")]
    Backend(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(String);

impl SourceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent control a backend reports per source"
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub seek: bool,
    pub shuffle: bool,
    pub repeat: bool,
    pub volume: bool,
    pub open: bool,
}

impl Capabilities {
    pub const ALL: Self = Self {
        seek: true,
        shuffle: true,
        repeat: true,
        volume: true,
        open: true,
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
    #[default]
    Stopped,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackState {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track_number: Option<u32>,
    pub artwork: Option<Artwork>,
    pub track_url: Option<String>,
    pub state: PlaybackState,
    pub position_secs: u32,
    pub duration_secs: u32,
    pub shuffling: bool,
    pub repeating: bool,
    pub volume: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceState {
    pub id: SourceId,
    pub name: String,
    pub capabilities: Capabilities,
    pub track: TrackState,
}

impl SourceState {
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.track.state == PlaybackState::Playing
    }

    fn has_media(&self) -> bool {
        self.track.state != PlaybackState::Stopped || !self.track.title.is_empty()
    }
}

pub trait Backend: Send + Sync {
    /// # Errors
    ///
    /// Fails when the backend cannot look for sources at all.
    fn list_sources(&self) -> Result<Vec<SourceState>, MediaError>;

    /// # Errors
    ///
    /// Fails when the source is gone, does not support the command, or refuses it.
    fn control(&self, source: &SourceId, command: Command) -> Result<(), MediaError>;
}

pub fn backend() -> &'static dyn Backend {
    static BACKEND: OnceLock<Platform> = OnceLock::new();
    BACKEND.get_or_init(Platform::default)
}

/// # Errors
///
/// Fails when the backend cannot list its sources.
pub fn visible_sources(
    backend: &dyn Backend,
    ignored: &IgnoredApps,
) -> Result<Vec<SourceState>, MediaError> {
    let mut seen = HashSet::new();
    let mut sources: Vec<SourceState> = backend
        .list_sources()?
        .into_iter()
        .filter(SourceState::has_media)
        .filter(|source| !ignored.ignores(source))
        .filter(|source| seen.insert(source.id.clone()))
        .collect();
    sources.sort_by_cached_key(|source| (source.name.to_lowercase(), source.id.clone()));
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Listing(Vec<SourceState>);

    impl Backend for Listing {
        fn list_sources(&self) -> Result<Vec<SourceState>, MediaError> {
            Ok(self.0.clone())
        }

        fn control(&self, _source: &SourceId, _command: Command) -> Result<(), MediaError> {
            Ok(())
        }
    }

    fn source(id: &str, name: &str, state: PlaybackState, title: &str) -> SourceState {
        SourceState {
            id: SourceId::new(id),
            name: name.to_owned(),
            track: TrackState {
                title: title.to_owned(),
                state,
                ..TrackState::default()
            },
            ..SourceState::default()
        }
    }

    fn playing(id: &str, name: &str) -> SourceState {
        source(id, name, PlaybackState::Playing, "Song")
    }

    fn names(sources: &[SourceState]) -> Vec<&str> {
        sources.iter().map(|source| source.name.as_str()).collect()
    }

    #[test]
    fn visible_sources_drop_ignored_and_duplicates_and_sort_by_name() -> Result<(), MediaError> {
        let backend = Listing(vec![
            playing("vlc", "VLC"),
            playing("com.google.Chrome", "Google Chrome"),
            playing("spotify", "Spotify"),
            playing("vlc", "VLC (again)"),
            playing("music", "apple Music"),
        ]);
        let visible = visible_sources(&backend, &IgnoredApps::parse("chrome"))?;
        assert_eq!(names(&visible), ["apple Music", "Spotify", "VLC"]);
        Ok(())
    }

    #[test]
    fn sources_without_media_are_not_visible() -> Result<(), MediaError> {
        let backend = Listing(vec![
            source("idle", "Idle", PlaybackState::Stopped, ""),
            source("loaded", "Loaded", PlaybackState::Stopped, "Song"),
            source("paused", "Paused", PlaybackState::Paused, ""),
        ]);
        let visible = visible_sources(&backend, &IgnoredApps::default())?;
        assert_eq!(names(&visible), ["Loaded", "Paused"]);
        Ok(())
    }

    #[test]
    #[ignore = "needs a media app running: cargo test -- --ignored"]
    fn lists_live_sources() -> Result<(), MediaError> {
        backend().list_sources().map(drop)
    }
}
