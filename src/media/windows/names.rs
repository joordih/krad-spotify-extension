const FIREFOX_DEFAULT_INSTALL_AUMID: &str = "308046b0af4a39cb";
const KNOWN_APPS: &[(&str, &str)] = &[
    ("spotify", "Spotify"),
    ("applemusic", "Apple Music"),
    ("zunemusic", "Media Player"),
    ("msedge", "Microsoft Edge"),
    ("chrome", "Google Chrome"),
    ("firefox", "Firefox"),
    (FIREFOX_DEFAULT_INSTALL_AUMID, "Firefox"),
    ("vlc", "VLC"),
];
const PATH_SEPARATORS: [char; 2] = ['\\', '/'];
const PACKAGE_ENTRY: char = '!';
const PACKAGE_FAMILY_HASH: char = '_';
const PUBLISHER_SEPARATOR: char = '.';
const EXE_SUFFIX: &str = ".exe";

pub(super) fn display_name(aumid: &str) -> String {
    let lower = aumid.to_lowercase();
    if let Some((_, name)) = KNOWN_APPS.iter().find(|(key, _)| lower.contains(key)) {
        return (*name).to_owned();
    }
    let file = aumid.rsplit(PATH_SEPARATORS).next().unwrap_or(aumid);
    let name = file
        .split_once(PACKAGE_ENTRY)
        .map_or(file, |(package, _)| app_of_package(package));
    let name = strip_exe(name).trim();
    if name.is_empty() {
        return aumid.to_owned();
    }
    capitalized(name)
}

fn app_of_package(package: &str) -> &str {
    let family = package
        .rsplit_once(PACKAGE_FAMILY_HASH)
        .map_or(package, |(family, _)| family);
    family.rsplit(PUBLISHER_SEPARATOR).next().unwrap_or(family)
}

fn strip_exe(name: &str) -> &str {
    name.len()
        .checked_sub(EXE_SUFFIX.len())
        .and_then(|at| name.split_at_checked(at))
        .filter(|(stem, suffix)| !stem.is_empty() && suffix.eq_ignore_ascii_case(EXE_SUFFIX))
        .map_or(name, |(stem, _)| stem)
}

fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aumids_become_readable_app_names() {
        let cases = [
            ("Spotify.exe", "Spotify"),
            ("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", "Spotify"),
            ("AppleInc.AppleMusicWin_nzyj5cx40ttqa!App", "Apple Music"),
            (
                "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic",
                "Media Player",
            ),
            ("Chrome", "Google Chrome"),
            ("MSEdge", "Microsoft Edge"),
            ("308046B0AF4A39CB", "Firefox"),
            (
                r"{6D809377-6AF0-444B-8957-A3773F02200E}\VideoLAN\VLC\vlc.exe",
                "VLC",
            ),
            ("Contoso.CoolPlayer_8wekyb3d8bbwe!App", "CoolPlayer"),
            ("foobar2000.exe", "Foobar2000"),
            (r"C:\Tools\mpv.EXE", "Mpv"),
            (".exe", ".exe"),
            ("", ""),
        ];
        for (aumid, name) in cases {
            assert_eq!(display_name(aumid), name, "{aumid}");
        }
    }

    #[test]
    fn non_ascii_aumids_keep_their_characters() {
        let cases = [
            ("Pläyer.exe", "Pläyer"),
            (r"C:\Programme\música.EXE", "Música"),
            ("Plä€", "Plä€"),
            ("ä€", "Ä€"),
            ("Contoso.Lecteur€_8wekyb3d8bbwe!App", "Lecteur€"),
        ];
        for (aumid, name) in cases {
            assert_eq!(display_name(aumid), name, "{aumid}");
        }
    }
}
