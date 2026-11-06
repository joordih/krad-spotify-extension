pub mod artwork;
pub mod spotify;

use krad_extension_sdk::api::protocol::{
    LogLevel, OpenViewParams, SettingValue, SettingsValues, ViewRef,
};
use krad_extension_sdk::api::view::{
    Align, ButtonVariant, ColorToken, Direction, Element, Enabled, Font, ImageSource, Justify,
    Node, Patch, Radius, Size, Space, Style, TextSize, TextStyle, Tone, ViewEvent, Weight, Wrap,
};
use krad_extension_sdk::api::{ActionId, CommandId, Icon, NodeKey, ViewId};
use krad_extension_sdk::{App, Extension, HandlerError};
use spotify::{SpotifyAdapter, TrackState};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

const VIEW_ID: &str = "spotify-player";
const ROOT_KEY: &str = "root";
/// The poll loop wakes this often to notice a new `poll-interval` without waiting out the old one.
const POLL_TICK: Duration = Duration::from_millis(250);
/// Segments of the progress bar; the SDK grid allows at most 12 columns.
const PROGRESS_SEGMENTS: u8 = 12;
const PROGRESS_HEIGHT: u16 = 4;

/// Actions shared by view buttons and palette commands (same ids).
const PREVIOUS: &str = "previous";
const PLAY_PAUSE: &str = "playpause";
const NEXT: &str = "next";
const SHUFFLE: &str = "shuffle";
const REPEAT: &str = "repeat";
const VOLUME_UP: &str = "volume-up";
const VOLUME_DOWN: &str = "volume-down";
const OPEN: &str = "open";
/// Shows or hides the control-center panel; not a Spotify action.
const TOGGLE_PANEL: &str = "toggle-panel";
const ACTIONS: [&str; 8] = [
    PREVIOUS,
    PLAY_PAUSE,
    NEXT,
    SHUFFLE,
    REPEAT,
    VOLUME_UP,
    VOLUME_DOWN,
    OPEN,
];

// Keys of the settings declared in `extension.toml`.
const SETTING_POLL_INTERVAL: &str = "poll-interval";
const SETTING_VOLUME_STEP: &str = "volume-step";
const SETTING_SHOW_COVER: &str = "show-cover";
const MIN_POLL_SECS: f64 = 1.0;
const MAX_POLL_SECS: f64 = 30.0;
const MIN_VOLUME_STEP: f64 = 1.0;
const MAX_VOLUME_STEP: f64 = 50.0;

/// User configuration; the defaults are what the extension did before it had any.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Settings {
    poll_interval: Duration,
    volume_step: u8,
    show_cover: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(1),
            volume_step: spotify::DEFAULT_VOLUME_STEP,
            show_cover: true,
        }
    }
}

