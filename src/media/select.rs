use super::{SourceId, SourceState};
use std::collections::HashMap;

const FORGET_AFTER_LISTINGS: u64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Candidate<'a> {
    id: &'a SourceId,
    playing: bool,
    started_playing: Option<u64>,
    last_playing: Option<u64>,
}

fn choose_active(
    candidates: &[Candidate<'_>],
    pinned: Option<&SourceId>,
    previous: Option<&SourceId>,
) -> Option<SourceId> {
    let find = |id: Option<&SourceId>| {
        id.and_then(|id| candidates.iter().find(|candidate| candidate.id == id))
    };
    let latest = |key: fn(&Candidate<'_>) -> Option<u64>, playing_only: bool| {
        candidates
            .iter()
            .rev()
            .filter(|candidate| !playing_only || candidate.playing)
            .max_by_key(|candidate| key(candidate))
    };
    find(pinned)
        .or_else(|| latest(|candidate| candidate.started_playing, true))
        .or_else(|| find(previous))
        .or_else(|| latest(|candidate| candidate.last_playing, false))
        .map(|candidate| candidate.id.clone())
}

#[derive(Clone, Copy, Debug, Default)]
struct History {
    playing: bool,
    started_playing: Option<u64>,
    last_playing: Option<u64>,
    last_seen: u64,
}

#[derive(Debug, Default)]
pub struct Selector {
    poll: u64,
    history: HashMap<SourceId, History>,
    order: Vec<SourceId>,
    pinned: Option<SourceId>,
    active: Option<SourceId>,
}

impl Selector {
    pub fn observe(&mut self, sources: &[SourceState]) -> Option<SourceId> {
        self.poll += 1;
        let poll = self.poll;
        for source in sources {
            let entry = self.history.entry(source.id.clone()).or_default();
            let playing = source.is_playing();
            if playing {
                if !entry.playing {
                    entry.started_playing = Some(poll);
                }
                entry.last_playing = Some(poll);
            }
            entry.playing = playing;
            entry.last_seen = poll;
        }
        self.history
            .retain(|_, entry| poll - entry.last_seen < FORGET_AFTER_LISTINGS);
        self.order = sources.iter().map(|source| source.id.clone()).collect();
        if self
            .pinned
            .as_ref()
            .is_some_and(|id| !self.history.contains_key(id))
        {
            self.pinned = None;
        }
        self.active = self.choose();
        self.active.clone()
    }

    fn choose(&self) -> Option<SourceId> {
        let candidates: Vec<Candidate<'_>> = self
            .order
            .iter()
            .map(|id| {
                let history = self.history.get(id).copied().unwrap_or_default();
                Candidate {
                    id,
                    playing: history.playing,
                    started_playing: history.started_playing,
                    last_playing: history.last_playing,
                }
            })
            .collect();
        choose_active(&candidates, self.pinned.as_ref(), self.active.as_ref())
    }

    #[must_use]
    pub fn pinned(&self) -> Option<&SourceId> {
        self.pinned.as_ref()
    }

    #[must_use]
    pub fn active(&self) -> Option<&SourceId> {
        self.active.as_ref()
    }

    pub fn pin(&mut self, id: Option<SourceId>) -> bool {
        if id.as_ref().is_some_and(|id| !self.order.contains(id)) {
            return false;
        }
        self.pinned = id;
        self.active = self.choose();
        true
    }

    pub fn cycle(&mut self) -> Option<SourceId> {
        let position = |id: Option<&SourceId>| {
            id.and_then(|id| self.order.iter().position(|other| other == id))
        };
        let next = match position(self.pinned.as_ref()) {
            Some(index) => self.order.get(index + 1).cloned(),
            None if self.order.is_empty() => None,
            None => {
                let after = position(self.active.as_ref()).map_or(0, |index| index + 1);
                self.order.get(after % self.order.len()).cloned()
            }
        };
        self.pin(next.clone());
        next
    }
}

#[cfg(test)]
mod tests {
    use super::super::PlaybackState;
    use super::*;

    fn id(text: &str) -> SourceId {
        SourceId::new(text)
    }

    fn candidate(
        id: &SourceId,
        playing: bool,
        started_playing: Option<u64>,
        last_playing: Option<u64>,
    ) -> Candidate<'_> {
        Candidate {
            id,
            playing,
            started_playing,
            last_playing,
        }
    }

    fn source(name: &str, playing: bool) -> SourceState {
        let mut source = SourceState {
            id: id(name),
            name: name.to_owned(),
            ..SourceState::default()
        };
        if playing {
            source.track.state = PlaybackState::Playing;
        }
        source
    }

    #[test]
    fn a_pin_wins_while_its_source_exists() {
        let (a, b) = (id("a"), id("b"));
        let list = [
            candidate(&a, true, Some(5), Some(5)),
            candidate(&b, false, None, None),
        ];
        assert_eq!(choose_active(&list, Some(&b), Some(&a)), Some(b.clone()));
        let gone = id("gone");
        assert_eq!(choose_active(&list, Some(&gone), None), Some(a));
    }

    #[test]
    fn the_most_recently_started_playing_source_wins() {
        let (a, b, c) = (id("a"), id("b"), id("c"));
        let list = [
            candidate(&a, true, Some(2), Some(9)),
            candidate(&b, true, Some(7), Some(9)),
            candidate(&c, false, Some(8), Some(8)),
        ];
        assert_eq!(choose_active(&list, None, Some(&a)), Some(b));
        let tied = [
            candidate(&a, true, Some(3), Some(3)),
            candidate(&c, true, Some(3), Some(3)),
        ];
        assert_eq!(choose_active(&tied, None, None), Some(a));
    }

    #[test]
    fn with_nothing_playing_the_last_active_source_stays() {
        let (a, b) = (id("a"), id("b"));
        let list = [
            candidate(&a, false, Some(1), Some(4)),
            candidate(&b, false, Some(2), Some(3)),
        ];
        assert_eq!(choose_active(&list, None, Some(&b)), Some(b.clone()));
        assert_eq!(choose_active(&list, None, None), Some(a.clone()));
        let gone = id("gone");
        assert_eq!(choose_active(&list, None, Some(&gone)), Some(a.clone()));
        let fresh = [
            candidate(&b, false, None, None),
            candidate(&a, false, None, None),
        ];
        assert_eq!(choose_active(&fresh, None, None), Some(b));
        assert_eq!(choose_active(&[], None, None), None);
    }

    #[test]
    fn the_selector_follows_whatever_started_playing_last() {
        let mut selector = Selector::default();
        assert_eq!(
            selector.observe(&[source("a", true), source("b", false)]),
            Some(id("a"))
        );
        assert_eq!(
            selector.observe(&[source("a", true), source("b", true)]),
            Some(id("b"))
        );
        assert_eq!(
            selector.observe(&[source("a", false), source("b", false)]),
            Some(id("b"))
        );
        assert_eq!(
            selector.observe(&[source("a", true), source("b", false)]),
            Some(id("a"))
        );
        assert_eq!(selector.observe(&[source("b", false)]), Some(id("b")));
        assert_eq!(selector.observe(&[]), None);
    }

    #[test]
    fn a_pin_survives_a_source_missing_from_a_few_listings() {
        let mut selector = Selector::default();
        let both = [source("a", true), source("b", false)];
        selector.observe(&both);
        assert!(!selector.pin(Some(id("unknown"))));
        assert!(selector.pin(Some(id("b"))));
        assert_eq!(selector.active(), Some(&id("b")));
        for _ in 1..FORGET_AFTER_LISTINGS {
            assert_eq!(selector.observe(&[source("a", true)]), Some(id("a")));
            assert_eq!(selector.pinned(), Some(&id("b")));
        }
        assert_eq!(selector.observe(&both), Some(id("b")));
    }

    #[test]
    fn a_pin_is_dropped_once_its_source_stays_away() {
        let mut selector = Selector::default();
        let both = [source("a", true), source("b", false)];
        selector.observe(&both);
        selector.pin(Some(id("b")));
        for _ in 0..FORGET_AFTER_LISTINGS {
            selector.observe(&[source("a", true)]);
        }
        assert_eq!(selector.pinned(), None);
        assert_eq!(selector.observe(&both), Some(id("a")));
    }

    #[test]
    fn a_source_missing_briefly_keeps_its_play_history() {
        let mut selector = Selector::default();
        selector.observe(&[source("a", true), source("b", false)]);
        assert_eq!(
            selector.observe(&[source("a", true), source("b", true)]),
            Some(id("b"))
        );
        selector.observe(&[source("b", true)]);
        assert_eq!(
            selector.observe(&[source("a", true), source("b", true)]),
            Some(id("b"))
        );
    }

    #[test]
    fn a_source_back_after_the_grace_period_counts_as_new() {
        let mut selector = Selector::default();
        selector.observe(&[source("a", true), source("b", false)]);
        selector.observe(&[source("a", true), source("b", true)]);
        for _ in 0..FORGET_AFTER_LISTINGS {
            selector.observe(&[source("b", true)]);
        }
        assert_eq!(
            selector.observe(&[source("a", true), source("b", true)]),
            Some(id("a"))
        );
    }

    #[test]
    fn next_source_steps_through_the_sources_then_back_to_automatic() {
        let mut selector = Selector::default();
        let sources = [source("a", false), source("b", true), source("c", false)];
        assert_eq!(selector.observe(&sources), Some(id("b")));
        assert_eq!(selector.cycle(), Some(id("c")));
        assert_eq!(selector.active(), Some(&id("c")));
        assert_eq!(selector.cycle(), None);
        assert_eq!(selector.active(), Some(&id("b")));
        assert_eq!(selector.cycle(), Some(id("c")));
        selector.pin(Some(id("a")));
        assert_eq!(selector.cycle(), Some(id("b")));

        let mut single = Selector::default();
        single.observe(&[source("only", false)]);
        assert_eq!(single.cycle(), Some(id("only")));
        assert_eq!(single.cycle(), None);
        assert_eq!(Selector::default().cycle(), None);
    }
}
