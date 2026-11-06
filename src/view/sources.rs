use super::action::{pin_action, SOURCE_AUTO};
use super::widgets::{button_with_label, flex};
use super::NowPlaying;
use crate::media::{SourceId, SourceState};
use krad_extension_sdk::api::view::{
    Align, ButtonVariant, Direction, Justify, Node, Size, Space, Style, Wrap,
};
use krad_extension_sdk::api::Icon;
use krad_extension_sdk::HandlerError;

const SOURCES_KEY: &str = "sources";
const MIN_SOURCES_FOR_CHIPS: usize = 2;
const AUTO_LABEL: &str = "Auto";
const AUTO_TOOLTIP: &str = "Follow whatever plays";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceChip {
    pub id: SourceId,
    pub name: String,
    pub playing: bool,
}

impl From<&SourceState> for SourceChip {
    fn from(source: &SourceState) -> Self {
        Self {
            id: source.id.clone(),
            name: source.name.clone(),
            playing: source.is_playing(),
        }
    }
}

pub(super) fn chips(view: &NowPlaying) -> Result<Option<Node>, HandlerError> {
    if view.sources.len() < MIN_SOURCES_FOR_CHIPS {
        return Ok(None);
    }
    let active = view.source.as_ref().map(|source| &source.id);
    let auto_variant = if view.pinned {
        ButtonVariant::Ghost
    } else {
        ButtonVariant::Primary
    };
    let mut chips = vec![button_with_label(
        SOURCE_AUTO,
        AUTO_LABEL,
        None,
        AUTO_TOOLTIP,
        auto_variant,
    )?];
    for source in &view.sources {
        let variant = match (Some(&source.id) == active, view.pinned) {
            (true, true) => ButtonVariant::Primary,
            (true, false) => ButtonVariant::Secondary,
            (false, _) => ButtonVariant::Ghost,
        };
        let tooltip = if source.playing {
            format!("Pin {} (playing)", source.name)
        } else {
            format!("Pin {}", source.name)
        };
        chips.push(button_with_label(
            &pin_action(&source.id),
            &source.name,
            source.playing.then_some(Icon::Play),
            &tooltip,
            variant,
        )?);
    }
    flex(
        SOURCES_KEY,
        Direction::Row,
        Align::Center,
        Justify::Start,
        Wrap::Wrap,
        Style {
            gap: Space::Xs,
            width: Size::Fill,
            ..Style::default()
        },
        chips,
    )
    .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_sdk::api::view::Element;

    fn chip(id: &str, playing: bool) -> SourceChip {
        SourceChip {
            id: SourceId::new(id),
            name: id.to_owned(),
            playing,
        }
    }

    fn variants(node: &Node) -> Vec<(String, ButtonVariant)> {
        node.children()
            .iter()
            .filter_map(|child| match &child.element {
                Element::Button { variant, .. } => Some((child.key.to_string(), *variant)),
                _ => None,
            })
            .collect()
    }

    fn showing(active: &str, pinned: bool) -> NowPlaying {
        NowPlaying {
            source: Some(SourceState {
                id: SourceId::new(active),
                ..SourceState::default()
            }),
            sources: vec![chip("chrome", false), chip("spotify", true)],
            pinned,
            ..NowPlaying::default()
        }
    }

    #[test]
    fn one_source_needs_no_chips() -> Result<(), HandlerError> {
        let view = NowPlaying {
            sources: vec![chip("spotify", true)],
            ..NowPlaying::default()
        };
        assert_eq!(chips(&view)?, None);
        Ok(())
    }

    #[test]
    fn the_chips_highlight_the_active_source_and_the_selection_mode() -> Result<(), HandlerError> {
        let automatic =
            chips(&showing("spotify", false))?.ok_or_else(|| HandlerError::new("none"))?;
        assert_eq!(
            variants(&automatic),
            [
                (SOURCE_AUTO.to_owned(), ButtonVariant::Primary),
                ("source:chrome".to_owned(), ButtonVariant::Ghost),
                ("source:spotify".to_owned(), ButtonVariant::Secondary),
            ]
        );
        let pinned = chips(&showing("chrome", true))?.ok_or_else(|| HandlerError::new("none"))?;
        assert_eq!(
            variants(&pinned),
            [
                (SOURCE_AUTO.to_owned(), ButtonVariant::Ghost),
                ("source:chrome".to_owned(), ButtonVariant::Primary),
                ("source:spotify".to_owned(), ButtonVariant::Ghost),
            ]
        );
        Ok(())
    }
}
