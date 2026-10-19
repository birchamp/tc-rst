//! `wire` (unit B1b): tokens cross the wire as their own kind only, a fold's delta becomes
//! the scope of a `Changed` event part by part, and the seam's errors become tC4's.

use tc_api::check::{CheckListRef, CheckUnit, Tool};
use tc_api::error::Feature;
use tc_api::event::{Aspect, ChangeCause, ChangeParts, ChangeScope, ChapterRef};
use tc_api::ids::StoryNo as WireStory;
use tc_api::refusal::{LockHolder, OpenRefusal};
use tc_api::shown::{PersonLabel, Who};
use tc_api::write::ReadOnlyReason;
use tc_api::{ApiError, NotSaved, Refusal};
use tc_app::backend::memory::{MEMORY_DEVICE, teammates};
use tc_app::backend::{CommandError, OpenFailure, ProjectStore, StoreCause, Stored};
use tc_app::jobs::ProgressSink;
use tc_app::wire::error::{command_error, open_failure, stored};
use tc_app::wire::scope::{
    change_cause, change_scope, person_label, project_id, record_key, record_ref,
};
use tc_app::wire::token::{edit_token, review, review_token, shown, token_update};
use tc_journal::fold::{Delta, PersonLabel as Label};
use tc_journal::keys::{MetaField, PinGroup, ResourcesKey, ResourcesLevel, SettingKey, UnknownKey};
use tc_journal::record::CommentTarget;
use tc_journal::token::TokenError;
use tc_journal::{
    BatchId, BookId, ChapterNo, CheckId, CheckKey, CheckTarget, CommentId, DeviceId, FrameNo,
    RecordKey, RefPart, RepoPath, StoryNo, ToolId, VerseNo,
};

use super::common::{folder, sample_backend, tit};

fn progress() -> ProgressSink {
    ProgressSink::new(tc_api::jobs::Phase::Open(
        tc_api::jobs::OpenPhase::ReadingChanges,
    ))
}

fn ch(n: u16) -> ChapterNo {
    ChapterNo::new(n).unwrap()
}

fn vn(n: u16) -> VerseNo {
    VerseNo::new(n).unwrap()
}

fn check(tool: ToolId, target: CheckTarget) -> CheckKey {
    CheckKey {
        tool,
        check_id: CheckId::parse("abc1").unwrap(),
        target,
        occurrence: 1,
    }
}

fn verse_target(book: BookId, chapter: &str, verse: &str) -> CheckTarget {
    CheckTarget::Verse {
        book,
        chapter: RefPart::parse(chapter).unwrap(),
        verse: RefPart::parse(verse).unwrap(),
    }
}

fn story(n: u8) -> StoryNo {
    StoryNo::new(n).unwrap()
}

fn wire_story(n: u8) -> WireStory {
    WireStory::new(n).unwrap()
}

fn delta_of(keys: Vec<RecordKey>) -> Delta {
    Delta {
        keys,
        ..Delta::default()
    }
}

/// The parts of a delta of `keys`, with no writer named and no comment readable.
fn parts(keys: Vec<RecordKey>) -> ChangeParts {
    match change_scope(&delta_of(keys), false, |_| None, |_| None) {
        ChangeScope::Parts(parts) => parts,
        ChangeScope::Everything => panic!("everything"),
    }
}

fn chapter(book: BookId, chapter: u16) -> ChapterRef {
    ChapterRef { book, chapter }
}

#[tokio::test]
async fn edit_and_review_tokens_cross_the_wire_with_their_prefixes() {
    let backend = sample_backend();
    let handle = backend.open(&folder("sample_burrito"), progress()).unwrap();
    let snapshot = handle.snapshot();
    let edit = snapshot.mint(&tit(1, 1), None).unwrap();
    let rt = snapshot.mint_review(&tit(1, 1)).unwrap();

    let edit_json = serde_json::to_string(&shown(&edit).unwrap()).unwrap();
    let review_json = serde_json::to_string(&review(&rt).unwrap()).unwrap();
    assert!(edit_json.starts_with("\"st1."), "{edit_json}");
    assert!(review_json.starts_with("\"rt1."), "{review_json}");

    let edit_back = edit_token(
        handle.as_ref(),
        &serde_json::from_str::<tc_api::ShownToken>(&edit_json).unwrap(),
    )
    .unwrap();
    assert_eq!(edit_back.key(), &tit(1, 1));
    assert_eq!(edit_back.heads(), edit.heads());
    let review_back = review_token(
        handle.as_ref(),
        &serde_json::from_str::<tc_api::ReviewToken>(&review_json).unwrap(),
    )
    .unwrap();
    assert_eq!(review_back.key(), &tit(1, 1));
    assert_eq!(review_back.heads(), rt.heads());

    let update = token_update(&tit(1, 1), &edit).unwrap();
    assert_eq!(update.record, record_ref(&tit(1, 1)).unwrap());
    assert_eq!(update.token, shown(&edit).unwrap());
}

