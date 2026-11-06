pub mod artwork;
mod cache;
pub mod media;
pub mod settings;
mod state;
mod titlebar;
pub mod view;

use krad_extension_sdk::api::protocol::{LogLevel, OpenViewParams, ViewRef};
use krad_extension_sdk::api::view::{Patch, Tone, ViewEvent};
use krad_extension_sdk::api::{CommandId, ViewId};
use krad_extension_sdk::{App, Extension, HandlerError};
use settings::Settings;
use state::{
    control_active, lock_state, next_source, observe_sources, only_time_moved, with_cover,
    SharedState, Shown,
};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use view::{parse, Action, SourceChoice};

const EXTENSION_NAME: &str = "Spotify";
const VIEW_ID: &str = "spotify-player";
const TOGGLE_PANEL: &str = "toggle-panel";
const POLL_TICK: Duration = Duration::from_millis(250);

const COMMANDS: [&str; 9] = [
    view::PREVIOUS,
    view::PLAY_PAUSE,
    view::NEXT,
    view::SHUFFLE,
    view::REPEAT,
    view::VOLUME_UP,
    view::VOLUME_DOWN,
    view::OPEN,
    view::NEXT_SOURCE,
];

fn log(app: &App, level: LogLevel, message: &str) {
    let _ = app.log(level, format!("{EXTENSION_NAME}: {message}"));
}

fn toast(app: &App, message: String, tone: Tone) {
    if let Err(error) = app.toast(message, tone) {
        log(
            app,
            LogLevel::Warn,
            &format!("could not show a toast: {error}"),
        );
    }
}

fn refresh(app: &App, state: &Mutex<SharedState>, force: bool) {
    observe_sources(state);
    let now_playing = with_cover(state);
    titlebar::update(app, state, &now_playing);

    let view = {
        let state = lock_state(state);
        let Some(shown) = &state.shown else { return };
        if !force && only_time_moved(&shown.now_playing, &now_playing, shown.at.elapsed()) {
            return;
        }
        shown.view.clone()
    };
    let root = match view::render(&now_playing) {
        Ok(root) => root,
        Err(error) => {
            log(
                app,
                LogLevel::Error,
                &format!("could not render the view: {error}"),
            );
            return;
        }
    };
    if let Err(error) = app.patch_view(view.clone(), vec![Patch::Replace { node: root }]) {
        log(
            app,
            LogLevel::Warn,
            &format!("could not update the view: {error}"),
        );
        return;
    }
    if let Some(shown) = lock_state(state)
        .shown
        .as_mut()
        .filter(|shown| shown.view == view)
    {
        shown.now_playing = now_playing;
        shown.at = Instant::now();
    }
}

fn run_action(app: &App, state: &Mutex<SharedState>, action: Action) {
    match action {
        Action::Control(control) => {
            if let Err(error) = control_active(state, control) {
                toast(app, view::friendly_error(Some(&error)), Tone::Error);
            }
        }
        Action::Choose(SourceChoice::Automatic) => {
            lock_state(state).selector.pin(None);
        }
        Action::Choose(SourceChoice::Pin(id)) => {
            lock_state(state).selector.pin(Some(id));
        }
        Action::NextSource => toast(app, next_source(state), Tone::Info),
    }
    refresh(app, state, true);
}

fn run_button(app: &App, state: &Mutex<SharedState>, action: &str) {
    let volume_step = lock_state(state).settings.volume_step;
    if let Some(action) = Action::parse(action, volume_step) {
        run_action(app, state, action);
    }
}

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

fn load_settings(app: &App, state: &Mutex<SharedState>) {
    match app.settings() {
        Ok(values) => lock_state(state).settings = Settings::from_values(&values),
        Err(error) => log(
            app,
            LogLevel::Warn,
            &format!("could not read settings, using defaults: {error}"),
        ),
    }
}