impl Settings {
    /// Reads the values the host sends. A missing, mistyped or non-finite value keeps its default
    /// and an out-of-range number is moved to the nearest limit, so a bad setting never breaks the
    /// view or the controls.
    fn from_values(values: &SettingsValues) -> Self {
        let mut settings = Self::default();
        for (key, value) in values {
            match (key.as_ref(), value) {
                (SETTING_POLL_INTERVAL, SettingValue::Number(secs)) if secs.is_finite() => {
                    settings.poll_interval =
                        Duration::from_secs_f64(secs.clamp(MIN_POLL_SECS, MAX_POLL_SECS));
                }
                (SETTING_VOLUME_STEP, SettingValue::Number(step)) if step.is_finite() => {
                    settings.volume_step =
                        step.round().clamp(MIN_VOLUME_STEP, MAX_VOLUME_STEP) as u8;
                }
                (SETTING_SHOW_COVER, SettingValue::Toggle(show)) => settings.show_cover = *show,
                _ => {}
            }
        }
        settings
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Player {
    track: Option<TrackState>,
    error: Option<String>,
    cover: Option<PathBuf>,
}

#[derive(Default)]
struct SharedState {
    view: Option<ViewRef>,
    last_rendered: Option<Player>,
    settings: Settings,
}

fn lock_state(state: &Mutex<SharedState>) -> std::sync::MutexGuard<'_, SharedState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn load_player(settings: &Settings) -> Player {
    match SpotifyAdapter::new().current_track() {
        Ok(track) => Player {
            // With `show-cover` off the cover is never downloaded.
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

fn run_action(action: &str, settings: &Settings) -> Option<Result<(), String>> {
    let adapter = SpotifyAdapter::with_volume_step(settings.volume_step);
    Some(match action {
        PREVIOUS => adapter.previous(),
        PLAY_PAUSE => adapter.play_pause(),
        NEXT => adapter.next(),
        SHUFFLE => adapter.toggle_shuffle(),
        REPEAT => adapter.toggle_repeat(),
        VOLUME_UP => adapter.volume_up(),
        VOLUME_DOWN => adapter.volume_down(),
        OPEN => adapter.open(),
        _ => return None,
    })
}

fn parse<T: std::str::FromStr>(value: &str) -> Result<T, HandlerError>
where
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|error| HandlerError::new(error.to_string()))
}

fn node(key: &str, element: Element) -> Result<Node, HandlerError> {
    Ok(Node {
        key: parse::<NodeKey>(key)?,
        element,
    })
}

fn text(key: &str, value: impl Into<String>, style: TextStyle) -> Result<Node, HandlerError> {
    node(
        key,
        Element::Text {
            text: value.into(),
            style,
        },
    )
}

fn flex(
    key: &str,
    direction: Direction,
    align: Align,
    justify: Justify,
    style: Style,
    children: Vec<Node>,
) -> Result<Node, HandlerError> {
    node(
        key,
        Element::Flex {
            direction,
            align,
            justify,
            wrap: Wrap::NoWrap,
            style,
            children,
        },
    )
}

fn row(key: &str, justify: Justify, gap: Space, children: Vec<Node>) -> Result<Node, HandlerError> {
    flex(
        key,
        Direction::Row,
        Align::Center,
        justify,
        Style {
            gap,
            ..Style::default()
        },
        children,
    )
}

fn button(
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

/// Secondary when on, ghost when off, so active modes stand out.
fn toggle_button(
    action: &str,
    icon: Icon,
    name: &str,
    on: bool,
    enabled: Enabled,
) -> Result<Node, HandlerError> {
    let (variant, state) = if on {
        (ButtonVariant::Secondary, "on")
    } else {
        (ButtonVariant::Ghost, "off")
    };
    button(action, icon, &format!("{name}: {state}"), variant, enabled)
}

fn style(size: TextSize, weight: Weight, color: ColorToken) -> TextStyle {
    TextStyle {
        size,
        weight,
        color: Some(color),
        wrap: Wrap::NoWrap,
        ..TextStyle::default()
    }
}

fn caption(color: ColorToken) -> TextStyle {
    TextStyle {
        font: Font::Mono,
        ..style(TextSize::Xs, Weight::Regular, color)
    }
}

fn clock(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn cover(path: Option<PathBuf>) -> Result<Node, HandlerError> {
    let size = Size::Fixed(artwork::COVER_SIZE);
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

fn friendly_error(error: Option<String>) -> String {
    match error.as_deref() {
        Some(spotify::NOT_RUNNING) => "Open Spotify to see what's playing".to_owned(),
        // -1743: macOS denied the Automation permission to control Spotify.
        Some(other) if other.contains("-1743") => {
            "macOS is blocking control of Spotify. Allow it in \
             System Settings → Privacy & Security → Automation"
                .to_owned()
        }
        // -1712: the Apple Event timed out (Spotify busy or not answering).
        Some(other) if other.contains("-1712") => {
            "Spotify didn't respond in time. Try again".to_owned()
        }
        Some(other) => other.to_owned(),
        None => "No player information".to_owned(),
    }
}

fn track_info(track: &TrackState) -> Result<Vec<Node>, HandlerError> {
    let title = if track.title.is_empty() {
        "No track"
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
            extra.join(" · "),
            style(TextSize::Xs, Weight::Regular, ColorToken::TextMuted),
        )?);
    }
    Ok(lines)
}

/// A bar of `PROGRESS_SEGMENTS` cells with the first `ratio` share filled. The SDK has no
/// progress or slider element, so a 12-column grid stands in for one.
fn segment_bar(key: &str, ratio: f64) -> Result<Node, HandlerError> {
    let filled = (ratio.clamp(0.0, 1.0) * f64::from(PROGRESS_SEGMENTS)).round() as u8;
    let segments = (0..PROGRESS_SEGMENTS)
        .map(|index| {
            flex(
                &format!("{key}-{index}"),
                Direction::Row,
                Align::Center,
                Justify::Start,
                Style {
                    height: Size::Fixed(PROGRESS_HEIGHT),
                    width: Size::Fill,
                    radius: Radius::Full,
                    background: Some(if index < filled {
                        ColorToken::Accent
                    } else {
                        ColorToken::Hover
                    }),
                    ..Style::default()
                },
                Vec::new(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    node(
        key,
        Element::Grid {
            columns: PROGRESS_SEGMENTS,
            style: Style {
                gap: Space::Xs,
                width: Size::Fill,
                ..Style::default()
            },
            children: segments,
        },
    )
}

fn progress(track: &TrackState) -> Result<Node, HandlerError> {
    let ratio = if track.duration_secs == 0 {
        0.0
    } else {
        f64::from(track.position_secs) / f64::from(track.duration_secs)
    };
    let times = row(
        "times",
        Justify::Between,
        Space::None,
        vec![
            text(
                "position",
                clock(track.position_secs),
                caption(ColorToken::TextSubtle),
            )?,
            text(
                "duration",
                clock(track.duration_secs),
                caption(ColorToken::TextMuted),
            )?,
        ],
    )?;
    column(
        "progress",
        Align::Stretch,
        Space::Xs,
        vec![segment_bar("progress-bar", ratio)?, times],
    )
}

fn column(key: &str, align: Align, gap: Space, children: Vec<Node>) -> Result<Node, HandlerError> {
    flex(
        key,
        Direction::Column,
        align,
        Justify::Start,
        Style {
            gap,
            width: Size::Fill,
            ..Style::default()
        },
        children,
    )
}

/// A Control Center–style module: rounded block on the panel colour, `Selected` when active.
fn tile(key: &str, active: bool, align: Align, children: Vec<Node>) -> Result<Node, HandlerError> {
    flex(
        key,
        Direction::Column,
        align,
        Justify::Start,
        Style {
            padding: Space::Md,
            gap: Space::Sm,
            background: Some(if active {
                ColorToken::Selected
            } else {
                ColorToken::Panel
            }),
            border: Some(ColorToken::Border),
            radius: Radius::Lg,
            width: Size::Fill,
            ..Style::default()
        },
        children,
    )
}

/// Small square-ish module for an on/off mode, like Control Center's toggles.
fn mode_tile(
    action: &str,
    icon: Icon,
    name: &str,
    on: bool,
    enabled: Enabled,
) -> Result<Node, HandlerError> {
    let state = if on { "On" } else { "Off" };
    tile(
        &format!("{action}-tile"),
        on,
        Align::Start,
        vec![row(
            &format!("{action}-row"),
            Justify::Start,
            Space::Sm,
            vec![
                toggle_button(action, icon, name, on, enabled)?,
                column(
                    &format!("{action}-labels"),
                    Align::Stretch,
                    Space::None,
                    vec![
                        text(
                            &format!("{action}-name"),
                            name,
                            style(TextSize::Sm, Weight::Medium, ColorToken::TextStrong),
                        )?,
                        text(
                            &format!("{action}-state"),
                            state,
                            style(TextSize::Xs, Weight::Regular, ColorToken::TextMuted),
                        )?,
                    ],
                )?,
            ],
        )?],
    )
}

fn now_playing(player: &Player, enabled: Enabled) -> Result<Node, HandlerError> {
    let track = player.track.as_ref();
    let playing = track.is_some_and(|track| track.player_state == "playing");
    let info = match track {
        Some(track) => track_info(track)?,
        None => vec![
            text(
                "title",
                "Spotify unavailable",
                style(TextSize::Md, Weight::Bold, ColorToken::TextStrong),
            )?,
            text(
                "artist",
                friendly_error(player.error.clone()),
                TextStyle {
                    wrap: Wrap::Wrap,
                    ..style(TextSize::Xs, Weight::Regular, ColorToken::TextMuted)
                },
            )?,
        ],
    };
    let (play_icon, play_tip) = if playing {
        (Icon::Stop, "Pause")
    } else {
        (Icon::Play, "Play")
    };
    let transport = row(
        "controls",
        Justify::Center,
        Space::Md,
        vec![
            button(
                PREVIOUS,
                Icon::ChevronLeft,
                "Previous",
                ButtonVariant::Ghost,
                enabled,
            )?,
            button(
                PLAY_PAUSE,
                play_icon,
                play_tip,
                ButtonVariant::Primary,
                enabled,
            )?,
            button(
                NEXT,
                Icon::ChevronRight,
                "Next",
                ButtonVariant::Ghost,
                enabled,
            )?,
        ],
    )?;

    let mut children = vec![
        cover(player.cover.clone())?,
        column("track-info", Align::Center, Space::Xs, info)?,
    ];
    if let Some(track) = track {
        children.push(progress(track)?);
    }
    children.push(transport);
    tile("now-playing", false, Align::Center, children)
}

fn volume_tile(track: Option<&TrackState>, enabled: Enabled) -> Result<Node, HandlerError> {
    let level = track.map_or(0, |track| track.volume);
    let label = track.map_or_else(|| "–".to_owned(), |_| format!("{level}%"));
    tile(
        "volume-tile",
        false,
        Align::Stretch,
        vec![
            row(
                "volume-header",
                Justify::Between,
                Space::Sm,
                vec![
                    text(
                        "volume-name",
                        "Volume",
                        style(TextSize::Sm, Weight::Medium, ColorToken::TextStrong),
                    )?,
                    text("volume-level", label, caption(ColorToken::TextSubtle))?,
                ],
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
                    segment_bar("volume-bar", f64::from(level) / 100.0)?,
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

fn render(player: Player) -> Result<Node, HandlerError> {
    let track = player.track.as_ref();
    let enabled = if track.is_some() {
        Enabled::Yes
    } else {
        Enabled::No
    };
    let (status, tone) = match track.map(|track| track.player_state.as_str()) {
        Some("playing") => ("Playing", Tone::Success),
        Some("paused") => ("Paused", Tone::Neutral),
        Some("stopped") => ("Stopped", Tone::Neutral),
        Some(_) => ("Unknown", Tone::Warning),
        None => ("Disconnected", Tone::Neutral),
    };

    let header = row(
        "header",
        Justify::Between,
        Space::Sm,
        vec![
            node(
                "status",
                Element::Badge {
                    text: status.to_owned(),
                    tone,
                },
            )?,
            button(
                OPEN,
                Icon::OpenExternal,
                "Open in Spotify",
                ButtonVariant::Ghost,
                enabled,
            )?,
        ],
    )?;

    // Only the modes this platform's backend can control get a tile.
    let capabilities = spotify::CAPABILITIES;
    let mut mode_tiles = Vec::new();
    if capabilities.shuffle {
        mode_tiles.push(mode_tile(
            SHUFFLE,
            Icon::Exchange,
            "Shuffle",
            track.is_some_and(|track| track.shuffling),
            enabled,
        )?);
    }
    if capabilities.repeat {
        mode_tiles.push(mode_tile(
            REPEAT,
            Icon::Repeat,
            "Repeat",
            track.is_some_and(|track| track.repeating),
            enabled,
        )?);
    }

    let mut tiles = vec![header, now_playing(&player, enabled)?];
    if !mode_tiles.is_empty() {
        tiles.push(node(
            "modes",
            Element::Grid {
                columns: mode_tiles.len() as u8,
                style: Style {
                    gap: Space::Sm,
                    width: Size::Fill,
                    ..Style::default()
                },
                children: mode_tiles,
            },
        )?);
    }
    if capabilities.volume {
        tiles.push(volume_tile(track, enabled)?);
    }
    node(
        ROOT_KEY,
        Element::Scroll {
            direction: Direction::Column,
            style: Style {
                padding: Space::Sm,
                gap: Space::Sm,
                width: Size::Fill,
                height: Size::Fill,
                ..Style::default()
            },
            children: tiles,
        },
    )
}

fn refresh(app: &App, state: &Mutex<SharedState>) {
    let (view, settings) = {
        let state = lock_state(state);
        (state.view.clone(), state.settings)
    };
    let Some(view) = view else { return };
    let player = load_player(&settings);
    if lock_state(state).last_rendered.as_ref() == Some(&player) {
        return;
    }
    match render(player.clone()) {
        Ok(root) => {
            if let Err(error) = app.patch_view(view, vec![Patch::Replace { node: root }]) {
                let _ = app.log(
                    LogLevel::Warn,
                    format!("Spotify: could not update the view: {error}"),
                );
            } else {
                lock_state(state).last_rendered = Some(player);
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

fn perform(app: &App, state: &Mutex<SharedState>, action: &str) {
    let settings = lock_state(state).settings;
    let Some(result) = run_action(action, &settings) else {
        return;
    };
    if let Err(error) = result {
        let _ = app.toast(friendly_error(Some(error)), Tone::Error);
    }
    refresh(app, state);
}

/// Closes the panel when this program has it open, opens (or reveals) it otherwise.
///
/// The host does not tell extensions when the user only hides the side panel, so after a manual
/// hide the first toggle closes the hidden view and the next one shows it again.
fn toggle_panel(
    app: &App,
    state: &Mutex<SharedState>,
    view_id: &ViewId,
) -> Result<(), HandlerError> {
    let open = lock_state(state).view.clone();
    let result = match open {
        Some(view) => {
            {
                let mut state = lock_state(state);
                state.view = None;
                state.last_rendered = None;
            }
            app.close_view(&view)
        }
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
    let initial_state = Arc::clone(&state);
    let event_state = Arc::clone(&state);
    let close_state = Arc::clone(&state);
    let poll_state = Arc::clone(&state);
    let settings_state = Arc::clone(&state);

    let mut extension = Extension::new()
        .on_initialize(move |app, _| {
            let app = app.clone();
            let state = Arc::clone(&poll_state);
            thread::spawn(move || {
                // Off the dispatcher thread: `app.settings()` waits for the host's reply.
                load_settings(&app, &state);
                let mut waited = Duration::ZERO;
                loop {
                    thread::sleep(POLL_TICK);
                    waited += POLL_TICK;
                    if waited >= lock_state(&state).settings.poll_interval {
                        waited = Duration::ZERO;
                        refresh(&app, &state);
                    }
                }
            });
        })
        .on_settings_changed(move |app, values| {
            lock_state(&settings_state).settings = Settings::from_values(&values);
            refresh(app, &settings_state);
        })
        .on_render(view_id.clone(), move |_, view| {
            let settings = lock_state(&initial_state).settings;
            let player = load_player(&settings);
            {
                let mut state = lock_state(&initial_state);
                state.view = Some(view);
                state.last_rendered = Some(player.clone());
            }
            render(player)
        })
        .on_view_event(view_id.clone(), move |app, params| {
            if let ViewEvent::Click { action } = params.event {
                perform(app, &event_state, action.as_ref());
            }
        })
        .on_view_closed(view_id, move |_, view| {
            let mut state = lock_state(&close_state);
            if state.view.as_ref() == Some(&view) {
                state.view = None;
                state.last_rendered = None;
            }
        });

    for action in ACTIONS {
        let command_state = Arc::clone(&state);
        extension = extension.on_command(parse::<CommandId>(action)?, move |app, _| {
            perform(app, &command_state, action);
            Ok(())
        });
    }
    let toggle_state = Arc::clone(&state);
    let toggle_view = parse::<ViewId>(VIEW_ID)?;
    extension = extension.on_command(parse::<CommandId>(TOGGLE_PANEL)?, move |app, _| {
        toggle_panel(app, &toggle_state, &toggle_view)
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
    use krad_extension_sdk::api::SettingKey;

    fn values(entries: &[(&str, SettingValue)]) -> SettingsValues {
        entries
            .iter()
            .map(|(key, value)| (key.parse::<SettingKey>().unwrap(), value.clone()))
            .collect()
    }

    #[test]
    fn no_settings_keep_the_defaults() {
        assert_eq!(
            Settings::from_values(&SettingsValues::new()),
            Settings::default()
        );
    }

    #[test]
    fn settings_are_read_from_the_host_values() {
        let settings = Settings::from_values(&values(&[
            (SETTING_POLL_INTERVAL, SettingValue::Number(5.0)),
            (SETTING_VOLUME_STEP, SettingValue::Number(20.0)),
            (SETTING_SHOW_COVER, SettingValue::Toggle(false)),
        ]));
        assert_eq!(settings.poll_interval, Duration::from_secs(5));
        assert_eq!(settings.volume_step, 20);
        assert!(!settings.show_cover);
    }

    #[test]
    fn out_of_range_numbers_move_to_the_nearest_limit() {
        let low = Settings::from_values(&values(&[
            (SETTING_POLL_INTERVAL, SettingValue::Number(0.0)),
            (SETTING_VOLUME_STEP, SettingValue::Number(-4.0)),
        ]));
        assert_eq!(low.poll_interval, Duration::from_secs(1));
        assert_eq!(low.volume_step, 1);
        let high = Settings::from_values(&values(&[
            (SETTING_POLL_INTERVAL, SettingValue::Number(9999.0)),
            (SETTING_VOLUME_STEP, SettingValue::Number(500.0)),
        ]));
        assert_eq!(high.poll_interval, Duration::from_secs(30));
        assert_eq!(high.volume_step, 50);
    }

    #[test]
    fn mistyped_or_non_finite_values_keep_the_defaults() {
        let settings = Settings::from_values(&values(&[
            (SETTING_POLL_INTERVAL, SettingValue::Text("fast".to_owned())),
            (SETTING_VOLUME_STEP, SettingValue::Number(f64::NAN)),
            (SETTING_SHOW_COVER, SettingValue::Number(0.0)),
            ("unknown", SettingValue::Toggle(false)),
        ]));
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn automation_errors_get_a_message_the_user_can_act_on() {
        let denied = friendly_error(Some("osascript failed: Not authorized (-1743)".to_owned()));
        assert!(denied.contains("Automation"));
        let timeout = friendly_error(Some("osascript failed: timed out (-1712)".to_owned()));
        assert!(timeout.contains("didn't respond"));
        assert_eq!(
            friendly_error(Some(spotify::NOT_RUNNING.to_owned())),
            "Open Spotify to see what's playing"
        );
    }

    #[test]
    fn manifest_is_valid_and_every_command_has_a_handler() {
        use krad_extension_sdk::api::manifest::{ExtensionManifest, ViewZone};
        let manifest: ExtensionManifest =
            toml::from_str(include_str!("../extension.toml")).unwrap();
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
        assert_eq!(manifest.contributes.views[0].zone, ViewZone::SidePanel);
    }

    #[test]
    fn control_center_renders_with_and_without_spotify() {
        let track = TrackState {
            title: "Song".to_owned(),
            duration_secs: 200,
            position_secs: 50,
            volume: 40,
            shuffling: true,
            player_state: "playing".to_owned(),
            ..TrackState::default()
        };
        let playing = render(Player {
            track: Some(track),
            ..Player::default()
        })
        .unwrap();
        assert_eq!(playing.key.as_ref(), ROOT_KEY);
        render(Player {
            error: Some(spotify::NOT_RUNNING.to_owned()),
            ..Player::default()
        })
        .unwrap();
    }
}