#[tokio::test]
async fn a_relabelled_review_token_is_bad_token() {
    let backend = sample_backend();
    let handle = backend.open(&folder("sample_burrito"), progress()).unwrap();
    let snapshot = handle.snapshot();
    let rt = review(&snapshot.mint_review(&tit(1, 1)).unwrap()).unwrap();
    let edit = shown(&snapshot.mint(&tit(1, 1), None).unwrap()).unwrap();

    // The review token's body under the edit prefix reads as JSON, and its MAC is good: only
    // the kind under the MAC (R19) refuses it.
    let relabelled = tc_api::ShownToken::from_body(rt.body()).unwrap();
    assert_eq!(
        edit_token(handle.as_ref(), &relabelled).unwrap_err(),
        ApiError::NotSaved(NotSaved::BadToken)
    );
    assert!(matches!(
        handle.parse_edit(&relabelled),
        Err(TokenError::Malformed(reason)) if reason.contains("review token where an edit token")
    ));
    let relabelled = tc_api::ReviewToken::from_body(edit.body()).unwrap();
    assert_eq!(
        review_token(handle.as_ref(), &relabelled).unwrap_err(),
        ApiError::NotSaved(NotSaved::BadToken)
    );
}

#[tokio::test]
async fn change_scope_of_two_teammates_editing_titus_1_1() {
    let backend = sample_backend();
    let project = folder("sample_burrito");
    let handle = backend.open(&project, progress()).unwrap();
    let before = handle.snapshot();
    let changes = backend
        .teammates_edit_verse(&project, &tit(1, 1), &["one", "two"])
        .unwrap();
    let delta = backend.teammate_edits(&project, changes).unwrap();
    let after = handle.snapshot();
    let me = DeviceId::local(MEMORY_DEVICE);
    let labels = after.fold().person_labels(me);
    let by = |key: &RecordKey| {
        let change = after.fold().projected_change(key)?;
        let device = after.fold().change(&change)?.device;
        labels.get(&device).map(person_label)
    };
    let conflicts_changed = before.fold().conflict_counts() != after.fold().conflict_counts();
    let scope = change_scope(&delta, conflicts_changed, by, |_| None);
    let ChangeScope::Parts(parts) = scope else {
        panic!("{scope:?}")
    };
    assert_eq!(parts.chapters, [chapter(BookId::Titus, 1)]);
    assert!(parts.books.is_empty() && parts.stories.is_empty() && parts.checks.is_empty());
    assert_eq!(parts.aspects, [Aspect::Conflicts]);
    assert_eq!(parts.records.len(), 1);
    assert_eq!(parts.records[0].record, record_ref(&tit(1, 1)).unwrap());
    // The later of the two is the provisional head the screen shows (§7.2 F7).
    assert_eq!(
        parts.records[0].by,
        Some(PersonLabel {
            who: Who::Teammate(2),
            computer: None
        })
    );
    let cause = change_cause(
        &StoreCause::Intake {
            devices: vec![teammates::device(1), teammates::device(2)],
        },
        |device| labels.get(device).map(person_label),
    );
    assert_eq!(
        cause,
        ChangeCause::Intake {
            from: vec![
                PersonLabel {
                    who: Who::Teammate(1),
                    computer: None
                },
                PersonLabel {
                    who: Who::Teammate(2),
                    computer: None
                },
            ]
        }
    );
}

