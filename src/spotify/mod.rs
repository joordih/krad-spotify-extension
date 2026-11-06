//! Local Spotify control, with one backend per operating system:
//!
//! - macOS: Spotify's AppleScript dictionary through `osascript`.
//! - Linux: the MPRIS D-Bus interface (`org.mpris.MediaPlayer2.spotify`).
//! - Windows: the system media transport controls (`Windows.Media.Control`).
//!
//! Each backend exposes the same functions; [`CAPABILITIES`] says which
//! features the current platform supports so the view can hide the rest.

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
#[path = "unsupported.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;

/// Volume points moved by `volume_up`/`volume_down` unless the user configures another step.
pub const DEFAULT_VOLUME_STEP: u8 = 10;
pub const NOT_RUNNING: &str = "Spotify is not running";
pub const UNSUPPORTED: &str = "Not supported by Spotify on this platform";

/// What the current platform's backend can read and control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// Jumping to a position in the track.
    pub seek: bool,
    pub shuffle: bool,
    pub repeat: bool,
    pub volume: bool,
}

pub const CAPABILITIES: Capabilities = platform::CAPABILITIES;

/// Where the current track's artwork comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Artwork {
    /// A remote HTTPS image (macOS and Linux).
    Url(String),
    /// The thumbnail the OS media session holds, read with [`thumbnail`]; the
    /// string identifies the track so the cached image can be reused (Windows).
    Thumbnail(String),
}

/// Current track metadata and playback state.
///
/// Missing text properties are represented as empty strings, and missing
/// optional values as `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackState {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track_number: Option<u32>,
    pub artwork: Option<Artwork>,
    /// `spotify:track:…` URI of the current track, when the platform exposes it.
    pub spotify_url: Option<String>,
    /// `playing`, `paused`, or `stopped`.
    pub player_state: String,
    pub position_secs: u32,
    pub duration_secs: u32,
    pub shuffling: bool,
    pub repeating: bool,
    /// 0–100.
    pub volume: u8,
}

/// A small adapter for the local Spotify desktop application.
#[derive(Clone, Copy, Debug)]
pub struct SpotifyAdapter {
    volume_step: u8,
}

impl Default for SpotifyAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SpotifyAdapter {
    pub fn new() -> Self {
        Self {
            volume_step: DEFAULT_VOLUME_STEP,
        }
    }

    /// An adapter whose volume commands move `step` points, kept between 1 and 100.
    pub fn with_volume_step(step: u8) -> Self {
        Self {
            volume_step: step.clamp(1, 100),
        }
    }

    /// Read the current track and player state from Spotify.
    pub fn current_track(&self) -> Result<TrackState, String> {
        platform::current_track()
    }

    pub fn previous(&self) -> Result<(), String> {
        platform::previous()
    }

    pub fn play_pause(&self) -> Result<(), String> {
        platform::play_pause()
    }

    pub fn next(&self) -> Result<(), String> {
        platform::next()
    }

    pub fn toggle_shuffle(&self) -> Result<(), String> {
        supported(CAPABILITIES.shuffle)?;
        platform::toggle_shuffle()
    }

    pub fn toggle_repeat(&self) -> Result<(), String> {
        supported(CAPABILITIES.repeat)?;
        platform::toggle_repeat()
    }

    pub fn volume_up(&self) -> Result<(), String> {
        supported(CAPABILITIES.volume)?;
        platform::change_volume(i16::from(self.volume_step))
    }

    pub fn volume_down(&self) -> Result<(), String> {
        supported(CAPABILITIES.volume)?;
        platform::change_volume(-i16::from(self.volume_step))
    }

    /// Sets the volume to `percent` (0–100).
    pub fn set_volume(&self, percent: f64) -> Result<(), String> {
        supported(CAPABILITIES.volume)?;
        platform::set_volume(percent_of(percent))
    }

    /// Jumps to `secs` seconds into the current track.
    pub fn seek(&self, secs: f64) -> Result<(), String> {
        supported(CAPABILITIES.seek)?;
        if !secs.is_finite() {
            return Err("Invalid position".to_owned());
        }
        platform::seek(secs.max(0.0))
    }

    /// Brings Spotify to the front, on the current track where the platform allows it.
    pub fn open(&self) -> Result<(), String> {
        platform::open()
    }
}

/// Image bytes of the current track's thumbnail, for [`Artwork::Thumbnail`].
pub fn thumbnail() -> Option<Vec<u8>> {
    platform::thumbnail()
}

fn supported(capability: bool) -> Result<(), String> {
    if capability {
        Ok(())
    } else {
        Err(UNSUPPORTED.to_owned())
    }
}

/// A slider value as a whole volume percentage within 0–100.
fn percent_of(value: f64) -> u8 {
    if value.is_finite() {
        value.round().clamp(0.0, 100.0) as u8
    } else {
        0
    }
}

/// `volume + delta`, kept within 0–100.
#[allow(dead_code)] // Unused by the Windows backend, which has no volume control.
fn stepped_volume(volume: u8, delta: i16) -> u8 {
    (i16::from(volume) + delta).clamp(0, 100) as u8
}

#[allow(dead_code)] // Only some backends report absent values as empty strings.
fn nonempty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_steps_stay_within_bounds() {
        assert_eq!(stepped_volume(95, 10), 100);
        assert_eq!(stepped_volume(5, -10), 0);
        assert_eq!(stepped_volume(50, 10), 60);
    }

    #[test]
    fn slider_values_become_whole_percentages() {
        assert_eq!(percent_of(42.6), 43);
        assert_eq!(percent_of(140.0), 100);
        assert_eq!(percent_of(-3.0), 0);
        assert_eq!(percent_of(f64::NAN), 0);
    }

    /// Needs Spotify open: `cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn reads_live_track() {
        let track = SpotifyAdapter::new().current_track().unwrap();
        println!("{track:#?}");
    }
}
