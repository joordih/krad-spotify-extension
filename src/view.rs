mod action;
mod now_playing;
mod sources;
mod widgets;

pub use action::{
    Action, SourceChoice, NEXT, NEXT_SOURCE, OPEN, PLAY_PAUSE, PREVIOUS, REPEAT, SHUFFLE,
    VOLUME_DOWN, VOLUME_UP,
};
pub use sources::SourceChip;

use crate::media::{Capabilities, MediaError, SourceState, TrackState};
use krad_extension_sdk::api::view::{
    Align, Direction, Enabled, Justify, Node, Size, Space, Style, Wrap,
};
use krad_extension_sdk::HandlerError;
use std::path::PathBuf;

pub const ROOT_KEY: &str = "root";
const POPOVER_WIDTH: u16 = 340;
const ROOT_PADDING: Space = Space::Sm;
const ROOT_PADDING_PX: u16 = 8;
const START_PLAYING: &str = "Start playing something in a media app or browser to control it here";
const AUTOMATION_DENIED: &str = "macOS is blocking control of the media app. Allow it in \
     System Settings → Privacy & Security → Automation";
const TIMED_OUT: &str = "The media app didn't respond in time. Try again";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NowPlaying {
    pub source: Option<SourceState>,
    pub sources: Vec<SourceChip>,
    pub pinned: bool,
    pub error: Option<MediaError>,
    pub cover: Option<PathBuf>,
}

impl NowPlaying {
    #[must_use]
    pub fn track(&self) -> Option<&TrackState> {
        self.source.as_ref().map(|source| &source.track)
    }

    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.source.as_ref().is_some_and(SourceState::is_playing)
    }

    fn capabilities(&self) -> Capabilities {
        self.source
            .as_ref()
            .map(|source| source.capabilities)
            .unwrap_or_default()
    }
}

#[must_use]
pub fn friendly_error(error: Option<&MediaError>) -> String {
    match error {
        None | Some(MediaError::NothingPlaying) => START_PLAYING.to_owned(),
        Some(MediaError::AutomationDenied { .. }) => AUTOMATION_DENIED.to_owned(),
        Some(MediaError::TimedOut) => TIMED_OUT.to_owned(),
        Some(other) => other.to_string(),
    }
}

/// # Errors
///
/// Fails when `value` is not a valid `T`.
pub fn parse<T: std::str::FromStr>(value: &str) -> Result<T, HandlerError>
where
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|error| HandlerError::new(error.to_string()))
}

/// # Errors
///
/// Fails when a node key or action id is invalid.
pub fn render(view: &NowPlaying) -> Result<Node, HandlerError> {
    let track = view.track();
    let enabled = if track.is_some() {
        Enabled::Yes
    } else {
        Enabled::No
    };
    let mut tiles = Vec::new();
    tiles.extend(sources::chips(view)?);
    tiles.push(now_playing::tile_for(view, enabled)?);
    if view.capabilities().volume {
        tiles.push(now_playing::volume_tile(track, enabled)?);
    }
    widgets::flex(
        ROOT_KEY,
        Direction::Column,
        Align::Stretch,
        Justify::Start,
        Wrap::NoWrap,
        Style {
            padding: ROOT_PADDING,
            gap: Space::Sm,
            width: Size::Fixed(POPOVER_WIDTH),
            ..Style::default()
        },
        tiles,
    )
}

#[cfg(test)]
mod tests {
    use super::action::{SEEK, SOURCE_AUTO, VOLUME};
    use super::*;
    use crate::media::{PlaybackState, SourceId};
    use krad_extension_sdk::api::view::{Element, SliderLabels, TextSize};

    const POPOVER_MAX_HEIGHT: f32 = 560.0;
    const SPARE_HEIGHT: f32 = 40.0;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn keys(node: &Node) -> Vec<String> {
        fn walk(node: &Node, found: &mut Vec<String>) {
            found.push(node.key.to_string());
            for child in node.children() {
                walk(child, found);
            }
        }
        let mut found = Vec::new();
        walk(node, &mut found);
        found
    }

