//! The result of one sync (X10, unit B10): the port of tC4's `test/syncButton.test.tsx`
//! (its result half; the button's is in `home.rs`, the question's in `internet.rs`) and of
//! `test/actorLabel.test.ts`, and the closed set of the refusals a sync can end with, each
//! drawn with its own sentence and never with Door43's words or a path (Q36).

use tc_api::ApiError;
use tc_api::error::Feature;
use tc_api::ids::{ProjectRef, ReportId};
use tc_api::jobs::SyncResultView;
use tc_api::refusal::{Message, Refusal, SyncBlock, SyncFailure, SyncRefusal};
use tc_api::shown::{PersonLabel, Who};
use tc_ui::app::sync::{Dot, SyncEnd, SyncToast, failure_text, person_label};
use tc_ui::i18n::{keys, t};

fn project() -> ProjectRef {
    ProjectRef::parse("equipo").unwrap()
}

fn label(who: Who) -> PersonLabel {
    PersonLabel {
        who,
        computer: None,
    }
}

/// tC4's `okSync()`: 3 + 2 teammate changes, one conflict that kept the newest version, two
/// verse conflicts, one change sent again.
fn ok_sync() -> SyncResultView {
    SyncResultView {
        received: 5,
        from: vec![
            label(Who::Teammate(1)),
            label(Who::Username("maria".into())),
        ],
        kept_newest: 1,
        verse_conflicts: 2,
        alignments: 0,
        restored: 1,
        missing_pins: 0,
        replaced: Vec::new(),
    }
}

fn synced(view: SyncResultView) -> SyncToast {
    SyncToast {
        project: project(),
        end: SyncEnd::Synced(view),
    }
}

fn failed(error: ApiError) -> SyncToast {
    SyncToast {
        project: project(),
        end: SyncEnd::Failed(error),
    }
}

fn counts(toast: &SyncToast) -> Vec<(&'static str, u32)> {
    toast
        .counts()
        .unwrap()
        .iter()
        .map(|c| (c.test_id, c.n))
        .collect()
}

fn notes(toast: &SyncToast) -> Vec<(&'static str, u32)> {
    toast.notes().iter().map(|c| (c.test_id, c.n)).collect()
}

#[test]
fn three_counts() {
    let toast = synced(ok_sync());
    assert_eq!(
        counts(&toast),
        [
            ("sync-count-received", 5),
            ("sync-count-newest", 1),
            ("sync-count-conflicts", 2)
        ]
    );
    assert_eq!(notes(&toast), [("sync-restored", 1)]);
    assert_eq!(toast.failure(), None);
    assert_eq!(toast.code(), "");
    let words: Vec<String> = toast.counts().unwrap().map(|c| c.text).into();
    assert_eq!(
        words,
        [
            "5 changes from teammates",
            "1 conflict kept the newest version",
            "2 verse conflicts to review"
        ]
    );
}

#[test]
fn a_sync_with_no_teammate_change_still_gives_all_three_as_0_and_no_restored_line() {
    let toast = synced(SyncResultView {
        received: 0,
        from: Vec::new(),
        kept_newest: 0,
        verse_conflicts: 0,
        alignments: 0,
        restored: 0,
        missing_pins: 0,
        replaced: Vec::new(),
    });
    assert_eq!(
        counts(&toast),
        [
            ("sync-count-received", 0),
            ("sync-count-newest", 0),
            ("sync-count-conflicts", 0)
        ]
    );
    assert!(notes(&toast).is_empty());
    assert!(!toast.review());
    assert_eq!(toast.from(), None);
    assert_eq!(toast.dot(), Dot::Valid);
}

#[test]
fn link_to_review() {
    assert!(synced(ok_sync()).review());
    let none = synced(SyncResultView {
        verse_conflicts: 0,
        ..ok_sync()
    });
    assert!(!none.review());
}

/// #791: a verse with two alignments is named under the line, and Review opens it.
#[test]
fn a_verse_with_two_alignments_is_named_and_offers_review() {
    let toast = synced(SyncResultView {
        verse_conflicts: 0,
        restored: 0,
        alignments: 1,
        ..ok_sync()
    });
    let notes = toast.notes();
    assert_eq!(
        notes.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(),
        ["1 verse has two alignments to review."]
    );
    assert!(toast.review());
    assert_eq!(toast.dot(), Dot::Kindle);
}

#[test]
fn a_version_this_computer_lacks_offers_download() {
    let toast = synced(SyncResultView {
        missing_pins: 2,
        replaced: Vec::new(),
        ..ok_sync()
    });
    assert_eq!(
        notes(&toast),
        [("sync-restored", 1), ("sync-missing-pins", 2)]
    );
    assert!(toast.download());
    let none = synced(ok_sync());
    assert!(!none.download());
    // tC4's capture `sync-ok`: nothing to review, but a download: the dot is Kindle.
    let only_pins = synced(SyncResultView {
        received: 0,
        from: Vec::new(),
        kept_newest: 0,
        verse_conflicts: 0,
        alignments: 0,
        restored: 0,
        missing_pins: 5,
        replaced: Vec::new(),
    });
    assert_eq!(only_pins.dot(), Dot::Kindle);
    assert_eq!(
        only_pins.notes()[0].text,
        "This project now uses 5 resource versions that are not on this computer."
    );
}

