//! Fallback for platforms without a Spotify backend.

use super::{Capabilities, TrackState};

pub const CAPABILITIES: Capabilities = Capabilities {
    shuffle: false,
    repeat: false,
    volume: false,
};

const UNAVAILABLE: &str = "Spotify control is not available on this platform";

pub fn current_track() -> Result<TrackState, String> {
    Err(UNAVAILABLE.to_owned())
}

pub fn previous() -> Result<(), String> {
    Err(UNAVAILABLE.to_owned())
}

pub fn play_pause() -> Result<(), String> {
    Err(UNAVAILABLE.to_owned())
}

pub fn next() -> Result<(), String> {
    Err(UNAVAILABLE.to_owned())
}

pub fn toggle_shuffle() -> Result<(), String> {
    Err(UNAVAILABLE.to_owned())
}

pub fn toggle_repeat() -> Result<(), String> {
    Err(UNAVAILABLE.to_owned())
}

pub fn change_volume(_delta: i16) -> Result<(), String> {
    Err(UNAVAILABLE.to_owned())
}

pub fn open() -> Result<(), String> {
    Err(UNAVAILABLE.to_owned())
}

pub fn thumbnail() -> Option<Vec<u8>> {
    None
}
