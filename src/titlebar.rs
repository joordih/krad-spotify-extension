use crate::state::{lock_state, SharedState};
use crate::view::{parse, NowPlaying};
use crate::{log, EXTENSION_NAME};
use krad_extension_sdk::api::protocol::{LogLevel, SetTitlebarItemParams};
use krad_extension_sdk::api::{Icon, TitlebarItemId};
use krad_extension_sdk::App;
use std::sync::Mutex;

pub(crate) const NOW_PLAYING_ITEM: &str = "now-playing";
pub(crate) const PLAY_PAUSE_ITEM: &str = "play-pause";
const PLAY_TITLE: &str = "Play";
const PAUSE_TITLE: &str = "Pause";
const TRACK_SEPARATOR: &str = " — ";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TitlebarState {
    tooltip: String,
    playing: bool,
    running: bool,
}

impl From<&NowPlaying> for TitlebarState {
    fn from(now_playing: &NowPlaying) -> Self {
        let tooltip = match now_playing.track() {
            Some(track) if !track.title.is_empty() && !track.artist.is_empty() => {
                format!("{}{TRACK_SEPARATOR}{}", track.title, track.artist)
            }
            Some(track) if !track.title.is_empty() => track.title.clone(),
            _ => EXTENSION_NAME.to_owned(),
        };
        Self {
            tooltip,
            playing: now_playing.is_playing(),
            running: now_playing.source.is_some(),
        }
    }
}

pub(crate) fn update(app: &App, state: &Mutex<SharedState>, now_playing: &NowPlaying) {
    let wanted = TitlebarState::from(now_playing);
    if lock_state(state).titlebar.as_ref() == Some(&wanted) {
        return;
    }
    let (Ok(now_playing_item), Ok(play_pause_item)) = (
        parse::<TitlebarItemId>(NOW_PLAYING_ITEM),
        parse::<TitlebarItemId>(PLAY_PAUSE_ITEM),
    ) else {
        return;
    };
    let (icon, title) = if wanted.playing {
        (Icon::Stop, PAUSE_TITLE)
    } else {
        (Icon::Play, PLAY_TITLE)
    };
    let sent = app
        .set_titlebar_item(&SetTitlebarItemParams {
            item: now_playing_item,
            icon: None,
            title: Some(wanted.tooltip.clone()),
            active: Some(wanted.playing),
            hidden: None,
        })
        .and_then(|()| {
            app.set_titlebar_item(&SetTitlebarItemParams {
                item: play_pause_item,
                icon: Some(icon),
                title: Some(title.to_owned()),
                active: None,
                hidden: Some(!wanted.running),
            })
        });
    match sent {
        Ok(()) => lock_state(state).titlebar = Some(wanted),
        Err(error) => log(
            app,
            LogLevel::Warn,
            &format!("could not update the titlebar: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{PlaybackState, SourceState, TrackState};

    fn playing(title: &str, artist: &str) -> NowPlaying {
        NowPlaying {
            source: Some(SourceState {
                track: TrackState {
                    title: title.to_owned(),
                    artist: artist.to_owned(),
                    state: PlaybackState::Playing,
                    ..TrackState::default()
                },
                ..SourceState::default()
            }),
            ..NowPlaying::default()
        }
    }

    #[test]
    fn the_titlebar_names_the_track_and_hides_play_when_closed() {
        let state = TitlebarState::from(&playing("Song", ""));
        assert_eq!(state.tooltip, "Song");
        assert!(state.playing && state.running);
        let with_artist = TitlebarState::from(&playing("Song", "Artist"));
        assert_eq!(with_artist.tooltip, "Song — Artist");
        let closed = TitlebarState::from(&NowPlaying::default());
        assert_eq!(closed.tooltip, EXTENSION_NAME);
        assert!(!closed.running);
    }
}
