use super::{Backend, Command, MediaError, SourceId, SourceState};

#[derive(Debug, Default)]
pub struct Platform;

impl Backend for Platform {
    fn list_sources(&self) -> Result<Vec<SourceState>, MediaError> {
        Err(MediaError::Unavailable)
    }

    fn control(&self, _source: &SourceId, _command: Command) -> Result<(), MediaError> {
        Err(MediaError::Unavailable)
    }
}