#[test]
fn every_record_kind_maps_to_its_change_part() {
    let tit = BookId::Titus;
    let books = |key| parts(vec![key]).books;
    assert_eq!(books(RecordKey::BookPresence(tit)), [tit]);
    assert_eq!(books(RecordKey::BookHeader(tit)), [tit]);
    assert_eq!(books(RecordKey::BookLayout(tit)), [tit]);

    let chapters = |key| parts(vec![key]).chapters;
    assert_eq!(
        chapters(RecordKey::ChapterFront(tit, ch(2))),
        [chapter(tit, 2)]
    );
    assert_eq!(
        chapters(RecordKey::Verse(tit, ch(2), vn(3))),
        [chapter(tit, 2)]
    );
    assert_eq!(
        chapters(RecordKey::Alignment(tit, ch(3), vn(1))),
        [chapter(tit, 3)]
    );

    let tn = parts(vec![RecordKey::Check(check(
        ToolId::TranslationNotes,
        verse_target(tit, "1", "4"),
    ))]);
    assert_eq!(
        tn.checks,
        [CheckListRef {
            tool: Tool::TranslationNotes,
            unit: CheckUnit::Book(tit)
        }]
    );
    assert!(tn.books.is_empty() && tn.chapters.is_empty());
    let tw_story = parts(vec![RecordKey::Check(check(
        ToolId::TranslationWords,
        CheckTarget::Frame {
            story: story(3),
            frame: FrameNo(4),
        },
    ))]);
    assert_eq!(
        tw_story.checks,
        [CheckListRef {
            tool: Tool::TranslationWords,
            unit: CheckUnit::Stories
        }]
    );
    assert_eq!(tw_story.stories, [wire_story(3)]);

    let aspects = |key| parts(vec![key]).aspects;
    assert_eq!(aspects(RecordKey::Pins(PinGroup::Primary)), [Aspect::Pins]);
    assert_eq!(
        aspects(RecordKey::ResourcesExtra(
            ResourcesKey::new(ResourcesLevel::Top, "custom").unwrap()
        )),
        [Aspect::Pins]
    );
    assert_eq!(
        aspects(RecordKey::Setting(SettingKey::parse("fontSize").unwrap())),
        [Aspect::Settings]
    );
    assert_eq!(
        aspects(RecordKey::ProjectMeta(MetaField::Name)),
        [Aspect::Meta]
    );
    assert_eq!(
        aspects(RecordKey::DeviceProfile(teammates::device(1))),
        [Aspect::Conflicts]
    );

    let stories = |key| parts(vec![key]).stories;
    assert_eq!(stories(RecordKey::StoryBase(story(7))), [wire_story(7)]);
    assert_eq!(
        stories(RecordKey::Frame(story(7), FrameNo(2))),
        [wire_story(7)]
    );
    assert_eq!(stories(RecordKey::StoryRef(story(8))), [wire_story(8)]);

    // The record alone: no screen of phase 1 shows these.
    for key in [
        RecordKey::CarriedFile(RepoPath::parse("ingredients/audio/a.mp3").unwrap()),
        RecordKey::BatchRelease(BatchId::from_bytes([3; 16])),
        RecordKey::Unknown(UnknownKey {
            kind: "future".into(),
            key_json: "{\"kind\":\"future\"}".into(),
        }),
    ] {
        let parts = parts(vec![key.clone()]);
        assert_eq!(
            parts,
            ChangeParts {
                records: parts.records.clone(),
                ..ChangeParts::default()
            },
            "{key:?}"
        );
        assert_eq!(parts.records.len(), 1, "{key:?}");
    }
}

#[test]
fn a_comment_marks_where_it_is() {
    let comment = RecordKey::Comment(CommentId::from_bytes([9; 16]));
    let on = |target: CommentTarget| match change_scope(
        &delta_of(vec![comment.clone()]),
        false,
        |_| None,
        |_| Some(target.clone()),
    ) {
        ChangeScope::Parts(parts) => parts,
        ChangeScope::Everything => panic!("everything"),
    };
    let verse = on(CommentTarget::Verse {
        book: BookId::Jonah,
        chapter: ch(2),
        verse: vn(1),
    });
    assert_eq!(verse.chapters, [chapter(BookId::Jonah, 2)]);
    assert_eq!(verse.records.len(), 1);
    let on_check = on(CommentTarget::Check(check(
        ToolId::TranslationNotes,
        verse_target(BookId::Jonah, "1", "1"),
    )));
    assert_eq!(
        on_check.checks,
        [CheckListRef {
            tool: Tool::TranslationNotes,
            unit: CheckUnit::Book(BookId::Jonah)
        }]
    );
    let on_frame = on(CommentTarget::Frame {
        story: story(5),
        frame: FrameNo(1),
    });
    assert_eq!(on_frame.stories, [wire_story(5)]);
    // A comment whose place cannot be read is a refresh of everything, never a scope that
    // could miss it.
    assert_eq!(
        change_scope(&delta_of(vec![comment]), false, |_| None, |_| None),
        ChangeScope::Everything
    );
}