    fn rendered(view: &NowPlaying) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let root = render(view)?;
        assert_eq!(root.key.as_ref(), ROOT_KEY);
        krad_extension_sdk::api::view::bounded(&mut root.clone())?;
        Ok(keys(&root))
    }

    fn find<'a>(node: &'a Node, key: &str) -> Option<&'a Node> {
        if node.key.as_ref() == key {
            return Some(node);
        }
        node.children().iter().find_map(|child| find(child, key))
    }

    fn source(id: &str, capabilities: Capabilities) -> SourceState {
        SourceState {
            id: SourceId::new(id),
            name: id.to_owned(),
            capabilities,
            track: TrackState {
                title: "Song".to_owned(),
                duration_secs: 200,
                position_secs: 50,
                volume: 40,
                shuffling: true,
                state: PlaybackState::Playing,
                ..TrackState::default()
            },
        }
    }

    fn showing(source: SourceState) -> NowPlaying {
        NowPlaying {
            sources: vec![SourceChip::from(&source)],
            source: Some(source),
            ..NowPlaying::default()
        }
    }

    fn worst_case() -> NowPlaying {
        let mut spotify = source("Spotify", Capabilities::ALL);
        spotify.track.artist = "Artist".to_owned();
        spotify.track.album = "Album".to_owned();
        spotify.track.track_number = Some(5);
        let music = source("Music", Capabilities::ALL);
        let chrome = source("Chrome", Capabilities::default());
        NowPlaying {
            sources: vec![
                SourceChip::from(&spotify),
                SourceChip::from(&music),
                SourceChip::from(&chrome),
            ],
            source: Some(spotify),
            pinned: true,
            ..NowPlaying::default()
        }
    }

    fn space_px(space: Space) -> f32 {
        match space {
            Space::None => 0.0,
            Space::Xs => 4.0,
            Space::Sm => 8.0,
            Space::Md => 12.0,
            Space::Lg => 20.0,
            Space::Xl => 24.0,
        }
    }

    fn line_px(size: TextSize) -> f32 {
        match size {
            TextSize::Xs | TextSize::Sm => 16.0,
            TextSize::Md | TextSize::Lg => 20.0,
        }
    }

    fn height(node: &Node) -> Result<f32, String> {
        Ok(match &node.element {
            Element::Flex {
                direction,
                style,
                children,
                ..
            } => {
                let heights = children.iter().map(height).collect::<Result<Vec<_>, _>>()?;
                let gaps: f32 = heights.iter().skip(1).map(|_| 1.0).sum();
                let content = match direction {
                    Direction::Column => heights.iter().sum::<f32>() + space_px(style.gap) * gaps,
                    Direction::Row => heights.iter().copied().fold(0.0, f32::max),
                };
                let border = if style.border.is_some() { 2.0 } else { 0.0 };
                let content = match style.height {
                    Size::Fixed(fixed) => f32::from(fixed),
                    _ => content + 2.0 * space_px(style.padding),
                };
                content + border
            }
            Element::Text { style, .. } => line_px(style.size),
            Element::Button { label: Some(_), .. } => 32.0,
            Element::Button { .. } => 28.0,
            Element::Badge { .. } => 18.0,
            Element::Icon { .. } => 16.0,
            Element::Image { height, .. } => match height {
                Size::Fixed(fixed) => f32::from(*fixed),
                _ => 48.0,
            },
            Element::Slider(slider) if slider.labels == SliderLabels::None => 24.0,
            Element::Slider(_) => 36.0,
            other => return Err(format!("no height for {other:?}")),
        })
    }

    #[test]
    fn errors_get_a_message_the_user_can_act_on() {
        let denied = friendly_error(Some(&MediaError::AutomationDenied {
            app: "Music".to_owned(),
        }));
        assert!(denied.contains("Automation"));
        assert!(friendly_error(Some(&MediaError::TimedOut)).contains("didn't respond"));
        assert_eq!(
            friendly_error(Some(&MediaError::NothingPlaying)),
            friendly_error(None)
        );
        assert_eq!(
            friendly_error(Some(&MediaError::Unsupported)),
            MediaError::Unsupported.to_string()
        );
    }

    #[test]
    fn the_tallest_layout_fits_the_host_popover() -> TestResult {
        let root = render(&worst_case())?;
        let found = keys(&root);
        for key in [SOURCE_AUTO, SEEK, VOLUME, SHUFFLE, REPEAT, OPEN] {
            assert!(found.iter().any(|found| found == key), "no {key}");
        }
        let tall = height(&root)?;
        assert!(tall + SPARE_HEIGHT <= POPOVER_MAX_HEIGHT, "{tall}px");
        Ok(())
    }

    #[test]
    fn status_source_and_open_sit_inside_the_now_playing_tile() -> TestResult {
        let root = render(&worst_case())?;
        let tile = find(&root, "now-playing").ok_or("no now-playing tile")?;
        let header = find(tile, "header").ok_or("no header")?;
        for key in ["status", "source-name", OPEN] {
            assert!(find(header, key).is_some(), "{key} outside the header");
        }
        let top: Vec<_> = root
            .children()
            .iter()
            .map(|child| child.key.as_ref())
            .collect();
        assert_eq!(top, ["sources", "now-playing", "volume-tile"]);
        Ok(())
    }

    #[test]
    fn shuffle_and_repeat_flank_the_transport_buttons() -> TestResult {
        let root = render(&worst_case())?;
        let controls = find(&root, "controls").ok_or("no controls")?;
        let order: Vec<_> = controls
            .children()
            .iter()
            .map(|child| child.key.as_ref())
            .collect();
        assert_eq!(order, [SHUFFLE, PREVIOUS, PLAY_PAUSE, NEXT, REPEAT]);
        Ok(())
    }

    #[test]
    fn a_fully_capable_source_shows_every_control() -> TestResult {
        let found = rendered(&showing(source("spotify", Capabilities::ALL)))?;
        for key in [SEEK, VOLUME, SHUFFLE, REPEAT, OPEN, "source-name"] {
            assert!(found.iter().any(|found| found == key), "no {key}");
        }
        Ok(())
    }

    #[test]
    fn controls_the_source_lacks_are_hidden() -> TestResult {
        let found = rendered(&showing(source("chrome", Capabilities::default())))?;
        for key in [VOLUME, SHUFFLE, REPEAT, OPEN] {
            assert!(!found.iter().any(|found| found == key), "{key} shown");
        }
        assert!(found.iter().any(|found| found == SEEK));
        let mut live = source("radio", Capabilities::ALL);
        live.track.duration_secs = 0;
        assert!(!rendered(&showing(live))?.iter().any(|found| found == SEEK));
        Ok(())
    }

    #[test]
    fn nothing_playing_renders_without_track_controls() -> TestResult {
        let found = rendered(&NowPlaying {
            error: Some(MediaError::NothingPlaying),
            ..NowPlaying::default()
        })?;
        for key in [SEEK, VOLUME, OPEN, SOURCE_AUTO] {
            assert!(!found.iter().any(|found| found == key), "{key} shown");
        }
        Ok(())
    }

    #[test]
    fn several_sources_get_chips_that_pin_them() -> TestResult {
        let spotify = source("spotify", Capabilities::ALL);
        let chrome = source("chrome", Capabilities::default());
        let found = rendered(&NowPlaying {
            sources: vec![SourceChip::from(&chrome), SourceChip::from(&spotify)],
            source: Some(spotify),
            ..NowPlaying::default()
        })?;
        for key in [SOURCE_AUTO, "source:chrome", "source:spotify"] {
            assert!(found.iter().any(|found| found == key), "no {key}");
        }
        Ok(())
    }
}
