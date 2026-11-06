mod files;
mod names;
mod payload;
mod snapshot;
mod stream;

use super::process;
use crate::media::{Command, MediaError, Position, SourceId, SourceState};
use snapshot::{Snapshot, REPEAT_ALL, REPEAT_OFF, SHUFFLE_OFF, SHUFFLE_TRACKS};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};
use stream::Adapter;

const SEND: &str = "send";
const SHUFFLE: &str = "shuffle";
const REPEAT: &str = "repeat";
const SEEK: &str = "seek";

#[derive(Clone, Copy)]
enum SendCommand {
    TogglePlayPause = 2,
    NextTrack = 4,
    PreviousTrack = 5,
}

#[derive(Default)]
pub(super) struct MediaRemote {
    adapter: OnceLock<Option<Arc<Adapter>>>,
}

impl MediaRemote {
    fn adapter(&self) -> Option<&Arc<Adapter>> {
        self.adapter.get_or_init(stream::start).as_ref()
    }

    pub(super) fn source(&self) -> Option<SourceState> {
        let snapshot = self.adapter()?.snapshot()?;
        Some(snapshot.source_at(SystemTime::now()))
    }

    pub(super) fn control(&self, id: &SourceId, command: Command) -> Result<(), MediaError> {
        match command {
            Command::SetVolume(_) => Err(MediaError::Unsupported),
            Command::Open => {
                self.current(id)?;
                open(id)
            }
            command => {
                let (adapter, snapshot) = self.current(id)?;
                run(adapter, &snapshot, command)
            }
        }
    }

    fn current(&self, id: &SourceId) -> Result<(&Arc<Adapter>, Snapshot), MediaError> {
        let adapter = self.adapter().ok_or(MediaError::SourceGone)?;
        let snapshot = adapter.snapshot().ok_or(MediaError::SourceGone)?;
        if &snapshot.source.id == id {
            Ok((adapter, snapshot))
        } else {
            Err(MediaError::SourceGone)
        }
    }
}

fn run(adapter: &Adapter, snapshot: &Snapshot, command: Command) -> Result<(), MediaError> {
    match command {
        Command::Previous => send(adapter, SendCommand::PreviousTrack),
        Command::PlayPause => send(adapter, SendCommand::TogglePlayPause),
        Command::Next => send(adapter, SendCommand::NextTrack),
        Command::ToggleShuffle => {
            let mode = snapshot.shuffle_mode.ok_or(MediaError::Unsupported)?;
            let next = if mode == SHUFFLE_OFF {
                SHUFFLE_TRACKS
            } else {
                SHUFFLE_OFF
            };
            adapter.run(&[SHUFFLE, &next.to_string()])
        }
        Command::ToggleRepeat => {
            let mode = snapshot.repeat_mode.ok_or(MediaError::Unsupported)?;
            let next = if mode == REPEAT_OFF {
                REPEAT_ALL
            } else {
                REPEAT_OFF
            };
            adapter.run(&[REPEAT, &next.to_string()])
        }
        Command::Seek(position) => seek(adapter, position),
        Command::SetVolume(_) | Command::Open => Err(MediaError::Unsupported),
    }
}

fn send(adapter: &Adapter, command: SendCommand) -> Result<(), MediaError> {
    adapter.run(&[SEND, &(command as u8).to_string()])
}

fn seek(adapter: &Adapter, position: Position) -> Result<(), MediaError> {
    let micros = Duration::try_from_secs_f64(position.secs())
        .map_err(|_| MediaError::InvalidValue)?
        .as_micros();
    adapter.run(&[SEEK, &micros.to_string()])
}

fn open(id: &SourceId) -> Result<(), MediaError> {
    if !process::is_bundle_id(id.as_str()) {
        return Err(MediaError::Unsupported);
    }
    process::open_app(id.as_str())
}
