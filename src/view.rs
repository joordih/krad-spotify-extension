//! The *Now Playing* popover: a stack of Control Center–style tiles built
//! from the SDK's view elements.

use crate::artwork;
use crate::spotify::{self, TrackState};
use krad_extension_sdk::api::view::{
    Align, ButtonVariant, ColorToken, Direction, Element, Enabled, ImageSource, Justify, Node,
    Radius, Size, SliderFormat, SliderLabels, Space, Style, TextSize, TextStyle, Tone, Weight,
    Wrap,
};
use krad_extension_sdk::api::{ActionId, Icon, NodeKey};
use krad_extension_sdk::{slider, HandlerError};
use std::path::PathBuf;

pub const ROOT_KEY: &str = "root";
/// Popover content width; the host allows 220–420 px.
const POPOVER_WIDTH: u16 = 320;

// Actions shared by view buttons and palette commands (same ids).
pub const PREVIOUS: &str = "previous";
pub const PLAY_PAUSE: &str = "playpause";
pub const NEXT: &str = "next";
pub const SHUFFLE: &str = "shuffle";
pub const REPEAT: &str = "repeat";
pub const VOLUME_UP: &str = "volume-up";
pub const VOLUME_DOWN: &str = "volume-down";
pub const OPEN: &str = "open";
// Slider actions; their value arrives with `ViewEvent::SliderCommit`.
pub const SEEK: &str = "seek";
pub const VOLUME: &str = "volume";

/// Seconds of playback per second of wall time, for the advancing timeline.
const PLAYBACK_RATE: f64 = 1.0;
const VOLUME_MAX: f64 = 100.0;
const VOLUME_STEP: f64 = 1.0;

/// What the view shows: Spotify's state, or why it is unavailable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Player {
    pub track: Option<TrackState>,
    pub error: Option<String>,
    pub cover: Option<PathBuf>,
}

impl Player {
    pub fn is_playing(&self) -> bool {
        self.track
            .as_ref()
            .is_some_and(|track| track.player_state == "playing")
    }
}

pub fn parse<T: std::str::FromStr>(value: &str) -> Result<T, HandlerError>
where
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|error| HandlerError::new(error.to_string()))
}

fn node(key: &str, element: Element) -> Result<Node, HandlerError> {
    Ok(Node {
        key: parse::<NodeKey>(key)?,
        element,
    })
}

fn text(key: &str, value: impl Into<String>, style: TextStyle) -> Result<Node, HandlerError> {
    node(
        key,
        Element::Text {
            text: value.into(),
            style,
        },
    )
}

fn flex(
    key: &str,
    direction: Direction,
    align: Align,
    justify: Justify,
    style: Style,
    children: Vec<Node>,
) -> Result<Node, HandlerError> {
    node(
        key,
        Element::Flex {
            direction,
            align,
            justify,
            wrap: Wrap::NoWrap,
            style,
            children,
        },
    )
}

fn row(key: &str, justify: Justify, gap: Space, children: Vec<Node>) -> Result<Node, HandlerError> {
    flex(
        key,
        Direction::Row,
        Align::Center,
        justify,
        Style {
            gap,
            ..Style::default()
        },
        children,
    )
}

fn column(key: &str, align: Align, gap: Space, children: Vec<Node>) -> Result<Node, HandlerError> {
    flex(
        key,
        Direction::Column,
        align,
        Justify::Start,
        Style {
            gap,
            width: Size::Fill,
            ..Style::default()
        },
        children,
    )
}

fn button(
    action: &str,
    icon: Icon,
    tooltip: &str,
    variant: ButtonVariant,
    enabled: Enabled,
) -> Result<Node, HandlerError> {
    node(
        action,
        Element::Button {
            action: parse::<ActionId>(action)?,
            label: None,
            icon: Some(icon),
            variant,
            enabled,
            tooltip: Some(tooltip.to_owned()),
        },
    )
}

/// Secondary when on, ghost when off, so active modes stand out.
fn toggle_button(
    action: &str,
    icon: Icon,
    name: &str,
    on: bool,
    enabled: Enabled,
) -> Result<Node, HandlerError> {
    let (variant, state) = if on {
        (ButtonVariant::Secondary, "on")
    } else {
        (ButtonVariant::Ghost, "off")
    };
    button(action, icon, &format!("{name}: {state}"), variant, enabled)
}

