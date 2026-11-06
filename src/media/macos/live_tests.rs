#![expect(clippy::print_stdout, reason = "live tests report what this Mac plays")]

use super::{App, Platform};
use crate::media::{
    Backend, Command, MediaError, Position, SourceId, SourceState, TrackState, Volume,
};
use std::thread;
use std::time::{Duration, Instant};

const ROUNDS: usize = 3;
const BETWEEN_ROUNDS: Duration = Duration::from_secs(2);
const STREAM_START: Duration = Duration::from_secs(2);
const SETTLE: Duration = Duration::from_millis(1500);
const MUSIC_REPEAT_DELAY: Duration = Duration::from_secs(1);
const SEEK_TARGET_SECS: f64 = 30.0;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
#[ignore = "reads the media apps running on this Mac: cargo test -- --ignored live_sources --nocapture"]
fn live_sources() {
    let platform = Platform::default();
    for round in 0..ROUNDS {
        let started = Instant::now();
        let sources = platform.list_sources();
        println!("round {round} took {:?}: {sources:#?}", started.elapsed());
        thread::sleep(BETWEEN_ROUNDS);
    }
}

#[test]
#[ignore = "pauses, seeks and resumes a MediaRemote source such as QuickTime Player: cargo test -- --ignored live_mediaremote_controls --nocapture"]
fn live_mediaremote_controls() -> TestResult {
    let platform = Platform::default();
    let remote = || -> Result<SourceState, Box<dyn std::error::Error>> {
        platform
            .list_sources()?
            .into_iter()
            .find(|source| App::from_id(source.id.as_str()).is_none())
            .ok_or_else(|| "no MediaRemote source plays".into())
    };
    drop(remote());
    thread::sleep(STREAM_START);
    let source = remote()?;
    println!("{source:#?}");
    let id = source.id;
    let timed = |what: &str, command: Command| -> TestResult {
        let started = Instant::now();
        let result = platform.control(&id, command);
        println!("{what}: {result:?} in {:?}", started.elapsed());
        thread::sleep(SETTLE);
        let now = remote()?;
        println!(
            "  -> {:?} at {} s",
            now.track.state, now.track.position_secs
        );
        Ok(())
    };
    timed("play_pause", Command::PlayPause)?;
    timed("seek", Command::Seek(Position::try_from(SEEK_TARGET_SECS)?))?;
    timed("play_pause", Command::PlayPause)?;
    let gone = platform.control(&SourceId::new("com.example.gone"), Command::PlayPause);
    println!("unknown id: {gone:?}");
    assert_eq!(gone, Err(MediaError::SourceGone));
    Ok(())
}

#[test]
#[ignore = "toggles Music's shuffle and repeat twice and restores its volume and position: cargo test -- --ignored live_music_controls --nocapture"]
fn live_music_controls() -> TestResult {
    let platform = Platform::default();
    let id = SourceId::new(App::Music.bundle_id());
    let music = || -> Result<TrackState, Box<dyn std::error::Error>> {
        let sources = platform.list_sources()?;
        let music = sources.into_iter().find(|source| source.id == id);
        Ok(music.ok_or("Music does not run with a track")?.track)
    };
    let before = music()?;
    for _ in 0..2 {
        platform.control(&id, Command::ToggleShuffle)?;
        platform.control(&id, Command::ToggleRepeat)?;
        thread::sleep(MUSIC_REPEAT_DELAY);
        let now = music()?;
        println!("shuffle {} repeat {}", now.shuffling, now.repeating);
    }
    platform.control(&id, Command::SetVolume(Volume::new(before.volume)))?;
    let position = Position::try_from(f64::from(before.position_secs))?;
    platform.control(&id, Command::Seek(position))?;
    let after = music()?;
    assert_eq!(
        (after.shuffling, after.repeating, after.volume),
        (before.shuffling, before.repeating, before.volume)
    );
    println!(
        "position {} -> {}",
        before.position_secs, after.position_secs
    );
    Ok(())
}
