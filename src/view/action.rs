use crate::media::{Control, SourceId};

pub const PREVIOUS: &str = "previous";
pub const PLAY_PAUSE: &str = "playpause";
pub const NEXT: &str = "next";
pub const SHUFFLE: &str = "shuffle";
pub const REPEAT: &str = "repeat";
pub const VOLUME_UP: &str = "volume-up";
pub const VOLUME_DOWN: &str = "volume-down";
pub const OPEN: &str = "open";
pub const NEXT_SOURCE: &str = "next-source";
pub const SOURCE_AUTO: &str = "source-auto";
const SOURCE_PREFIX: &str = "source:";
pub const SEEK: &str = "seek";
pub const VOLUME: &str = "volume";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceChoice {
    Automatic,
    Pin(SourceId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Control(Control),
    Choose(SourceChoice),
    NextSource,
}

impl Action {
    #[must_use]
    pub fn parse(action: &str, volume_step: u8) -> Option<Self> {
        let step = i16::from(volume_step);
        let control = match action {
            PREVIOUS => Control::Previous,
            PLAY_PAUSE => Control::PlayPause,
            NEXT => Control::Next,
            SHUFFLE => Control::ToggleShuffle,
            REPEAT => Control::ToggleRepeat,
            VOLUME_UP => Control::ChangeVolume(step),
            VOLUME_DOWN => Control::ChangeVolume(-step),
            OPEN => Control::Open,
            NEXT_SOURCE => return Some(Self::NextSource),
            SOURCE_AUTO => return Some(Self::Choose(SourceChoice::Automatic)),
            other => {
                let id = other.strip_prefix(SOURCE_PREFIX)?;
                return Some(Self::Choose(SourceChoice::Pin(SourceId::new(id))));
            }
        };
        Some(Self::Control(control))
    }

    #[must_use]
    pub fn from_slider(action: &str, value: f64) -> Option<Self> {
        let control = match action {
            SEEK => Control::Seek(value),
            VOLUME => Control::SetVolume(value),
            _ => return None,
        };
        Some(Self::Control(control))
    }
}

pub(super) fn pin_action(id: &SourceId) -> String {
    format!("{SOURCE_PREFIX}{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: u8 = 7;

    #[test]
    fn button_and_command_ids_become_actions() {
        let controls = [
            (PREVIOUS, Control::Previous),
            (PLAY_PAUSE, Control::PlayPause),
            (NEXT, Control::Next),
            (SHUFFLE, Control::ToggleShuffle),
            (REPEAT, Control::ToggleRepeat),
            (VOLUME_UP, Control::ChangeVolume(7)),
            (VOLUME_DOWN, Control::ChangeVolume(-7)),
            (OPEN, Control::Open),
        ];
        for (id, control) in controls {
            assert_eq!(
                Action::parse(id, STEP),
                Some(Action::Control(control)),
                "{id}"
            );
        }
        assert_eq!(Action::parse(NEXT_SOURCE, STEP), Some(Action::NextSource));
        assert_eq!(Action::parse("unknown", STEP), None);
    }

    #[test]
    fn source_chips_pin_their_source_or_return_to_automatic() {
        assert_eq!(
            Action::parse(SOURCE_AUTO, STEP),
            Some(Action::Choose(SourceChoice::Automatic))
        );
        let id = SourceId::new("com.google.Chrome");
        assert_eq!(
            Action::parse(&pin_action(&id), STEP),
            Some(Action::Choose(SourceChoice::Pin(id)))
        );
    }

    #[test]
    fn sliders_send_their_value() {
        assert_eq!(
            Action::from_slider(SEEK, 12.0),
            Some(Action::Control(Control::Seek(12.0)))
        );
        assert_eq!(
            Action::from_slider(VOLUME, 40.0),
            Some(Action::Control(Control::SetVolume(40.0)))
        );
        assert_eq!(Action::from_slider(PLAY_PAUSE, 1.0), None);
    }
}
