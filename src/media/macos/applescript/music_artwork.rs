use super::super::process;
use super::App;
use crate::media::Artwork;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

const SCRIPT_TIMEOUT_SECS: u64 = 10;
const PROCESS_MARGIN: Duration = Duration::from_secs(2);
const PROCESS_TIMEOUT: Duration =
    Duration::from_secs(SCRIPT_TIMEOUT_SECS).saturating_add(PROCESS_MARGIN);
const DATA_PREFIX: &str = "«data ";
const DATA_SUFFIX: char = '»';
const TYPE_CODE_LEN: usize = 4;
const HEX_RADIX: u32 = 16;
const HEX_RADIX_BYTE: u8 = 16;

#[derive(Default)]
struct Cover {
    track: String,
    artwork: Option<Artwork>,
}

#[derive(Default)]
pub(super) struct MusicArtwork(Arc<Mutex<Cover>>);

impl MusicArtwork {
    pub(super) fn for_track(&self, track: &str) -> Option<Artwork> {
        let mut cached = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if cached.track == track {
            return cached.artwork.clone();
        }
        track.clone_into(&mut cached.track);
        cached.artwork = None;
        if track.is_empty() {
            return None;
        }
        let shared = Arc::clone(&self.0);
        let track = track.to_owned();
        thread::spawn(move || {
            let artwork = fetch();
            let mut cached = shared.lock().unwrap_or_else(PoisonError::into_inner);
            if cached.track == track {
                cached.artwork = artwork;
            }
        });
        None
    }
}

fn fetch() -> Option<Artwork> {
    let id = App::Music.bundle_id();
    let script = format!(
        "if application id \"{id}\" is running then\n\
         with timeout of {SCRIPT_TIMEOUT_SECS} seconds\n\
         tell application id \"{id}\" to return raw data of artwork 1 of current track\n\
         end timeout\n\
         end if\n\
         return \"\""
    );
    let stdout = process::osascript(&script, PROCESS_TIMEOUT).ok()?;
    Artwork::from_bytes(parse_data_literal(&String::from_utf8_lossy(&stdout))?)
}

fn parse_data_literal(text: &str) -> Option<Vec<u8>> {
    let body = text
        .trim()
        .strip_prefix(DATA_PREFIX)?
        .strip_suffix(DATA_SUFFIX)?;
    let (pairs, odd) = body.get(TYPE_CODE_LEN..)?.as_bytes().as_chunks::<2>();
    if !odd.is_empty() {
        return None;
    }
    pairs
        .iter()
        .map(|&[high, low]| Some(hex_digit(high)? * HEX_RADIX_BYTE + hex_digit(low)?))
        .collect()
}

fn hex_digit(byte: u8) -> Option<u8> {
    char::from(byte)
        .to_digit(HEX_RADIX)
        .and_then(|digit| u8::try_from(digit).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_literals_decode_to_bytes() {
        assert_eq!(
            parse_data_literal("«data JPEGFFD8FF00»\n"),
            Some(vec![0xff, 0xd8, 0xff, 0x00])
        );
        assert_eq!(parse_data_literal("«data JPEGFFD»"), None);
        assert_eq!(parse_data_literal("missing value"), None);
    }
}
