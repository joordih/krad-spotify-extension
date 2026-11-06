//! macOS backend: Spotify's AppleScript dictionary (`Spotify.sdef`) through `osascript`.

use super::{nonempty, Artwork, Capabilities, TrackState, NOT_RUNNING};
use std::process::{Command, Output};

pub const CAPABILITIES: Capabilities = Capabilities {
    shuffle: true,
    repeat: true,
    volume: true,
};

const SPOTIFY_BUNDLE_ID: &str = "com.spotify.client";
const FIELD_SEPARATOR: char = '\u{1f}';
const FIELD_COUNT: usize = 13;

pub fn current_track() -> Result<TrackState, String> {
    ensure_spotify_running()?;

    // Each property is guarded independently because Spotify can return an
    // empty/undefined value for a field (for example, when nothing is queued).
    // Numbers are read into variables first: `player position` cannot be
    // coerced inline, and reals are rounded to avoid locale decimal commas.
    let script = format!(
        "tell application id \"{SPOTIFY_BUNDLE_ID}\"\n\
         set sep to character id 31\n\
         set f to {{\"\", \"\", \"\", \"\", \"\", \"\", \"\", \"\", \"\", \"\", \"\", \"\", \"\"}}\n\
         try\n\
         set t to current track\n\
         try\n set item 1 of f to name of t as text\n end try\n\
         try\n set item 2 of f to artist of t as text\n end try\n\
         try\n set item 3 of f to album of t as text\n end try\n\
         try\n set item 4 of f to album artist of t as text\n end try\n\
         try\n set n to track number of t\n set item 5 of f to n as text\n end try\n\
         try\n set item 6 of f to artwork url of t as text\n end try\n\
         try\n set item 7 of f to spotify url of t as text\n end try\n\
         try\n set d to duration of t\n set item 10 of f to (round (d / 1000)) as text\n end try\n\
         end try\n\
         try\n set item 8 of f to player state as text\n end try\n\
         try\n set p to player position\n set item 9 of f to (round p) as text\n end try\n\
         try\n set s to shuffling\n set item 11 of f to s as text\n end try\n\
         try\n set r to repeating\n set item 12 of f to r as text\n end try\n\
         try\n set v to sound volume\n set item 13 of f to v as text\n end try\n\
         set AppleScript's text item delimiters to sep\n\
         set out to f as text\n\
         set AppleScript's text item delimiters to \"\"\n\
         return out\n\
         end tell"
    );
    let output = run_script(&script)?;
    let stdout = check_success(output)?;
    let text = String::from_utf8_lossy(&stdout);
    let fields = text
        .trim_end_matches(['\r', '\n'])
        .split(FIELD_SEPARATOR)
        .collect::<Vec<_>>();
    if fields.len() != FIELD_COUNT {
        return Err("Spotify returned track metadata in an unexpected format".to_owned());
    }

    Ok(TrackState {
        title: fields[0].to_owned(),
        artist: fields[1].to_owned(),
        album: fields[2].to_owned(),
        album_artist: fields[3].to_owned(),
        track_number: fields[4].parse().ok().filter(|number| *number > 0),
        artwork: nonempty(fields[5]).map(Artwork::Url),
        spotify_url: nonempty(fields[6]),
        player_state: fields[7].to_owned(),
        position_secs: fields[8].parse().unwrap_or(0),
        duration_secs: fields[9].parse().unwrap_or(0),
        shuffling: fields[10] == "true",
        repeating: fields[11] == "true",
        volume: fields[12].parse::<u8>().unwrap_or(0).min(100),
    })
}

pub fn previous() -> Result<(), String> {
    tell("previous track")
}

pub fn play_pause() -> Result<(), String> {
    tell("playpause")
}

pub fn next() -> Result<(), String> {
    tell("next track")
}

pub fn toggle_shuffle() -> Result<(), String> {
    tell("set shuffling to not shuffling")
}

pub fn toggle_repeat() -> Result<(), String> {
    tell("set repeating to not repeating")
}

pub fn change_volume(delta: i16) -> Result<(), String> {
    tell(&format!(
        "set v to (sound volume) + ({delta})\n\
         if v > 100 then set v to 100\n\
         if v < 0 then set v to 0\n\
         set sound volume to v"
    ))
}

pub fn open() -> Result<(), String> {
    ensure_spotify_running()?;
    let url = current_track()?
        .spotify_url
        .filter(|url| url.starts_with("spotify:"));
    let mut command = Command::new("/usr/bin/open");
    match url {
        Some(url) => command.arg(url),
        None => command.args(["-b", SPOTIFY_BUNDLE_ID]),
    };
    let output = command
        .output()
        .map_err(|error| format!("Could not start /usr/bin/open: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error("open", &output))
    }
}

/// macOS exposes an artwork URL instead.
pub fn thumbnail() -> Option<Vec<u8>> {
    None
}

fn tell(body: &str) -> Result<(), String> {
    ensure_spotify_running()?;
    run_script(&format!(
        "tell application id \"{SPOTIFY_BUNDLE_ID}\"\n{body}\nend tell"
    ))
    .and_then(check_success)
    .map(|_| ())
}

fn ensure_spotify_running() -> Result<(), String> {
    let output = Command::new("/usr/bin/pgrep")
        .args(["-x", "Spotify"])
        .output()
        .map_err(|error| format!("Could not start /usr/bin/pgrep: {error}"))?;
    if output.status.success() {
        Ok(())
    } else if output.status.code() == Some(1) {
        Err(NOT_RUNNING.to_owned())
    } else {
        Err(command_error("pgrep", &output))
    }
}

fn run_script(script: &str) -> Result<Output, String> {
    Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .output()
        .map_err(|error| format!("Could not start /usr/bin/osascript: {error}"))
}

fn check_success(output: Output) -> Result<Vec<u8>, String> {
    if output.status.success() {
        return Ok(output.stdout);
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if detail.is_empty() {
        Err(format!("osascript failed with status {}", output.status))
    } else {
        Err(format!("osascript failed: {detail}"))
    }
}

fn command_error(program: &str, output: &Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if detail.is_empty() {
        format!("{program} failed with status {}", output.status)
    } else {
        format!("{program} failed: {detail}")
    }
}
