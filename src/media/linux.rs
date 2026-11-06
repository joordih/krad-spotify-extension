mod bus;
mod introspection;
mod mpris;
mod names;
mod value;

use super::{Backend, Command, MediaError, Position, SourceId, SourceState, Volume};
use bus::{Endpoint, SessionBus};
use mpris::{Interface, LoopStatus};
use names::App;
use std::collections::HashMap;
use std::panic;
use std::process;
use std::sync::{Mutex, PoisonError};
use std::thread;
use zbus::zvariant::Value;

const PREVIOUS: &str = "Previous";
const PLAY_PAUSE: &str = "PlayPause";
const NEXT: &str = "Next";
const SET_POSITION: &str = "SetPosition";
const SEEK: &str = "Seek";
const RAISE: &str = "Raise";
const URI_OPENER: &str = "xdg-open";

#[derive(Default)]
pub struct Platform {
    bus: SessionBus,
    volume_writable: Mutex<HashMap<String, bool>>,
}

impl Backend for Platform {
    fn list_sources(&self) -> Result<Vec<SourceState>, MediaError> {
        let (connection, bus_names) = self.bus.names()?;
        let sources = names::source_names(bus_names);
        self.forget_gone_sources(&sources);
        thread::scope(|scope| {
            let readers = sources
                .iter()
                .map(|name| {
                    let endpoint = Endpoint::new(connection.clone(), name);
                    thread::Builder::new().spawn_scoped(scope, move || self.read_source(&endpoint))
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    MediaError::Backend(format!("Could not start a D-Bus reader: {error}"))
                })?;
            Ok(readers
                .into_iter()
                .filter_map(|reader| {
                    reader
                        .join()
                        .unwrap_or_else(|payload| panic::resume_unwind(payload))
                })
                .collect())
        })
    }

    fn control(&self, source: &SourceId, command: Command) -> Result<(), MediaError> {
        let endpoint = self.endpoint(source)?;
        match command {
            Command::Previous => endpoint.call(Interface::Player, PREVIOUS, &()),
            Command::PlayPause => endpoint.call(Interface::Player, PLAY_PAUSE, &()),
            Command::Next => endpoint.call(Interface::Player, NEXT, &()),
            Command::ToggleShuffle => toggle_shuffle(&endpoint),
            Command::ToggleRepeat => toggle_repeat(&endpoint),
            Command::SetVolume(volume) => set_volume(&endpoint, volume),
            Command::Seek(position) => seek(&endpoint, position),
            Command::Open => open(&endpoint),
        }
    }
}

impl Platform {
    fn endpoint(&self, source: &SourceId) -> Result<Endpoint, MediaError> {
        if !names::is_source_name(source.as_str()) {
            return Err(MediaError::SourceGone);
        }
        self.bus.endpoint(source.as_str())
    }

    fn read_source(&self, endpoint: &Endpoint) -> Option<SourceState> {
        let props = endpoint.get_all(Interface::Player).ok()?;
        let app = App::of(&endpoint.name);
        let track = mpris::track_state(&props, app);
        let root = endpoint.get_all(Interface::Root).unwrap_or_default();
        let volume_writable = mpris::publishes_volume(&props) && self.volume_writable(endpoint);
        Some(SourceState {
            id: SourceId::new(endpoint.name.as_str()),
            name: names::display_name(mpris::identity(&root).as_deref(), &endpoint.name),
            capabilities: mpris::capabilities(app, &props, &root, volume_writable, &track),
            track,
        })
    }

    fn volume_writable(&self, endpoint: &Endpoint) -> bool {
        if let Some(writable) = self.known_volume_writable(&endpoint.name) {
            return writable;
        }
        let Ok(xml) = endpoint.introspect() else {
            return false;
        };
        let writable = introspection::property_writable(&xml, Interface::Player, mpris::VOLUME);
        self.volume_writable
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(endpoint.name.clone(), writable);
        writable
    }

    fn known_volume_writable(&self, name: &str) -> Option<bool> {
        self.volume_writable
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
            .copied()
    }

    fn forget_gone_sources(&self, present: &[String]) {
        self.volume_writable
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|name, _| present.contains(name));
    }
}

fn toggle_shuffle(endpoint: &Endpoint) -> Result<(), MediaError> {
    let shuffling =
        value::boolean(&*endpoint.get(mpris::SHUFFLE)?).ok_or(MediaError::Unsupported)?;
    endpoint.set(mpris::SHUFFLE, Value::from(!shuffling))
}

fn toggle_repeat(endpoint: &Endpoint) -> Result<(), MediaError> {
    let status =
        LoopStatus::of(&*endpoint.get(mpris::LOOP_STATUS)?).ok_or(MediaError::Unsupported)?;
    endpoint.set(mpris::LOOP_STATUS, Value::from(status.toggled()))
}

fn set_volume(endpoint: &Endpoint, volume: Volume) -> Result<(), MediaError> {
    endpoint.set(mpris::VOLUME, Value::from(mpris::volume_fraction(volume)))
}

fn seek(endpoint: &Endpoint, position: Position) -> Result<(), MediaError> {
    let props = endpoint.get_all(Interface::Player)?;
    let target = mpris::micros(position)?;
    if let Some(track) = mpris::seekable_track_id(&props) {
        return endpoint.call(Interface::Player, SET_POSITION, &(track, target));
    }
    let offset = target.saturating_sub(mpris::position_micros(&props).unwrap_or(0));
    endpoint.call(Interface::Player, SEEK, &offset)
}

fn open(endpoint: &Endpoint) -> Result<(), MediaError> {
    if App::of(&endpoint.name) == App::Spotify {
        let props = endpoint.get_all(Interface::Player)?;
        if let Some(uri) = mpris::track_state(&props, App::Spotify).track_url {
            return open_uri(&uri);
        }
    }
    endpoint.call(Interface::Root, RAISE, &())
}

fn open_uri(uri: &str) -> Result<(), MediaError> {
    let status = process::Command::new(URI_OPENER)
        .arg(uri)
        .status()
        .map_err(|error| MediaError::Backend(format!("Could not start {URI_OPENER}: {error}")))?;
    if !status.success() {
        return Err(MediaError::Backend(format!(
            "{URI_OPENER} failed with status {status}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "plays/pauses, seeks, changes the volume of and raises every MPRIS source on the session bus"]
    fn controls_live_sources() -> Result<(), MediaError> {
        let platform = Platform::default();
        for source in platform.list_sources()? {
            platform.control(&source.id, Command::PlayPause)?;
            if source.capabilities.seek {
                platform.control(&source.id, Command::Seek(Position::try_from(10.0)?))?;
            }
            if source.capabilities.volume {
                let quieter = Volume::new(source.track.volume.saturating_sub(10));
                platform.control(&source.id, Command::SetVolume(quieter))?;
            }
            if source.capabilities.open && App::of(source.id.as_str()) != App::Spotify {
                platform.control(&source.id, Command::Open)?;
            }
        }
        let gone = SourceId::new("org.mpris.MediaPlayer2.gone");
        assert_eq!(
            platform.control(&gone, Command::Next),
            Err(MediaError::SourceGone)
        );
        Ok(())
    }
}
