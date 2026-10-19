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

// ---------------------------------------------------------------------------------------
// Unit P: reserved chapters and the day-by-day timeline
// ---------------------------------------------------------------------------------------

/// `Spec::reserved` and the timeline change nothing of the corpus an existing spec makes:
/// the values below were recorded from the generator before they existed.
#[test]
fn generate_is_unchanged() {
    let full = Corpus::generate(&Spec::psalms(1)).expect("builds");
    assert_eq!(full.journal.stats.changes, 47_326);
    assert_eq!(full.journal.packs.len(), 131);
    assert_eq!(
        full.journal
            .packs
            .iter()
            .map(|p| p.bytes.len())
            .sum::<usize>(),
        48_249_253
    );
    assert_eq!(
        full.journal.packs[0].id.to_string(),
        "234fb861be3acee6f8e9ab21334c09d007c043742edb01956248be34e1409daf"
    );
    let short = Corpus::generate(&Spec::psalms(1).with_history(60, 0.2)).expect("builds");
    assert_eq!(short.journal.stats.changes, 11_553);
    assert_eq!(short.journal.packs.len(), 59);
    assert_eq!(
        short.journal.packs[0].id.to_string(),
        "972ed7c51fd9d3b9803d63fdd2630c6a11c1ada5a9258b9b278b4639e00f180e"
    );
}

/// A reserved chapter is seeded with its book and the team never works on it again: its
/// verses hold only the seed, and no alignment, decision or comment targets it.
#[test]
fn reserved_chapters_hold_only_their_seed() {
    use tc_journal::{Cause, RecordKey};
    let spec = Spec::psalms(1)
        .with_history(60, 0.2)
        .with_reserved(vec![(tc_core::BookId::Psalms, 119)]);
    let corpus = Corpus::generate(&spec).expect("builds");
    let mut seeded = 0;
    for line in corpus.journal.lines() {
        let Some(change) = line.as_change() else {
            continue;
        };
        let body = change.body();
        let key = body.record.key();
        let in_119 = match &key {
            RecordKey::Verse(b, c, _) | RecordKey::Alignment(b, c, _) => {
                *b == tc_core::BookId::Psalms && c.get() == 119
            }
            RecordKey::Check(check) => matches!(
                &check.target,
                tc_journal::CheckTarget::Verse { book, chapter, .. }
                    if *book == tc_core::BookId::Psalms && chapter.number() == Some(119)
            ),
            RecordKey::Comment(_) => matches!(
                &body.record,
                tc_journal::Record::Comment { value, .. }
                    if matches!(&value.target, tc_journal::record::CommentTarget::Verse { book, chapter, .. }
                        if *book == tc_core::BookId::Psalms && chapter.get() == 119)
            ),
            _ => false,
        };
        if in_119 {
            assert!(
                matches!(key, RecordKey::Verse(..)) && matches!(body.cause, Cause::Seed),
                "the team changed a reserved chapter: {key:?} by {:?}",
                body.cause
            );
            seeded += 1;
        }
    }
    assert_eq!(seeded, 176, "the 176 verses of Psalm 119 are seeded");
    // The rest of the corpus is still worked on.
    assert!(corpus.journal.stats.changes > 5_000);
}

#[test]
fn the_timeline_ends_with_the_changes_of_the_corpus() {
    use tc_corpus::timeline::Schedule;
    let spec = Spec::psalms(1).with_history(60, 0.2);
    let corpus = Corpus::generate(&spec).expect("builds");
    let want: BTreeSet<_> = corpus.journal.lines().iter().map(|l| l.id()).collect();
    let project = corpus.journal.project;
    let mut held: std::collections::BTreeMap<String, Vec<u8>> = Default::default();
    let mut days = 0;
    let mut last = 0;
    let stats = Corpus::timeline(&spec, &Schedule::default(), &mut |day| {
        for path in &day.remove {
            assert!(held.remove(path).is_some(), "{path} was not held");
        }
        for pack in &day.add {
            held.insert(pack.path.clone(), pack.bytes.clone());
        }
        days += 1;
        last = day.changes;
        Ok(())
    })
    .expect("the timeline runs");
    assert_eq!(stats.changes, corpus.journal.stats.changes);
    assert!(days >= 40, "{days} working days");
    let mut got = BTreeSet::new();
    for (path, bytes) in &held {
        let read = read_pack(path, bytes, project);
        assert_eq!(read.status, PackStatus::Valid, "{path}");
        got.extend(read.entries.iter().map(|e| e.id()));
    }
    assert_eq!(
        got, want,
        "every change, once, in the packs held at the end"
    );
    let counted: usize = held
        .iter()
        .map(|(path, bytes)| read_pack(path, bytes, project).entries.len())
        .sum();
    assert_eq!(
        last, counted,
        "the day's count is the changes in the packs held"
    );
}

#[test]
fn the_folder_never_holds_a_redundant_pack() {
    use std::collections::BTreeMap;
    use tc_corpus::timeline::Schedule;
    let spec = Spec::psalms(1).with_history(90, 0.3);
    let project = Corpus::generate(&spec).expect("builds").journal.project;
    let mut held: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut merged_somewhere = false;
    Corpus::timeline(&spec, &Schedule::default(), &mut |day| {
        for path in &day.remove {
            held.remove(path);
        }
        for pack in &day.add {
            held.insert(pack.path.clone(), pack.bytes.clone());
        }
        let reads: Vec<(String, tc_journal::pack::PackRead)> = held
            .iter()
            .map(|(p, b)| (p.clone(), read_pack(p, b, project)))
            .collect();
        for (path, read) in &reads {
            let header = read.header.as_ref().expect("a header");
            if !header.replaces.is_empty() {
                merged_somewhere = true;
            }
            let own = read.path.expect("a pack path").sealer;
            for replaced in &header.replaces {
                assert!(
                    reads.iter().all(|(other, r)| {
                        r.path
                            .is_none_or(|p| p.name != *replaced || p.sealer == own)
                            && !(r.path.is_some_and(|p| p.name == *replaced) && other != path)
                    }),
                    "day {}: {path} replaces a pack that is still held, or another sealer's",
                    day.day
                );
            }
        }
        Ok(())
    })
    .expect("the timeline runs");
    assert!(merged_somewhere, "the team merged packs");
}
