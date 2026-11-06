mod applescript;
#[cfg(test)]
mod live_tests;
mod mediaremote;
mod process;

pub(crate) use process::spawn;

use super::{Backend, Command, MediaError, SourceId, SourceState};
use applescript::{App, Listing, Scripted};
use mediaremote::MediaRemote;

#[derive(Default)]
pub struct Platform {
    scripted: Scripted,
    remote: MediaRemote,
}

impl Backend for Platform {
    fn list_sources(&self) -> Result<Vec<SourceState>, MediaError> {
        let remote = self.remote.source();
        combine(self.scripted.list(), remote)
    }

    fn control(&self, source: &SourceId, command: Command) -> Result<(), MediaError> {
        match App::from_id(source.as_str()) {
            Some(app) => applescript::control(app, command),
            None => self.remote.control(source, command),
        }
    }
}

fn combine(
    scripted: Result<Listing, MediaError>,
    remote: Option<SourceState>,
) -> Result<Vec<SourceState>, MediaError> {
    let Listing {
        mut sources,
        answered,
        failure,
    } = scripted.unwrap_or_else(Listing::failed);
    let answered_for = |source: &SourceState| {
        App::from_id(source.id.as_str()).is_some_and(|app| answered.contains(&app))
    };
    sources.extend(remote.filter(|source| !answered_for(source)));
    match failure {
        Some(error) if sources.is_empty() => Err(error),
        _ => Ok(sources),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(app_id: &str) -> SourceState {
        SourceState {
            id: SourceId::new(app_id),
            name: app_id.to_owned(),
            ..SourceState::default()
        }
    }

    fn ids(sources: &[SourceState]) -> Vec<&str> {
        sources.iter().map(|source| source.id.as_str()).collect()
    }

    fn denied(app: &str) -> MediaError {
        MediaError::AutomationDenied {
            app: app.to_owned(),
        }
    }

    #[test]
    fn scripted_apps_win_over_their_mediaremote_duplicate() -> Result<(), MediaError> {
        let music = App::Music.bundle_id();
        let listing = Listing {
            sources: vec![source(music)],
            answered: vec![App::Music],
            failure: None,
        };
        let sources = combine(Ok(listing), Some(source(music)))?;
        assert_eq!(ids(&sources), [music]);
        Ok(())
    }

    #[test]
    fn mediaremote_stands_in_for_a_failing_scripted_app() -> Result<(), MediaError> {
        let music = App::Music.bundle_id();
        let listing = Listing {
            sources: vec![source(App::Spotify.bundle_id())],
            answered: vec![App::Spotify],
            failure: Some(denied("Music")),
        };
        let sources = combine(Ok(listing), Some(source(music)))?;
        assert_eq!(ids(&sources), [App::Spotify.bundle_id(), music]);
        Ok(())
    }

    #[test]
    fn a_failing_app_is_reported_when_nothing_else_plays() {
        let listing = Listing {
            answered: vec![App::Spotify],
            failure: Some(denied("Music")),
            ..Listing::default()
        };
        assert_eq!(combine(Ok(listing), None), Err(denied("Music")));
        assert_eq!(
            combine(Err(MediaError::TimedOut), Some(source("com.apple.Safari"))),
            Ok(vec![source("com.apple.Safari")])
        );
        assert_eq!(combine(Ok(Listing::default()), None), Ok(Vec::new()));
    }
}
