const BUS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const NAME_SEPARATOR: char = '.';
const SPOTIFY: &str = "spotify";
const BROWSERS: &[&str] = &[
    "chromium",
    "chrome",
    "firefox",
    "brave",
    "edge",
    "msedge",
    "vivaldi",
    "opera",
    "librewolf",
    "zen",
    "floorp",
    "waterfox",
    "epiphany",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum App {
    Spotify,
    Browser,
    Other,
}

impl App {
    pub(super) fn of(bus: &str) -> Self {
        let app = app_part(bus);
        if app == SPOTIFY {
            return Self::Spotify;
        }
        let app = app.to_ascii_lowercase();
        if BROWSERS.iter().any(|browser| app.starts_with(browser)) {
            Self::Browser
        } else {
            Self::Other
        }
    }
}

pub(super) fn is_source_name(bus: &str) -> bool {
    source_part(bus).is_some()
}

#[must_use]
pub(super) fn source_names(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut sources: Vec<String> = names
        .into_iter()
        .filter(|name| is_source_name(name))
        .collect();
    sources.sort();
    sources.dedup();
    sources
}

#[must_use]
pub(super) fn display_name(identity: Option<&str>, bus: &str) -> String {
    if let Some(identity) = identity.map(str::trim).filter(|name| !name.is_empty()) {
        return identity.to_owned();
    }
    let mut chars = app_part(bus).chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => bus.to_owned(),
    }
}

fn source_part(bus: &str) -> Option<&str> {
    bus.strip_prefix(BUS_PREFIX)
        .filter(|rest| !rest.is_empty() && !rest.starts_with(NAME_SEPARATOR))
}

fn app_part(bus: &str) -> &str {
    source_part(bus)
        .and_then(|rest| rest.split(NAME_SEPARATOR).next())
        .unwrap_or(bus)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHROMIUM: &str = "org.mpris.MediaPlayer2.chromium.instance1234";
    const FIREFOX: &str = "org.mpris.MediaPlayer2.firefox.instance_1_23";
    const SPOTIFY_BUS: &str = "org.mpris.MediaPlayer2.spotify";
    const VLC: &str = "org.mpris.MediaPlayer2.vlc";

    #[test]
    fn only_mpris_bus_names_are_sources() {
        let names = [
            ":1.42",
            "org.freedesktop.DBus",
            "org.mpris.MediaPlayer2",
            "org.mpris.MediaPlayer2.",
            "org.mpris.MediaPlayer2..x",
            SPOTIFY_BUS,
            CHROMIUM,
            FIREFOX,
            SPOTIFY_BUS,
            "org.gnome.Shell",
        ];
        assert_eq!(
            source_names(names.map(str::to_owned)),
            [CHROMIUM, FIREFOX, SPOTIFY_BUS]
        );
    }

    #[test]
    fn names_come_from_identity_or_the_bus_name() {
        assert_eq!(
            display_name(Some("Google Chrome"), CHROMIUM),
            "Google Chrome"
        );
        assert_eq!(display_name(Some("  "), CHROMIUM), "Chromium");
        assert_eq!(display_name(None, VLC), "Vlc");
        assert_eq!(display_name(None, SPOTIFY_BUS), "Spotify");
    }

    #[test]
    fn apps_are_told_apart_by_their_bus_name() {
        assert_eq!(App::of(SPOTIFY_BUS), App::Spotify);
        assert_eq!(
            App::of("org.mpris.MediaPlayer2.spotifyd.instance99"),
            App::Other
        );
        assert_eq!(App::of(CHROMIUM), App::Browser);
        assert_eq!(App::of(FIREFOX), App::Browser);
        assert_eq!(App::of(VLC), App::Other);
    }
}
