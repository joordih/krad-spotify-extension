pub mod artwork;
pub mod settings;
pub mod spotify;
pub mod view;

use krad_extension_sdk::api::protocol::{LogLevel, OpenViewParams, SetTitlebarItemParams, ViewRef};
use krad_extension_sdk::api::view::{Patch, Tone, ViewEvent};
use krad_extension_sdk::api::{CommandId, Icon, TitlebarItemId, ViewId};
use krad_extension_sdk::{App, Extension, HandlerError};
use settings::Settings;
use spotify::SpotifyAdapter;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};
use view::{parse, Player};

const VIEW_ID: &str = "spotify-player";
/// Titlebar button that opens the popover.
const NOW_PLAYING_ITEM: &str = "now-playing";
/// Titlebar button that runs play/pause.
const PLAY_PAUSE_ITEM: &str = "play-pause";
/// Opens or closes the popover from the keyboard or the command palette.
const TOGGLE_PANEL: &str = "toggle-panel";
/// The poll loop wakes this often to notice a new `poll-interval` without waiting out the old one.
const POLL_TICK: Duration = Duration::from_millis(250);
/// The app advances the timeline by itself; a patch is only needed when Spotify's position
/// strays further than this from where the timeline should be.
const POSITION_DRIFT_SECS: f64 = 2.0;

const ACTIONS: [&str; 8] = [
    view::PREVIOUS,
    view::PLAY_PAUSE,
    view::NEXT,
    view::SHUFFLE,
    view::REPEAT,
    view::VOLUME_UP,
    view::VOLUME_DOWN,
    view::OPEN,
];

/// What the open popover last showed, and when.
struct Shown {
    view: ViewRef,
    player: Player,
    at: Instant,
}

/// What the titlebar buttons last showed, to send only changes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TitlebarState {
    tooltip: String,
    playing: bool,
    running: bool,
}

#[derive(Default)]
struct SharedState {
    shown: Option<Shown>,
    titlebar: Option<TitlebarState>,
    settings: Settings,
}

fn lock_state(state: &Mutex<SharedState>) -> std::sync::MutexGuard<'_, SharedState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn load_player(settings: &Settings) -> Player {
    match SpotifyAdapter::new().current_track() {
        Ok(track) => Player {
            // With `show-cover` off the cover is never fetched.
            cover: track
                .artwork
                .as_ref()
                .filter(|_| settings.show_cover)
                .and_then(artwork::local_cover),
            track: Some(track),
            error: None,
        },
        Err(error) => Player {
            track: None,
            error: Some(error),
            cover: None,
        },
    }
}

/// Whether `new` only differs from `old` by the playback time the app's
/// advancing timeline already shows after `elapsed`.
fn only_time_moved(old: &Player, new: &Player, elapsed: Duration) -> bool {
    let (Some(before), Some(after)) = (&old.track, &new.track) else {
        return old == new;
    };
    let mut aligned = after.clone();
    aligned.position_secs = before.position_secs;
    if old.cover != new.cover || old.error != new.error || *before != aligned {
        return false;
    }
    let advanced = if old.is_playing() {
        elapsed.as_secs_f64()
    } else {
        0.0
    };
    let expected =
        (f64::from(before.position_secs) + advanced).min(f64::from(before.duration_secs));
    (f64::from(after.position_secs) - expected).abs() <= POSITION_DRIFT_SECS
}

fn titlebar_state(player: &Player) -> TitlebarState {
    let tooltip = match &player.track {
        Some(track) if !track.title.is_empty() && !track.artist.is_empty() => {
            format!("{} — {}", track.title, track.artist)
        }
        Some(track) if !track.title.is_empty() => track.title.clone(),
        _ => "Spotify".to_owned(),
    };
    TitlebarState {
        tooltip,
        playing: player.is_playing(),
        running: player.track.is_some(),
    }
}

fn update_titlebar(app: &App, state: &Mutex<SharedState>, player: &Player) {
    let wanted = titlebar_state(player);
    if lock_state(state).titlebar.as_ref() == Some(&wanted) {
        return;
    }
    let (Ok(now_playing), Ok(play_pause)) = (
        parse::<TitlebarItemId>(NOW_PLAYING_ITEM),
        parse::<TitlebarItemId>(PLAY_PAUSE_ITEM),
    ) else {
        return;
    };
    let (icon, title) = if wanted.playing {
        (Icon::Stop, "Pause")
    } else {
        (Icon::Play, "Play")
    };
    let sent = app
        .set_titlebar_item(&SetTitlebarItemParams {
            item: now_playing,
            icon: None,
            title: Some(wanted.tooltip.clone()),
            active: Some(wanted.playing),
            hidden: None,
        })
        .and_then(|()| {
            app.set_titlebar_item(&SetTitlebarItemParams {
                item: play_pause,
                icon: Some(icon),
                title: Some(title.to_owned()),
                active: None,
                hidden: Some(!wanted.running),
            })
        });
    match sent {
        Ok(()) => lock_state(state).titlebar = Some(wanted),
        Err(error) => {
            let _ = app.log(
                LogLevel::Warn,
                format!("Spotify: could not update the titlebar: {error}"),
            );
        }
    }
}

