//! The rules the domain crates share with the fold (`docs/STORAGE.md` §5.3, §5.4, §7.7,
//! §8.5, §13.5): check identity as tC4 writes it, the no-text rule, the decision edit, the
//! ladder over OBS slots, the OBS decision and comment files, `resources.json` members no pin
//! group holds, person labels, the verse conflicts, an alignment's validity and order, and the
//! aligned-USFM split.

// Tests build fixed, known-valid values and may fail loudly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod support;

use proptest::prelude::*;
use support::*;
use tc_journal::alignment::{AlignedRun, Stale, split_aligned};
use tc_journal::command::{DecisionBasis, DecisionChange, DecisionEdit};
use tc_journal::decision::{TargetText, assess, verse_text_md5_v1};
use tc_journal::fold::{ConflictKind, DerivedFile, FoldInput, FoldState, PersonLabel, ProjectKind};
use tc_journal::json::{Json, JsonMap};
use tc_journal::pins::Collection;
use tc_journal::record::{
    CommentTarget, DeviceProfileValue, Door43Username, ResourceRef, Selection, TargetWord,
    UserStatus,
};
use tc_journal::{
    Cause, Change, CheckId, CheckKey, CheckTarget, CommentId, FrameNo, PinGroup, Record, RecordKey,
    RefPart, ResourcesKey, ResourcesLevel, StoryNo, ToolId, VerseNo,
};

fn tn_slot(pin: &ResourceRef, books: Option<&[&str]>) -> Json {
    let mut fields = vec![
        ("repoPath", Json::from(pin.repo_path.as_ref())),
        ("sha", Json::from(pin.sha.to_hex())),
        ("version", Json::from("v91")),
    ];
    if let Some(books) = books {
        fields.push((
            "books",
            Json::Arr(books.iter().map(|b| Json::from(*b)).collect()),
        ));
    }
    obj(&fields)
}

fn selection(text: &str, occurrences: u32) -> Selection {
    Selection {
        text: text.into(),
        occurrence: 1,
        occurrences,
        extra: JsonMap::new(),
    }
}

fn key_at(id: &str, chapter: &str, verse: &str) -> CheckKey {
    CheckKey {
        tool: ToolId::TranslationNotes,
        check_id: CheckId::parse(id).unwrap(),
        target: CheckTarget::Verse {
            book: BOOK,
            chapter: RefPart::parse(chapter).unwrap(),
            verse: RefPart::parse(verse).unwrap(),
        },
        occurrence: 1,
    }
}

fn decision_file(state: &FoldState) -> serde_json::Value {
    rendered(
        state,
        &DerivedFile::Decisions(ToolId::TranslationNotes, BOOK),
    )
    .expect("the decision file")
}

