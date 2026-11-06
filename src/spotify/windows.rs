//! Windows backend: the system media transport controls
//! (`Windows.Media.Control`), which the Spotify desktop client feeds with the
//! current track, playback state and thumbnail.
//!
//! These controls have no per-app volume, so volume is unsupported here.

use super::{Artwork, Capabilities, TrackState, NOT_RUNNING};
use std::hash::{Hash, Hasher};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::core::Interface;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Media::MediaPlaybackAutoRepeatMode as RepeatMode;
use windows::Storage::Streams::{DataReader, IInputStream};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows_future::IAsyncOperation;

pub const CAPABILITIES: Capabilities = Capabilities {
    seek: true,
    shuffle: true,
    repeat: true,
    volume: false,
};

/// Spotify's media session is identified by its app id, e.g. `Spotify.exe`
/// or `SpotifyAB.SpotifyMusic_…!Spotify` for the Microsoft Store build.
const SPOTIFY_APP: &str = "spotify";
/// `TimeSpan`/`DateTime` tick: 100 ns.
const TICKS_PER_SEC: i64 = 10_000_000;
/// Seconds between 1601-01-01 (Windows epoch) and 1970-01-01 (Unix epoch).
const WINDOWS_TO_UNIX_SECS: i64 = 11_644_473_600;
/// Hides the console window of helper processes.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn failure(error: windows::core::Error) -> String {
    format!("Windows media error: {}", error.message())
}

/// The Spotify media session, or [`NOT_RUNNING`] when Spotify has none.
fn session() -> Result<Session, String> {
    // WinRT needs an initialised apartment on the calling thread; a repeat
    // call on the same thread is harmless.
    // SAFETY: plain FFI call with a valid flag.
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let manager = SessionManager::RequestAsync()
        .and_then(|request| request.get())
        .map_err(failure)?;
    let sessions = manager.GetSessions().map_err(failure)?;
    let count = sessions.Size().map_err(failure)?;
    (0..count)
        .filter_map(|index| sessions.GetAt(index).ok())
        .find(|session| {
            session
                .SourceAppUserModelId()
                .map(|id| id.to_string_lossy().to_lowercase().contains(SPOTIFY_APP))
                .unwrap_or(false)
        })
        .ok_or_else(|| NOT_RUNNING.to_owned())
}

fn now_ticks() -> i64 {
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (unix.as_secs() as i64 + WINDOWS_TO_UNIX_SECS) * TICKS_PER_SEC
        + i64::from(unix.subsec_nanos()) / 100
}

fn secs(ticks: i64) -> u32 {
    u32::try_from(ticks.max(0) / TICKS_PER_SEC).unwrap_or(u32::MAX)
}

