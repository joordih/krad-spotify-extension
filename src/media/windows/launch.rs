use std::io;
use std::os::windows::process::CommandExt;
use std::process::Command;

const SPOTIFY: &str = "spotify";
const SPOTIFY_URI: &str = "spotify:";
const SHELL: &str = "cmd";
const SHELL_START: [&str; 3] = ["/C", "start", ""];
const EXPLORER: &str = "explorer.exe";
const APPS_FOLDER: &str = r"shell:AppsFolder\";
const PACKAGE_ENTRY: char = '!';
const PLAIN_PUNCTUATION: [char; 4] = ['.', '_', '-', PACKAGE_ENTRY];
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Launch {
    SpotifyUri,
    AppsFolder(String),
}

impl Launch {
    pub(super) fn spawn(&self) -> io::Result<()> {
        let mut command = match self {
            Self::SpotifyUri => {
                let mut command = Command::new(SHELL);
                command.args(SHELL_START).arg(SPOTIFY_URI);
                command
            }
            Self::AppsFolder(aumid) => {
                let mut command = Command::new(EXPLORER);
                command.arg(format!("{APPS_FOLDER}{aumid}"));
                command
            }
        };
        command.creation_flags(CREATE_NO_WINDOW).spawn().map(drop)
    }
}

pub(super) fn launch_for(aumid: &str) -> Option<Launch> {
    if aumid.to_lowercase().contains(SPOTIFY) {
        return Some(Launch::SpotifyUri);
    }
    if !aumid.contains(PACKAGE_ENTRY) || !is_plain(aumid) {
        return None;
    }
    Some(Launch::AppsFolder(aumid.to_owned()))
}

fn is_plain(aumid: &str) -> bool {
    aumid
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || PLAIN_PUNCTUATION.contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_spotify_and_safe_packaged_apps_can_be_launched() {
        assert_eq!(launch_for("Spotify.exe"), Some(Launch::SpotifyUri));
        assert_eq!(
            launch_for("AppleInc.AppleMusicWin_nzyj5cx40ttqa!App"),
            Some(Launch::AppsFolder(
                "AppleInc.AppleMusicWin_nzyj5cx40ttqa!App".to_owned()
            ))
        );
        assert_eq!(
            launch_for(r"{6D809377-6AF0-444B-8957-A3773F02200E}\VideoLAN\VLC\vlc.exe"),
            None
        );
        assert_eq!(launch_for("Chrome"), None);
        assert_eq!(launch_for("Evil_x!App & calc"), None);
        assert_eq!(launch_for("Pläyer_x!App"), None);
    }
}
