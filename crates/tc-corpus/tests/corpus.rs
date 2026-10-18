//! The corpus generator: deterministic, at the §14 rates, with conflicts, and every part
//! labelled.

use std::collections::BTreeSet;
use tc_corpus::{Corpus, Origin, Source, Spec};
use tc_journal::pack::{PackStatus, read_pack};
use tc_journal::pending::{PendingStatus, read_pending};

#[test]
fn psalms_corpus_has_the_rates_conflicts_and_labels() {
    let started = std::time::Instant::now();
    let corpus = Corpus::generate(&Spec::psalms(7)).expect("the Psalms corpus builds");
    eprintln!("{}generated in {:?}", corpus.describe(), started.elapsed());
    let stats = &corpus.journal.stats;
    let psalms = corpus.domain.book(tc_core::BookId::Psalms).expect("Psalms");
    assert_eq!(psalms.verse_count(), 2_461);
    assert_eq!(
        psalms.checks.len(),
        8_212 + 7_683,
        "every tN and TWL row of Psalms"
    );
    // §14 rates: about 8 verse changes per verse, 1.5 alignments per verse, 1.3 decisions
    // per check.
    let kind = |k: &str| stats.by_kind.get(k).copied().unwrap_or(0) as f64;
    let verses = psalms.verse_count() as f64;
    let checks = psalms.checks.len() as f64;
    let seeds = verses + 150.0 + 2.0;
    assert!((kind("verse") - seeds) / verses > 7.0, "{stats:?}");
    assert!((kind("verse") - seeds) / verses < 9.5, "{stats:?}");
    assert!(kind("alignment") / verses > 1.3 && kind("alignment") / verses < 1.7);
    assert!(kind("check") / checks > 1.1 && kind("check") / checks < 1.5);
    assert!(stats.section_saves > 0 && stats.concurrent_edits > 0);
    assert!(stats.conflicts_seen > 0 && stats.resolutions > 0);
    // The packs and pending files read back whole.
    let mut ids = BTreeSet::new();
    for pack in &corpus.journal.packs {
        let read = read_pack(&pack.path, &pack.bytes, corpus.journal.project);
        assert_eq!(read.status, PackStatus::Valid, "{}", pack.path);
        assert!(read.entries.iter().all(|e| e.as_change().is_some()));
        ids.extend(read.entries.iter().map(|e| e.id()));
    }
    assert!(
        !corpus.journal.pending.is_empty(),
        "device 0's last day is pending"
    );
    for file in &corpus.journal.pending {
        let read = read_pending(&file.bytes, corpus.journal.project);
        assert_eq!(read.status, PendingStatus::Valid);
        ids.extend(read.entries.iter().map(|e| e.id()));
    }
    assert_eq!(
        ids.len(),
        stats.changes,
        "every change is in exactly one file"
    );
    // Every part is labelled; Psalms is real.
    assert_eq!(psalms.origin, Origin::Real(tc_corpus::fixtures::PSA_ULT));
    assert!(psalms.original.is_some());
    let description = corpus.describe();
    assert!(description.contains("generated: the history"));
    assert!(description.contains("real, from fixtures/scale/psa/en_twl/twl_PSA.tsv"));
}

#[test]
fn one_seed_gives_one_corpus() {
    let spec = Spec::psalms(11).with_history(40, 0.05);
    let a = Corpus::generate(&spec).expect("builds");
    let b = Corpus::generate(&spec).expect("builds");
    let c = Corpus::generate(&Spec { seed: 12, ..spec }).expect("builds");
    let bytes = |c: &Corpus| -> Vec<Vec<u8>> {
        c.journal
            .packs
            .iter()
            .map(|p| p.bytes.clone())
            .chain(c.journal.pending.iter().map(|p| p.bytes.clone()))
            .collect()
    };
    assert_eq!(bytes(&a), bytes(&b));
    assert_ne!(bytes(&a), bytes(&c));
    assert_eq!(a.decisions, b.decisions);
}

#[test]
fn a_scaled_corpus_has_more_books_devices_and_history() {
    let spec = Spec::scaled(3, 10);
    assert_eq!(spec.source, Source::Psalms);
    assert_eq!(spec.devices, 30);
    let domain = Corpus::domain(&spec).expect("builds");
    let verses = domain.verse_count();
    assert!(verses >= 10 * 2_461, "{verses}");
    let sampled = domain
        .books
        .iter()
        .filter(|b| b.book != tc_core::BookId::Psalms)
        .all(|b| matches!(b.verses[0].origin, Origin::Sampled(_)));
    assert!(sampled, "books other than Psalms are labelled as sampled");
}