fn style(size: TextSize, weight: Weight, color: ColorToken) -> TextStyle {
    TextStyle {
        size,
        weight,
        color: Some(color),
        wrap: Wrap::NoWrap,
        ..TextStyle::default()
    }
}

fn cover(path: Option<PathBuf>) -> Result<Node, HandlerError> {
    let size = Size::Fixed(artwork::COVER_SIZE);
    if let Some(path) = path {
        return node(
            "cover",
            Element::Image {
                source: ImageSource::Bundled(path),
                width: size,
                height: size,
            },
        );
    }
    flex(
        "cover",
        Direction::Row,
        Align::Center,
        Justify::Center,
        Style {
            width: size,
            height: size,
            radius: Radius::Lg,
            background: Some(ColorToken::Hover),
            ..Style::default()
        },
        vec![node(
            "cover-icon",
            Element::Icon {
                icon: Icon::Play,
                color: Some(ColorToken::TextMuted),
            },
        )?],
    )
}

pub fn friendly_error(error: Option<String>) -> String {
    match error.as_deref() {
        Some(spotify::NOT_RUNNING) => "Open Spotify to see what's playing".to_owned(),
        // -1743: macOS denied the Automation permission to control Spotify.
        Some(other) if other.contains("-1743") => {
            "macOS is blocking control of Spotify. Allow it in \
             System Settings → Privacy & Security → Automation"
                .to_owned()
        }
        // -1712: the Apple Event timed out (Spotify busy or not answering).
        Some(other) if other.contains("-1712") => {
            "Spotify didn't respond in time. Try again".to_owned()
        }
        Some(other) => other.to_owned(),
        None => "No player information".to_owned(),
    }
}

fn track_info(track: &TrackState) -> Result<Vec<Node>, HandlerError> {
    let title = if track.title.is_empty() {
        "No track"
    } else {
        &track.title
    };
    let mut lines = vec![
        text(
            "title",
            title,
            style(TextSize::Md, Weight::Bold, ColorToken::TextStrong),
        )?,
        text(
            "artist",
            track.artist.clone(),
            style(TextSize::Sm, Weight::Regular, ColorToken::TextSubtle),
        )?,
    ];
    if !track.album.is_empty() {
        lines.push(text(
            "album",
            track.album.clone(),
            style(TextSize::Xs, Weight::Regular, ColorToken::TextMuted),
        )?);
    }
    let mut extra = Vec::new();
    if let Some(number) = track.track_number {
        extra.push(format!("Track {number}"));
    }
    if !track.album_artist.is_empty() && track.album_artist != track.artist {
        extra.push(format!("Album by {}", track.album_artist));
    }
    if !extra.is_empty() {
        lines.push(text(
            "album-info",
            extra.join(" · "),
            style(TextSize::Xs, Weight::Regular, ColorToken::TextMuted),
        )?);
    }
    Ok(lines)
}

/// The track timeline: elapsed and remaining time, advanced by the app while
/// playing, and draggable to seek where the platform supports it.
fn timeline(track: &TrackState) -> Result<Node, HandlerError> {
    let duration = f64::from(track.duration_secs.max(1));
    let enabled = if spotify::CAPABILITIES.seek && track.duration_secs > 0 {
        Enabled::Yes
    } else {
        Enabled::No
    };
    let mut built = slider(
        parse(SEEK)?,
        parse(SEEK)?,
        f64::from(track.position_secs),
        duration,
    )
    .step(1.0)
    .format(SliderFormat::Duration)
    .labels(SliderLabels::ElapsedRemaining)
    .enabled(enabled);
    if track.player_state == "playing" {
        built = built.advancing(PLAYBACK_RATE);
    }
    Ok(built.into())
}

/// A Control Center–style module: rounded block on the panel colour, `Selected` when active.
fn tile(key: &str, active: bool, align: Align, children: Vec<Node>) -> Result<Node, HandlerError> {
    flex(
        key,
        Direction::Column,
        align,
        Justify::Start,
        Style {
            padding: Space::Md,
            gap: Space::Sm,
            background: Some(if active {
                ColorToken::Selected
            } else {
                ColorToken::Panel
            }),
            border: Some(ColorToken::Border),
            radius: Radius::Lg,
            width: Size::Fill,
            ..Style::default()
        },
        children,
    )
}

