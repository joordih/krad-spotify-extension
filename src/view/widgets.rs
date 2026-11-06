use super::parse;
use krad_extension_sdk::api::view::{
    Align, ButtonVariant, ColorToken, Direction, Element, Enabled, Justify, Node, Radius, Size,
    Space, Style, TextSize, TextStyle, Weight, Wrap,
};
use krad_extension_sdk::api::{ActionId, Icon, NodeKey};
use krad_extension_sdk::HandlerError;

pub(super) const TILE_PADDING: Space = Space::Md;
pub(super) const TILE_PADDING_PX: u16 = 12;
pub(super) const TILE_BORDER_PX: u16 = 1;
const ON: &str = "on";
const OFF: &str = "off";

pub(super) fn node(key: &str, element: Element) -> Result<Node, HandlerError> {
    Ok(Node {
        key: parse::<NodeKey>(key)?,
        element,
    })
}

pub(super) fn text(
    key: &str,
    value: impl Into<String>,
    style: TextStyle,
) -> Result<Node, HandlerError> {
    node(
        key,
        Element::Text {
            text: value.into(),
            style,
        },
    )
}

pub(super) fn flex(
    key: &str,
    direction: Direction,
    align: Align,
    justify: Justify,
    wrap: Wrap,
    style: Style,
    children: Vec<Node>,
) -> Result<Node, HandlerError> {
    node(
        key,
        Element::Flex {
            direction,
            align,
            justify,
            wrap,
            style,
            children,
        },
    )
}

pub(super) fn row(
    key: &str,
    justify: Justify,
    gap: Space,
    children: Vec<Node>,
) -> Result<Node, HandlerError> {
    flex(
        key,
        Direction::Row,
        Align::Center,
        justify,
        Wrap::NoWrap,
        Style {
            gap,
            ..Style::default()
        },
        children,
    )
}

pub(super) fn column(
    key: &str,
    align: Align,
    gap: Space,
    children: Vec<Node>,
) -> Result<Node, HandlerError> {
    flex(
        key,
        Direction::Column,
        align,
        Justify::Start,
        Wrap::NoWrap,
        Style {
            gap,
            width: Size::Fill,
            ..Style::default()
        },
        children,
    )
}

pub(super) fn tile(key: &str, align: Align, children: Vec<Node>) -> Result<Node, HandlerError> {
    flex(
        key,
        Direction::Column,
        align,
        Justify::Start,
        Wrap::NoWrap,
        Style {
            padding: TILE_PADDING,
            gap: Space::Sm,
            background: Some(ColorToken::Panel),
            border: Some(ColorToken::Border),
            radius: Radius::Lg,
            width: Size::Fill,
            ..Style::default()
        },
        children,
    )
}

pub(super) fn button(
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

pub(super) fn button_with_label(
    action: &str,
    label: &str,
    icon: Option<Icon>,
    tooltip: &str,
    variant: ButtonVariant,
) -> Result<Node, HandlerError> {
    node(
        action,
        Element::Button {
            action: parse::<ActionId>(action)?,
            label: Some(label.to_owned()),
            icon,
            variant,
            enabled: Enabled::Yes,
            tooltip: Some(tooltip.to_owned()),
        },
    )
}

pub(super) fn toggle_button(
    action: &str,
    icon: Icon,
    name: &str,
    on: bool,
    enabled: Enabled,
) -> Result<Node, HandlerError> {
    let (variant, state) = if on {
        (ButtonVariant::Secondary, ON)
    } else {
        (ButtonVariant::Ghost, OFF)
    };
    button(action, icon, &format!("{name}: {state}"), variant, enabled)
}

pub(super) fn style(size: TextSize, weight: Weight, color: ColorToken) -> TextStyle {
    TextStyle {
        size,
        weight,
        color: Some(color),
        wrap: Wrap::NoWrap,
        ..TextStyle::default()
    }
}