/// Reads Spotify, updates the titlebar and, when the popover is open and shows something
/// out of date, patches it. `force` patches even when only the time moved (after a seek).
fn refresh(app: &App, state: &Mutex<SharedState>, force: bool) {
    let settings = lock_state(state).settings;
    let player = load_player(&settings);
    update_titlebar(app, state, &player);

    let view = {
        let state = lock_state(state);
        let Some(shown) = &state.shown else { return };
        if !force && only_time_moved(&shown.player, &player, shown.at.elapsed()) {
            return;
        }
        shown.view.clone()
    };
    match view::render(&player) {
        Ok(root) => {
            if let Err(error) = app.patch_view(view.clone(), vec![Patch::Replace { node: root }]) {
                let _ = app.log(
                    LogLevel::Warn,
                    format!("Spotify: could not update the view: {error}"),
                );
            } else if let Some(shown) = lock_state(state)
                .shown
                .as_mut()
                .filter(|shown| shown.view == view)
            {
                shown.player = player;
                shown.at = Instant::now();
            }
        }
        Err(error) => {
            let _ = app.log(
                LogLevel::Error,
                format!("Spotify: could not render the view: {error}"),
            );
        }
    }
}

fn run_action(action: &str, settings: &Settings) -> Option<Result<(), String>> {
    let adapter = SpotifyAdapter::with_volume_step(settings.volume_step);
    Some(match action {
        view::PREVIOUS => adapter.previous(),
        view::PLAY_PAUSE => adapter.play_pause(),
        view::NEXT => adapter.next(),
        view::SHUFFLE => adapter.toggle_shuffle(),
        view::REPEAT => adapter.toggle_repeat(),
        view::VOLUME_UP => adapter.volume_up(),
        view::VOLUME_DOWN => adapter.volume_down(),
        view::OPEN => adapter.open(),
        _ => return None,
    })
}

fn report(app: &App, state: &Mutex<SharedState>, result: Result<(), String>) {
    if let Err(error) = result {
        let _ = app.toast(view::friendly_error(Some(error)), Tone::Error);
    }
    refresh(app, state, true);
}

fn perform(app: &App, state: &Mutex<SharedState>, action: &str) {
    let settings = lock_state(state).settings;
    if let Some(result) = run_action(action, &settings) {
        report(app, state, result);
    }
}

/// A slider was released (or moved with the keyboard) at `value`.
fn slider_commit(app: &App, state: &Mutex<SharedState>, action: &str, value: f64) {
    let adapter = SpotifyAdapter::new();
    let result = match action {
        view::SEEK => adapter.seek(value),
        view::VOLUME => adapter.set_volume(value),
        _ => return,
    };
    report(app, state, result);
}

/// Closes the popover when it is open, opens it otherwise.
fn toggle_panel(
    app: &App,
    state: &Mutex<SharedState>,
    view_id: &ViewId,
) -> Result<(), HandlerError> {
    let open = lock_state(state).shown.take().map(|shown| shown.view);
    let result = match open {
        Some(view) => app.close_view(&view),
        None => app.open_view(&OpenViewParams {
            view: ViewRef {
                view: view_id.clone(),
                instance: None,
            },
            title: None,
        }),
    };
    result.map_err(|error| HandlerError::new(error.to_string()))
}

/// Asks the host for the user's settings. On failure the defaults stay and the cause is logged.
fn load_settings(app: &App, state: &Mutex<SharedState>) {
    match app.settings() {
        Ok(values) => lock_state(state).settings = Settings::from_values(&values),
        Err(error) => {
            let _ = app.log(
                LogLevel::Warn,
                format!("Spotify: could not read settings, using defaults: {error}"),
            );
        }
    }
}

fn extension() -> Result<Extension, HandlerError> {
    let state = Arc::new(Mutex::new(SharedState::default()));
    let view_id = parse::<ViewId>(VIEW_ID)?;
    let open_state = Arc::clone(&state);
    let close_state = Arc::clone(&state);
    let event_state = Arc::clone(&state);
    let poll_state = Arc::clone(&state);
    let settings_state = Arc::clone(&state);

    let mut extension = Extension::new()
        .on_initialize(move |app, params| {
            artwork::set_extension_folder(params.extension_folder);
            let app = app.clone();
            let state = Arc::clone(&poll_state);
            thread::spawn(move || {
                // Off the dispatcher thread: `app.settings()` waits for the host's reply.
                load_settings(&app, &state);
                // Polls even while the popover is closed, to keep the titlebar buttons current.
                refresh(&app, &state, false);
                let mut waited = Duration::ZERO;
                loop {
                    thread::sleep(POLL_TICK);
                    waited += POLL_TICK;
                    if waited >= lock_state(&state).settings.poll_interval {
                        waited = Duration::ZERO;
                        refresh(&app, &state, false);
                    }
                }
            });
        })
        .on_settings_changed(move |app, values| {
            lock_state(&settings_state).settings = Settings::from_values(&values);
            refresh(app, &settings_state, true);
        })
        .on_popover(
            view_id.clone(),
            move |_, view| {
                let settings = lock_state(&open_state).settings;
                let player = load_player(&settings);
                let root = view::render(&player)?;
                lock_state(&open_state).shown = Some(Shown {
                    view,
                    player,
                    at: Instant::now(),
                });
                Ok(root)
            },
            move |_, view| {
                let mut state = lock_state(&close_state);
                if state.shown.as_ref().is_some_and(|shown| shown.view == view) {
                    state.shown = None;
                }
            },
        )
        .on_view_event(view_id.clone(), move |app, params| match params.event {
            ViewEvent::Click { action } => perform(app, &event_state, action.as_ref()),
            ViewEvent::SliderCommit { action, value } => {
                slider_commit(app, &event_state, action.as_ref(), value);
            }
            _ => {}
        });

    for action in ACTIONS {
        let command_state = Arc::clone(&state);
        extension = extension.on_command(parse::<CommandId>(action)?, move |app, _| {
            perform(app, &command_state, action);
            Ok(())
        });
    }
    let toggle_state = Arc::clone(&state);
    extension = extension.on_command(parse::<CommandId>(TOGGLE_PANEL)?, move |app, _| {
        toggle_panel(app, &toggle_state, &view_id)
    });
    Ok(extension)
}

