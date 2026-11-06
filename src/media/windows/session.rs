use super::Session;
use crate::media::{Capabilities, MediaError, PlaybackState, Position, TrackState};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::core::HSTRING;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionMediaProperties as MediaProperties,
    GlobalSystemMediaTransportControlsSessionPlaybackInfo as PlaybackInfo,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    GlobalSystemMediaTransportControlsSessionTimelineProperties as Timeline,
};
use windows::Media::MediaPlaybackAutoRepeatMode as RepeatMode;

const NANOS_PER_TICK: u128 = 100;
const TICKS_PER_SEC: i64 = 10_000_000;
const WINDOWS_EPOCH_BEFORE_UNIX: Duration = Duration::from_hours(3_234_576);

pub(super) fn properties(session: &Session) -> Result<MediaProperties, MediaError> {
    session
        .TryGetMediaPropertiesAsync()
        .and_then(|request| request.get())
        .map_err(MediaError::from)
}

pub(super) fn capabilities(session: &Session, open: bool) -> Result<Capabilities, MediaError> {
    let controls = session.GetPlaybackInfo().and_then(|info| info.Controls())?;
    Ok(Capabilities {
        seek: controls.IsPlaybackPositionEnabled().unwrap_or(false),
        shuffle: controls.IsShuffleEnabled().unwrap_or(false),
        repeat: controls.IsRepeatEnabled().unwrap_or(false),
        volume: false,
        open,
    })
}

pub(super) fn track(
    session: &Session,
    properties: &MediaProperties,
) -> Result<TrackState, MediaError> {
    let playback = session.GetPlaybackInfo()?;
    let status = playback.PlaybackStatus()?;
    let state = match status {
        PlaybackStatus::Playing => PlaybackState::Playing,
        PlaybackStatus::Paused => PlaybackState::Paused,
        _ => PlaybackState::Stopped,
    };
    let timeline = session.GetTimelineProperties()?;
    let (position, duration) = progress(&timeline, state);
    Ok(TrackState {
        title: text(properties.Title()),
        artist: text(properties.Artist()),
        album: text(properties.AlbumTitle()),
        album_artist: text(properties.AlbumArtist()),
        track_number: properties
            .TrackNumber()
            .ok()
            .and_then(|number| u32::try_from(number).ok())
            .filter(|&number| number > 0),
        artwork: None,
        track_url: None,
        state,
        position_secs: secs(position),
        duration_secs: secs(duration),
        shuffling: is_shuffling(&playback),
        repeating: is_repeating(&playback),
        volume: 0,
    })
}

pub(super) fn is_shuffling(playback: &PlaybackInfo) -> bool {
    playback
        .IsShuffleActive()
        .and_then(|value| value.Value())
        .unwrap_or(false)
}

pub(super) fn is_repeating(playback: &PlaybackInfo) -> bool {
    playback
        .AutoRepeatMode()
        .and_then(|value| value.Value())
        .is_ok_and(|mode| mode != RepeatMode::None)
}

pub(super) fn seek_ticks(session: &Session, position: Position) -> Result<i64, MediaError> {
    let offset = Duration::try_from_secs_f64(position.secs())
        .ok()
        .and_then(ticks)
        .ok_or(MediaError::InvalidValue)?;
    let start = session
        .GetTimelineProperties()
        .map_or(0, |timeline| start_ticks(&timeline));
    Ok(start.saturating_add(offset))
}

fn progress(timeline: &Timeline, state: PlaybackState) -> (i64, i64) {
    let start = start_ticks(timeline);
    let end = timeline.EndTime().map_or(0, |span| span.Duration);
    let reported = timeline.Position().map_or(0, |span| span.Duration);
    let position = if state == PlaybackState::Playing {
        reported.saturating_add(elapsed_since_update(timeline))
    } else {
        reported
    };
    let duration = end.saturating_sub(start).max(0);
    (position.saturating_sub(start).clamp(0, duration), duration)
}

fn start_ticks(timeline: &Timeline) -> i64 {
    timeline.StartTime().map_or(0, |span| span.Duration)
}

fn elapsed_since_update(timeline: &Timeline) -> i64 {
    let Ok(updated) = timeline.LastUpdatedTime() else {
        return 0;
    };
    now_ticks().map_or(0, |now| now.saturating_sub(updated.UniversalTime).max(0))
}

fn now_ticks() -> Option<i64> {
    let since_unix = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    since_unix
        .checked_add(WINDOWS_EPOCH_BEFORE_UNIX)
        .and_then(ticks)
}

fn ticks(duration: Duration) -> Option<i64> {
    i64::try_from(duration.as_nanos() / NANOS_PER_TICK).ok()
}

fn secs(ticks: i64) -> u32 {
    u32::try_from(ticks.max(0) / TICKS_PER_SEC).unwrap_or(u32::MAX)
}

fn text(value: windows::core::Result<HSTRING>) -> String {
    value.map(|text| text.to_string_lossy()).unwrap_or_default()
}
