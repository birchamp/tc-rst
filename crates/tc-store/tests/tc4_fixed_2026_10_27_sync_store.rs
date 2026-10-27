//! tC4 issues fixed upstream by 2026-10-27, tried on the rebuild's store (report of
//! 2026-10-27, area sync-store).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    reason = "test code: a failed unwrap is a failed test, and tests set up files directly"
)]

use std::path::PathBuf;
use std::sync::Arc;

use tc_journal::command::{AlignmentEdit, Command, SectionValue, SeedSource};
use tc_journal::fold::ProjectKind;
use tc_journal::record::{
    AlignmentGroup, Scope, SourceWord, TargetWord, VerseValue, VersificationValue,
};
use tc_journal::{BookId, ChapterNo, MetaField, MetaPath, Record, RecordKey, VerseNo};
use tc_store::{
    FakePlatform, ManualClock, NewProject, OpenOptions, ProjectHandle, Store, StoreConfig,
};

const TIT: BookId = BookId::Titus;

fn json(text: &str) -> tc_journal::Json {
    tc_journal::json::parse(text.as_bytes()).unwrap()
}

fn titus(dir: &std::path::Path) -> (Store, PathBuf) {
    let projects = dir.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    let store = Store::start(StoreConfig {
        app_data: dir.join("app"),
        projects,
        vfs: Arc::new(tc_store::StdFs),
        platform: Arc::new(FakePlatform::default()),
        clock: Arc::new(ManualClock::at(1_792_195_200_000)),
        threads: 2,
        watch_debounce: std::time::Duration::from_millis(20),
    })
    .unwrap();
    store.wait_cleanup();
    let meta = |field: MetaField, value: &str| Record::ProjectMeta {
        field,
        value: Some(json(value)),
    };
    let extra = |path: &str| MetaField::Extra(MetaPath::parse(path).unwrap());
    let usfm = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/tc4/test-fixtures/en_ult/TIT.usfm"),
    )
    .unwrap();
    let folder = store
        .create(&NewProject {
            name: "Titus".into(),
            kind: ProjectKind::Bible,
            records: vec![
                meta(MetaField::Name, r#"{"en":"Titus"}"#),
                meta(MetaField::Abbreviation, r#"{"en":"tit"}"#),
                meta(
                    MetaField::Languages,
                    r#"[{"name":{"en":"English"},"scriptDirection":"ltr","tag":"en"}]"#,
                ),
                meta(extra("meta.dateCreated"), r#""2026-10-17T00:00:00.000Z""#),
                meta(extra("meta.defaultLocale"), r#""en""#),
                Record::Versification {
                    value: VersificationValue {
                        name: Some("eng".into()),
                        bytes: tc_versification::StandardScheme::Eng.json().to_owned(),
                    },
                },
            ],
            books: vec![(TIT, Scope::Whole, SeedSource { usfm })],
        })
        .unwrap();
    (store, folder)
}

fn verse(v: u16) -> RecordKey {
    RecordKey::Verse(TIT, ChapterNo::new(1).unwrap(), VerseNo::new(v).unwrap())
}

fn alignment_key(v: u16) -> RecordKey {
    RecordKey::Alignment(TIT, ChapterNo::new(1).unwrap(), VerseNo::new(v).unwrap())
}

fn target(word: &str) -> TargetWord {
    TargetWord {
        word: word.into(),
        occurrence: 1,
        occurrences: 1,
        extra: Default::default(),
    }
}

fn card(source: &str, on: &str) -> AlignmentGroup {
    AlignmentGroup {
        top_words: vec![SourceWord {
            word: source.into(),
            strong: None,
            lemma: Some(source.into()),
            morph: None,
            occurrence: 1,
            occurrences: 1,
            extra: Default::default(),
        }],
        bottom_words: vec![target(on)],
        extra: Default::default(),
    }
}

fn text_of(handle: &ProjectHandle, v: u16) -> VerseValue {
    match handle.snapshot().fold().projected(&verse(v)) {
        Some(Record::Verse { value, .. }) => value,
        other => panic!("1:{v}: {other:?}"),
    }
}

fn detached_alignments(handle: &ProjectHandle) -> Vec<RecordKey> {
    handle
        .snapshot()
        .fold()
        .detached()
        .filter(|k| matches!(k, RecordKey::Alignment(..)))
        .cloned()
        .collect()
}

fn save_alignment(
    handle: &ProjectHandle,
    shown: tc_journal::ShownToken,
    verse_shown: tc_journal::ShownToken,
    on: &str,
    bank: &str,
) -> Result<tc_store::StoreReply, tc_store::CommandError> {
    handle
        .submit(Command::SaveAlignment {
            shown,
            verse_shown,
            value: AlignmentEdit {
                alignments: vec![card("ἐπ’", on)],
                word_bank: vec![target(bank)],
                done: false,
            },
        })
        .wait()
}

/// tC4 #792 "Align save: a span created while an alignment save waits for Retry leaves orphan
/// alignment records".
///
/// Steps, as the rebuild can reach them: Titus 1:2 is "in hope" and aligned. The aligner
/// holds a second save of 1:2 made against the verse as it showed it (its tokens minted
/// before the span), which waits (tC4: kept for Retry). The translator then joins 1:1-2 into
/// one span in Translate. The waiting alignment save of 1:2 then goes through (tC4: Retry, or
/// a navigation drains it). No alignment record may be left on the verse the span removed:
/// nothing under Detached, now or after the project opens again.
#[test]
fn tc4_792_a_late_alignment_save_after_a_span_leaves_no_orphan_alignment() {
    let dir = tempfile::tempdir().unwrap();
    let (store, folder) = titus(dir.path());
    let handle = store.open(&folder, &OpenOptions::default()).unwrap();
    let shown = handle.mint(&verse(2), None).unwrap();
    handle
        .submit(Command::SaveVerse {
            shown,
            words: "in hope".to_owned(),
        })
        .wait()
        .unwrap();
    save_alignment(
        &handle,
        handle.mint(&alignment_key(2), None).unwrap(),
        handle.mint(&verse(2), None).unwrap(),
        "in",
        "hope",
    )
    .unwrap();
    assert!(
        handle
            .snapshot()
            .fold()
            .projected(&alignment_key(2))
            .is_some()
    );
    // The aligner's next save of 1:2, made against what it showed, waits.
    let waiting = (
        handle.mint(&alignment_key(2), None).unwrap(),
        handle.mint(&verse(2), None).unwrap(),
    );
    // Translate joins 1:1-2.
    let VerseValue::Text {
        marker,
        words,
        layout,
        from,
        ..
    } = text_of(&handle, 1)
    else {
        panic!("1:1 is text");
    };
    handle
        .submit(Command::SaveSection {
            book: TIT,
            shown: vec![
                (
                    handle.mint(&verse(1), None).unwrap(),
                    Some(SectionValue::Verse(VerseValue::Text {
                        marker,
                        words: format!("{} in hope", words.trim_end()),
                        layout,
                        last: VerseNo::new(2).unwrap(),
                        from,
                    })),
                ),
                (
                    handle.mint(&verse(2), None).unwrap(),
                    Some(SectionValue::Verse(VerseValue::Covered {
                        by: VerseNo::new(1).unwrap(),
                    })),
                ),
            ],
            layout: None,
        })
        .wait()
        .unwrap();
    assert!(
        matches!(text_of(&handle, 2), VerseValue::Covered { .. }),
        "the span is made"
    );
    let after_span = detached_alignments(&handle);
    // The waiting save goes through now.
    let late = save_alignment(&handle, waiting.0, waiting.1, "hope", "in");
    handle.wait_idle();
    let detached = detached_alignments(&handle);
    assert!(
        detached.is_empty(),
        "no alignment record of the removed verse is orphaned (after the span alone: \
         {after_span:?}; after the late save: {detached:?}; late save: {late:?})"
    );
    handle.close().unwrap();
    let handle = store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    assert_eq!(
        detached_alignments(&handle),
        Vec::<RecordKey>::new(),
        "none after the project opens again"
    );
    handle.close().unwrap();
}
