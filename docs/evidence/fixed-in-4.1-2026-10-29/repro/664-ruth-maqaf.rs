//! Evidence for tC4 #664 (fixed-in-4.1 check, 2026-10-29). Goes at
//! `crates/tc-app/tests/repro_664.rs`. Run:
//! `tc-heavy cargo test -p tc-app --test repro_664 -- --nocapture`
//!
//! Every Ruth note of the bundled en_tn v91 whose quote holds a maqaf, highlighted on the
//! bundled en_ult v91 Ruth through the app's own pane path (`tc_app::sources::pane_book`,
//! the same `VerseModel::aligned` Understand and Translate light). Negative control: the
//! Ruth notes without a maqaf.
#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;

use tc_core::BookId;
use tc_helps::highlight::{MatchQuality, highlight_quote};
use tc_helps::tsv::{TsvScope, parse_notes};
use tc_helps::{BibleLocus, HelpRef};

fn zip_entry(zip: &str, entry: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bundle/en").join(zip);
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let mut text = String::new();
    archive.by_name(entry).unwrap().read_to_string(&mut text).unwrap();
    text
}

#[test]
fn repro_664_ruth_maqaf_quotes_light_in_the_ult() {
    let ult = tc_app::sources::pane_book(&zip_entry("en_ult_v91.sb.zip", "en_ult/ingredients/RUT.usfm"));
    let notes = parse_notes(
        &zip_entry("en_tn_v91.sb.zip", "en_tn/ingredients/RUT.tsv"),
        TsvScope::Book(BookId::Ruth),
    )
    .unwrap()
    .notes;
    let mut tally: BTreeMap<(bool, String), usize> = BTreeMap::new();
    let mut shown = 0;
    for n in &notes {
        let HelpRef::Bible(r) = &n.reference else { continue };
        let BibleLocus::Verses(list) = &r.locus else { continue };
        let [range] = list.ranges() else { continue };
        if range.start() != range.end() || n.quote.is_empty() {
            continue;
        }
        let (c, v) = (range.start().chapter().get(), range.start().verse().get());
        let Some(model) = ult.chapters.get(&c).and_then(|ch| ch.verses.iter().find(|m| m.from <= v && v <= m.to)) else {
            continue;
        };
        let maqaf = n.quote.as_str().contains('\u{05BE}');
        let h = highlight_quote(&model.aligned, &n.quote, n.occurrence);
        *tally.entry((maqaf, format!("{:?}", h.quality))).or_default() += 1;
        if maqaf && shown < 3 && h.quality == MatchQuality::Exact {
            shown += 1;
            println!("RUT {c}:{v} {:?} -> {:?}", n.quote.as_str(), h.texts(&model.aligned).join(" "));
        }
        if maqaf && h.quality != MatchQuality::Exact {
            println!("NOT EXACT RUT {c}:{v} {:?} occ {:?} -> {:?} {:?}", n.quote.as_str(), n.occurrence, h.quality, h.texts(&model.aligned).join(" "));
        }
    }
    for ((maqaf, q), n) in &tally {
        println!("maqaf={maqaf} {q}: {n}");
    }
    let maqaf_total: usize = tally.iter().filter(|((m, _), _)| *m).map(|(_, n)| n).sum();
    let maqaf_none = tally.get(&(true, "None".to_owned())).copied().unwrap_or(0);
    println!("maqaf rows: {maqaf_total}, lit nothing: {maqaf_none}");
    assert!(maqaf_total > 0);
    assert_eq!(maqaf_none, 0, "a maqaf quote lit nothing");
}