/// Small module for an on/off mode, like Control Center's toggles.
fn mode_tile(
    action: &str,
    icon: Icon,
    name: &str,
    on: bool,
    enabled: Enabled,
) -> Result<Node, HandlerError> {
    let state = if on { "On" } else { "Off" };
    tile(
        &format!("{action}-tile"),
        on,
        Align::Start,
        vec![row(
            &format!("{action}-row"),
            Justify::Start,
            Space::Sm,
            vec![
                toggle_button(action, icon, name, on, enabled)?,
                column(
                    &format!("{action}-labels"),
                    Align::Stretch,
                    Space::None,
                    vec![
                        text(
                            &format!("{action}-name"),
                            name,
                            style(TextSize::Sm, Weight::Medium, ColorToken::TextStrong),
                        )?,
                        text(
                            &format!("{action}-state"),
                            state,
                            style(TextSize::Xs, Weight::Regular, ColorToken::TextMuted),
                        )?,
                    ],
                )?,
            ],
        )?],
    )
}

fn now_playing(player: &Player, enabled: Enabled) -> Result<Node, HandlerError> {
    let track = player.track.as_ref();
    let info = match track {
        Some(track) => track_info(track)?,
        None => vec![
            text(
                "title",
                "Spotify unavailable",
                style(TextSize::Md, Weight::Bold, ColorToken::TextStrong),
            )?,
            text(
                "artist",
                friendly_error(player.error.clone()),
                TextStyle {
                    wrap: Wrap::Wrap,
                    ..style(TextSize::Xs, Weight::Regular, ColorToken::TextMuted)
                },
            )?,
        ],
    };
    let (play_icon, play_tip) = if player.is_playing() {
        (Icon::Stop, "Pause")
    } else {
        (Icon::Play, "Play")
    };
    let transport = row(
        "controls",
        Justify::Center,
        Space::Md,
        vec![
            button(
                PREVIOUS,
                Icon::ChevronLeft,
                "Previous",
                ButtonVariant::Ghost,
                enabled,
            )?,
            button(
                PLAY_PAUSE,
                play_icon,
                play_tip,
                ButtonVariant::Primary,
                enabled,
            )?,
            button(
                NEXT,
                Icon::ChevronRight,
                "Next",
                ButtonVariant::Ghost,
                enabled,
            )?,
        ],
    )?;

    let mut children = vec![
        cover(player.cover.clone())?,
        column("track-info", Align::Center, Space::Xs, info)?,
    ];
    if let Some(track) = track {
        children.push(column(
            "timeline-row",
            Align::Stretch,
            Space::None,
            vec![timeline(track)?],
        )?);
    }
    children.push(transport);
    tile("now-playing", false, Align::Center, children)
}

fn volume_tile(track: Option<&TrackState>, enabled: Enabled) -> Result<Node, HandlerError> {
    let level = f64::from(track.map_or(0, |track| track.volume));
    let volume: Node = slider(parse(VOLUME)?, parse(VOLUME)?, level, VOLUME_MAX)
        .step(VOLUME_STEP)
        .format(SliderFormat::Percent)
        .labels(SliderLabels::Value)
        .enabled(enabled)
        .into();
    tile(
        "volume-tile",
        false,
        Align::Stretch,
        vec![
            text(
                "volume-name",
                "Volume",
                style(TextSize::Sm, Weight::Medium, ColorToken::TextStrong),
            )?,
            row(
                "volume-controls",
                Justify::Between,
                Space::Sm,
                vec![
                    button(
                        VOLUME_DOWN,
                        Icon::Minus,
                        "Volume down",
                        ButtonVariant::Ghost,
                        enabled,
                    )?,
                    column("volume-slider", Align::Stretch, Space::None, vec![volume])?,
                    button(
                        VOLUME_UP,
                        Icon::Add,
                        "Volume up",
                        ButtonVariant::Ghost,
                        enabled,
                    )?,
                ],
            )?,
        ],
    )
}