#[test]
fn a_first_record_in_a_book_does_not_mark_the_book_changed() {
    // tc-journal names a book in `Delta.books` when one of its keys becomes active for the
    // first time: here, a first decision. The book did not change as a whole, so no rail of
    // the book is fetched again for it.
    let key = RecordKey::Check(check(
        ToolId::TranslationNotes,
        verse_target(BookId::Titus, "1", "4"),
    ));
    let delta = Delta {
        keys: vec![key],
        books: vec![BookId::Titus],
        ..Delta::default()
    };
    let ChangeScope::Parts(parts) = change_scope(&delta, false, |_| None, |_| None) else {
        panic!("everything")
    };
    assert!(parts.books.is_empty(), "{:?}", parts.books);
    assert_eq!(
        parts.checks,
        [CheckListRef {
            tool: Tool::TranslationNotes,
            unit: CheckUnit::Book(BookId::Titus)
        }]
    );
}

#[test]
fn more_than_256_records_is_everything() {
    let keys = |n: u16| -> Vec<RecordKey> {
        (1..=n)
            .map(|i| RecordKey::Verse(BookId::Psalms, ch(1 + i / 100), vn(1 + i % 100)))
            .collect()
    };
    assert!(matches!(
        change_scope(&delta_of(keys(256)), false, |_| None, |_| None),
        ChangeScope::Parts(parts) if parts.records.len() == 256
    ));
    assert_eq!(
        change_scope(&delta_of(keys(257)), false, |_| None, |_| None),
        ChangeScope::Everything
    );
}

#[test]
fn a_versification_change_is_everything() {
    assert_eq!(
        change_scope(
            &delta_of(vec![tit(1, 1), RecordKey::Versification]),
            false,
            |_| None,
            |_| None
        ),
        ChangeScope::Everything
    );
}

#[test]
fn everything_stays_everything() {
    let delta = Delta {
        keys: vec![tit(1, 1)],
        everything: true,
        ..Delta::default()
    };
    assert_eq!(
        change_scope(&delta, false, |_| None, |_| None),
        ChangeScope::Everything
    );
}

#[test]
fn a_resolution_marks_the_conflict_list() {
    let ChangeScope::Parts(parts) =
        change_scope(&delta_of(vec![tit(1, 1)]), true, |_| None, |_| None)
    else {
        panic!("everything")
    };
    assert_eq!(parts.aspects, [Aspect::Conflicts]);
    let pins = Delta {
        pins: true,
        ..Delta::default()
    };
    let ChangeScope::Parts(parts) = change_scope(&pins, false, |_| None, |_| None) else {
        panic!("everything")
    };
    assert_eq!(parts.aspects, [Aspect::Pins]);
}

#[test]
fn a_translation_questions_check_lists_its_record_and_no_check_list() {
    let key = RecordKey::Check(check(
        ToolId::TranslationQuestions,
        verse_target(BookId::Titus, "1", "1"),
    ));
    let parts = parts(vec![key.clone()]);
    assert!(parts.checks.is_empty());
    assert_eq!(parts.records.len(), 1);
    assert_eq!(parts.records[0].record, record_ref(&key).unwrap());
}

