use super::{Backend, Capabilities, MediaError, SourceState};

const MAX_PERCENT: u8 = 100;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Volume(u8);

impl Volume {
    #[must_use]
    pub fn new(percent: u8) -> Self {
        Self(percent.min(MAX_PERCENT))
    }

    #[must_use]
    pub fn percent(self) -> u8 {
        self.0
    }

    fn stepped(self, delta: i16) -> Self {
        let stepped = i16::from(self.0)
            .saturating_add(delta)
            .clamp(0, i16::from(MAX_PERCENT));
        u8::try_from(stepped).map_or(self, Self)
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is finite, rounded and clamped to 0..=100 first"
    )]
    fn from_slider(value: f64) -> Self {
        if !value.is_finite() {
            return Self::default();
        }
        Self(value.round().clamp(0.0, f64::from(MAX_PERCENT)) as u8)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Position(f64);

impl Position {
    #[must_use]
    pub fn secs(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Position {
    type Error = MediaError;

    fn try_from(secs: f64) -> Result<Self, Self::Error> {
        if secs.is_finite() {
            Ok(Self(secs.max(0.0)))
        } else {
            Err(MediaError::InvalidValue)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Previous,
    PlayPause,
    Next,
    ToggleShuffle,
    ToggleRepeat,
    SetVolume(Volume),
    Seek(Position),
    Open,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    Previous,
    PlayPause,
    Next,
    ToggleShuffle,
    ToggleRepeat,
    ChangeVolume(i16),
    SetVolume(f64),
    Seek(f64),
    Open,
}

impl Capabilities {
    fn allow(self, control: Control) -> bool {
        match control {
            Control::Previous | Control::PlayPause | Control::Next => true,
            Control::ToggleShuffle => self.shuffle,
            Control::ToggleRepeat => self.repeat,
            Control::ChangeVolume(_) | Control::SetVolume(_) => self.volume,
            Control::Seek(_) => self.seek,
            Control::Open => self.open,
        }
    }
}

/// # Errors
///
/// Fails when the source lacks the capability, the value is invalid, or the backend fails.
pub fn perform(
    backend: &dyn Backend,
    source: &SourceState,
    control: Control,
) -> Result<(), MediaError> {
    backend.control(&source.id, command_for(source, control)?)
}

fn command_for(source: &SourceState, control: Control) -> Result<Command, MediaError> {
    if !source.capabilities.allow(control) {
        return Err(MediaError::Unsupported);
    }
    Ok(match control {
        Control::Previous => Command::Previous,
        Control::PlayPause => Command::PlayPause,
        Control::Next => Command::Next,
        Control::ToggleShuffle => Command::ToggleShuffle,
        Control::ToggleRepeat => Command::ToggleRepeat,
        Control::ChangeVolume(delta) => {
            Command::SetVolume(Volume::new(source.track.volume).stepped(delta))
        }
        Control::SetVolume(value) => Command::SetVolume(Volume::from_slider(value)),
        Control::Seek(secs) => Command::Seek(Position::try_from(secs)?),
        Control::Open => Command::Open,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{SourceId, TrackState};
    use super::*;
    use std::sync::{Mutex, PoisonError};

    #[derive(Default)]
    struct Recorder {
        commands: Mutex<Vec<(SourceId, Command)>>,
    }

    impl Recorder {
        fn commands(&self) -> Vec<(SourceId, Command)> {
            self.commands
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    impl Backend for Recorder {
        fn list_sources(&self) -> Result<Vec<SourceState>, MediaError> {
            Ok(Vec::new())
        }

        fn control(&self, source: &SourceId, command: Command) -> Result<(), MediaError> {
            self.commands
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((source.clone(), command));
            Ok(())
        }
    }

    fn source(id: &str, capabilities: Capabilities, volume: u8) -> SourceState {
        SourceState {
            id: SourceId::new(id),
            name: id.to_owned(),
            capabilities,
            track: TrackState {
                volume,
                ..TrackState::default()
            },
        }
    }

    #[test]
    fn controls_reach_the_source_only_when_it_supports_them() -> Result<(), MediaError> {
        let recorder = Recorder::default();
        let browser = source("chrome", Capabilities::default(), 0);
        for control in [
            Control::ChangeVolume(10),
            Control::SetVolume(50.0),
            Control::Seek(3.0),
            Control::Seek(f64::NAN),
            Control::ToggleShuffle,
            Control::ToggleRepeat,
            Control::Open,
        ] {
            assert_eq!(
                perform(&recorder, &browser, control),
                Err(MediaError::Unsupported),
                "{control:?}"
            );
        }
        perform(&recorder, &browser, Control::PlayPause)?;
        assert_eq!(
            recorder.commands(),
            [(SourceId::new("chrome"), Command::PlayPause)]
        );
        Ok(())
    }

    #[test]
    fn slider_values_become_valid_commands() -> Result<(), MediaError> {
        let recorder = Recorder::default();
        let spotify = source("spotify", Capabilities::ALL, 40);
        perform(&recorder, &spotify, Control::SetVolume(140.0))?;
        perform(&recorder, &spotify, Control::SetVolume(42.6))?;
        perform(&recorder, &spotify, Control::SetVolume(f64::NAN))?;
        perform(&recorder, &spotify, Control::Seek(-5.0))?;
        assert_eq!(
            perform(&recorder, &spotify, Control::Seek(f64::INFINITY)),
            Err(MediaError::InvalidValue)
        );
        let commands: Vec<Command> = recorder
            .commands()
            .into_iter()
            .map(|(_, command)| command)
            .collect();
        assert_eq!(
            commands,
            [
                Command::SetVolume(Volume::new(100)),
                Command::SetVolume(Volume::new(43)),
                Command::SetVolume(Volume::new(0)),
                Command::Seek(Position::try_from(0.0)?),
            ]
        );
        Ok(())
    }

    #[test]
    fn volume_steps_start_from_the_source_volume_and_stay_within_bounds() -> Result<(), MediaError>
    {
        let recorder = Recorder::default();
        for (volume, delta, expected) in [(50, 10, 60), (95, 10, 100), (5, -10, 0)] {
            let spotify = source("spotify", Capabilities::ALL, volume);
            perform(&recorder, &spotify, Control::ChangeVolume(delta))?;
            let last = recorder.commands().pop().map(|(_, command)| command);
            assert_eq!(last, Some(Command::SetVolume(Volume::new(expected))));
        }
        Ok(())
    }
}
