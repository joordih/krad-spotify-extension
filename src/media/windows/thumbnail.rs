use crate::media::{content_key, Artwork, SourceId, TrackState};
use std::collections::{HashMap, HashSet};
use windows::core::Interface;
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties as MediaProperties;
use windows::Storage::Streams::{DataReader, IInputStream};

struct Cover {
    track: String,
    artwork: Artwork,
}

#[derive(Default)]
pub(super) struct Thumbnails(HashMap<SourceId, Cover>);

impl Thumbnails {
    pub(super) fn artwork(
        &mut self,
        source: &SourceId,
        track: &TrackState,
        properties: &MediaProperties,
    ) -> Option<Artwork> {
        let key = content_key((source, &track.title, &track.artist, &track.album));
        if let Some(cover) = self.0.get(source).filter(|cover| cover.track == key) {
            return Some(cover.artwork.clone());
        }
        let Some(artwork) = read(properties).and_then(Artwork::from_bytes) else {
            self.0.remove(source);
            return None;
        };
        self.0.insert(
            source.clone(),
            Cover {
                track: key,
                artwork: artwork.clone(),
            },
        );
        Some(artwork)
    }

    pub(super) fn keep_only(&mut self, listed: &HashSet<&SourceId>) {
        self.0.retain(|source, _| listed.contains(source));
    }
}

fn read(properties: &MediaProperties) -> Option<Vec<u8>> {
    let stream = properties
        .Thumbnail()
        .and_then(|reference| reference.OpenReadAsync())
        .and_then(|request| request.get())
        .ok()?;
    let size = u32::try_from(stream.Size().ok()?).ok()?;
    let input: IInputStream = stream.cast().ok()?;
    let reader = DataReader::CreateDataReader(&input).ok()?;
    reader.LoadAsync(size).and_then(|load| load.get()).ok()?;
    let mut bytes = vec![0; usize::try_from(size).ok()?];
    reader.ReadBytes(&mut bytes).ok()?;
    Some(bytes)
}
