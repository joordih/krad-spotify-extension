use super::action::{
    NEXT, OPEN, PLAY_PAUSE, PREVIOUS, REPEAT, SEEK, SHUFFLE, VOLUME, VOLUME_DOWN, VOLUME_UP,
};
use super::widgets::{
    button, column, flex, node, row, style, text, tile, toggle_button, TILE_BORDER_PX,
    TILE_PADDING_PX,
};
use super::{friendly_error, parse, NowPlaying, POPOVER_WIDTH, ROOT_PADDING_PX};
use crate::media::{PlaybackState, TrackState};
use krad_extension_sdk::api::view::{
    Align, ButtonVariant, ColorToken, Direction, Element, Enabled, ImageSource, Justify, Node,
    Radius, Size, SliderFormat, SliderLabels, Space, Style, TextSize, TextStyle, Tone, Weight,
    Wrap,
};
use krad_extension_sdk::api::Icon;
use krad_extension_sdk::{slider, HandlerError};
use std::path::PathBuf;

const COVER_DISPLAY: u16 = 96;
const TRACK_GAP: Space = Space::Md;
const TRACK_GAP_PX: u16 = 12;
const TRACK_INFO_WIDTH: u16 = POPOVER_WIDTH
    - 2 * ROOT_PADDING_PX
    - 2 * TILE_PADDING_PX
    - 2 * TILE_BORDER_PX
    - COVER_DISPLAY
    - TRACK_GAP_PX;
const PLAYBACK_RATE: f64 = 1.0;
const SEEK_STEP: f64 = 1.0;
const VOLUME_MAX: f64 = 100.0;
const VOLUME_STEP: f64 = 1.0;
const NO_TRACK: &str = "No track";
const NOTHING_PLAYING: &str = "Nothing playing";
const DETAIL_SEPARATOR: &str = " · ";

pub(super) fn tile_for(view: &NowPlaying, enabled: Enabled) -> Result<Node, HandlerError> {
    let track = view.track();
    let info = match track {
        Some(track) => track_info(track)?,
        None => vec![
            text(
                "title",
                NOTHING_PLAYING,
                style(TextSize::Md, Weight::Bold, ColorToken::TextStrong),
            )?,
            text(
                "artist",
                friendly_error(view.error.as_ref()),
                TextStyle {
                    wrap: Wrap::Wrap,
                    ..style(TextSize::Xs, Weight::Regular, ColorToken::TextMuted)
                },
            )?,
        ],
    };
    let mut children = vec![
        header(view, enabled)?,
        row(
            "track",
            Justify::Start,
            TRACK_GAP,
            vec![
                cover(view.cover.clone())?,
                flex(
                    "track-info",
                    Direction::Column,
                    Align::Stretch,
                    Justify::Center,
                    Wrap::NoWrap,
                    Style {
                        width: Size::Fixed(TRACK_INFO_WIDTH),
                        ..Style::default()
                    },
                    info,
                )?,
            ],
        )?,
    ];
    if let Some(track) = track.filter(|track| track.duration_secs > 0) {
        children.push(column(
            "timeline-row",
            Align::Stretch,
            Space::None,
            vec![timeline(track, view.capabilities().seek)?],
        )?);
    }
    children.push(transport(view, enabled)?);
    tile("now-playing", Align::Stretch, children)
}

