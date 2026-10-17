//! Round trips and models for the parsing and selection code.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use tc_burrito::ScopeRange;
use tc_check::key::part_json;
use tc_check::{RefPart, TargetText, scope, selection};

fn verse_text() -> impl Strategy<Value = String> {
    let word = prop::sample::select(vec![
        "Dios", "dios", "de", "del", "fe", "la", "las", "Pablo", "123", "Señor", "y", "the",
        "theory",
    ]);
    let gap = prop::sample::select(vec![" ", ", ", ". ", " — ", "\n", "  ", "; "]);
    prop::collection::vec((word, gap), 1..14)
        .prop_map(|parts| parts.into_iter().map(|(w, g)| format!("{w}{g}")).collect())
}

proptest! {
    #[test]
    fn tapped_words_restore_to_the_same_words(text in verse_text(), picks in prop::collection::vec(any::<prop::sample::Index>(), 0..5)) {
        let target = TargetText::new(text.clone());
        let shown = target.shown();
        let count = selection::target_words(shown).len();
        let mut indices: Vec<usize> = picks.iter().map(|p| p.index(count)).collect();
        let picked = selection::from_indices(&target, &indices).unwrap();
        indices.sort_unstable();
        indices.dedup();
        prop_assert_eq!(selection::indices(shown, &picked), indices.clone());
        let joined: String = selection::pieces(shown, &picked).iter().map(|p| p.text).collect();
        prop_assert_eq!(joined, shown);
        prop_assert_eq!(selection::selected_text(shown, &picked).is_some(), !indices.is_empty());
        // Every word holds on the text it was picked on, and on an edit elsewhere.
        prop_assert!(target.fits(&picked));
        let edited = TargetText::new(format!("{text}!"));
        prop_assert!(edited.fits(&picked));
    }

    #[test]
    fn numeric_references_are_in_scope_as_the_range_reads(
        c in 1u16..6, v in 1u16..30, c1 in 1u32..6, v1 in 1u32..30, c2 in 1u32..6, v2 in 1u32..30,
    ) {
        let (lo, hi) = ((c1, v1).min((c2, v2)), (c1, v1).max((c2, v2)));
        let text = [
            format!("{}", lo.0),
            format!("{}-{}", lo.0, hi.0),
            format!("{}:{}", lo.0, lo.1),
            format!("{}:{}-{}", lo.0, lo.1.min(hi.1), lo.1.max(hi.1)),
            format!("{}:{}-{}:{}", lo.0, lo.1, hi.0, hi.1),
        ];
        let (c32, v32) = (u32::from(c), u32::from(v));
        let model = [
            c32 == lo.0,
            lo.0 <= c32 && c32 <= hi.0,
            (c32, v32) == lo,
            c32 == lo.0 && lo.1.min(hi.1) <= v32 && v32 <= lo.1.max(hi.1),
            // bible-reference-range reads a span's end only in its last chapter, so a
            // span within one chapter runs to the chapter's end.
            lo.0 <= c32 && c32 <= hi.0
                && (if c32 == lo.0 { v32 >= lo.1 } else if c32 == hi.0 { v32 <= hi.1 } else { true }),
        ];
        for (range, expected) in text.iter().zip(model) {
            let range: ScopeRange = range.parse().unwrap();
            let got = scope::in_scope(&[range], &RefPart::from_number(c), &RefPart::from_number(v));
            prop_assert_eq!(got, expected, "{} contains {}:{}", range, c, v);
        }
        prop_assert!(scope::in_scope(&[], &RefPart::from_number(c), &RefPart::from_number(v)));
    }

    #[test]
    fn reference_parts_read_digits_as_numbers_and_keep_other_text(cell in "[0-9]{1,6}|[a-z0-9,-]{1,8}") {
        let part = RefPart::from_cell(&cell).unwrap();
        if cell.bytes().all(|b| b.is_ascii_digit()) {
            let n: u64 = cell.parse().unwrap();
            prop_assert_eq!(part.as_str(), n.to_string());
            prop_assert_eq!(part_json(&part), serde_json::Value::from(n));
        } else {
            prop_assert_eq!(part.as_str(), cell.as_str());
            prop_assert_eq!(part_json(&part), serde_json::Value::from(cell.as_str()));
        }
    }
}