fn flags(file: &serde_json::Value, id: &str) -> (bool, bool) {
    let record = file["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["contextId"]["checkId"] == id)
        .unwrap_or_else(|| panic!("no decision {id}"));
    (
        record["invalidated"].as_bool().unwrap(),
        record["verseEdits"].as_bool().unwrap(),
    )
}

/// A decision made against `md5("x")`, with a selection that no verse below holds.
fn stale_decision(id: &str) -> tc_journal::record::DecisionRecord {
    let mut d = decision(Some(UserStatus::Valid), false, None);
    d.context = JsonMap::from([("checkId".to_owned(), Json::from(id))]);
    d.judgement.selections = Some(vec![selection("gone", 1)]);
    d
}

/// en_tn's `sbh4` is a check on verse `1,3,8,12` of PSA 5, as tC4 keys it; such a target,
/// a `front`, a span no run carries, and an undrafted verse name no text, so a decision on
/// them is never invalidated by the text (tC4's `decisionIsStale` finds no draft text). A
/// drafted verse, and a span a run carries, are read.
#[test]
fn the_no_text_rule_reads_only_the_texts_tc4_reads() {
    let mut dev = Dev::new(1);
    let mut changes = seed_book(&mut dev, &[4]);
    let v = |n: u16| on(&changes, &verse_key(1, n)).id();
    let edits = dev.batch(
        vec![
            (verse(1, 1, text("Paul, a servant of God", 1)), vec![v(1)]),
            (verse(1, 2, text("in hope of eternal life", 3)), vec![v(2)]),
            (verse(1, 3, covered(2)), vec![v(3)]),
        ],
        Cause::Edit,
    );
    changes.extend(edits);
    let pin = tn_pin();
    changes.push(dev.one(
        pins(
            PinGroup::Primary,
            &[("translationNotes", tn_slot(&pin, None))],
        ),
        vec![],
    ));
    for (id, chapter, verse) in [
        ("comma", "1", "1,3"),
        ("front", "1", "front"),
        ("loose", "1", "1-2"),
        ("stub", "1", "4"),
        ("drafted", "1", "1"),
        ("member", "1", "3"),
        ("span", "1", "2-3"),
    ] {
        changes.push(dev.one(
            check(&key_at(id, chapter, verse), stale_decision(id)),
            vec![],
        ));
    }
    let state = fold(&changes);
    let file = decision_file(&state);
    for id in ["comma", "front", "loose", "stub"] {
        assert_eq!(flags(&file, id), (false, false), "{id}");
    }
    for id in ["drafted", "member", "span"] {
        assert_eq!(flags(&file, id), (true, true), "{id}");
    }
    // What a Check screen opens a session with carries the file's flags, decision by
    // decision.
    let projected = state.decisions(ToolId::TranslationNotes, Collection::Book(BOOK));
    assert_eq!(projected.len(), 7);
    for d in &projected {
        let id = d.key.check_id.as_str();
        assert_eq!(
            (
                d.assessment.invalidation.is_some(),
                d.assessment.verse_edits
            ),
            flags(&file, id),
            "{id}"
        );
        assert!(!d.detached, "{id}: made against the resolution");
    }
    assert!(
        state
            .decisions(ToolId::TranslationWords, Collection::Book(BOOK))
            .is_empty()
    );
}

/// §7.7 and owner ruling H12: a verse is drafted when its words on any line hold more than
/// the stub. A poetry verse whose first line is a `\q1` line of its own, the stub followed by
/// poetry, and a verse that opens with a blank line have their text, though tC4's indexer,
/// which reads only up to the first paragraph line, gives them none; a verse whose marker
/// stands inside a line after `\q1 ` is read from its marker (tC4 does not index it at all).
#[test]
fn the_no_text_rule_reads_the_words_on_every_line() {
    use tc_usfm::codec::{MarkerTerm, MarkerWs};
    let mut dev = Dev::new(1);
    let mut changes = seed_book(&mut dev, &[5]);
    let laid = |words: &str, layout: &str, term: MarkerTerm| tc_journal::record::VerseValue::Text {
        marker: MarkerWs::new(" ", term).unwrap(),
        words: words.into(),
        layout: layout.into(),
        last: VerseNo::new(1).unwrap(),
        from: None,
    };
    let at = |n: u16, value: tc_journal::record::VerseValue| match value {
        tc_journal::record::VerseValue::Text {
            marker,
            words,
            layout,
            from,
            ..
        } => verse(
            1,
            n,
            tc_journal::record::VerseValue::Text {
                marker,
                words,
                layout,
                last: VerseNo::new(n).unwrap(),
                from,
            },
        ),
        other => verse(1, n, other),
    };
    let v = |n: u16| on(&changes, &verse_key(1, n)).id();
    let edits = dev.batch(
        vec![
            (
                at(1, laid("\\q1 Paul a servant", "\n", MarkerTerm::Lf)),
                vec![v(1)],
            ),
            (
                at(2, laid("___\n\\q2 of God", "\n", MarkerTerm::Space)),
                vec![v(2)],
            ),
            (
                at(
                    3,
                    laid("\n\\b\n\\q1 and an apostle", "\n", MarkerTerm::Space),
                ),
                vec![v(3)],
            ),
            (
                at(
                    4,
                    laid("of Jesus\n\\q2 Christ", "\n\\q1 ", MarkerTerm::Space),
                ),
                vec![v(4)],
            ),
            (
                at(5, laid("for the faith", "\n", MarkerTerm::Space)),
                vec![v(5)],
            ),
        ],
        Cause::Edit,
    );
    changes.extend(edits);
    let pin = tn_pin();
    changes.push(dev.one(
        pins(
            PinGroup::Primary,
            &[("translationNotes", tn_slot(&pin, None))],
        ),
        vec![],
    ));
    for (id, n) in [
        ("lfq1", "1"),
        ("stbq", "2"),
        ("blnk", "3"),
        ("text", "4"),
        ("midl", "5"),
    ] {
        changes.push(dev.one(check(&key_at(id, "1", n), stale_decision(id)), vec![]));
    }
    let state = fold(&changes);
    let usfm = usfm(&state);
    assert!(
        usfm.contains("\\v 1\n\\q1 Paul") && usfm.contains("\\q1 \\v 5 for"),
        "{usfm}"
    );
    let file = decision_file(&state);
    for id in ["lfq1", "stbq", "blnk", "text", "midl"] {
        assert_eq!(flags(&file, id), (true, true), "{id}");
    }
}

/// §7.7: the text the Check screen shows and the aligner banks is tC4's `verseText` of its
/// book model, but for the declared differences: the items of a `\li` list are shown and
/// banked, as the export counts them, where tC4 hides them
/// (`fixtures/usfm/check-text-vectors.json`); the hash reads the content.
#[test]
fn the_shown_text_shows_the_list_items_tc4_hides() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/tc4/test-fixtures/usfm-corpus/exotic-poetry-footnotes.usfm");
    let parts = tc_usfm::codec::decompose(&std::fs::read_to_string(path).unwrap()).unwrap();
    let tc_usfm::codec::VersePart::Text(verse) = &parts.chapters[&2].verses[&2] else {
        panic!("2:2 holds text")
    };
    let content = tc_usfm::codec::join_verse(&verse.words, &verse.layout);
    assert!(content.contains("\\li1 the first member"), "{content}");
    let text = TargetText::new(content.clone());
    assert_eq!(
        text.shown(),
        "The list has these members:\nthe first member,\na nested member,\nthe last member.\nThe list ends here."
    );
    let aligner = tc_align::TargetVerse::new(&content);
    let words: Vec<&str> = aligner.words().iter().map(|w| w.word.as_str()).collect();
    assert_eq!(
        words,
        [
            "The", "list", "has", "these", "members", "the", "first", "member", "a", "nested",
            "member", "the", "last", "member", "The", "list", "ends", "here"
        ]
    );
}

