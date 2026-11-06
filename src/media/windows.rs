mod ids;
mod launch;
mod names;
mod session;
mod thumbnail;

use super::{Backend, Command, MediaError, Position, SourceId, SourceState};
use ids::Numbered;
use launch::Launch;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};
use thumbnail::Thumbnails;
use windows::core::{IUnknown, Interface, HSTRING};
use windows::ApplicationModel::AppInfo;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
};
use windows::Media::MediaPlaybackAutoRepeatMode as RepeatMode;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows_future::IAsyncOperation;

const REFUSED: &str = "The app refused the request";

impl From<windows::core::Error> for MediaError {
    fn from(error: windows::core::Error) -> Self {
        Self::Backend(format!("Windows media error: {}", error.message()))
    }
}

struct Handle {
    session: Session,
    identity: Option<usize>,
}

impl Handle {
    fn new(session: Session) -> Self {
        let identity = session
            .cast::<IUnknown>()
            .ok()
            .map(|unknown| unknown.as_raw().addr());
        Self { session, identity }
    }
}

impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        matches!((&self.identity, &other.identity), (Some(mine), Some(theirs)) if mine == theirs)
    }
}

#[derive(Default)]
struct Sessions {
    manager: Option<SessionManager>,
    numbered: Vec<Numbered<Handle>>,
}

impl Sessions {
    fn manager(&mut self) -> Result<SessionManager, MediaError> {
        if let Some(manager) = &self.manager {
            return Ok(manager.clone());
        }
        let manager = SessionManager::RequestAsync().and_then(|request| request.get())?;
        self.manager = Some(manager.clone());
        Ok(manager)
    }

    fn refresh(&mut self) -> Result<Vec<Entry>, MediaError> {
        let manager = self.manager()?;
        let current = listed(&manager).inspect_err(|_| self.manager = None)?;
        self.numbered = ids::renumber(&self.numbered, current);
        Ok(self.numbered.iter().map(Entry::from).collect())
    }
}

fn listed(manager: &SessionManager) -> Result<Vec<(String, Handle)>, MediaError> {
    let list = manager.GetSessions()?;
    let count = list.Size()?;
    Ok((0..count)
        .filter_map(|index| list.GetAt(index).ok())
        .map(|session| (aumid(&session), Handle::new(session)))
        .filter(|(aumid, _)| !aumid.is_empty())
        .collect())
}

fn aumid(session: &Session) -> String {
    session
        .SourceAppUserModelId()
        .map(|id| id.to_string_lossy())
        .unwrap_or_default()
}

struct Entry {
    id: SourceId,
    aumid: String,
    repeat: Option<usize>,
    session: Session,
}