/// The closed set of what a sync can end with: a match with no catch-all, so a new variant
/// fails to compile here until it has its sample.
fn every_failure() -> Vec<SyncFailure> {
    let refusals = |r: &SyncRefusal| match r {
        SyncRefusal::Offline
        | SyncRefusal::NotSignedIn
        | SyncRefusal::AuthFailed
        | SyncRefusal::NoWriteAccess { .. }
        | SyncRefusal::RefRejected { .. }
        | SyncRefusal::Busy { .. }
        | SyncRefusal::RemoteMissing
        | SyncRefusal::Unreachable
        | SyncRefusal::ReadOnly
        | SyncRefusal::Io { .. } => (),
    };
    let blocks = |b: &SyncBlock| match b {
        SyncBlock::NotShared
        | SyncBlock::ProjectMismatch
        | SyncBlock::NeedsNewerApp { .. }
        | SyncBlock::Tc4TeamRepository => (),
    };
    let door43 = "protected branch hook declined";
    let path = "/home/ana/translationCore4/projects/equipo/.git/index.lock";
    let steps = [
        SyncRefusal::Offline,
        SyncRefusal::NotSignedIn,
        SyncRefusal::AuthFailed,
        SyncRefusal::NoWriteAccess {
            server_message: door43.into(),
        },
        SyncRefusal::RefRejected {
            server_message: door43.into(),
        },
        SyncRefusal::Busy { attempts: 6 },
        SyncRefusal::RemoteMissing,
        SyncRefusal::Unreachable,
        SyncRefusal::ReadOnly,
        SyncRefusal::Io {
            reason: format!("{path}: No space left on device (os error 28)"),
        },
    ];
    let stops = [
        SyncBlock::NotShared,
        SyncBlock::ProjectMismatch,
        SyncBlock::NeedsNewerApp {
            major: 3,
            min_reader_minor: 0,
        },
        SyncBlock::Tc4TeamRepository,
    ];
    steps.iter().for_each(refusals);
    stops.iter().for_each(blocks);
    let mut all: Vec<SyncFailure> = stops.into_iter().map(SyncFailure::Blocked).collect();
    all.extend(steps.iter().cloned().map(SyncFailure::BeforeIntake));
    all.extend(steps.into_iter().map(SyncFailure::AfterIntake));
    all
}

/// tC4's "one message per code": every failure shows its own sentence from the catalogs,
/// with its code on the toast, no counts, and none of Door43's words or a path; two codes
/// never read the same.
#[test]
fn every_sync_refusal_shows_its_own_sentence_never_door43s_words_or_a_path() {
    let mut by_text: std::collections::BTreeMap<String, Option<&'static str>> =
        std::collections::BTreeMap::new();
    for failure in every_failure() {
        let toast = failed(ApiError::Refused(Refusal::Sync(failure.clone())));
        assert_eq!(toast.dot(), Dot::Invalid);
        assert_eq!(toast.counts(), None);
        assert!(toast.notes().is_empty() && toast.from().is_none());
        assert!(!toast.review() && !toast.download());
        assert_eq!(toast.code(), failure.code().unwrap_or(""));
        let text = toast.failure().unwrap();
        assert!(!text.is_empty() && text != failure.key(), "{failure:?}");
        assert!(!text.contains('{'), "{failure:?}: {text}");
        for words in ["hook declined", "/home/", "os error"] {
            assert!(!text.contains(words), "{failure:?}: {text}");
        }
        if let (Some(Some(before)), Some(code)) =
            (by_text.insert(text.clone(), failure.code()), failure.code())
        {
            assert_eq!(before, code, "two codes read {text:?}");
        }
    }
    let write = failed(ApiError::Refused(Refusal::Sync(SyncFailure::BeforeIntake(
        SyncRefusal::NoWriteAccess {
            server_message: "x".into(),
        },
    ))));
    assert_eq!(
        write.failure().unwrap(),
        "Ask the project owner for write access on Door43."
    );
    let clone = failed(ApiError::Refused(Refusal::Sync(SyncFailure::BeforeIntake(
        SyncRefusal::Unreachable,
    ))));
    assert_eq!(clone.code(), "sync.clone-failed");
    assert_eq!(
        clone.failure().unwrap(),
        "tC4 could not get the project from Door43. Nothing changed. Try again."
    );
}

/// tC4's "a failure with no code": any other end shows one fixed sentence, with no reason.
#[test]
fn a_failure_with_no_refusal_shows_one_fixed_sentence() {
    let fixed = t(keys::sync_result::error::UNEXPECTED, &[]);
    for error in [
        ApiError::Unavailable(Feature::Sync),
        ApiError::Internal {
            report: ReportId::new(4),
        },
        ApiError::BadRequest {
            field: "door43/sync".into(),
            reason: "/home/ana/x: denied".into(),
        },
    ] {
        assert_eq!(failure_text(&error), fixed);
        assert_eq!(failed(error).code(), "");
    }
}

