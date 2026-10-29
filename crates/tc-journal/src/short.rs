//! The text of an identity string, kept in the value itself when it is short.
//!
//! A whole-Bible project holds about 215,000 check keys, each with a check id and a chapter
//! and verse part of a few bytes. As boxed strings those were three heap allocations a key,
//! about a hundred bytes of the allocator's chunks, and a loader that made them on its
//! reading threads left them scattered through those threads' heaps for as long as the fold
//! kept the key (`docs/evidence/storage-fold-2026-10-16.md`). Up to [`INLINE`] bytes live in
//! the 16 bytes the boxed string took.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU8;

/// The most bytes kept without an allocation.
const INLINE: usize = 15;

/// One representation for each text: the inline form for every text of up to [`INLINE`]
/// bytes and the boxed form for every longer one, so the derived equality is the equality of
/// the texts.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum ShortStr {
    /// The bytes, zero after the text, and the text's length plus one: a length that is
    /// never zero gives the other variant its tag, so the value stays 16 bytes.
    Inline { bytes: [u8; INLINE], len: NonZeroU8 },
    /// A longer text, behind a thin pointer, for the same reason.
    Heap(Box<Box<str>>),
}

impl ShortStr {
    pub(crate) fn new(text: &str) -> Self {
        let len = text.len();
        let tag = u8::try_from(len)
            .ok()
            .filter(|_| len <= INLINE)
            .map(|n| NonZeroU8::MIN.saturating_add(n));
        match tag {
            Some(tag) => {
                let mut bytes = [0u8; INLINE];
                bytes[..len].copy_from_slice(text.as_bytes());
                Self::Inline { bytes, len: tag }
            }
            _ => Self::Heap(Box::new(text.into())),
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        match self {
            // Copied from a `&str` and cut at its length, so the bytes are always UTF-8; the
            // empty fallback is never taken.
            Self::Inline { bytes, len } => {
                std::str::from_utf8(&bytes[..usize::from(len.get() - 1)]).unwrap_or_default()
            }
            Self::Heap(text) => text,
        }
    }

    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Inline { bytes, len } => &bytes[..usize::from(len.get() - 1)],
            Self::Heap(text) => text.as_bytes(),
        }
    }
}

/// The order of the texts, as `str` orders them.
impl Ord for ShortStr {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}

impl PartialOrd for ShortStr {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The hash of the text, as `str` hashes it.
impl Hash for ShortStr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl fmt::Debug for ShortStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;

    fn hash_of<T: Hash + ?Sized>(value: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn a_short_string_takes_no_more_room_than_a_boxed_one() {
        assert_eq!(std::mem::size_of::<ShortStr>(), 16);
        assert!(matches!(ShortStr::new("1"), ShortStr::Inline { .. }));
        assert!(matches!(
            ShortStr::new(&"x".repeat(15)),
            ShortStr::Inline { .. }
        ));
        assert!(matches!(ShortStr::new(&"x".repeat(16)), ShortStr::Heap(_)));
    }

    #[test]
    fn texts_compare_hash_and_read_back_as_strings_do() {
        let texts = [
            "",
            "1",
            "10",
            "9-10",
            "front",
            "1,3,8,12",
            "ab\u{e9}",
            "fifteen bytes!!",
            "sixteen bytes!!!",
            "a much longer identity string than fifteen bytes",
            "\u{5d1}\u{5bc}\u{5b0}",
        ];
        for a in texts {
            let short = ShortStr::new(a);
            assert_eq!(short.as_str(), a);
            assert_eq!(hash_of(&short), hash_of(a));
            for b in texts {
                let other = ShortStr::new(b);
                assert_eq!(short.cmp(&other), a.cmp(b), "{a:?} {b:?}");
                assert_eq!(short == other, a == b, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn every_length_round_trips_and_only_short_texts_stay_inline() {
        for len in (0..=17).chain([254, 255, 256, 257, 510, 511, 512, 513]) {
            let text = "x".repeat(len);
            let short = ShortStr::new(&text);
            assert_eq!(short.as_str(), text);
            assert_eq!(
                matches!(short, ShortStr::Inline { .. }),
                len <= INLINE,
                "{len}"
            );
        }
    }

    proptest::proptest! {
        #[test]
        fn any_text_reads_back_and_orders_as_str(a in ".{0,600}", b in ".{0,600}") {
            let (sa, sb) = (ShortStr::new(&a), ShortStr::new(&b));
            proptest::prop_assert_eq!(sa.as_str(), a.as_str());
            proptest::prop_assert_eq!(sa.cmp(&sb), a.cmp(&b));
        }
    }
}
