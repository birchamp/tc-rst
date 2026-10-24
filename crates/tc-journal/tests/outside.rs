//! Outside edits (`docs/STORAGE.md` §11.1 X2 and X3) as the fold's half: a file decoded and
//! compared with what the app wrote, one test for each rule of `plan_outside_edit`. The
//! store's half (reading the bytes, the copy, the batch) is `tc-store`'s tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::*;
use tc_journal::change::ExternalSource;
use tc_journal::command::{Choice, OutsideKeep};
use tc_journal::fold::{
    AutoRule, ConflictKind, DerivedFile, FoldInput, FoldState, ProjectKind, Resolved,
};
use tc_journal::outside::{Base, OutsideFile, OutsidePlan, base_records, decode_file};
use tc_journal::record::{CarriedValue, VerseValue};
use tc_journal::{Cause, Change, Md5, Record, RecordKey, RepoPath};

fn path() -> RepoPath {
    RepoPath::parse("ingredients/TIT.usfm").unwrap()
}

fn source(bytes: &[u8]) -> ExternalSource {
    ExternalSource::LocalFile {
        path: path(),
        seen_md5: Md5::of(bytes),
    }
}

fn usfm_file() -> OutsideFile {
    OutsideFile::Derived(DerivedFile::Usfm(BOOK))
}

/// A book of the given chapters, drafted from the start, and what the app writes for it.
struct World {
    changes: Vec<Change>,
    dev: Dev,
}

impl World {
    fn new(chapters: &[u16]) -> Self {
        let mut dev = Dev::new(1);
        let changes = imported_book(&mut dev, chapters);
        Self { changes, dev }
    }

    fn state(&self) -> FoldState {
        fold(&self.changes)
    }

    /// The bytes the app wrote, and the base of an edit of them.
    fn written(&self, file: &DerivedFile) -> (Vec<u8>, Base) {
        let state = self.state();
        let rendered = state.render_with_digests(file).expect("the file renders");
        let base = Base::Heads(base_records(state.file_heads(file), rendered.digests));
        (rendered.bytes, base)
    }
}

fn plan_of(state: &FoldState, bytes: &[u8], base: &Base, force: bool) -> OutsidePlan {
    let decoded = decode_file(&usfm_file(), bytes, &path()).expect("decodes");
    state.plan_outside_edit(&usfm_file(), &decoded, base, source(bytes), force)
}

fn edited(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(text.contains(from), "{from:?} is not in the file");
    text.replacen(from, to, 1).into_bytes()
}

fn words(record: &Record) -> String {
    words_of(record).expect("a verse with words")
}

#[test]
fn an_untouched_file_plans_nothing_and_looks_at_no_key() {
    let world = World::new(&[5, 5]);
    let (bytes, base) = world.written(&DerivedFile::Usfm(BOOK));
    let plan = plan_of(&world.state(), &bytes, &base, false);
    assert!(plan.changes.is_empty());
    assert_eq!(plan.keys_examined, 0, "the digests show nothing changed");
    assert_eq!(plan.whitespace_only, 0);
}

#[test]
fn an_edited_verse_is_one_change_with_the_heads_it_started_from() {
    let world = World::new(&[5, 5]);
    let state = world.state();
    let (bytes, base) = world.written(&DerivedFile::Usfm(BOOK));
    let file = edited(&bytes, "words of 1:3", "words of 1:3, fixed");
    let plan = plan_of(&state, &file, &base, false);
    assert_eq!(plan.keys_examined, 1, "only the changed key is looked at");
    assert_eq!(plan.changes.len(), 1);
    let change = &plan.changes[0];
    assert_eq!(change.record.key(), verse_key(1, 3));
    assert_eq!(words(&change.record), "words of 1:3, fixed");
    assert_eq!(change.parents, state.heads(&verse_key(1, 3)));
    assert!(matches!(
        &change.cause,
        Cause::External { unknown_base: false, source: ExternalSource::LocalFile { seen_md5, .. } }
            if *seen_md5 == Md5::of(&file)
    ));
    assert!(!plan.unknown_base);
}

#[test]
fn the_cost_of_a_plan_follows_the_changed_verses_not_the_book() {
    let small = World::new(&[8]);
    let large = World::new(&[8; 12]);
    let cost = |world: &World| {
        let state = world.state();
        let (bytes, base) = world.written(&DerivedFile::Usfm(BOOK));
        let mut file = bytes;
        for verse in ["1:2", "1:5", "1:7"] {
            file = edited(
                &file,
                &format!("words of {verse}"),
                &format!("words of {verse}!"),
            );
        }
        let plan = plan_of(&state, &file, &base, false);
        assert_eq!(plan.changes.len(), 3);
        plan.keys_examined
    };
    assert_eq!(cost(&small), 3);
    assert_eq!(cost(&large), 3);
}

