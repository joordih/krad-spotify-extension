use super::SourceState;

const ENTRY_SEPARATOR: char = ',';
const ID_SEPARATORS: [char; 6] = ['.', '_', '!', '/', '#', '-'];
const GENERIC_ID_SEGMENTS: [&str; 6] = ["com", "org", "net", "mpris", "mediaplayer2", "exe"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IgnoredApps(Vec<String>);

impl IgnoredApps {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        Self(
            text.split(ENTRY_SEPARATOR)
                .map(|entry| entry.trim().to_lowercase())
                .filter(|entry| !entry.is_empty())
                .collect(),
        )
    }

    #[must_use]
    pub fn ignores(&self, source: &SourceState) -> bool {
        if self.0.is_empty() {
            return false;
        }
        let name = source.name.to_lowercase();
        let id = source.id.as_str().to_lowercase();
        let segments: Vec<&str> = id
            .split(|c: char| ID_SEPARATORS.contains(&c) || c.is_whitespace())
            .filter(|segment| !GENERIC_ID_SEGMENTS.contains(segment))
            .collect();
        self.0
            .iter()
            .any(|entry| name.contains(entry.as_str()) || segments.contains(&entry.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::SourceId;
    use super::*;

    fn source(id: &str, name: &str) -> SourceState {
        SourceState {
            id: SourceId::new(id),
            name: name.to_owned(),
            ..SourceState::default()
        }
    }

    #[test]
    fn nothing_is_ignored_by_default() {
        assert!(!IgnoredApps::parse("").ignores(&source("com.spotify.client", "Spotify")));
        assert_eq!(IgnoredApps::parse(" , ,"), IgnoredApps::default());
    }

    #[test]
    fn entries_match_part_of_the_name_or_a_whole_part_of_the_id() {
        let ignored = IgnoredApps::parse("chrome, SAFARI ,vlc, spotify");
        assert!(ignored.ignores(&source("com.google.Chrome", "Google Chrome")));
        assert!(ignored.ignores(&source("x", "Safari")));
        assert!(ignored.ignores(&source("org.mpris.MediaPlayer2.vlc.instance42", "")));
        assert!(ignored.ignores(&source("Spotify.exe", "")));
        assert!(ignored.ignores(&source("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", "")));
        assert!(!ignored.ignores(&source(
            "org.mpris.MediaPlayer2.chromium.instance42",
            "Chromium"
        )));
        assert!(!ignored.ignores(&source("com.apple.Music", "Music")));
    }

    #[test]
    fn generic_words_do_not_hide_every_source() {
        let sources = [
            source("org.mpris.MediaPlayer2.chromium.instance42", "Chromium"),
            source("com.spotify.client", "Spotify"),
            source("com.apple.Music", "Music"),
            source(
                "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic",
                "Media Player",
            ),
        ];
        for entry in ["player", "org", "com", "mpris", "exe"] {
            let ignored = IgnoredApps::parse(entry);
            let hidden: Vec<&str> = sources
                .iter()
                .filter(|source| ignored.ignores(source))
                .map(|source| source.name.as_str())
                .collect();
            let expected: &[&str] = if entry == "player" {
                &["Media Player"]
            } else {
                &[]
            };
            assert_eq!(hidden, expected, "{entry}");
        }
    }
}