pub fn render(player: &Player) -> Result<Node, HandlerError> {
    let track = player.track.as_ref();
    let enabled = if track.is_some() {
        Enabled::Yes
    } else {
        Enabled::No
    };
    let (status, tone) = match track.map(|track| track.player_state.as_str()) {
        Some("playing") => ("Playing", Tone::Success),
        Some("paused") => ("Paused", Tone::Neutral),
        Some("stopped") => ("Stopped", Tone::Neutral),
        Some(_) => ("Unknown", Tone::Warning),
        None => ("Disconnected", Tone::Neutral),
    };

    let header = row(
        "header",
        Justify::Between,
        Space::Sm,
        vec![
            node(
                "status",
                Element::Badge {
                    text: status.to_owned(),
                    tone,
                },
            )?,
            button(
                OPEN,
                Icon::OpenExternal,
                "Open in Spotify",
                ButtonVariant::Ghost,
                enabled,
            )?,
        ],
    )?;

    // Only the modes this platform's backend can control get a tile.
    let capabilities = spotify::CAPABILITIES;
    let mut mode_tiles = Vec::new();
    if capabilities.shuffle {
        mode_tiles.push(mode_tile(
            SHUFFLE,
            Icon::Exchange,
            "Shuffle",
            track.is_some_and(|track| track.shuffling),
            enabled,
        )?);
    }
    if capabilities.repeat {
        mode_tiles.push(mode_tile(
            REPEAT,
            Icon::Repeat,
            "Repeat",
            track.is_some_and(|track| track.repeating),
            enabled,
        )?);
    }

    let mut tiles = vec![header, now_playing(player, enabled)?];
    if !mode_tiles.is_empty() {
        tiles.push(node(
            "modes",
            Element::Grid {
                columns: mode_tiles.len() as u8,
                style: Style {
                    gap: Space::Sm,
                    width: Size::Fill,
                    ..Style::default()
                },
                children: mode_tiles,
            },
        )?);
    }
    if capabilities.volume {
        tiles.push(volume_tile(track, enabled)?);
    }
    // The popover supplies the rounded background, border and scrolling.
    flex(
        ROOT_KEY,
        Direction::Column,
        Align::Stretch,
        Justify::Start,
        Style {
            padding: Space::Md,
            gap: Space::Sm,
            width: Size::Fixed(POPOVER_WIDTH),
            ..Style::default()
        },
        tiles,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(node: &Node, found: &mut Vec<String>) {
        found.push(node.key.to_string());
        for child in node.children() {
            keys(child, found);
        }
    }

    #[test]
    fn automation_errors_get_a_message_the_user_can_act_on() {
        let denied = friendly_error(Some("osascript failed: Not authorized (-1743)".to_owned()));
        assert!(denied.contains("Automation"));
        let timeout = friendly_error(Some("osascript failed: timed out (-1712)".to_owned()));
        assert!(timeout.contains("didn't respond"));
        assert_eq!(
            friendly_error(Some(spotify::NOT_RUNNING.to_owned())),
            "Open Spotify to see what's playing"
        );
    }

    #[test]
    fn the_popover_renders_with_and_without_spotify() {
        let track = TrackState {
            title: "Song".to_owned(),
            duration_secs: 200,
            position_secs: 50,
            volume: 40,
            shuffling: true,
            player_state: "playing".to_owned(),
            ..TrackState::default()
        };
        let playing = render(&Player {
            track: Some(track),
            ..Player::default()
        })
        .unwrap();
        assert_eq!(playing.key.as_ref(), ROOT_KEY);
        // The same check the app runs on every view it receives.
        krad_extension_sdk::api::view::bounded(&mut playing.clone()).unwrap();
        let mut found = Vec::new();
        keys(&playing, &mut found);
        assert!(found.iter().any(|key| key == SEEK), "no timeline slider");
        assert_eq!(
            found.iter().any(|key| key == VOLUME),
            spotify::CAPABILITIES.volume
        );

        let closed = render(&Player {
            error: Some(spotify::NOT_RUNNING.to_owned()),
            ..Player::default()
        })
        .unwrap();
        krad_extension_sdk::api::view::bounded(&mut closed.clone()).unwrap();
        let mut found = Vec::new();
        keys(&closed, &mut found);
        assert!(!found.iter().any(|key| key == SEEK));
    }

    #[test]
    fn the_timeline_advances_only_while_playing() {
        let mut track = TrackState {
            duration_secs: 200,
            position_secs: 10,
            player_state: "playing".to_owned(),
            ..TrackState::default()
        };
        let Element::Slider(playing) = timeline(&track).unwrap().element else {
            panic!("not a slider")
        };
        assert!(playing.advancing.is_some());
        assert_eq!(playing.max, 200.0);
        track.player_state = "paused".to_owned();
        let Element::Slider(paused) = timeline(&track).unwrap().element else {
            panic!("not a slider")
        };
        assert!(paused.advancing.is_none());
    }
}
