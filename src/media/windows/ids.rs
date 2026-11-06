use std::collections::{HashMap, HashSet};

const FIRST_INDEX: usize = 1;
const REPEAT_SEPARATOR: char = '#';

pub(super) struct Numbered<K> {
    pub(super) aumid: String,
    pub(super) key: K,
    index: usize,
}

impl<K> Numbered<K> {
    pub(super) fn id(&self) -> String {
        match self.repeat() {
            Some(index) => format!("{}{REPEAT_SEPARATOR}{index}", self.aumid),
            None => self.aumid.clone(),
        }
    }

    pub(super) fn repeat(&self) -> Option<usize> {
        (self.index > FIRST_INDEX).then_some(self.index)
    }
}

pub(super) fn renumber<K: PartialEq>(
    previous: &[Numbered<K>],
    current: Vec<(String, K)>,
) -> Vec<Numbered<K>> {
    let mut taken: HashMap<String, HashSet<usize>> = HashMap::new();
    let kept: Vec<Option<usize>> = current
        .iter()
        .map(|(aumid, key)| {
            previous
                .iter()
                .find(|known| known.aumid == *aumid && known.key == *key)
                .map(|known| known.index)
                .filter(|&index| taken.entry(aumid.clone()).or_default().insert(index))
        })
        .collect();
    current
        .into_iter()
        .zip(kept)
        .map(|((aumid, key), index)| {
            let index =
                index.unwrap_or_else(|| claim_free(taken.entry(aumid.clone()).or_default()));
            Numbered { aumid, key, index }
        })
        .collect()
}

fn claim_free(used: &mut HashSet<usize>) -> usize {
    let past_last = used.len() + FIRST_INDEX;
    let index = (FIRST_INDEX..past_last)
        .find(|index| !used.contains(index))
        .unwrap_or(past_last);
    used.insert(index);
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(sessions: &[(&str, u32)]) -> Vec<(String, u32)> {
        sessions
            .iter()
            .map(|&(aumid, key)| (aumid.to_owned(), key))
            .collect()
    }

    fn ids(numbered: &[Numbered<u32>]) -> Vec<String> {
        numbered.iter().map(Numbered::id).collect()
    }

    #[test]
    fn repeated_aumids_get_numbered_ids_in_listing_order() {
        let numbered = renumber(
            &[],
            listing(&[
                ("Chrome", 1),
                ("Spotify.exe", 2),
                ("Chrome", 3),
                ("Chrome", 4),
            ]),
        );
        assert_eq!(
            ids(&numbered),
            ["Chrome", "Spotify.exe", "Chrome#2", "Chrome#3"]
        );
        let repeats: Vec<Option<usize>> = numbered.iter().map(Numbered::repeat).collect();
        assert_eq!(repeats, [None, None, Some(2), Some(3)]);
    }

    #[test]
    fn ids_survive_reordering_and_other_sessions_closing() {
        let first = renumber(&[], listing(&[("Chrome", 1), ("Chrome", 2), ("Chrome", 3)]));
        let second = renumber(&first, listing(&[("Chrome", 3), ("Chrome", 2)]));
        assert_eq!(ids(&second), ["Chrome#3", "Chrome#2"]);
        let third = renumber(
            &second,
            listing(&[("Chrome", 2), ("Chrome", 4), ("Chrome", 3)]),
        );
        assert_eq!(ids(&third), ["Chrome#2", "Chrome", "Chrome#3"]);
    }

    #[test]
    fn a_reopened_session_takes_the_lowest_free_id() {
        let first = renumber(&[], listing(&[("Chrome", 1)]));
        let closed = renumber(&first, Vec::new());
        let reopened = renumber(&closed, listing(&[("Chrome", 2), ("Chrome", 3)]));
        assert_eq!(ids(&reopened), ["Chrome", "Chrome#2"]);
    }
}
