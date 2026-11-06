use super::payload::NowPlaying;
use crate::media::{Artwork, Capabilities, SourceId, SourceState, TrackState};
use std::time::{Duration, SystemTime};

pub(super) const SHUFFLE_OFF: i64 = 1;
pub(super) const SHUFFLE_TRACKS: i64 = 3;
pub(super) const REPEAT_OFF: i64 = 1;
pub(super) const REPEAT_ALL: i64 = 3;
const PLAYING_RATE: f64 = 1.0;
const HALF_SECOND: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Snapshot {
    pub(super) source: SourceState,
    elapsed: Duration,
    reported_at: Option<SystemTime>,
    rate: f64,
    pub(super) shuffle_mode: Option<i64>,
    pub(super) repeat_mode: Option<i64>,
}

impl Snapshot {
    pub(super) fn new(now_playing: NowPlaying, name: String, artwork: Option<Artwork>) -> Self {
        let duration_secs = now_playing.duration.map_or(0, whole_secs);
        let shuffle_mode = now_playing.shuffle_mode;
        let repeat_mode = now_playing.repeat_mode;
        Self {
            source: SourceState {
                id: SourceId::new(now_playing.id),
                name,
                capabilities: Capabilities {
                    seek: duration_secs > 0,
                    shuffle: shuffle_mode.is_some(),
                    repeat: repeat_mode.is_some(),
                    volume: false,
                    open: true,
                },
                track: TrackState {
                    title: now_playing.title,
                    artist: now_playing.artist,
                    album: now_playing.album,
                    track_number: now_playing.track_number,
                    artwork,
                    state: now_playing.state,
                    duration_secs,
                    shuffling: shuffle_mode.is_some_and(|mode| mode != SHUFFLE_OFF),
                    repeating: repeat_mode.is_some_and(|mode| mode != REPEAT_OFF),
                    ..TrackState::default()
                },
            },
            elapsed: now_playing.elapsed,
            reported_at: now_playing.reported_at,
            rate: now_playing.rate.unwrap_or(PLAYING_RATE),
            shuffle_mode,
            repeat_mode,
        }
    }

    pub(super) fn source_at(&self, now: SystemTime) -> SourceState {
        let mut source = self.source.clone();
        source.track.position_secs = self.position_secs(now);
        source
    }

    fn position_secs(&self, now: SystemTime) -> u32 {
        let mut elapsed = self.elapsed;
        if let Some(reported_at) = self.reported_at.filter(|_| self.source.is_playing()) {
            let since = now.duration_since(reported_at).unwrap_or_default();
            if self.rate > 0.0 {
                elapsed += Duration::try_from_secs_f64(since.as_secs_f64() * self.rate)
                    .unwrap_or_default();
            }
        }
        let duration = Duration::from_secs(u64::from(self.source.track.duration_secs));
        if !duration.is_zero() {
            elapsed = elapsed.min(duration);
        }
        whole_secs(elapsed)
    }
}

fn whole_secs(duration: Duration) -> u32 {
    u32::try_from(duration.saturating_add(HALF_SECOND).as_secs()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::super::payload::{Payload, Update};
    use super::*;
    use crate::media::PlaybackState;
    use std::time::UNIX_EPOCH;

    const SAFARI: &str = r#"{"type":"data","diff":false,"payload":{"title":"A video","artist":"A channel","bundleIdentifier":"com.apple.WebKit.GPU","parentApplicationBundleIdentifier":"com.apple.Safari","processIdentifier":4,"playing":true,"durationMicros":120000000,"elapsedTimeMicros":10000000,"timestampEpochMicros":1000000000,"playbackRate":1,"artworkData":"AQID"}}"#;

    type TestResult = Result<(), &'static str>;

    fn fold(lines: &[&str]) -> Result<Option<Snapshot>, &'static str> {
        let mut payload = Payload::default();
        let mut artwork = None;
        for line in lines {
            if payload.apply(Update::parse(line).ok_or("not an update")?) {
                artwork = payload.artwork();
            }
        }
        Ok(payload.now_playing().map(|now_playing| {
            let name = now_playing.id.clone();
            Snapshot::new(now_playing, name, artwork)
        }))
    }

    fn at_micros(micros: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_micros(micros)
    }

    #[test]
    fn browser_media_becomes_a_source_of_the_parent_app() -> TestResult {
        let snapshot = fold(&[SAFARI])?.ok_or("nothing plays")?;
        let source = &snapshot.source;
        assert_eq!(source.id.as_str(), "com.apple.Safari");
        assert_eq!(source.track.title, "A video");
        assert_eq!(source.track.state, PlaybackState::Playing);
        assert_eq!(source.track.duration_secs, 120);
        assert_eq!(source.track.artwork, Artwork::from_bytes(vec![1, 2, 3]));
        assert_eq!(
            source.capabilities,
            Capabilities {
                seek: true,
                open: true,
                ..Capabilities::default()
            }
        );
        assert_eq!(snapshot.position_secs(at_micros(1_005_000_000)), 15);
        assert_eq!(
            snapshot.position_secs(at_micros(9_000_000_000_000_000)),
            120,
            "stays within the track"
        );
        Ok(())
    }

    #[test]
    fn diffs_update_and_remove_keys() -> TestResult {
        let pause = r#"{"type":"data","diff":true,"payload":{"playing":false,"artworkData":null,"elapsedTimeMicros":30000000}}"#;
        let snapshot = fold(&[SAFARI, pause])?.ok_or("nothing plays")?;
        assert_eq!(snapshot.source.track.state, PlaybackState::Paused);
        assert_eq!(snapshot.source.track.artwork, None);
        assert_eq!(
            snapshot.position_secs(at_micros(5_000_000_000_000_000)),
            30,
            "paused does not advance"
        );
        assert_eq!(
            snapshot.source.track.artist, "A channel",
            "kept from before"
        );
        Ok(())
    }

    #[test]
    fn an_empty_payload_means_nothing_plays() -> TestResult {
        let empty = r#"{"type":"data","diff":false,"payload":{}}"#;
        assert_eq!(fold(&[SAFARI, empty])?, None);
        Ok(())
    }

    #[test]
    fn shuffle_and_repeat_are_offered_when_reported() -> TestResult {
        let vlc = r#"{"type":"data","diff":false,"payload":{"title":"Song","bundleIdentifier":"org.videolan.vlc","processIdentifier":9,"playing":false,"shuffleMode":3,"repeatMode":1,"trackNumber":2}}"#;
        let source = fold(&[vlc])?.ok_or("nothing plays")?.source;
        assert!(source.capabilities.shuffle && source.capabilities.repeat);
        assert!(!source.capabilities.seek, "no duration, no seeking");
        assert!(source.track.shuffling && !source.track.repeating);
        assert_eq!(source.track.track_number, Some(2));
        Ok(())
    }
}