#[test]
fn a_whitespace_only_rewrite_records_nothing() {
    let world = World::new(&[5, 5]);
    let state = world.state();
    let (bytes, base) = world.written(&DerivedFile::Usfm(BOOK));
    // A web editor that turns every line end into CRLF and trims lines.
    let text = String::from_utf8(bytes).unwrap().replace('\n', "  \r\n");
    let plan = plan_of(&state, text.as_bytes(), &base, false);
    assert!(plan.changes.is_empty(), "{plan:?}");
}

#[test]
fn without_digests_whitespace_differences_are_counted_not_recorded() {
    let world = World::new(&[5]);
    let state = world.state();
    let (bytes, base) = world.written(&DerivedFile::Usfm(BOOK));
    // A heads file of S1's form holds no digests: every key is looked at.
    let Base::Heads(mut records) = base else {
        unreachable!()
    };
    for record in &mut records {
        record.shown = None;
    }
    let text = String::from_utf8(bytes).unwrap().replace('\n', " \n");
    let plan = plan_of(&state, text.as_bytes(), &Base::Heads(records), false);
    assert!(plan.changes.is_empty(), "{plan:?}");
    assert!(plan.whitespace_only >= 5, "{plan:?}");
}

#[test]
fn a_fixed_typo_in_a_crlf_file_comes_back_with_crlf() {
    let mut dev = Dev::new(1);
    let mut items = vec![
        (present(tc_journal::record::PresentHow::Added), vec![]),
        (header(), vec![]),
    ];
    items.push((front(1), vec![]));
    for v in 1..=3 {
        items.push((
            verse(1, v, text_layout(&format!("crlf verse {v}"), "\r\n", v)),
            vec![],
        ));
    }
    let changes = dev.batch(items, Cause::Seed);
    let state = fold(&changes);
    let rendered = state.render_with_digests(&DerivedFile::Usfm(BOOK)).unwrap();
    let base = Base::Heads(base_records(
        state.file_heads(&DerivedFile::Usfm(BOOK)),
        rendered.digests,
    ));
    // The editor drops every CR and fixes one word.
    let text = String::from_utf8(rendered.bytes).unwrap().replace('\r', "");
    let file = text.replace("crlf verse 2", "crlf verse two");
    let plan = plan_of(&state, file.as_bytes(), &base, false);
    assert_eq!(plan.changes.len(), 1, "{plan:?}");
    match &plan.changes[0].record {
        Record::Verse {
            value: VerseValue::Text { words, layout, .. },
            ..
        } => {
            assert_eq!(words, "crlf verse two");
            assert_eq!(layout, "\r\n", "the file's line endings come back");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_verse_the_file_shows_as_a_span_keeps_what_a_file_cannot_show() {
    // A renumbered verse remembers where its text came from; USFM cannot (§2.3).
    let mut dev = Dev::new(1);
    let mut changes = imported_book(&mut dev, &[3]);
    let key = verse_key(1, 2);
    let renumbered = dev.one(
        verse(1, 2, text_from("moved words", 2, (1, 3))),
        vec![on(&changes, &key).id()],
    );
    changes.push(renumbered);
    let world = World { changes, dev };
    let state = world.state();
    let (bytes, base) = world.written(&DerivedFile::Usfm(BOOK));
    let file = edited(&bytes, "moved words", "moved words, fixed");
    let plan = plan_of(&state, &file, &base, false);
    match &plan.changes[0].record {
        Record::Verse {
            value: VerseValue::Text { from, words, .. },
            ..
        } => {
            assert_eq!(words, "moved words, fixed");
            assert!(from.is_some(), "the renumber's origin is kept");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn what_the_project_already_shows_is_not_a_change() {
    // The app wrote an old version; a crash left the file stale; the file now holds the
    // text the project already has (the second examination of the same bytes).
    let mut world = World::new(&[4]);
    let (stale, base) = world.written(&DerivedFile::Usfm(BOOK));
    let key = verse_key(1, 2);
    let newer = world.dev.one(
        verse(1, 2, text("a newer verse", 2)),
        vec![on(&world.changes, &key).id()],
    );
    world.changes.push(newer);
    let state = world.state();
    // The file is stale: the app's own older bytes. It differs from the base by nothing.
    assert!(plan_of(&state, &stale, &base, false).changes.is_empty());
    // A person copies the project's text into the stale file by hand.
    let file = edited(&stale, "words of 1:2", "a newer verse");
    let plan = plan_of(&state, &file, &base, false);
    assert!(
        plan.changes.is_empty(),
        "the project shows it already: {plan:?}"
    );
}

#[test]
fn loss_outside_mass_delete_is_conflict() {
    let world = World::new(&[30, 30]);
    let state = world.state();
    let (bytes, base) = world.written(&DerivedFile::Usfm(BOOK));
    let text = String::from_utf8(bytes).unwrap();
    let truncated = text[..text.find("\\c 2").unwrap()].to_owned();
    let plan = plan_of(&state, truncated.as_bytes(), &base, false);
    assert!(plan.unknown_base);
    assert!(plan.changes.len() >= 30, "{}", plan.changes.len());
    assert!(
        plan.changes.iter().all(|c| c.parents.is_empty()
            && matches!(
                c.cause,
                Cause::External {
                    unknown_base: true,
                    ..
                }
            )),
        "every change of a file that removes records is recorded without parents"
    );
    // Applied, every verse of chapter 2 is a conflict with the project's text, and the
    // project's text stays provisional (rank 0, F7).
    let mut after = world.changes.clone();
    let mut outside = Dev::new(9);
    after.extend(outside.apply(plan.changes));
    let after = fold(&after);
    assert!(after.conflicts().count() >= 30);
    assert_eq!(
        projected_words(&after, 2, 7).as_deref(),
        Some("words of 2:7"),
        "the project's text is what shows"
    );
}

#[test]
fn loss_outside_revert_is_conflict() {
    let mut world = World::new(&[4]);
    let (old, _) = world.written(&DerivedFile::Usfm(BOOK));
    let key = verse_key(1, 2);
    let newer = world.dev.one(
        verse(1, 2, text("a newer verse", 2)),
        vec![on(&world.changes, &key).id()],
    );
    world.changes.push(newer);
    // The app materialized the newer text; then a cloud client puts the older file back.
    let (_, base) = world.written(&DerivedFile::Usfm(BOOK));
    let state = world.state();
    let plan = plan_of(&state, &old, &base, false);
    assert_eq!(plan.changes.len(), 1, "{plan:?}");
    assert!(plan.changes[0].parents.is_empty());
    assert!(plan.unknown_base);
    assert_eq!(words(&plan.changes[0].record), "words of 1:2");
}

#[test]
fn loss_poetry_crlf_edit_records_one_key() {
    let mut dev = Dev::new(1);
    let mut items = vec![
        (present(tc_journal::record::PresentHow::Added), vec![]),
        (header(), vec![]),
        (front(1), vec![]),
    ];
    for v in 1..=4 {
        items.push((
            verse(1, v, text_layout(&format!("line {v}"), "\n\\q1\n", v)),
            vec![],
        ));
    }
    let changes = dev.batch(items, Cause::Seed);
    let state = fold(&changes);
    let rendered = state.render_with_digests(&DerivedFile::Usfm(BOOK)).unwrap();
    let base = Base::Heads(base_records(
        state.file_heads(&DerivedFile::Usfm(BOOK)),
        rendered.digests,
    ));
    let text = String::from_utf8(rendered.bytes).unwrap();
    let file = text.replace('\n', "\r\n").replace("line 3", "line three");
    let plan = plan_of(&state, file.as_bytes(), &base, false);
    assert_eq!(plan.changes.len(), 1, "{plan:?}");
    assert_eq!(plan.changes[0].record.key(), verse_key(1, 3));
}

#[test]
fn an_unknown_base_records_every_change_without_parents() {
    let world = World::new(&[5]);
    let state = world.state();
    let (bytes, _) = world.written(&DerivedFile::Usfm(BOOK));
    let file = edited(&bytes, "words of 1:3", "words of 1:3, fixed");
    let plan = plan_of(&state, &file, &Base::Unknown, false);
    assert_eq!(plan.changes.len(), 1);
    assert!(plan.changes[0].parents.is_empty());
    assert!(plan.unknown_base);
    // A known base forced unknown (the person took in a held file) does the same.
    let (_, base) = world.written(&DerivedFile::Usfm(BOOK));
    let forced = plan_of(&state, &file, &base, true);
    assert!(forced.changes[0].parents.is_empty());
    assert!(forced.unknown_base);
}

#[test]
fn more_than_fifty_changed_keys_are_recorded_without_parents() {
    let world = World::new(&[60]);
    let state = world.state();
    let (bytes, base) = world.written(&DerivedFile::Usfm(BOOK));
    let text = String::from_utf8(bytes)
        .unwrap()
        .replace("words of", "text of");
    let plan = plan_of(&state, text.as_bytes(), &base, false);
    assert_eq!(plan.changes.len(), 60);
    assert!(plan.unknown_base);
    assert!(plan.changes.iter().all(|c| c.parents.is_empty()));
}

#[test]
fn a_book_file_of_an_absent_book_adds_the_book() {
    let world = World::new(&[3]);
    let state = world.state();
    let file = OutsideFile::Derived(DerivedFile::Usfm(tc_journal::BookId::Ruth));
    let usfm = "\\id RUT\n\\c 1\n\\p\n\\v 1 In the days\n\\v 2 A man\n";
    let rut = RepoPath::parse("ingredients/RUT.usfm").unwrap();
    let decoded = decode_file(&file, usfm.as_bytes(), &rut).unwrap();
    let plan = state.plan_outside_edit(
        &file,
        &decoded,
        &Base::Heads(Vec::new()),
        ExternalSource::LocalFile {
            path: rut,
            seen_md5: Md5::of(usfm.as_bytes()),
        },
        false,
    );
    assert_eq!(plan.book_added, Some(tc_journal::BookId::Ruth));
    assert!(
        plan.changes.iter().any(|c| matches!(
            c.record,
            Record::BookPresence {
                value: tc_journal::record::BookPresence::Present {
                    how: tc_journal::record::PresentHow::Added,
                    ..
                },
                ..
            }
        ) && c.parents.is_empty()),
        "{plan:?}"
    );
    let mut all = world.changes.clone();
    let mut outside = Dev::new(9);
    all.extend(outside.apply(plan.changes));
    let after = fold(&all);
    assert!(after.projected_books().contains(&tc_journal::BookId::Ruth));
}

#[test]
fn an_outside_comment_edit_with_no_known_base_never_wins_and_stays_in_history() {
    let mut world = World::new(&[3]);
    let note = world
        .dev
        .one(comment(7, verse_target(1, 1), "first note"), vec![]);
    world.changes.push(note.clone());
    let state = world.state();
    let file = DerivedFile::Comments(BOOK);
    let bytes = state.render(&file).expect("comments");
    let base = Base::Heads(base_records(state.file_heads(&file), Vec::new()));
    let edited_bytes = edited(&bytes, "first note", "first note, edited");
    let cpath = RepoPath::parse("ingredients/tc/comments/TIT.json").unwrap();
    let outside_file = OutsideFile::Derived(file);
    let decoded = decode_file(&outside_file, &edited_bytes, &cpath).unwrap();
    let plan = state.plan_outside_edit(
        &outside_file,
        &decoded,
        &base,
        ExternalSource::LocalFile {
            path: cpath,
            seen_md5: Md5::of(&edited_bytes),
        },
        false,
    );
    assert_eq!(plan.changes.len(), 1, "{plan:?}");
    assert!(
        plan.changes[0].parents.is_empty(),
        "V6: a comment change names no parents"
    );
    assert!(plan.unknown_base);
    let mut all = world.changes.clone();
    let mut outside = Dev::new(9);
    let written = outside.apply(plan.changes);
    all.extend(written.clone());
    let after = fold(&all);
    // Ruling H5: the newest wins, but a version from outside with no known base ranks
    // below every other (F7), so the app's own note stays shown; the other is in History.
    let key = note.body().record.key();
    assert_eq!(after.conflict_count(), 0);
    assert_eq!(
        after.key_state(&key).map(|k| k.state),
        Some(Resolved::SettledByRule {
            head: note.id(),
            rule: AutoRule::Newest
        })
    );
    assert!(after.history(&key).contains(&written[0].id()));
}

#[test]
fn outside_alignment_deletion_is_kept() {
    let mut world = World::new(&[3]);
    let a = world.dev.one(alignment(1, 1, 1, "words"), vec![]);
    world.changes.push(a);
    let state = world.state();
    let file = DerivedFile::Alignments(BOOK);
    let bytes = state.render(&file).expect("alignments");
    let base = Base::Heads(base_records(state.file_heads(&file), Vec::new()));
    let mut doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    doc["chapters"] = serde_json::json!({});
    let emptied = serde_json::to_vec(&doc).unwrap();
    let apath = RepoPath::parse("ingredients/checking/alignments/TIT.json").unwrap();
    let outside_file = OutsideFile::Derived(file);
    let decoded = decode_file(&outside_file, &emptied, &apath).unwrap();
    let plan = state.plan_outside_edit(
        &outside_file,
        &decoded,
        &base,
        ExternalSource::LocalFile {
            path: apath,
            seen_md5: Md5::of(&emptied),
        },
        false,
    );
    assert!(plan.changes.is_empty(), "no change can remove an alignment");
    assert_eq!(plan.kept_removals.len(), 1);
}

#[test]
fn a_second_examination_of_the_same_bytes_records_nothing() {
    let world = World::new(&[5]);
    let state = world.state();
    let (bytes, base) = world.written(&DerivedFile::Usfm(BOOK));
    let file = edited(&bytes, "words of 1:3", "words of 1:3, fixed");
    let first = plan_of(&state, &file, &base, false);
    assert_eq!(first.changes.len(), 1);
    // The batch is durable, then the app stops before it rewrites the file: the same bytes
    // are examined again.
    let mut all = world.changes.clone();
    let mut outside = Dev::new(9);
    all.extend(outside.apply(first.changes));
    let after = fold(&all);
    let second = plan_of(&after, &file, &base, false);
    assert!(second.changes.is_empty(), "{second:?}");
}

#[test]
fn derived_paths_round_trip() {
    let mut files = vec![
        DerivedFile::Resources,
        DerivedFile::Settings,
        DerivedFile::Versification,
        DerivedFile::Conflicts,
        DerivedFile::StoryComments,
    ];
    for book in tc_journal::BookId::ALL {
        files.push(DerivedFile::Usfm(book));
        files.push(DerivedFile::Alignments(book));
        files.push(DerivedFile::Comments(book));
        for tool in tc_journal::ToolId::ALL {
            files.push(DerivedFile::Decisions(tool, book));
        }
    }
    for tool in tc_journal::ToolId::ALL {
        files.push(DerivedFile::StoryDecisions(tool));
    }
    for n in 1..=50u8 {
        files.push(DerivedFile::Story(
            tc_journal::ids::StoryNo::new(n).unwrap(),
        ));
    }
    for file in &files {
        assert_eq!(DerivedFile::from_path(&file.path()).as_ref(), Some(file));
    }
    for other in [
        "ingredients/content/7.md",
        "ingredients/content/51.md",
        "ingredients/content/00.md",
        "ingredients/TIT.USFM",
        "ingredients/XYZ.usfm",
        "ingredients/checking/other/TIT.json",
        "ingredients/audio/a.mp3",
        "metadata.json",
        "ingredients/tc/project.json",
        "ingredients/checking/translationNotes/TIT.json.bak",
        "TIT.usfm",
    ] {
        if let Some(file) = DerivedFile::from_path(other) {
            assert_eq!(file.path(), other, "{other}");
        }
    }
    assert_eq!(DerivedFile::from_path("ingredients/content/7.md"), None);
    assert_eq!(DerivedFile::from_path("ingredients/audio/a.mp3"), None);
}

#[test]
fn delta_flags_name_what_changed() {
    let mut dev = Dev::new(1);
    let mut state = FoldState::fold(ProjectKind::Bible, Vec::<FoldInput>::new());
    let book = imported_book(&mut dev, &[3]);
    let added = state.add(book.iter().cloned().map(FoldInput::from));
    assert!(
        added.paths && added.books.contains(&BOOK),
        "a book appeared"
    );
    let key = verse_key(1, 2);
    let edit = dev.one(verse(1, 2, text("an edit", 2)), vec![on(&book, &key).id()]);
    let delta = state.add([FoldInput::from(edit)]);
    assert!(!delta.paths && !delta.carried && !delta.pins);
    assert!(!delta.conflicts_changed);
    let file = dev.one(carried("ingredients/audio/a.mp3", 1), vec![]);
    let delta = state.add([FoldInput::from(file)]);
    assert!(delta.carried && !delta.paths, "{delta:?}");
    // A concurrent edit makes a conflict.
    let mut other = Dev::new(2);
    let rival = other.one(
        verse(1, 2, text("a rival edit", 2)),
        vec![on(&book, &key).id()],
    );
    let delta = state.add([FoldInput::from(rival)]);
    assert!(delta.conflicts_changed, "{delta:?}");
}

// ---------------------------------------------------------------------------------------
// Choices
// ---------------------------------------------------------------------------------------

fn two_versions_of_a_file() -> (
    Vec<Change>,
    FoldState,
    tc_journal::fold::ConflictId,
    Change,
    Change,
) {
    let mut a = Dev::new(1);
    let mut b = Dev::new(2);
    let mut changes = imported_book(&mut a, &[2]);
    let v1 = a.one(carried("ingredients/audio/a.mp3", 1), vec![]);
    let v2 = b.one(carried("ingredients/audio/a.mp3", 2), vec![]);
    changes.extend([v1.clone(), v2.clone()]);
    let state = fold(&changes);
    let conflict = state
        .conflicts_of(ConflictKind::CarriedFile)
        .next()
        .expect("two versions of a file")
        .id;
    (changes, state, conflict, v1, v2)
}

#[test]
fn keep_both_gives_the_other_version_its_own_path() {
    let (changes, state, id, v1, v2) = two_versions_of_a_file();
    let to = RepoPath::parse("ingredients/audio/a-second.mp3").unwrap();
    let planned = state
        .resolution(
            &id,
            &Choice::KeepBothCarried {
                keep: v1.id(),
                other: v2.id(),
                to: to.clone(),
            },
        )
        .unwrap();
    assert_eq!(planned.len(), 2);
    let mut all = changes;
    let mut dev = Dev::new(3);
    all.extend(dev.apply(planned));
    let after = fold(&all);
    assert!(
        after
            .conflicts_of(ConflictKind::CarriedFile)
            .next()
            .is_none()
    );
    let at = |p: &RepoPath| after.projected(&RecordKey::CarriedFile(p.clone()));
    let original = RepoPath::parse("ingredients/audio/a.mp3").unwrap();
    assert_eq!(at(&original), Some(carried("ingredients/audio/a.mp3", 1)));
    match at(&to) {
        Some(Record::CarriedFile {
            value: CarriedValue::Present { oid, .. },
            ..
        }) => assert_eq!(oid, oid_of(2)),
        other => panic!("{other:?}"),
    }
    // Not a candidate, the same version twice, or a target something holds: refused.
    assert!(
        state
            .resolution(
                &id,
                &Choice::KeepBothCarried {
                    keep: v1.id(),
                    other: v1.id(),
                    to: to.clone()
                }
            )
            .is_err()
    );
    assert!(
        state
            .resolution(
                &id,
                &Choice::KeepBothCarried {
                    keep: v1.id(),
                    other: v2.id(),
                    to: original
                }
            )
            .is_err()
    );
}

fn oid_of(n: u8) -> tc_journal::GitOid {
    oid(n)
}

#[test]
fn outside_resolution_keeps_the_files_side_and_skips_ambiguous() {
    let mut world = World::new(&[6]);
    // A teammate changes verses 1 and 2 (one action each); an outside edit (unknown base)
    // changes 1, 2 and 3.
    let mut juan = Dev::new(2);
    for v in [1u16, 2] {
        let change = juan.one(
            verse(1, v, text(&format!("juan {v}"), v)),
            vec![on(&world.changes, &verse_key(1, v)).id()],
        );
        world.changes.push(change);
    }
    let state = world.state();
    let mut outside = Dev::new(9);
    let plan: Vec<(Record, Vec<tc_journal::ChangeId>)> = [1u16, 2, 3]
        .iter()
        .map(|&v| (verse(1, v, text(&format!("file {v}"), v)), Vec::new()))
        .collect();
    let batch = outside.batch(
        plan,
        Cause::External {
            source: ExternalSource::LocalFile {
                path: path(),
                seen_md5: Md5::of(b"x"),
            },
            unknown_base: true,
        },
    );
    let batch_id = state.change(&batch[0].id());
    let mut all = world.changes.clone();
    all.extend(batch.iter().cloned());
    let state = fold(&all);
    assert!(batch_id.is_none(), "the batch was not in the earlier fold");
    let outside_batch = state.change(&batch[0].id()).unwrap().batch;
    let rows: Vec<_> = state.conflicts().map(|c| c.id).collect();
    assert_eq!(rows.len(), 3, "verses 1, 2 and 3 conflict");
    let (resolved, skipped) = state.outside_resolution(outside_batch, OutsideKeep::File, &rows);
    assert_eq!(resolved.len(), 3);
    assert!(skipped.is_empty());
    let (project, _) = state.outside_resolution(outside_batch, OutsideKeep::Project, &rows);
    assert_eq!(project.len(), 3);
    let mut applied = all.clone();
    let mut chooser = Dev::new(4);
    for (_, planned) in resolved {
        applied.extend(chooser.apply(planned));
    }
    let after = fold(&applied);
    assert!(after.conflicts().next().is_none());
    assert_eq!(projected_words(&after, 1, 1).as_deref(), Some("file 1"));
    // A conflict with the file's side listed twice is ambiguous for the project's side.
    let (none, skipped) = state.outside_resolution(
        outside_batch,
        OutsideKeep::File,
        &[tc_journal::fold::ConflictId::of(&[])],
    );
    assert!(
        none.is_empty() && skipped.is_empty(),
        "an unknown row is ignored"
    );
}

#[test]
fn undo_outside_restores_previous_values_with_the_edit_as_parent() {
    let world = World::new(&[5]);
    let state = world.state();
    let (bytes, base) = world.written(&DerivedFile::Usfm(BOOK));
    let file = edited(&bytes, "words of 1:3", "words of 1:3, fixed");
    let plan = plan_of(&state, &file, &base, false);
    let mut outside = Dev::new(9);
    let external = outside.apply(plan.changes);
    let mut all = world.changes.clone();
    all.extend(external.iter().cloned());
    let after_edit = fold(&all);
    assert_eq!(
        projected_words(&after_edit, 1, 3).as_deref(),
        Some("words of 1:3, fixed")
    );
    let batch = after_edit.change(&external[0].id()).unwrap().batch;
    let (undo, left) = after_edit.undo_outside(batch, outside.id).unwrap();
    assert!(left.is_empty());
    assert_eq!(undo.len(), 1);
    assert_eq!(
        undo[0].parents,
        vec![external[0].id()],
        "the edit is the parent"
    );
    assert_eq!(words(&undo[0].record), "words of 1:3");
    let mut undone = all;
    undone.extend(outside.apply(undo));
    let after = fold(&undone);
    assert_eq!(
        projected_words(&after, 1, 3).as_deref(),
        Some("words of 1:3")
    );
    assert!(after.conflicts().next().is_none());
    // Only the device's own outside batch can be undone.
    assert!(after_edit.undo_outside(batch, Dev::new(3).id).is_err());
}

#[test]
fn undo_after_a_later_edit_is_a_conflict_not_a_revert() {
    let world = World::new(&[5]);
    let state = world.state();
    let (bytes, base) = world.written(&DerivedFile::Usfm(BOOK));
    let file = edited(&bytes, "words of 1:3", "words of 1:3, fixed");
    let plan = plan_of(&state, &file, &base, false);
    let mut outside = Dev::new(9);
    let external = outside.apply(plan.changes);
    let mut all = world.changes.clone();
    all.extend(external.iter().cloned());
    let after_edit = fold(&all);
    // The translator edits the verse after the outside edit, then someone undoes the edit.
    let mut maria = Dev::new(3);
    let later = maria.one(verse(1, 3, text("maria later", 3)), vec![external[0].id()]);
    all.push(later);
    let batch = after_edit.change(&external[0].id()).unwrap().batch;
    let (undo, _) = after_edit.undo_outside(batch, outside.id).unwrap();
    all.extend(outside.apply(undo));
    let after = fold(&all);
    assert!(
        after.conflicts_of(ConflictKind::VerseText).next().is_some(),
        "the undo's parent is the edit, not Maria's text"
    );
}

// ---------------------------------------------------------------------------------------
// The decoders are the inverse of the projections (§11.1 X2)
// ---------------------------------------------------------------------------------------

fn sorted(mut records: Vec<Record>) -> Vec<Record> {
    records.sort_by_key(Record::key);
    records
}

#[test]
fn resources_and_settings_decode_what_they_render() {
    use tc_journal::fold::{decode_resources, decode_settings};
    use tc_journal::json::Json;
    use tc_journal::keys::{ResourcesKey, ResourcesLevel};
    let mut dev = Dev::new(1);
    let mut changes = imported_book(&mut dev, &[2]);
    let pin = |name: &str| {
        obj(&[
            ("repoPath", Json::from(format!("unfoldingWord/en_{name}"))),
            ("sha", Json::from("a".repeat(40))),
        ])
    };
    let pins_records = vec![
        pins(
            tc_journal::PinGroup::Primary,
            &[("ult", pin("ult")), ("ust", pin("ust"))],
        ),
        pins(tc_journal::PinGroup::Fallback, &[("ult", pin("ult"))]),
        pins(
            tc_journal::PinGroup::OriginalLanguage,
            &[("uhb", pin("uhb"))],
        ),
        pins(tc_journal::PinGroup::Lexicon, &[("lexicon", pin("lex"))]),
        pins(
            tc_journal::PinGroup::ExtraScripture,
            &[("extraScripture", Json::Arr(vec![pin("web")]))],
        ),
        Record::ResourcesExtra {
            key: ResourcesKey::new(ResourcesLevel::Top, "customTop").unwrap(),
            value: Some(Json::from("kept")),
        },
        Record::ResourcesExtra {
            key: ResourcesKey::new(ResourcesLevel::LanguageSets, "third").unwrap(),
            value: Some(obj(&[("x", Json::from("y"))])),
        },
        Record::ResourcesExtra {
            key: ResourcesKey::new(ResourcesLevel::Resources, "more").unwrap(),
            value: Some(Json::Arr(vec![Json::from("a")])),
        },
    ];
    let setting_records = vec![
        Record::Setting {
            key: tc_journal::SettingKey::parse("checkCategories").unwrap(),
            value: Some(obj(&[("on", Json::from("yes"))])),
        },
        Record::Setting {
            key: tc_journal::SettingKey::parse("showTips").unwrap(),
            value: Some(Json::from("true")),
        },
    ];
    for record in pins_records.iter().chain(&setting_records) {
        changes.push(dev.one(record.clone(), vec![]));
    }
    let state = fold(&changes);
    let bytes = state
        .render(&DerivedFile::Resources)
        .expect("resources.json");
    assert_eq!(
        sorted(decode_resources(&bytes).expect("decodes")),
        sorted(pins_records),
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let bytes = state.render(&DerivedFile::Settings).expect("settings.json");
    assert_eq!(
        sorted(decode_settings(&bytes).expect("decodes")),
        sorted(setting_records)
    );
    // A float cannot be a setting (the model holds integers), and a wrong version is no
    // file this format reads: both say so in words.
    assert!(decode_settings(br#"{"schemaVersion":1,"x":1.5}"#).is_err());
    assert!(decode_resources(br#"{"schemaVersion":3}"#).is_err());
    assert!(decode_resources(b"not json").is_err());
}

#[test]
fn comments_decode_what_they_render() {
    use tc_journal::fold::decode_comments;
    let mut dev = Dev::new(1);
    let mut changes = imported_book(&mut dev, &[3]);
    changes.push(dev.one(
        comment(3, verse_target(1, 2), "a note \"quoted\"\nsecond line"),
        vec![],
    ));
    changes.push(dev.one(comment(4, verse_target(2, 1), "another"), vec![]));
    let state = fold(&changes);
    let bytes = state
        .render(&DerivedFile::Comments(BOOK))
        .expect("comments");
    let got = decode_comments(Some(BOOK), &bytes).expect("decodes");
    assert_eq!(got.len(), 2);
    for decoded in &got {
        let want = changes
            .iter()
            .find_map(|c| match &c.body().record {
                Record::Comment { id, value } if Some(*id) == decoded.id => Some(value.clone()),
                _ => None,
            })
            .expect("the comment is one of the records");
        assert_eq!(decoded.target, want.target);
        assert_eq!(decoded.text, want.text);
    }
}

#[test]
fn metadata_decodes_into_the_fields_the_records_hold() {
    use tc_journal::fold::decode_metadata;
    use tc_journal::json::Json;
    let mut dev = Dev::new(1);
    let mut changes = imported_book(&mut dev, &[2]);
    let meta = |field: tc_journal::MetaField, value: Json| Record::ProjectMeta {
        field,
        value: Some(value),
    };
    let extra =
        |path: &str| tc_journal::MetaField::Extra(tc_journal::keys::MetaPath::parse(path).unwrap());
    let records = vec![
        meta(
            tc_journal::MetaField::Name,
            obj(&[("en", Json::from("Tito"))]),
        ),
        meta(
            extra("localizedNames"),
            obj(&[("TIT", obj(&[("short", obj(&[("en", Json::from("Tit"))]))]))]),
        ),
        meta(
            extra("idAuthorities.ebible"),
            obj(&[
                ("id", Json::from("https://ebible.org")),
                ("name", obj(&[("en", Json::from("eBible"))])),
            ]),
        ),
        meta(extra("meta.defaultLocale"), Json::from("es")),
    ];
    for record in &records {
        changes.push(dev.one(record.clone(), vec![]));
    }
    let state = fold(&changes);
    let source = state.metadata_source((1, 0));
    let mut table = Vec::new();
    for file in state.derived_files() {
        let bytes = state.render(&file).expect("renders");
        table.push(tc_burrito::IngredientEntry {
            path: tc_burrito::IngredientPath::parse(&file.path()).unwrap(),
            md5: tc_burrito::Md5::of(&bytes),
            size: bytes.len() as u64,
            role: file.role().map(str::to_owned),
            scope: file.scope(&[]),
        });
    }
    let bytes = tc_burrito::build_metadata(&source, &table).expect("valid");
    let decoded = decode_metadata(&bytes).expect("decodes");
    for want in &records {
        let Record::ProjectMeta { field, value } = want else {
            unreachable!()
        };
        let found = decoded.fields.iter().find(|(f, _)| f == field);
        assert_eq!(found.map(|(_, v)| v), value.as_ref(), "{field:?}");
    }
    assert!(decoded.derived.contains_key("type"));
    assert!(decoded.derived.contains_key("meta.version"));
    assert!(
        decoded.unrecordable.is_empty(),
        "{:?}",
        decoded.unrecordable
    );
    // The fields the builder fills in are fields too, and `fields_not_shown` drops the ones
    // the project shows already.
    let shown: Vec<Record> = decoded
        .fields
        .iter()
        .map(|(f, v)| Record::ProjectMeta {
            field: f.clone(),
            value: Some(v.clone()),
        })
        .collect();
    assert!(tc_journal::outside::fields_not_shown(shown.clone(), &shown, |_| false).is_empty());
    // A field the project holds a record for stays, so the planner does not read it as lost.
    assert_eq!(
        tc_journal::outside::fields_not_shown(shown.clone(), &shown, |_| true).len(),
        shown.len()
    );
}

/// `written(k)` of X3: the value the heads a base lists stand for. A settled record shows its
/// one head; several heads with equal values show that value; several that differ are no
/// value a file showed (the key is then planned without a base); a record that is in
/// conflict now shows what the base's heads said, not what the fold shows today; a head that
/// cannot be read is `Unreadable`.
#[test]
fn shown_value_is_what_the_file_showed() {
    use tc_journal::outside::ShownValue;
    let mut world = World::new(&[3]);
    let key = verse_key(1, 2);
    let first = on(&world.changes, &key);
    let mine = world
        .dev
        .one(verse(1, 2, text("my words", 2)), vec![first.id()]);
    let mut juan = Dev::new(2);
    let theirs = juan.one(verse(1, 2, text("their words", 2)), vec![first.id()]);
    let same = juan.one(verse(1, 2, text("my words", 2)), vec![first.id()]);
    world.changes.push(mine.clone());
    let value =
        |state: &FoldState, heads: &[tc_journal::ChangeId]| match state.shown_value(&key, heads) {
            ShownValue::Value(record) => Some(words(&record)),
            _ => None,
        };

    // One head: its record.
    let state = world.state();
    assert_eq!(value(&state, &[mine.id()]).as_deref(), Some("my words"));
    // No heads: nothing was written.
    assert!(matches!(state.shown_value(&key, &[]), ShownValue::Nothing));
    // An older version of the record, which is what the file showed before the edit.
    assert_eq!(
        value(&state, &[first.id()]).as_deref(),
        Some("words of 1:2")
    );

    // Two heads of different value: the fold shows one of them, the base listed both.
    world.changes.push(theirs.clone());
    let state = world.state();
    assert!(matches!(
        state.shown_value(&key, &[mine.id(), theirs.id()]),
        ShownValue::Conflicting | ShownValue::Value(_)
    ));
    // The heads the file was made from, when they differ in value and are not the record's
    // current heads: no single value.
    let other = juan.one(verse(1, 2, text("other words", 2)), vec![first.id()]);
    world.changes.push(other.clone());
    let state = world.state();
    assert!(matches!(
        state.shown_value(&key, &[mine.id(), other.id()]),
        ShownValue::Conflicting
    ));

    // Heads of equal value are that value, whether or not the fold has them as heads.
    world.changes.push(same.clone());
    let state = world.state();
    assert_eq!(
        value(&state, &[mine.id(), same.id()]).as_deref(),
        Some("my words")
    );

    // A change this computer has not got cannot be read.
    let missing = juan.one(verse(1, 2, text("never arrived", 2)), vec![first.id()]);
    assert!(matches!(
        state.shown_value(&key, &[missing.id()]),
        ShownValue::Unreadable
    ));
}