/// §5.4: a judgement stamps the text's md5, clears an imported invalidation and records the
/// pin of the list the person judged in; To do changes only the status; a bookmark only
/// itself.
#[test]
fn a_rejudgement_records_the_current_resource_and_to_do_changes_only_the_status() {
    let old = tn_pin();
    let new = ResourceRef {
        repo_path: old.repo_path.clone(),
        sha: oid(9),
    };
    let mut stored = stale_decision("abcd");
    stored.imported_invalid = true;
    let edit = |changes: Vec<DecisionChange>| DecisionEdit {
        basis: None,
        resource: new.clone(),
        changes,
    };
    let todo = edit(vec![DecisionChange::Status(Some(UserStatus::Todo))])
        .apply(Some(&stored), Some("Paul"))
        .unwrap();
    assert_eq!(todo.judgement.status, Some(UserStatus::Todo));
    assert_eq!(
        todo.judgement.verse_text_md5,
        stored.judgement.verse_text_md5
    );
    assert_eq!(todo.resource, old);
    assert!(todo.imported_invalid);
    let bookmark = edit(vec![DecisionChange::Bookmarked(true)])
        .apply(Some(&stored), Some("Paul"))
        .unwrap();
    assert!(bookmark.bookmarked && bookmark.resource == old);
    let valid = edit(vec![
        DecisionChange::Selections(Some(vec![selection("Paul", 1)])),
        DecisionChange::NothingToSelect(false),
        DecisionChange::Status(Some(UserStatus::Valid)),
    ])
    .apply(Some(&stored), Some("Paul"))
    .unwrap();
    assert_eq!(valid.resource, new);
    assert!(!valid.imported_invalid);
    assert_eq!(valid.judgement.verse_text_md5, verse_text_md5_v1("Paul"));
    // The re-judged decision holds against the new resolution, as the screen shows it.
    let text = TargetText::new("Paul".into());
    assert!(
        assess(&valid, Some(&new), Some(&text))
            .invalidation
            .is_none()
    );
    assert!(
        assess(&todo, Some(&new), Some(&text))
            .invalidation
            .is_some()
    );
    // A first decision needs the item's context.
    let first = edit(vec![DecisionChange::Status(Some(UserStatus::Invalid))]);
    assert!(first.apply(None, None).is_err());
    let first = DecisionEdit {
        basis: Some(DecisionBasis {
            context: JsonMap::new(),
            category: None,
        }),
        ..first
    };
    let made = first.apply(None, None).unwrap();
    assert_eq!(made.judgement.verse_text_md5, verse_text_md5_v1(""));
    assert_eq!(made.resource, new);
}

fn obs_changes() -> Vec<Change> {
    let mut dev = Dev::new(2);
    let story = StoryNo::new(1).unwrap();
    let base = "# The Creation\n\n![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-01.jpg)\n\nThis is how God made everything.\n\n![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-02.jpg)\n\nThen God said.\n\n_A Bible story from: Genesis 1-2_\n";
    let mut out = vec![dev.one(
        Record::StoryBase {
            story,
            value: base.into(),
        },
        vec![],
    )];
    out.push(dev.one(
        Record::Frame {
            story,
            frame: FrameNo::new(1),
            value: "Así es como Dios hizo todo.".into(),
        },
        vec![],
    ));
    let pin = ResourceRef {
        repo_path: "git.door43.org/es-419_gl/es-419_obs-tn".into(),
        sha: oid(3),
    };
    out.push(dev.one(
        pins(PinGroup::Primary, &[("obs-tn", tn_slot(&pin, None))]),
        vec![],
    ));
    for (id, frame, words) in [("f1", 1, "Dios"), ("f2", 2, "Dios")] {
        let key = CheckKey {
            tool: ToolId::TranslationNotes,
            check_id: CheckId::parse(id).unwrap(),
            target: CheckTarget::Frame {
                story,
                frame: FrameNo::new(frame),
            },
            occurrence: 1,
        };
        let mut d = decision(Some(UserStatus::Valid), false, None);
        d.context = JsonMap::from([
            ("checkId".to_owned(), Json::from(id)),
            ("occurrence".to_owned(), Json::from(1)),
            (
                "reference".to_owned(),
                obj(&[("frame", Json::from(frame)), ("story", Json::from(1))]),
            ),
        ]);
        d.resource = pin.clone();
        d.judgement.verse_text_md5 = verse_text_md5_v1("Así es como Dios hizo todo.");
        d.judgement.selections = Some(vec![selection(words, 1)]);
        out.push(dev.one(check(&key, d), vec![]));
        if frame == 1 {
            out.push(dev.one(
                comment(4, CommentTarget::Check(key), "¿Es la palabra correcta?"),
                vec![],
            ));
        }
    }
    out.push(dev.one(
        comment(
            5,
            CommentTarget::Frame {
                story,
                frame: FrameNo::new(2),
            },
            "Falta traducir",
        ),
        vec![],
    ));
    out
}