fn main() -> std::process::ExitCode {
    match extension() {
        Ok(extension) => match extension.run() {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("krad-spotify: {error}");
                std::process::ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("krad-spotify: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_sdk::api::manifest::{ExtensionManifest, TitlebarAction, ViewZone};
    use spotify::TrackState;

    fn manifest() -> ExtensionManifest {
        toml::from_str(include_str!("../extension.toml")).unwrap()
    }

    fn playing_at(position_secs: u32) -> Player {
        Player {
            track: Some(TrackState {
                title: "Song".to_owned(),
                duration_secs: 200,
                position_secs,
                player_state: "playing".to_owned(),
                ..TrackState::default()
            }),
            ..Player::default()
        }
    }

    #[test]
    fn manifest_is_valid_and_every_command_has_a_handler() {
        let manifest = manifest();
        manifest.check().unwrap();
        let declared: Vec<String> = manifest
            .contributes
            .commands
            .iter()
            .map(|command| command.id.to_string())
            .collect();
        let handled: Vec<&str> = ACTIONS.iter().copied().chain([TOGGLE_PANEL]).collect();
        assert_eq!(declared.len(), handled.len());
        for id in handled {
            assert!(
                declared.iter().any(|declared| declared == id),
                "{id} not declared"
            );
        }
        for binding in &manifest.contributes.keybindings {
            assert!(declared.contains(&binding.command.to_string()));
            let id = &binding.command;
            assert!(binding.macos.is_some(), "{id} has no macOS shortcut");
            assert!(binding.linux.is_some(), "{id} has no Linux shortcut");
            assert!(binding.windows.is_some(), "{id} has no Windows shortcut");
        }
    }

    #[test]
    fn the_view_is_a_popover_opened_from_the_titlebar() {
        let manifest = manifest();
        assert_eq!(manifest.contributes.views[0].id.as_ref(), VIEW_ID);
        assert_eq!(manifest.contributes.views[0].zone, ViewZone::Popover);
        let items = &manifest.contributes.titlebar_items;
        let opener = items
            .iter()
            .find(|item| item.id.as_ref() == NOW_PLAYING_ITEM)
            .unwrap();
        assert_eq!(opener.action, TitlebarAction::View(parse(VIEW_ID).unwrap()));
        let play = items
            .iter()
            .find(|item| item.id.as_ref() == PLAY_PAUSE_ITEM)
            .unwrap();
        assert_eq!(
            play.action,
            TitlebarAction::Command(parse(view::PLAY_PAUSE).unwrap())
        );
    }

    #[test]
    fn steady_playback_needs_no_patch_but_a_jump_does() {
        let shown = playing_at(10);
        let elapsed = Duration::from_secs(5);
        assert!(only_time_moved(&shown, &playing_at(15), elapsed));
        assert!(only_time_moved(&shown, &playing_at(16), elapsed));
        assert!(!only_time_moved(&shown, &playing_at(60), elapsed));
        let mut other = playing_at(15);
        other.track.as_mut().unwrap().title = "Next song".to_owned();
        assert!(!only_time_moved(&shown, &other, elapsed));
        let mut paused = playing_at(10);
        paused.track.as_mut().unwrap().player_state = "paused".to_owned();
        assert!(!only_time_moved(&shown, &paused, elapsed));
    }

    #[test]
    fn the_titlebar_names_the_track_and_hides_play_when_closed() {
        let state = titlebar_state(&playing_at(0));
        assert_eq!(state.tooltip, "Song");
        assert!(state.playing && state.running);
        let mut with_artist = playing_at(0);
        with_artist.track.as_mut().unwrap().artist = "Artist".to_owned();
        assert_eq!(titlebar_state(&with_artist).tooltip, "Song — Artist");
        let closed = titlebar_state(&Player::default());
        assert_eq!(closed.tooltip, "Spotify");
        assert!(!closed.running);
    }
}