pub(super) fn volume_tile(
    track: Option<&TrackState>,
    enabled: Enabled,
) -> Result<Node, HandlerError> {
    let level = f64::from(track.map_or(0, |track| track.volume));
    let volume: Node = slider(parse(VOLUME)?, parse(VOLUME)?, level, VOLUME_MAX)
        .step(VOLUME_STEP)
        .format(SliderFormat::Percent)
        .labels(SliderLabels::Value)
        .enabled(enabled)
        .into();
    tile(
        "volume-tile",
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

fn cover(path: Option<PathBuf>) -> Result<Node, HandlerError> {
    let size = Size::Fixed(COVER_DISPLAY);
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
        Wrap::NoWrap,
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

fn track_info(track: &TrackState) -> Result<Vec<Node>, HandlerError> {
    let title = if track.title.is_empty() {
        NO_TRACK
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
            extra.join(DETAIL_SEPARATOR),
            style(TextSize::Xs, Weight::Regular, ColorToken::TextMuted),
        )?);
    }
    Ok(lines)
}

fn timeline(track: &TrackState, seek: bool) -> Result<Node, HandlerError> {
    let duration = f64::from(track.duration_secs.max(1));
    let enabled = if seek && track.duration_secs > 0 {
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
    .step(SEEK_STEP)
    .format(SliderFormat::Duration)
    .labels(SliderLabels::ElapsedRemaining)
    .enabled(enabled);
    if track.state == PlaybackState::Playing {
        built = built.advancing(PLAYBACK_RATE);
    }
    Ok(built.into())
}

fn header(view: &NowPlaying, enabled: Enabled) -> Result<Node, HandlerError> {
    let (status, tone) = match view.track().map(|track| track.state) {
        Some(PlaybackState::Playing) => ("Playing", Tone::Success),
        Some(PlaybackState::Paused) => ("Paused", Tone::Neutral),
        Some(PlaybackState::Stopped) => ("Stopped", Tone::Neutral),
        None => ("Idle", Tone::Neutral),
    };
    let mut left = vec![node(
        "status",
        Element::Badge {
            text: status.to_owned(),
            tone,
        },
    )?];
    if let Some(source) = &view.source {
        left.push(text(
            "source-name",
            source.name.clone(),
            style(TextSize::Xs, Weight::Medium, ColorToken::TextSubtle),
        )?);
    }
    let mut children = vec![row("header-left", Justify::Start, Space::Sm, left)?];
    if let Some(source) = view.source.as_ref().filter(|_| view.capabilities().open) {
        children.push(button(
            OPEN,
            Icon::OpenExternal,
            &format!("Open {}", source.name),
            ButtonVariant::Ghost,
            enabled,
        )?);
    }
    flex(
        "header",
        Direction::Row,
        Align::Center,
        Justify::Between,
        Wrap::NoWrap,
        Style {
            gap: Space::Sm,
            width: Size::Fill,
            ..Style::default()
        },
        children,
    )
}

fn transport(view: &NowPlaying, enabled: Enabled) -> Result<Node, HandlerError> {
    let track = view.track();
    let capabilities = view.capabilities();
    let (play_icon, play_tip) = if view.is_playing() {
        (Icon::Stop, "Pause")
    } else {
        (Icon::Play, "Play")
    };
    let mut children = Vec::new();
    if capabilities.shuffle {
        children.push(toggle_button(
            SHUFFLE,
            Icon::Exchange,
            "Shuffle",
            track.is_some_and(|track| track.shuffling),
            enabled,
        )?);
    }
    children.push(button(
        PREVIOUS,
        Icon::ChevronLeft,
        "Previous",
        ButtonVariant::Ghost,
        enabled,
    )?);
    children.push(button(
        PLAY_PAUSE,
        play_icon,
        play_tip,
        ButtonVariant::Primary,
        enabled,
    )?);
    children.push(button(
        NEXT,
        Icon::ChevronRight,
        "Next",
        ButtonVariant::Ghost,
        enabled,
    )?);
    if capabilities.repeat {
        children.push(toggle_button(
            REPEAT,
            Icon::Repeat,
            "Repeat",
            track.is_some_and(|track| track.repeating),
            enabled,
        )?);
    }
    flex(
        "controls",
        Direction::Row,
        Align::Center,
        Justify::Center,
        Wrap::NoWrap,
        Style {
            gap: Space::Md,
            width: Size::Fill,
            ..Style::default()
        },
        children,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slider_of(node: Node) -> Result<krad_extension_sdk::api::view::Slider, HandlerError> {
        match node.element {
            Element::Slider(slider) => Ok(slider),
            _ => Err(HandlerError::new("not a slider")),
        }
    }

    #[test]
    fn the_timeline_advances_only_while_playing() -> Result<(), HandlerError> {
        let mut track = TrackState {
            duration_secs: 200,
            position_secs: 10,
            state: PlaybackState::Playing,
            ..TrackState::default()
        };
        let playing = slider_of(timeline(&track, true)?)?;
        assert!(playing.advancing.is_some());
        assert!((playing.max - 200.0).abs() < f64::EPSILON);
        track.state = PlaybackState::Paused;
        assert!(slider_of(timeline(&track, true)?)?.advancing.is_none());
        Ok(())
    }

    #[test]
    fn the_track_details_name_the_track_number_and_a_different_album_artist(
    ) -> Result<(), HandlerError> {
        let track = TrackState {
            title: "Song".to_owned(),
            artist: "Artist".to_owned(),
            album: "Album".to_owned(),
            album_artist: "Band".to_owned(),
            track_number: Some(5),
            ..TrackState::default()
        };
        let lines = track_info(&track)?;
        let keys: Vec<&str> = lines.iter().map(|line| line.key.as_ref()).collect();
        assert_eq!(keys, ["title", "artist", "album", "album-info"]);
        let Some(Element::Text { text, .. }) = lines.last().map(|line| &line.element) else {
            return Err(HandlerError::new("no detail line"));
        };
        assert_eq!(text, "Track 5 · Album by Band");
        Ok(())
    }
}