/// #680: the senders by the review's labels, "Changes from: Teammate 1 · maria"; nothing with
/// no teammate change or for a sync that did not complete.
#[test]
fn the_result_names_each_teammate_as_the_review_does() {
    assert_eq!(
        synced(ok_sync()).from().unwrap(),
        "Changes from: Teammate 1 · maria"
    );
    let none = synced(SyncResultView {
        received: 0,
        from: Vec::new(),
        ..ok_sync()
    });
    assert_eq!(none.from(), None);
    assert_eq!(failed(ApiError::Unavailable(Feature::Sync)).from(), None);
}

/// The port of `test/actorLabel.test.ts`: "Mine", the Door43 name, else "Teammate n".
#[test]
fn one_label_for_a_person() {
    assert_eq!(person_label(&label(Who::Mine)), "Mine");
    assert_eq!(
        person_label(&label(Who::Username("door43-c".into()))),
        "door43-c"
    );
    assert_eq!(person_label(&label(Who::Teammate(2))), "Teammate 2");
}

/// INV-NO-SILENT-REPLACEMENT (owner ruling H5): every version a sync replaced with no question
/// is named on the Received sheet, with the person, the place and where the older version is;
/// the person's own alignment of one verse is tC4's sentence.
#[test]
fn the_received_sheet_names_each_replaced_version() {
    use tc_api::check::{CheckPlace, Tool};
    use tc_api::conflicts::ReviewPlace;
    use tc_api::jobs::{ReplacedKind, ReplacedValue, ReplacedView};
    use tc_core::BookId;
    let titus_1 = |verse| ReviewPlace::Verse {
        book: BookId::Titus,
        chapter: 1,
        verse,
    };
    let row = |whose, kind, place, count| ReplacedView {
        by: label(Who::Username("juan".into())),
        whose: label(whose),
        kind,
        place,
        count,
        was: None,
    };
    let check = ReviewPlace::Check {
        tool: Some(Tool::TranslationNotes),
        place: CheckPlace::Verse {
            book: BookId::Titus,
            chapter: tc_api::ids::RefPart::parse("1").unwrap(),
            verse: tc_api::ids::RefPart::parse("4").unwrap(),
        },
    };
    let toast = synced(SyncResultView {
        replaced: vec![
            row(Who::Mine, ReplacedKind::Alignment, titus_1(1), 1),
            row(Who::Mine, ReplacedKind::Alignment, titus_1(2), 3),
            row(Who::Teammate(2), ReplacedKind::Check, check, 1),
            row(Who::Mine, ReplacedKind::Setting, titus_1(1), 1),
            ReplacedView {
                was: Some(ReplacedValue::Font("Noto Sans".into())),
                ..row(Who::Mine, ReplacedKind::Setting, titus_1(1), 1)
            },
            ReplacedView {
                was: Some(ReplacedValue::Direction(tc_api::shown::Direction::Rtl)),
                ..row(Who::Mine, ReplacedKind::Setting, titus_1(1), 1)
            },
            ReplacedView {
                was: Some(ReplacedValue::Resources(vec![
                    "en_tn v90".into(),
                    "en_ta v90".into(),
                ])),
                ..row(Who::Mine, ReplacedKind::Resources, titus_1(1), 1)
            },
            row(
                Who::Mine,
                ReplacedKind::Book,
                ReviewPlace::Book {
                    book: BookId::Jonah,
                },
                1,
            ),
        ],
        ..ok_sync()
    });
    let rows: Vec<(&str, u32, String)> = toast
        .replaced()
        .into_iter()
        .map(|r| (r.test_id, r.n, r.text))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("sync-replaced", 1, "Titus 1:1: juan's alignment replaced yours. Your version is in History.".to_owned()),
            ("sync-replaced", 3, "Titus 1:2 and 2 more: juan's alignments replaced yours. Your version is in History.".to_owned()),
            ("sync-replaced", 1, "Titus 1:4 · Translation Notes: juan's check replaced Teammate 2's. The older version is in History.".to_owned()),
            ("sync-replaced", 1, "juan's project settings replaced yours. Settings can set it back.".to_owned()),
            ("sync-replaced", 1, "juan's text font replaced yours (was Noto Sans). Settings can set it back.".to_owned()),
            ("sync-replaced", 1, "juan's text direction replaced yours (was Right to left). Settings can set it back.".to_owned()),
            ("sync-replaced", 1, "juan's resource choices replaced yours (was en_tn v90, en_ta v90). Settings can set it back.".to_owned()),
            ("sync-replaced", 1, "Jonah: juan removed this book.".to_owned()),
        ]
    );
    assert!(synced(ok_sync()).replaced().is_empty());
}
