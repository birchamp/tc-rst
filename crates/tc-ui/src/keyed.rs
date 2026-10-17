//! Keyed lists as React keeps them: a child keeps its node while its key stays in the list.
//!
//! React keys a child by the key its component gives it, and the keys of one list may repeat
//! (two rows with one `id`, two Select groups with one name). React then still draws every
//! child, matching the n-th child with a key to the n-th child with that key before. A keyed
//! list here does the same: the n-th child with key `k` is the slot `(k, n)`, so no two
//! children share a slot, and each slot finds its own child by position, never by its key.

use std::collections::HashMap;

/// One child of a keyed list: its key and which repeat of that key it is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Slot {
    key: String,
    nth: usize,
}

impl Slot {
    /// The key the list gave the child.
    pub fn key(&self) -> &str {
        &self.key
    }
}

/// The slots of a list, in order, with each slot's position.
#[derive(Debug, Clone, Default)]
pub struct Slots {
    order: Vec<Slot>,
    at: HashMap<Slot, usize>,
}

/// Two lists are equal when their slots are, in order: the positions follow from the order.
impl PartialEq for Slots {
    fn eq(&self, other: &Self) -> bool {
        self.order == other.order
    }
}

impl Slots {
    /// The slots of a list whose children have these keys, in order.
    pub fn new<I>(keys: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut order = Vec::new();
        let mut at = HashMap::new();
        for key in keys {
            let n = seen.entry(key.clone()).or_insert(0);
            let slot = Slot { key, nth: *n };
            *n += 1;
            at.insert(slot.clone(), order.len());
            order.push(slot);
        }
        Self { order, at }
    }

    /// The slots in list order.
    pub fn order(&self) -> &[Slot] {
        &self.order
    }

    /// Where `slot` is in the list, if it is still there.
    pub fn index(&self, slot: &Slot) -> Option<usize> {
        self.at.get(slot).copied()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(keys: &[&str]) -> Slots {
        Slots::new(keys.iter().map(|k| k.to_string()))
    }

    #[test]
    fn a_repeated_key_gives_every_child_its_own_slot() {
        let s = slots(&["tit", "tit", "rut"]);
        assert_eq!(s.len(), 3);
        let [a, b, c] = [&s.order()[0], &s.order()[1], &s.order()[2]];
        assert_ne!(a, b);
        assert_eq!((a.key(), b.key(), c.key()), ("tit", "tit", "rut"));
        assert_eq!(
            (s.index(a), s.index(b), s.index(c)),
            (Some(0), Some(1), Some(2))
        );
    }

    #[test]
    fn a_slot_follows_its_child_when_rows_are_inserted_above_it() {
        let before = slots(&["tit", "jon", "rut"]);
        let after = slots(&["est", "tit", "jon", "rut"]);
        let jon = before.order()[1].clone();
        assert_eq!(after.index(&jon), Some(2));
        assert_eq!(after.order()[2], jon);
    }

    #[test]
    fn a_removed_child_has_no_position() {
        let before = slots(&["ot", "nt", "ot"]);
        let after = slots(&["ot", "nt"]);
        assert_eq!(after.index(&before.order()[2]), None);
        assert_eq!(after.index(&before.order()[0]), Some(0));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod properties {
    use std::collections::HashSet;

    use proptest::prelude::*;

    use super::*;

    proptest! {
        /// Every child gets a distinct slot with its own key, and each slot's position is the
        /// child's: a list never draws one child's data in another's place.
        #[test]
        fn slots_are_distinct_and_find_their_own_child(keys in prop::collection::vec("[a-c]{0,2}", 0..24)) {
            let s = Slots::new(keys.clone());
            prop_assert_eq!(s.len(), keys.len());
            let distinct: HashSet<&Slot> = s.order().iter().collect();
            prop_assert_eq!(distinct.len(), keys.len());
            for (i, slot) in s.order().iter().enumerate() {
                prop_assert_eq!(slot.key(), keys[i].as_str());
                prop_assert_eq!(s.index(slot), Some(i));
            }
        }

        /// A list that keeps the relative order of a key's children keeps their slots: the
        /// n-th child with a key is the same slot before and after.
        #[test]
        fn children_keep_their_slots_across_insertions(
            keys in prop::collection::vec("[a-c]", 0..16),
            inserts in prop::collection::vec((0usize..17, "[a-d]"), 0..6),
        ) {
            let before = Slots::new(keys.clone());
            let mut tagged: Vec<(String, Option<usize>)> =
                keys.iter().cloned().enumerate().map(|(i, k)| (k, Some(i))).collect();
            for (at, k) in inserts {
                tagged.insert(at.min(tagged.len()), (k, None));
            }
            let after = Slots::new(tagged.iter().map(|(k, _)| k.clone()));
            // Children of a key that gained no new child before them keep their slot.
            for (j, (k, was)) in tagged.iter().enumerate() {
                let Some(i) = was else { continue };
                let new_before = tagged[..j].iter().any(|(k2, w)| k2 == k && w.is_none());
                if !new_before {
                    prop_assert_eq!(&after.order()[j], &before.order()[*i]);
                }
            }
        }
    }
}