/// §3.2, §7.7: decisions on frames resolve over the `obs-` slots and project to
/// `checking/<tool>/OBS.json`, each read against its frame's text (the frame's record, else
/// the template's paragraph); comments on frames and frame checks project to
/// `tc/comments/OBS.json`. The decision file decodes back to the same records.
#[test]
fn obs_decisions_and_comments_have_their_files() {
    let changes = obs_changes();
    let state = FoldState::fold(
        ProjectKind::Stories,
        changes.iter().cloned().map(FoldInput::from),
    );
    let files = state.derived_files();
    let tn = DerivedFile::StoryDecisions(ToolId::TranslationNotes);
    assert!(files.contains(&tn), "{files:?}");
    assert!(files.contains(&DerivedFile::StoryComments));
    assert_eq!(tn.path(), "ingredients/checking/translationNotes/OBS.json");
    let doc = rendered(&state, &tn).unwrap();
    assert_eq!(doc["book"], "OBS");
    assert_eq!(doc["resource"]["languageSet"], "primary");
    let decisions = doc["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 2);
    // f1 was judged against frame 1's text; f2's frame still holds the template's text,
    // which does not hold the selected word.
    assert_eq!(decisions[0]["invalidated"], false);
    assert_eq!(decisions[1]["invalidated"], true);
    assert_eq!(decisions[1]["verseEdits"], true);
    let comments = rendered(&state, &DerivedFile::StoryComments).unwrap();
    let placed: Vec<&serde_json::Value> = comments["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| &c["placed"])
        .collect();
    assert_eq!(placed.len(), 2);
    assert_eq!(placed[0]["frame"], 1);
    assert!(placed[0]["check"].is_object());
    assert_eq!(placed[1]["frame"], 2);
    let bytes = state.render(&tn).unwrap();
    let decoded =
        tc_journal::fold::decode_decisions(ToolId::TranslationNotes, None, &bytes).unwrap();
    let stored: Vec<Record> = changes
        .iter()
        .map(|c| c.body().record.clone())
        .filter(|r| matches!(r, Record::Check { .. }))
        .collect();
    assert_eq!(decoded, stored);
}

/// §7.7: a decision made against another resource than the resolution is listed under
/// Detached, on a verse and on a frame alike, and the Check screen's session lists it. A
/// comment on a frame is listed only when no screen places it: its story holds no text for
/// that frame. An incremental fold lists the same records as a fold from scratch, whatever
/// order the pins, the story and the records arrive in.
#[test]
fn decisions_against_another_resource_and_unplaced_frame_comments_are_detached() {
    let other = |pin: &ResourceRef| ResourceRef {
        repo_path: pin.repo_path.clone(),
        sha: oid(9),
    };
    // A verse.
    let mut dev = Dev::new(1);
    let mut changes = seed_book(&mut dev, &[1]);
    let pin = tn_pin();
    changes.push(dev.one(
        pins(
            PinGroup::Primary,
            &[("translationNotes", tn_slot(&pin, None))],
        ),
        vec![],
    ));
    for (id, resource) in [("same", pin.clone()), ("othr", other(&pin))] {
        let mut d = decision(Some(UserStatus::Valid), false, None);
        d.resource = resource;
        changes.push(dev.one(check(&key_at(id, "1", "1"), d), vec![]));
    }
    let state = assert_converges(&changes);
    let listed = |state: &FoldState, collection| -> Vec<(String, bool)> {
        state
            .decisions(ToolId::TranslationNotes, collection)
            .iter()
            .map(|d| (d.key.check_id.to_string(), d.detached))
            .collect()
    };
    assert_eq!(
        listed(&state, Collection::Book(BOOK)),
        [("othr".to_owned(), true), ("same".to_owned(), false)]
    );
    // A frame.
    let mut changes = obs_changes();
    let mut dev = Dev::new(4);
    let story = StoryNo::new(1).unwrap();
    let obs_pin = ResourceRef {
        repo_path: "git.door43.org/es-419_gl/es-419_obs-tn".into(),
        sha: oid(3),
    };
    let mut d = decision(Some(UserStatus::Valid), false, None);
    d.resource = other(&obs_pin);
    let f3 = CheckKey {
        tool: ToolId::TranslationNotes,
        check_id: CheckId::parse("f3").unwrap(),
        target: CheckTarget::Frame {
            story,
            frame: FrameNo::new(1),
        },
        occurrence: 1,
    };
    changes.push(dev.one(check(&f3, d), vec![]));
    // The story's template holds two frames: a comment on a ninth has no place.
    let nowhere = CommentTarget::Frame {
        story,
        frame: FrameNo::new(9),
    };
    changes.push(dev.one(comment(6, nowhere, "¿Dónde?"), vec![]));
    let state = assert_converges_as(ProjectKind::Stories, &changes);
    // The pins arrive last: every frame decision is placed again.
    let (pinned, rest): (Vec<Change>, Vec<Change>) = changes
        .iter()
        .cloned()
        .partition(|c| matches!(c.body().record, Record::Pins { .. }));
    let mut late = FoldState::new(ProjectKind::Stories);
    late.add(rest.into_iter().map(FoldInput::from));
    late.add(pinned.into_iter().map(FoldInput::from));
    same_fold(&state, &late, "pins last");
    let decisions = state.decisions(ToolId::TranslationNotes, Collection::Stories);
    assert_eq!(
        listed(&state, Collection::Stories),
        [
            ("f1".to_owned(), false),
            ("f2".to_owned(), false),
            ("f3".to_owned(), true)
        ]
    );
    let comments: Vec<&RecordKey> = state
        .detached()
        .filter(|k| matches!(k, RecordKey::Comment(_)))
        .collect();
    assert_eq!(
        comments,
        [&RecordKey::Comment(CommentId::from_bytes([6; 16]))]
    );
    let file = rendered(&state, &DerivedFile::StoryComments).unwrap();
    let placed: Vec<bool> = file["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| !c["placed"].is_null())
        .collect();
    assert_eq!(placed, [true, true, false]);
    // The Check screen lists what the fold lists.
    let session = tc_check::session::open(vec![], &decisions, Some(&obs_pin), |_| None);
    let shown: Vec<&str> = session
        .detached
        .iter()
        .map(|&s| decisions[s].key.check_id.as_str())
        .collect();
    assert_eq!(shown, ["f3"]);
}

/// A `resources.json` member no pin group holds is a record of its own, projected at its
/// level after the modelled members, so no import or outside edit loses it.
#[test]
fn unknown_resources_members_are_kept_at_their_level() {
    let mut dev = Dev::new(3);
    let mut changes = seed_book(&mut dev, &[1]);
    changes.push(dev.one(
        pins(
            PinGroup::Primary,
            &[("translationNotes", tn_slot(&tn_pin(), None))],
        ),
        vec![],
    ));
    for (level, name, value) in [
        (ResourcesLevel::Top, "translationCoreNext", Json::from(true)),
        (ResourcesLevel::LanguageSets, "secondary", obj(&[])),
        (ResourcesLevel::Resources, "glossary", Json::from("x")),
    ] {
        changes.push(dev.one(
            Record::ResourcesExtra {
                key: ResourcesKey::new(level, name).unwrap(),
                value: Some(value),
            },
            vec![],
        ));
    }
    let state = fold(&changes);
    let doc = rendered(&state, &DerivedFile::Resources).unwrap();
    assert_eq!(doc["translationCoreNext"], true);
    assert!(doc["languageSets"]["secondary"].is_object());
    assert!(doc["languageSets"]["primary"]["translationNotes"].is_object());
    assert_eq!(doc["resources"]["glossary"], "x");
    let keys: Vec<&String> = doc.as_object().unwrap().keys().collect();
    assert_eq!(keys.last().map(|k| k.as_str()), Some("translationCoreNext"));
    for owned in ["languageSets", "schemaVersion"] {
        assert!(
            ResourcesKey::new(ResourcesLevel::Top, owned).is_err(),
            "{owned}"
        );
    }
    assert!(ResourcesKey::new(ResourcesLevel::LanguageSets, "primary").is_err());
    assert!(ResourcesKey::new(ResourcesLevel::Resources, "constructor").is_err());
}

fn profile(username: Option<&str>, display: Option<&str>) -> DeviceProfileValue {
    DeviceProfileValue {
        display_name: display.map(Into::into),
        door43_username: username.map(|u| Door43Username::parse(u).unwrap()),
        legacy: None,
    }
}

/// §8.5: one label per person on the review and the sync result: Mine for this device and
/// its Door43 user, the username, the display name, else Teammate n in id order.
#[test]
fn person_labels_name_each_person_once() {
    let mut me = Dev::new(10);
    let mut other_me = Dev::new(11);
    let mut juan = Dev::new(12);
    let mut ana = Dev::new(13);
    let mut quiet = [Dev::new(14), Dev::new(15)];
    let mut changes = Vec::new();
    for (dev, value) in [
        (&mut me, profile(Some("maria"), None)),
        (
            &mut other_me,
            profile(Some("maria"), Some("Maria's laptop")),
        ),
        (&mut juan, profile(Some("juan"), Some("Juan"))),
        (&mut ana, profile(None, Some("Ana"))),
    ] {
        let id = dev.id;
        changes.push(dev.one(Record::DeviceProfile { device: id, value }, vec![]));
    }
    for dev in &mut quiet {
        changes.push(dev.one(verse(1, 1, text("words", 1)), vec![]));
    }
    let labels = fold(&changes).person_labels(me.id);
    assert_eq!(labels[&me.id], PersonLabel::Mine);
    assert_eq!(labels[&other_me.id], PersonLabel::Mine);
    assert_eq!(labels[&juan.id], PersonLabel::Username("juan".into()));
    assert_eq!(labels[&ana.id], PersonLabel::DisplayName("Ana".into()));
    let mut unnamed = [quiet[0].id, quiet[1].id];
    unnamed.sort();
    assert_eq!(labels[&unnamed[0]], PersonLabel::Teammate(1));
    assert_eq!(labels[&unnamed[1]], PersonLabel::Teammate(2));
}

#[test]
fn the_verse_conflicts_are_the_four_verse_kinds() {
    let verse = [
        ConflictKind::VerseText,
        ConflictKind::VerseStructure,
        ConflictKind::Passage,
        ConflictKind::StructureIssue,
    ];
    for kind in verse {
        assert!(kind.is_verse(), "{kind:?}");
    }
    for kind in [
        ConflictKind::Alignment,
        ConflictKind::Check,
        ConflictKind::Pins,
        ConflictKind::BookPresence,
        ConflictKind::ChapterFront,
    ] {
        assert!(!kind.is_verse(), "{kind:?}");
    }
}

fn word(text: &str, occurrence: u32, occurrences: u32) -> TargetWord {
    TargetWord {
        word: text.into(),
        occurrence,
        occurrences,
        extra: JsonMap::new(),
    }
}

/// §7.7: an alignment vouches for the run it is placed at when that run covers as many
/// verses, counted from its start, as the alignment did from its anchor; a renumbered text
/// keeps its alignment valid.
#[test]
fn alignment_validity_follows_the_anchor() {
    let Record::Alignment { mut value, .. } = alignment(2, 4, 5, "words") else {
        panic!("an alignment")
    };
    let at = |run_start, run_end| AlignedRun {
        anchor: 4,
        run_start,
        run_end,
        text_md5: md5("words"),
        source_sha: None,
    };
    assert!(value.is_valid(&at(4, 5)));
    assert!(value.is_valid(&at(7, 8)), "moved by a renumber");
    assert_eq!(value.staleness(&at(7, 7)), vec![Stale::SpanChanged]);
    value.marked_invalid = true;
    assert_eq!(
        value.staleness(&AlignedRun {
            text_md5: md5("other"),
            ..at(4, 5)
        }),
        vec![Stale::TextChanged, Stale::MarkedInvalid]
    );
}

/// §5.3 (Q15, Q19): the bank and each card in verse order, a word the verse lacks last.
#[test]
fn a_saved_alignment_is_in_verse_order() {
    let Record::Alignment { mut value, .. } = alignment(1, 1, 1, "x") else {
        panic!("an alignment")
    };
    value.word_bank = vec![
        word("cat", 1, 1),
        word("gone", 1, 1),
        word("the", 2, 2),
        word("the", 1, 2),
    ];
    let verse = [("the", 1), ("cat", 1), ("saw", 1), ("the", 2)];
    assert!(!value.in_verse_order(verse));
    value.put_in_verse_order(verse);
    let order: Vec<(&str, u32)> = value
        .word_bank
        .iter()
        .map(|w| (w.word.as_str(), w.occurrence))
        .collect();
    assert_eq!(order, [("the", 1), ("cat", 1), ("the", 2), ("gone", 1)]);
    assert!(value.in_verse_order(verse));
}

/// §13.5: every verse of en_ult Titus is aligned USFM written by tC4's export. Each splits
/// into its plain text and an `Alignment` value that weaves back into the verse byte for
/// byte, and the plain text holds the words that `plain_text_v1` reads.
#[test]
fn aligned_titus_verses_split_into_text_and_alignment() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/tc4/test-fixtures/en_ult/TIT.usfm");
    let text = std::fs::read_to_string(path).unwrap();
    let book = tc_usfm::Book::parse(&text);
    let (mut split, mut identical) = (0, 0);
    for unit in book.verses() {
        let content = unit.body();
        let last = VerseNo::new(1).unwrap();
        let (plain, value) = split_aligned(content, last)
            .unwrap_or_else(|| panic!("{:?}:{} did not split", unit.chapter(), unit.label()));
        assert!(!plain.contains("\\zaln"), "{plain}");
        assert!(
            tc_usfm::plain_text_v1(content)
                .chars()
                .filter(|c| c.is_alphabetic())
                .count()
                < tc_usfm::plain_text_v1(&plain)
                    .chars()
                    .filter(|c| c.is_alphabetic())
                    .count()
        );
        assert_eq!(value.target_verse_md5, verse_text_md5_v1(&plain));
        // tC4's cards write en_ult's bytes, or, where en_ult writes one source word over
        // two runs, the same links with the card's words in one run.
        let woven = tc_usfm::weave(&plain, &value.usfm_groups()).unwrap();
        let links = |text: &str| -> std::collections::BTreeSet<(String, u32, String, u32)> {
            tc_usfm::parse_aligned(text)
                .unwrap()
                .groups
                .iter()
                .flat_map(|g| {
                    g.sources.iter().flat_map(move |s| {
                        g.targets.iter().map(move |t| {
                            (
                                s.content.clone(),
                                s.occurrence,
                                t.text.clone(),
                                t.occurrence,
                            )
                        })
                    })
                })
                .collect()
        };
        assert_eq!(links(&woven), links(content));
        if woven == content {
            identical += 1;
        }
        assert!(value.source.is_none() && !value.marked_invalid);
        split += 1;
    }
    assert_eq!(identical, 41);
    assert_eq!(split, 46);
    // Unaligned text is not split.
    assert!(split_aligned("Paul, a servant of God", VerseNo::new(1).unwrap()).is_none());
    // A verse whose `\w` words carry more than the occurrence attributes is kept as written.
    let odd = "\\zaln-s |x-strong=\"G39720\" x-lemma=\"Παῦλος\" x-morph=\"Gr,N,,,,,NMS,\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"Παῦλος\"\\*\\w Paul|x-occurrence=\"1\" x-occurrences=\"1\" x-lemma=\"p\"\\w*\\zaln-e\\*,";
    assert!(split_aligned(odd, VerseNo::new(1).unwrap()).is_none());
}

/// Two aligned words of en_ult TIT 1:1 as tC4's export writes them.
const PAUL_A_SERVANT: &str = "\\zaln-s |x-strong=\"G39720\" x-lemma=\"Παῦλος\" x-morph=\"Gr,N,,,,,NMS,\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"Παῦλος\"\\*\\w Paul|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*,\n\\zaln-s |x-strong=\"G14010\" x-lemma=\"δοῦλος\" x-morph=\"Gr,N,,,,,NMS,\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"δοῦλος\"\\*\\w a|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\n\\w servant|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*";

/// §13.5: an aligned verse that reads apart and weaves, but not back into the same bytes, is
/// imported as written. Each input here passes `parse_aligned` and `weave`, so only the
/// byte-for-byte check keeps the import from rewriting the translator's markup.
#[test]
fn an_aligned_verse_weave_would_rewrite_is_kept_as_written() {
    let one = VerseNo::new(1).unwrap();
    assert!(split_aligned(PAUL_A_SERVANT, one).is_some());
    let rewritten = [
        // Milestone attributes in another order.
        PAUL_A_SERVANT.replacen(
            "x-strong=\"G39720\" x-lemma=\"Παῦλος\"",
            "x-lemma=\"Παῦλος\" x-strong=\"G39720\"",
            1,
        ),
        // A line break before `\zaln-e\*`.
        PAUL_A_SERVANT.replacen("\\w*\\zaln-e\\*,", "\\w*\n\\zaln-e\\*,", 1),
        // Two spaces where the export writes one line break.
        PAUL_A_SERVANT.replacen(",\n\\zaln-s", ",  \\zaln-s", 1),
        // An unaligned `\w` word after a space.
        format!("{PAUL_A_SERVANT} \\w of|x-occurrence=\"1\" x-occurrences=\"1\"\\w*"),
        // A bare unaligned word.
        format!("{PAUL_A_SERVANT} of"),
        // An attribute beyond the six named ones, written before `x-content`.
        PAUL_A_SERVANT.replacen(
            "x-content=\"Παῦλος\"",
            "x-tw=\"rc://*/tw/dict/bible/names/paul\" x-content=\"Παῦλος\"",
            1,
        ),
        // Two such attributes out of name order: the codec writes them back, the record's
        // map would not.
        PAUL_A_SERVANT.replacen(
            "x-content=\"Παῦλος\"",
            "x-content=\"Παῦλος\" x-tw=\"b\" x-abc=\"a\"",
            1,
        ),
    ];
    for content in &rewritten {
        let verse = tc_usfm::parse_aligned(content).unwrap();
        tc_usfm::weave(&verse.plain, &verse.groups).unwrap();
        assert!(
            split_aligned(content, one).is_none(),
            "split although weave does not give back {content:?}"
        );
    }
}

/// A generated aligned verse: words of a small vocabulary, each opening a group, joining
/// the open one, standing as an unaligned `\w` or bare, with the separators, milestone
/// attribute orders, extra attributes, `\zaln-e` placements and occurrence counts that
/// tC4's export writes and the ones it does not.
fn arb_aligned_verse() -> impl Strategy<Value = String> {
    const VOCAB: [&str; 5] = ["Paul", "a", "servant", "of", "God"];
    // The export's own separators first: a space before a word is written as a line break.
    const SEPARATORS: [&str; 6] = ["\n", ",\n", " ", "  ", ", ", " \n"];
    let rare = || prop::bool::weighted(0.08);
    let word = (
        0..VOCAB.len(),
        prop_oneof![4 => Just(0u8), 4 => Just(1u8), 1 => Just(2u8), 1 => Just(3u8)],
        prop_oneof![8 => 0..2usize, 1 => 2..SEPARATORS.len()],
        [rare(), rare(), rare(), rare()],
    );
    (prop::collection::vec(word, 1..7), 0..3usize).prop_map(|(words, end)| {
        let mut total = std::collections::HashMap::new();
        for (w, ..) in &words {
            *total.entry(VOCAB[*w]).or_insert(0u32) += 1;
        }
        let mut seen = std::collections::HashMap::new();
        let mut out = String::new();
        let mut open = false;
        for (i, (w, mode, sep, [late_close, swap, extra, miscount])) in words.iter().enumerate() {
            let text = VOCAB[*w];
            let joins = *mode == 1 && open;
            if i > 0 {
                let close = open && !joins;
                if close && !late_close {
                    out.push_str("\\zaln-e\\*");
                }
                out.push_str(SEPARATORS[*sep]);
                if close && *late_close {
                    out.push_str("\\zaln-e\\*");
                }
            }
            if !joins {
                open = false;
            }
            if *mode < 2 && !joins {
                let (first, second) = (format!("x-strong=\"G{i}\""), format!("x-lemma=\"l{i}\""));
                let (first, second) = if *swap { (second, first) } else { (first, second) };
                let tw = if *extra { " x-tw=\"rc://t\"" } else { "" };
                out.push_str(&format!(
                    "\\zaln-s |{first} {second} x-morph=\"Gr,N\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"c{i}\"{tw}\\*"
                ));
                open = true;
            }
            let occurrence = seen.entry(text).or_insert(0u32);
            *occurrence += 1;
            let occurrences = total[text] + u32::from(*miscount);
            if *mode < 3 {
                out.push_str(&format!(
                    "\\w {text}|x-occurrence=\"{occurrence}\" x-occurrences=\"{occurrences}\"\\w*"
                ));
            } else {
                out.push_str(text);
            }
        }
        if open {
            out.push_str("\\zaln-e\\*");
        }
        out.push_str(["", ",", "."][end]);
        out
    })
}

/// §13.5: whatever the aligned-USFM split takes apart weaves back into the same bytes, so an
/// import never rewrites a verse it splits. The census proves the generator reaches both
/// outcomes.
#[test]
fn a_split_verse_weaves_back_byte_for_byte() {
    let one = VerseNo::new(1).unwrap();
    let (split, kept) = (std::cell::Cell::new(0), std::cell::Cell::new(0));
    let config = proptest::test_runner::Config {
        cases: 1024,
        ..Default::default()
    };
    let mut runner = proptest::test_runner::TestRunner::new_with_rng(
        config,
        proptest::test_runner::TestRng::deterministic_rng(
            proptest::test_runner::RngAlgorithm::ChaCha,
        ),
    );
    runner
        .run(&arb_aligned_verse(), |content| {
            match split_aligned(&content, one) {
                Some((plain, value)) => {
                    split.set(split.get() + 1);
                    prop_assert_eq!(
                        tc_usfm::weave(&plain, &value.usfm_groups()).unwrap(),
                        content.clone()
                    );
                    prop_assert_eq!(&plain, &tc_usfm::parse_aligned(&content).unwrap().plain);
                    prop_assert_eq!(value.target_verse_md5, verse_text_md5_v1(&plain));
                }
                None => kept.set(kept.get() + 1),
            }
            Ok(())
        })
        .unwrap();
    assert!(
        split.get() >= 100 && kept.get() >= 100,
        "split {} kept {}",
        split.get(),
        kept.get()
    );
}

fn greek(content: &str, lemma: &str) -> tc_usfm::SourceWord {
    tc_usfm::SourceWord {
        content: content.to_owned(),
        strong: None,
        lemma: Some(lemma.to_owned()),
        morph: None,
        occurrence: 1,
        occurrences: 1,
        extra: Vec::new(),
    }
}

fn english(text: &str) -> tc_usfm::TargetWord {
    tc_usfm::TargetWord {
        text: text.to_owned(),
        occurrence: 1,
        occurrences: 1,
    }
}

/// Aligned USFM as an aligner writes it: `plain` with each run's source words over its
/// target words.
fn woven(plain: &str, runs: &[(&[tc_usfm::SourceWord], &[&str])]) -> String {
    let groups: Vec<tc_usfm::AlignmentGroup> = runs
        .iter()
        .map(|(sources, targets)| tc_usfm::AlignmentGroup {
            sources: sources.to_vec(),
            targets: targets.iter().map(|t| english(t)).collect(),
        })
        .collect();
    tc_usfm::weave(plain, &groups).unwrap()
}

/// INV-ONE-READER-PER-JOB keeps no-loss: tC4's cards join runs under one outermost source word,
/// and a verse whose cards would make a link its runs do not (or lose a source word's
/// attributes) is kept as written.
#[test]
fn an_aligned_verse_whose_cards_would_change_its_links_is_kept_as_written() {
    let (a, b, c) = (greek("α", "α"), greek("β", "β"), greek("γ", "γ"));
    let last = VerseNo::new(1).unwrap();
    // `A B: x` and `A C: y`: one card would link x to C and y to B.
    let crossed = woven(
        "x y",
        &[(&[a.clone(), b.clone()], &["x"]), (&[a.clone(), c], &["y"])],
    );
    assert_eq!(split_aligned(&crossed, last), None);
    // The same outer word with another lemma in the second run: one card would drop it.
    let relemma = woven(
        "x y",
        &[
            (std::slice::from_ref(&a), &["x"]),
            (&[greek("α", "ζ")], &["y"]),
        ],
    );
    assert_eq!(split_aligned(&relemma, last), None);
    // `A B: x` and `A B: y`, en_ult's TIT 1:1 shape: one card, the same links.
    let joined = woven(
        "x y",
        &[(&[a.clone(), b.clone()], &["x"]), (&[a, b], &["y"])],
    );
    let (_, value) = split_aligned(&joined, last).unwrap();
    assert_eq!(value.alignments.len(), 1);
    assert_eq!(value.alignments[0].bottom_words.len(), 2);
}