impl From<&Numbered<Handle>> for Entry {
    fn from(numbered: &Numbered<Handle>) -> Self {
        Self {
            id: SourceId::new(numbered.id()),
            aumid: numbered.aumid.clone(),
            repeat: numbered.repeat(),
            session: numbered.key.session.clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct App {
    name: String,
    launch: Option<Launch>,
}

impl App {
    fn new(aumid: &str) -> Self {
        let name = AppInfo::GetFromAppUserModelId(&HSTRING::from(aumid))
            .and_then(|info| info.DisplayInfo())
            .and_then(|display| display.DisplayName())
            .map(|name| name.to_string_lossy().trim().to_owned())
            .ok()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| names::display_name(aumid));
        Self {
            name,
            launch: launch::launch_for(aumid),
        }
    }
}

#[derive(Default)]
pub struct Platform {
    sessions: Mutex<Sessions>,
    apps: Mutex<HashMap<String, App>>,
    thumbnails: Mutex<Thumbnails>,
}

impl Platform {
    fn entries(&self) -> Result<Vec<Entry>, MediaError> {
        join_apartment()?;
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .refresh()
    }

    fn find(&self, source: &SourceId) -> Result<Entry, MediaError> {
        self.entries()?
            .into_iter()
            .find(|entry| entry.id == *source)
            .ok_or(MediaError::SourceGone)
    }

    fn app(&self, aumid: &str) -> App {
        self.apps
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(aumid.to_owned())
            .or_insert_with(|| App::new(aumid))
            .clone()
    }

    fn source_state(
        &self,
        entry: &Entry,
        thumbnails: &mut Thumbnails,
    ) -> Result<SourceState, MediaError> {
        let app = self.app(&entry.aumid);
        let properties = session::properties(&entry.session)?;
        let mut track = session::track(&entry.session, &properties)?;
        track.artwork = thumbnails.artwork(&entry.id, &track, &properties);
        Ok(SourceState {
            id: entry.id.clone(),
            name: match entry.repeat {
                Some(index) => format!("{} ({index})", app.name),
                None => app.name,
            },
            capabilities: session::capabilities(&entry.session, app.launch.is_some())?,
            track,
        })
    }

    fn open(&self, entry: &Entry) -> Result<(), MediaError> {
        let app = self.app(&entry.aumid);
        let launch = app.launch.ok_or(MediaError::Unsupported)?;
        launch
            .spawn()
            .map_err(|error| MediaError::Backend(format!("Could not open {}: {error}", app.name)))
    }
}

impl Backend for Platform {
    fn list_sources(&self) -> Result<Vec<SourceState>, MediaError> {
        let entries = self.entries()?;
        let mut thumbnails = self
            .thumbnails
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        thumbnails.keep_only(
            &entries
                .iter()
                .map(|entry| &entry.id)
                .collect::<HashSet<_>>(),
        );
        Ok(entries
            .iter()
            .filter_map(|entry| self.source_state(entry, &mut thumbnails).ok())
            .collect())
    }

    fn control(&self, source: &SourceId, command: Command) -> Result<(), MediaError> {
        let entry = self.find(source)?;
        let session = &entry.session;
        match command {
            Command::Previous => request(session, Session::TrySkipPreviousAsync),
            Command::PlayPause => request(session, Session::TryTogglePlayPauseAsync),
            Command::Next => request(session, Session::TrySkipNextAsync),
            Command::ToggleShuffle => toggle_shuffle(session),
            Command::ToggleRepeat => toggle_repeat(session),
            Command::SetVolume(_) => Err(MediaError::Unsupported),
            Command::Seek(position) => seek(session, position),
            Command::Open => self.open(&entry),
        }
    }
}

fn join_apartment() -> Result<(), MediaError> {
    // SAFETY: RoInitialize takes no pointers; a repeated call on an initialised thread only
    // returns S_FALSE or RPC_E_CHANGED_MODE, and either apartment can use the agile media classes.
    match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        Err(error) if error.code() != RPC_E_CHANGED_MODE => Err(error.into()),
        _ => Ok(()),
    }
}

fn toggle_shuffle(session: &Session) -> Result<(), MediaError> {
    let shuffling = session
        .GetPlaybackInfo()
        .is_ok_and(|playback| session::is_shuffling(&playback));
    request(session, |session| {
        session.TryChangeShuffleActiveAsync(!shuffling)
    })
}

fn toggle_repeat(session: &Session) -> Result<(), MediaError> {
    let repeating = session
        .GetPlaybackInfo()
        .is_ok_and(|playback| session::is_repeating(&playback));
    let mode = if repeating {
        RepeatMode::None
    } else {
        RepeatMode::List
    };
    request(session, |session| {
        session.TryChangeAutoRepeatModeAsync(mode)
    })
}

fn seek(session: &Session, position: Position) -> Result<(), MediaError> {
    let ticks = session::seek_ticks(session, position)?;
    request(session, |session| {
        session.TryChangePlaybackPositionAsync(ticks)
    })
}

fn request(
    session: &Session,
    operation: impl FnOnce(&Session) -> windows::core::Result<IAsyncOperation<bool>>,
) -> Result<(), MediaError> {
    let accepted = operation(session).and_then(|operation| operation.get())?;
    if !accepted {
        return Err(MediaError::Backend(REFUSED.to_owned()));
    }
    Ok(())
}