fn poll_forever(app: &App, state: &Mutex<SharedState>) {
    load_settings(app, state);
    refresh(app, state, false);
    let mut waited = Duration::ZERO;
    loop {
        thread::sleep(POLL_TICK);
        waited += POLL_TICK;
        if waited >= lock_state(state).settings.poll_interval {
            waited = Duration::ZERO;
            refresh(app, state, false);
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
            thread::spawn(move || poll_forever(&app, &state));
        })
        .on_settings_changed(move |app, values| {
            lock_state(&settings_state).settings = Settings::from_values(&values);
            refresh(app, &settings_state, true);
        })
        .on_popover(
            view_id.clone(),
            move |_, view| {
                observe_sources(&open_state);
                let now_playing = with_cover(&open_state);
                let root = view::render(&now_playing)?;
                lock_state(&open_state).shown = Some(Shown {
                    view,
                    now_playing,
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
            ViewEvent::Click { action } => run_button(app, &event_state, action.as_ref()),
            ViewEvent::SliderCommit { action, value } => {
                if let Some(action) = Action::from_slider(action.as_ref(), value) {
                    run_action(app, &event_state, action);
                }
            }
            _ => {}
        });

    for command in COMMANDS {
        let command_state = Arc::clone(&state);
        extension = extension.on_command(parse::<CommandId>(command)?, move |app, _| {
            run_button(app, &command_state, command);
            Ok(())
        });
    }
    let toggle_state = Arc::clone(&state);
    extension = extension.on_command(parse::<CommandId>(TOGGLE_PANEL)?, move |app, _| {
        toggle_panel(app, &toggle_state, &view_id)
    });
    Ok(extension)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    extension()?.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_sdk::api::manifest::{ExtensionManifest, TitlebarAction, ViewZone};
    use titlebar::{NOW_PLAYING_ITEM, PLAY_PAUSE_ITEM};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn manifest() -> Result<ExtensionManifest, toml::de::Error> {
        toml::from_str(include_str!("../extension.toml"))
    }

    #[test]
    fn manifest_is_valid_and_every_command_has_a_handler() -> TestResult {
        let manifest = manifest()?;
        manifest.check()?;
        let declared: Vec<String> = manifest
            .contributes
            .commands
            .iter()
            .map(|command| command.id.to_string())
            .collect();
        let handled: Vec<&str> = COMMANDS.iter().copied().chain([TOGGLE_PANEL]).collect();
        assert_eq!(declared.len(), handled.len());
        for id in handled {
            assert!(
                declared.iter().any(|declared| declared == id),
                "{id} not declared"
            );
        }
        for command in COMMANDS {
            assert!(
                Action::parse(command, 1).is_some(),
                "{command} is no action"
            );
        }
        for binding in &manifest.contributes.keybindings {
            assert!(declared.contains(&binding.command.to_string()));
            let id = &binding.command;
            assert!(binding.macos.is_some(), "{id} has no macOS shortcut");
            assert!(binding.linux.is_some(), "{id} has no Linux shortcut");
            assert!(binding.windows.is_some(), "{id} has no Windows shortcut");
        }
        Ok(())
    }

    #[test]
    fn the_view_is_a_popover_opened_from_the_titlebar() -> TestResult {
        let manifest = manifest()?;
        let popover = manifest.contributes.views.first().ok_or("no view")?;
        assert_eq!(popover.id.as_ref(), VIEW_ID);
        assert_eq!(popover.zone, ViewZone::Popover);
        let items = &manifest.contributes.titlebar_items;
        let opener = items
            .iter()
            .find(|item| item.id.as_ref() == NOW_PLAYING_ITEM)
            .ok_or("no opener")?;
        assert_eq!(opener.action, TitlebarAction::View(parse(VIEW_ID)?));
        let play = items
            .iter()
            .find(|item| item.id.as_ref() == PLAY_PAUSE_ITEM)
            .ok_or("no play button")?;
        assert_eq!(
            play.action,
            TitlebarAction::Command(parse(view::PLAY_PAUSE)?)
        );
        Ok(())
    }
}