#[test]
fn command_errors_map_to_not_saved() {
    assert_eq!(
        command_error(CommandError::NotWritten("full".into())),
        ApiError::NotSaved(NotSaved::Disk("full".into()))
    );
    assert_eq!(
        command_error(CommandError::ReadOnly(ReadOnlyReason::SecondWriter)),
        ApiError::NotSaved(NotSaved::ReadOnly(ReadOnlyReason::SecondWriter))
    );
    assert_eq!(
        command_error(CommandError::BadToken(TokenError::BadMac)),
        ApiError::NotSaved(NotSaved::BadToken)
    );
    assert_eq!(
        command_error(CommandError::Closed),
        ApiError::NotSaved(NotSaved::StaleScope)
    );
    assert_eq!(
        command_error(CommandError::Unavailable),
        ApiError::Unavailable(Feature::Writes)
    );
}

#[test]
fn open_failures_map_to_open_refusals() {
    let open = |r| ApiError::Refused(Refusal::Open(r));
    assert_eq!(
        open_failure(OpenFailure::NotAProject),
        open(OpenRefusal::NotAProject)
    );
    assert_eq!(
        open_failure(OpenFailure::Unreadable {
            reason: "no access".into()
        }),
        open(OpenRefusal::Unreadable {
            reason: "no access".into()
        })
    );
    assert_eq!(
        open_failure(OpenFailure::LockHeld {
            holder: Some(("laptop".into(), 42))
        }),
        open(OpenRefusal::LockHeld {
            holder: Some(LockHolder {
                host: "laptop".into(),
                pid: 42
            })
        })
    );
    assert_eq!(
        open_failure(OpenFailure::LockHeld { holder: None }),
        open(OpenRefusal::LockHeld { holder: None })
    );
    assert_eq!(
        open_failure(OpenFailure::NewerFormat { major: 2 }),
        open(OpenRefusal::NewerFormat { format: 2 })
    );
    assert_eq!(
        open_failure(OpenFailure::Unavailable),
        ApiError::Unavailable(Feature::Writes)
    );
}

#[test]
fn stored_maps_to_the_ack_fields() {
    use tc_api::write::Stored as AckStored;
    assert_eq!(stored(&Stored::Durable), (AckStored::Written, None));
    assert_eq!(
        stored(&Stored::WrittenNotDurable("fsync".into())),
        (AckStored::Written, Some("fsync".into()))
    );
    assert_eq!(stored(&Stored::Unchanged), (AckStored::Unchanged, None));
}

#[test]
fn person_labels_keep_mine_names_and_numbers() {
    let who = |label| person_label(&label).who;
    assert_eq!(who(Label::Mine), Who::Mine);
    assert_eq!(
        who(Label::Username("juan".into())),
        Who::Username("juan".into())
    );
    assert_eq!(
        who(Label::DisplayName("Maria".into())),
        Who::DisplayName("Maria".into())
    );
    assert_eq!(who(Label::Teammate(3)), Who::Teammate(3));
    assert_eq!(who(Label::Teammate(70_000)), Who::Teammate(u16::MAX));
    assert_eq!(person_label(&Label::Mine).computer, None);
}

#[test]
fn a_record_key_survives_the_round_trip() {
    let project = super::common::sample_bible();
    let id = project_id(project.id);
    assert_eq!(id.as_str(), project.id.to_string());
    let keys = [
        tit(1, 1),
        RecordKey::BookPresence(BookId::Titus),
        RecordKey::BookHeader(BookId::Jonah),
        RecordKey::ChapterFront(BookId::Titus, ch(2)),
        RecordKey::Alignment(BookId::Titus, ch(1), vn(1)),
        RecordKey::Check(check(
            ToolId::TranslationWords,
            verse_target(BookId::Titus, "front", "intro"),
        )),
        RecordKey::Comment(CommentId::from_bytes([1; 16])),
        RecordKey::Pins(PinGroup::OriginalLanguage),
        RecordKey::Setting(SettingKey::parse("checkCategories").unwrap()),
        RecordKey::ProjectMeta(MetaField::Languages),
        RecordKey::Versification,
        RecordKey::StoryBase(story(50)),
        RecordKey::Frame(story(1), FrameNo(0)),
    ];
    for key in keys {
        let record = record_ref(&key).unwrap();
        assert_eq!(record_key(&record).unwrap(), key, "{record:?}");
    }
    assert!(matches!(
        record_key(&tc_api::ids::RecordRef::parse("{\"kind\":\"nothing\"}").unwrap()),
        Err(ApiError::BadRequest { .. })
    ));
}
