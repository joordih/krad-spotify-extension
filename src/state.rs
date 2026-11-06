use crate::media::select::Selector;
use crate::media::{self, Control, MediaError, SourceId, SourceState};
use crate::settings::Settings;
use crate::titlebar::TitlebarState;
use crate::view::{NowPlaying, SourceChip};
use crate::{artwork, EXTENSION_NAME};
use krad_extension_sdk::api::protocol::ViewRef;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

const AUTOMATIC_SOURCE: &str = "automatic";
const POSITION_DRIFT_SECS: f64 = 2.0;

pub(crate) struct Shown {
    pub(crate) view: ViewRef,
    pub(crate) now_playing: NowPlaying,
    pub(crate) at: Instant,
}

#[derive(Default)]
pub(crate) struct SharedState {
    pub(crate) shown: Option<Shown>,
    pub(crate) titlebar: Option<TitlebarState>,
    pub(crate) settings: Settings,
    pub(crate) selector: Selector,
    sources: Vec<SourceState>,
    listing_error: Option<MediaError>,
}

impl SharedState {
    fn active(&self) -> Option<&SourceState> {
        let id = self.selector.active()?;
        self.sources.iter().find(|source| &source.id == id)
    }

    fn now_playing(&self) -> NowPlaying {
        if let Some(error) = &self.listing_error {
            return NowPlaying {
                error: Some(error.clone()),
                ..NowPlaying::default()
            };
        }
        let source = self.active().cloned();
        NowPlaying {
            error: source.is_none().then_some(MediaError::NothingPlaying),
            sources: self.sources.iter().map(SourceChip::from).collect(),
            pinned: self.selector.pinned().is_some(),
            source,
            cover: None,
        }
    }

    fn source_name(&self, id: &SourceId) -> String {
        self.sources
            .iter()
            .find(|source| &source.id == id)
            .map_or_else(|| id.to_string(), |source| source.name.clone())
    }
}

pub(crate) fn lock_state(state: &Mutex<SharedState>) -> MutexGuard<'_, SharedState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn observe_sources(state: &Mutex<SharedState>) {
    let ignored = lock_state(state).settings.ignored_apps.clone();
    let listed = media::visible_sources(media::backend(), &ignored);
    let mut state = lock_state(state);
    match listed {
        Ok(sources) => {
            state.selector.observe(&sources);
            state.sources = sources;
            state.listing_error = None;
        }
        Err(error) => {
            state.sources.clear();
            state.listing_error = Some(error);
        }
    }
}

pub(crate) fn with_cover(state: &Mutex<SharedState>) -> NowPlaying {
    let (mut now_playing, show_cover) = {
        let state = lock_state(state);
        (state.now_playing(), state.settings.show_cover)
    };
    if show_cover {
        now_playing.cover = now_playing
            .track()
            .and_then(|track| track.artwork.as_ref())
            .and_then(artwork::local_cover);
    }
    now_playing
}

pub(crate) fn control_active(
    state: &Mutex<SharedState>,
    control: Control,
) -> Result<(), MediaError> {
    observe_sources(state);
    let source = {
        let state = lock_state(state);
        if let Some(error) = &state.listing_error {
            return Err(error.clone());
        }
        state.active().cloned().ok_or(MediaError::NothingPlaying)?
    };
    media::perform(media::backend(), &source, control)
}

pub(crate) fn next_source(state: &Mutex<SharedState>) -> String {
    let mut state = lock_state(state);
    let chosen = match state.selector.cycle() {
        Some(id) => state.source_name(&id),
        None => AUTOMATIC_SOURCE.to_owned(),
    };
    format!("{EXTENSION_NAME}: source set to {chosen}")
}

pub(crate) fn only_time_moved(old: &NowPlaying, new: &NowPlaying, elapsed: Duration) -> bool {
    let (Some(before), Some(after)) = (&old.source, &new.source) else {
        return old == new;
    };
    let mut aligned = after.clone();
    aligned.track.position_secs = before.track.position_secs;
    if old.cover != new.cover
        || old.error != new.error
        || old.sources != new.sources
        || old.pinned != new.pinned
        || *before != aligned
    {
        return false;
    }
    let (before, after) = (&before.track, &after.track);
    let advanced = if old.is_playing() {
        elapsed.as_secs_f64()
    } else {
        0.0
    };
    let expected =
        (f64::from(before.position_secs) + advanced).min(f64::from(before.duration_secs));
    (f64::from(after.position_secs) - expected).abs() <= POSITION_DRIFT_SECS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{PlaybackState, TrackState};

    fn spotify_at(position_secs: u32) -> SourceState {
        SourceState {
            id: SourceId::new("spotify"),
            name: "Spotify".to_owned(),
            track: TrackState {
                title: "Song".to_owned(),
                duration_secs: 200,
                position_secs,
                state: PlaybackState::Playing,
                ..TrackState::default()
            },
            ..SourceState::default()
        }
    }

    fn playing_at(position_secs: u32) -> NowPlaying {
        let source = spotify_at(position_secs);
        NowPlaying {
            sources: vec![SourceChip::from(&source)],
            source: Some(source),
            ..NowPlaying::default()
        }
    }

    fn with_track(position_secs: u32, change: impl FnOnce(&mut TrackState)) -> NowPlaying {
        let mut now_playing = playing_at(position_secs);
        if let Some(source) = now_playing.source.as_mut() {
            change(&mut source.track);
        }
        now_playing
    }

    #[test]
    fn steady_playback_needs_no_patch_but_a_jump_does() {
        let shown = playing_at(10);
        let elapsed = Duration::from_secs(5);
        assert!(only_time_moved(&shown, &playing_at(15), elapsed));
        assert!(only_time_moved(&shown, &playing_at(16), elapsed));
        assert!(!only_time_moved(&shown, &playing_at(60), elapsed));
        let other = with_track(15, |track| track.title = "Next song".to_owned());
        assert!(!only_time_moved(&shown, &other, elapsed));
        let paused = with_track(10, |track| track.state = PlaybackState::Paused);
        assert!(!only_time_moved(&shown, &paused, elapsed));
        let mut more = playing_at(15);
        more.sources.push(SourceChip::default());
        assert!(!only_time_moved(&shown, &more, elapsed));
        let mut pinned = playing_at(15);
        pinned.pinned = true;
        assert!(!only_time_moved(&shown, &pinned, elapsed));
    }

    #[test]
    fn the_active_source_comes_from_the_last_listing() {
        let spotify = spotify_at(0);
        let mut state = SharedState::default();
        assert_eq!(state.now_playing().error, Some(MediaError::NothingPlaying));
        state.selector.observe(std::slice::from_ref(&spotify));
        state.sources = vec![spotify.clone()];
        assert_eq!(state.now_playing().source, Some(spotify));
        state.listing_error = Some(MediaError::Unavailable);
        assert_eq!(state.now_playing().error, Some(MediaError::Unavailable));
        assert_eq!(state.now_playing().source, None);
    }

    #[test]
    fn next_source_names_the_pinned_source_or_automatic() {
        let state = Mutex::new(SharedState::default());
        {
            let mut state = lock_state(&state);
            let spotify = spotify_at(0);
            state.selector.observe(std::slice::from_ref(&spotify));
            state.sources = vec![spotify];
        }
        assert_eq!(next_source(&state), "Spotify: source set to Spotify");
        assert_eq!(next_source(&state), "Spotify: source set to automatic");
    }
}
