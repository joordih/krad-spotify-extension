use super::super::process::{self, SCRIPT_TIMEOUT};
use super::{running_apps, App};
use crate::media::{Command, MediaError, Position, Volume};
use std::time::Duration;

const APP_TIMEOUT_SECS: u64 = 2;
const DONE: &str = "ok";
const GONE: &str = "gone";
const SPOTIFY_SCHEME: &str = "spotify:";
const HALF_SECOND: Duration = Duration::from_millis(500);

pub(in crate::media::macos) fn control(app: App, command: Command) -> Result<(), MediaError> {
    match command {
        Command::Previous => tell(app, "previous track"),
        Command::PlayPause => tell(app, "playpause"),
        Command::Next => tell(app, "next track"),
        Command::ToggleShuffle => tell(app, toggle_shuffle(app)),
        Command::ToggleRepeat => tell(app, toggle_repeat(app)),
        Command::SetVolume(volume) => set_volume(app, volume),
        Command::Seek(position) => seek(app, position),
        Command::Open => open(app),
    }
}

fn toggle_shuffle(app: App) -> &'static str {
    match app {
        App::Spotify => "set shuffling to not shuffling",
        App::Music => "set shuffle enabled to not shuffle enabled",
    }
}

fn toggle_repeat(app: App) -> &'static str {
    match app {
        App::Spotify => "set repeating to not repeating",
        App::Music => {
            "if song repeat is off then\n\
             set song repeat to all\n\
             else\n\
             set song repeat to off\n\
             end if"
        }
    }
}

fn set_volume(app: App, volume: Volume) -> Result<(), MediaError> {
    tell(app, &format!("set sound volume to {}", volume.percent()))
}

fn seek(app: App, position: Position) -> Result<(), MediaError> {
    let secs = Duration::try_from_secs_f64(position.secs())
        .map_err(|_| MediaError::InvalidValue)?
        .saturating_add(HALF_SECOND)
        .as_secs();
    tell(app, &format!("set player position to {secs}"))
}

fn open(app: App) -> Result<(), MediaError> {
    ensure_running(app)?;
    let url = match app {
        App::Spotify => spotify_url()?,
        App::Music => None,
    };
    match url {
        Some(url) => process::open_url(&url),
        None => process::open_app(app.bundle_id()),
    }
}

fn spotify_url() -> Result<Option<String>, MediaError> {
    let app = App::Spotify;
    let id = app.bundle_id();
    let stdout = process::osascript(
        &format!(
            "if application id \"{id}\" is running then\n\
             tell application id \"{id}\" to return spotify url of current track\n\
             end if\n\
             return \"\""
        ),
        SCRIPT_TIMEOUT,
    )
    .map_err(|error| error.for_app(app.name()))?;
    let url = String::from_utf8_lossy(&stdout).trim().to_owned();
    Ok(Some(url).filter(|url| url.starts_with(SPOTIFY_SCHEME)))
}

fn tell(app: App, body: &str) -> Result<(), MediaError> {
    ensure_running(app)?;
    let id = app.bundle_id();
    let stdout = process::osascript(
        &format!(
            "if application id \"{id}\" is running then\n\
             with timeout of {APP_TIMEOUT_SECS} seconds\n\
             tell application id \"{id}\"\n{body}\nend tell\n\
             end timeout\n\
             return \"{DONE}\"\n\
             end if\n\
             return \"{GONE}\""
        ),
        SCRIPT_TIMEOUT,
    )
    .map_err(|error| error.for_app(app.name()))?;
    if String::from_utf8_lossy(&stdout).trim() == DONE {
        Ok(())
    } else {
        Err(MediaError::SourceGone)
    }
}

fn ensure_running(app: App) -> Result<(), MediaError> {
    if running_apps()?.contains(&app) {
        Ok(())
    } else {
        Err(MediaError::SourceGone)
    }
}
