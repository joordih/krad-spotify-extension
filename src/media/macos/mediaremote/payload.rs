use crate::media::{Artwork, PlaybackState};
use base64::Engine as _;
use serde_json::{Map, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TYPE: &str = "type";
const DATA: &str = "data";
const DIFF: &str = "diff";
const PAYLOAD: &str = "payload";
const ARTWORK: &str = "artworkData";
const PARENT_BUNDLE_ID: &str = "parentApplicationBundleIdentifier";
const BUNDLE_ID: &str = "bundleIdentifier";
const TITLE: &str = "title";
const ARTIST: &str = "artist";
const ALBUM: &str = "album";
const PLAYING: &str = "playing";
const DURATION: &str = "durationMicros";
const ELAPSED: &str = "elapsedTimeMicros";
const TIMESTAMP: &str = "timestampEpochMicros";
const RATE: &str = "playbackRate";
const TRACK_NUMBER: &str = "trackNumber";
const SHUFFLE_MODE: &str = "shuffleMode";
const REPEAT_MODE: &str = "repeatMode";
const MICROS_PER_SEC: f64 = 1e6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Full,
    Diff,
}

#[derive(Debug)]
pub(super) struct Update {
    kind: Kind,
    payload: Map<String, Value>,
}

impl Update {
    pub(super) fn parse(line: &str) -> Option<Self> {
        let Value::Object(mut line) = serde_json::from_str(line).ok()? else {
            return None;
        };
        if line.get(TYPE)?.as_str()? != DATA {
            return None;
        }
        let kind = if line.get(DIFF).and_then(Value::as_bool) == Some(true) {
            Kind::Diff
        } else {
            Kind::Full
        };
        let Value::Object(payload) = line.remove(PAYLOAD)? else {
            return None;
        };
        Some(Self { kind, payload })
    }
}

#[derive(Debug, Default)]
pub(super) struct Payload(Map<String, Value>);

impl Payload {
    pub(super) fn apply(&mut self, update: Update) -> bool {
        match update.kind {
            Kind::Diff => {
                let artwork_changed = update.payload.contains_key(ARTWORK);
                for (key, value) in update.payload {
                    if value.is_null() {
                        self.0.remove(&key);
                    } else {
                        self.0.insert(key, value);
                    }
                }
                artwork_changed
            }
            Kind::Full => {
                let artwork_changed = self.0.get(ARTWORK) != update.payload.get(ARTWORK);
                self.0 = update.payload;
                artwork_changed
            }
        }
    }

    pub(super) fn artwork(&self) -> Option<Artwork> {
        let data = self.0.get(ARTWORK)?.as_str()?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .ok()?;
        Artwork::from_bytes(bytes)
    }

    pub(super) fn now_playing(&self) -> Option<NowPlaying> {
        let id = [PARENT_BUNDLE_ID, BUNDLE_ID]
            .into_iter()
            .filter_map(|key| self.text(key))
            .find(|id| !id.is_empty())?
            .to_owned();
        let title = self.text(TITLE).unwrap_or_default().to_owned();
        if title.is_empty() {
            return None;
        }
        let state = if self.0.get(PLAYING).and_then(Value::as_bool)? {
            PlaybackState::Playing
        } else {
            PlaybackState::Paused
        };
        Some(NowPlaying {
            id,
            title,
            artist: self.text(ARTIST).unwrap_or_default().to_owned(),
            album: self.text(ALBUM).unwrap_or_default().to_owned(),
            state,
            duration: self.micros(DURATION),
            elapsed: self.micros(ELAPSED).unwrap_or_default(),
            reported_at: self
                .micros(TIMESTAMP)
                .filter(|since| !since.is_zero())
                .and_then(|since| UNIX_EPOCH.checked_add(since)),
            rate: self.number(RATE),
            track_number: self.number(TRACK_NUMBER).and_then(whole_number),
            shuffle_mode: self.0.get(SHUFFLE_MODE).and_then(Value::as_i64),
            repeat_mode: self.0.get(REPEAT_MODE).and_then(Value::as_i64),
        })
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }

    fn number(&self, key: &str) -> Option<f64> {
        self.0.get(key).and_then(Value::as_f64)
    }

    fn micros(&self, key: &str) -> Option<Duration> {
        self.number(key)
            .and_then(|micros| Duration::try_from_secs_f64(micros / MICROS_PER_SEC).ok())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NowPlaying {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) artist: String,
    pub(super) album: String,
    pub(super) state: PlaybackState,
    pub(super) duration: Option<Duration>,
    pub(super) elapsed: Duration,
    pub(super) reported_at: Option<SystemTime>,
    pub(super) rate: Option<f64>,
    pub(super) track_number: Option<u32>,
    pub(super) shuffle_mode: Option<i64>,
    pub(super) repeat_mode: Option<i64>,
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the number is finite and within 1..=u32::MAX first"
)]
fn whole_number(number: f64) -> Option<u32> {
    (number.is_finite() && (1.0..=f64::from(u32::MAX)).contains(&number)).then_some(number as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_lines_are_skipped() {
        assert!(Update::parse("not json").is_none());
        assert!(Update::parse(r#"{"type":"error","payload":{}}"#).is_none());
        assert!(Update::parse(r#"{"type":"data","payload":[]}"#).is_none());
    }
}