/// Identifies a track so its cached thumbnail can be reused.
fn thumbnail_key(title: &str, artist: &str, album: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (title, artist, album).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn current_track() -> Result<TrackState, String> {
    let session = session()?;
    let properties = session
        .TryGetMediaPropertiesAsync()
        .and_then(|request| request.get())
        .map_err(failure)?;
    let read = |value: windows::core::Result<windows::core::HSTRING>| {
        value.map(|text| text.to_string_lossy()).unwrap_or_default()
    };
    let title = read(properties.Title());
    let artist = read(properties.Artist());
    let album = read(properties.AlbumTitle());
    let album_artist = read(properties.AlbumArtist());
    let has_thumbnail = properties.Thumbnail().is_ok();

    let playback = session.GetPlaybackInfo().map_err(failure)?;
    let status = playback.PlaybackStatus().map_err(failure)?;
    let playing = status == PlaybackStatus::Playing;
    let shuffling = playback
        .IsShuffleActive()
        .and_then(|value| value.Value())
        .unwrap_or(false);
    let repeating = playback
        .AutoRepeatMode()
        .and_then(|value| value.Value())
        .map(|mode| mode != RepeatMode::None)
        .unwrap_or(false);

    // The timeline is a snapshot from `LastUpdatedTime`; while playing, the
    // time since then is added so the position keeps moving between updates.
    let timeline = session.GetTimelineProperties().map_err(failure)?;
    let start = timeline.StartTime().map(|span| span.Duration).unwrap_or(0);
    let end = timeline.EndTime().map(|span| span.Duration).unwrap_or(0);
    let mut position = timeline.Position().map(|span| span.Duration).unwrap_or(0);
    if playing {
        if let Ok(updated) = timeline.LastUpdatedTime() {
            position += (now_ticks() - updated.UniversalTime).max(0);
        }
    }
    let duration = (end - start).max(0);

    Ok(TrackState {
        artwork: has_thumbnail.then(|| Artwork::Thumbnail(thumbnail_key(&title, &artist, &album))),
        title,
        artist,
        album,
        album_artist,
        track_number: properties
            .TrackNumber()
            .ok()
            .and_then(|number| u32::try_from(number).ok())
            .filter(|number| *number > 0),
        spotify_url: None,
        player_state: match status {
            PlaybackStatus::Playing => "playing",
            PlaybackStatus::Paused => "paused",
            _ => "stopped",
        }
        .to_owned(),
        position_secs: secs((position - start).clamp(0, duration)),
        duration_secs: secs(duration),
        shuffling,
        repeating,
        volume: 0,
    })
}

/// Runs a `Try…Async` control; Windows answers `false` when the app refuses it.
fn control(
    request: impl FnOnce(&Session) -> windows::core::Result<IAsyncOperation<bool>>,
) -> Result<(), String> {
    let session = session()?;
    let accepted = request(&session)
        .and_then(|operation| operation.get())
        .map_err(failure)?;
    if accepted {
        Ok(())
    } else {
        Err("Spotify refused the request".to_owned())
    }
}

pub fn previous() -> Result<(), String> {
    control(|session| session.TrySkipPreviousAsync())
}

pub fn play_pause() -> Result<(), String> {
    control(|session| session.TryTogglePlayPauseAsync())
}

pub fn next() -> Result<(), String> {
    control(|session| session.TrySkipNextAsync())
}

pub fn toggle_shuffle() -> Result<(), String> {
    let shuffling = current_track()?.shuffling;
    control(|session| session.TryChangeShuffleActiveAsync(!shuffling))
}

/// Switches between no repeat and repeating the list, like the macOS toggle.
pub fn toggle_repeat() -> Result<(), String> {
    let mode = if current_track()?.repeating {
        RepeatMode::None
    } else {
        RepeatMode::List
    };
    control(|session| session.TryChangeAutoRepeatModeAsync(mode))
}

pub fn change_volume(_delta: i16) -> Result<(), String> {
    Err(super::UNSUPPORTED.to_owned())
}

pub fn set_volume(_percent: u8) -> Result<(), String> {
    Err(super::UNSUPPORTED.to_owned())
}

/// The position is given in timeline ticks from the start of the track.
pub fn seek(secs: f64) -> Result<(), String> {
    let session = session()?;
    let start = session
        .GetTimelineProperties()
        .and_then(|timeline| timeline.StartTime())
        .map(|span| span.Duration)
        .unwrap_or(0);
    let ticks = start + (secs * TICKS_PER_SEC as f64) as i64;
    control(|session| session.TryChangePlaybackPositionAsync(ticks))
}

/// Brings Spotify to the front through its `spotify:` URI handler. The media
/// session does not expose the track id, so it opens on whatever is showing.
pub fn open() -> Result<(), String> {
    session()?;
    let status = Command::new("cmd")
        .args(["/C", "start", "", "spotify:"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|error| format!("Could not start cmd: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Opening Spotify failed with status {status}"))
    }
}

/// Image bytes of the session's thumbnail (usually JPEG or PNG).
pub fn thumbnail() -> Option<Vec<u8>> {
    let properties = session()
        .ok()?
        .TryGetMediaPropertiesAsync()
        .and_then(|request| request.get())
        .ok()?;
    let stream = properties
        .Thumbnail()
        .and_then(|reference| reference.OpenReadAsync())
        .and_then(|request| request.get())
        .ok()?;
    let size = u32::try_from(stream.Size().ok()?).ok()?;
    let input: IInputStream = stream.cast().ok()?;
    let reader = DataReader::CreateDataReader(&input).ok()?;
    reader.LoadAsync(size).and_then(|load| load.get()).ok()?;
    let mut bytes = vec![0; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    (!bytes.is_empty()).then_some(bytes)
}
