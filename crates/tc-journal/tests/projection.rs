//! The derived files (§7.5, §7.7) and metadata.json (§3.4).

// Helpers outside `#[test]` functions build known-valid values and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use support::*;
use tc_journal::fold::{
    DerivedFile, FoldInput, FoldState, ProjectKind, decode_alignments, decode_decisions,
    records_from_usfm,
};
use tc_journal::json::Json;
use tc_journal::record::{PresentHow, Scope, UserStatus};
use tc_journal::{BookId, Cause, ImportSource, PinGroup, Record, RecordKey};

fn fixtures() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn import(dev: &mut Dev, book: BookId, usfm: &str) -> Vec<tc_journal::Change> {
    let parts = tc_usfm::codec::decompose(usfm).expect("decomposes");
    let mut items = vec![(
        Record::BookPresence {
            book,
            value: tc_journal::record::BookPresence::Present {
                scope: Scope::Whole,
                how: PresentHow::Added,
            },
        },
        vec![],
    )];
    items.extend(
        records_from_usfm(book, &parts)
            .into_iter()
            .map(|r| (r, vec![])),
    );
    dev.batch(
        items,
        Cause::Import(ImportSource::Usfm {
            file_name: "book.usfm".into(),
            unknown_base: false,
        }),
    )
}

#[test]
fn an_imported_book_projects_byte_for_byte() {
    let mut ana = Dev::new(1);
    for (book, path) in [
        (BookId::Psalms, "scale/psa/en_ult/19-PSA.usfm"),
        (BookId::Titus, "tc4/sample-burrito/ingredients/TIT.usfm"),
        (BookId::Jonah, "tc4/sample-burrito/ingredients/JON.usfm"),
    ] {
        let usfm = std::fs::read_to_string(fixtures().join(path)).expect("fixture");
        let changes = import(&mut ana, book, &usfm);
        let state = fold(&changes);
        assert_eq!(
            state.book_usfm(book).as_deref(),
            Some(usfm.as_str()),
            "{path}"
        );
        assert_eq!(state.conflicts().count(), 0, "{path}");
    }
}

#[test]
fn the_provisional_usfm_of_a_join_against_an_edit_shows_each_word_once() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    let v4 = on(&book, &verse_key(1, 4));
    let v5 = on(&book, &verse_key(1, 5));
    let join = maria.batch(
        vec![
            (verse(1, 4, text("FOUR FIVE", 5)), vec![v4.id()]),
            (verse(1, 5, covered(4)), vec![v5.id()]),
        ],
        Cause::Edit,
    );
    let edit = juan.one(verse(1, 5, text("JUAN", 5)), vec![v5.id()]);
    // Juan's edit is older: Maria's join is provisional.
    let mut all = book.clone();
    all.extend(join);
    all.push(edit);
    let state = fold(&all);
    let usfm = state.book_usfm(BOOK).expect("projects");
    assert!(usfm.contains("\\v 4-5 FOUR FIVE"), "{usfm}");
    assert!(!usfm.contains("JUAN"));
    assert_eq!(usfm.matches("\\v 5 ").count(), 0);
}

#[test]
fn a_stray_covered_verse_shows_its_own_newest_text() {
    let mut maria = Dev::new(1);
    let mut juan = Dev::new(2);
    let book = imported_book(&mut maria, &[6]);
    // Maria splits 2-3 differently from what Juan covers: Juan marks 3 covered by 2 while
    // Maria keeps 2 a single verse.
    let v3 = on(&book, &verse_key(1, 3));
    let covered3 = juan.one(verse(1, 3, covered(2)), vec![v3.id()]);
    let mut all = book.clone();
    all.push(covered3);
    let state = fold(&all);
    let usfm = state.book_usfm(BOOK).expect("projects");
    assert!(usfm.contains("\\v 3 words of 1:3"), "{usfm}");
    assert!(
        state
            .conflicts()
            .any(|c| c.kind == tc_journal::fold::ConflictKind::StructureIssue)
    );
}

#[test]
fn sidecars_compute_validity_and_decode_to_the_same_records() {
    let mut ana = Dev::new(1);
    let book = imported_book(&mut ana, &[3]);
    let pins = ana.one(
        pins(
            PinGroup::Primary,
            &[(
                "translationNotes",
                obj(&[
                    ("repoPath", Json::from(tn_pin().repo_path.as_ref())),
                    ("sha", Json::from(tn_pin().sha.to_hex())),
                    ("version", Json::from("v91")),
                    ("flavor", Json::from("parascriptural/x-bcvnotes")),
                ]),
            )],
        ),
        vec![],
    );
    let key = check_key("abcd", 1, 2);
    let mut good = decision(Some(UserStatus::Valid), true, Some("ok"));
    good.judgement.verse_text_md5 = md5(&tc_usfm::plain_text_v1("words of 1:2\n"));
    let stale_key = check_key("efgh", 1, 3);
    let stale = decision(Some(UserStatus::Invalid), false, None);
    let d1 = ana.one(check(&key, good.clone()), vec![]);
    let d2 = ana.one(check(&stale_key, stale.clone()), vec![]);
    let align = ana.one(alignment(1, 1, 1, "x"), vec![]);
    let mut all = book.clone();
    all.extend([pins, d1, d2, align]);
    let state = fold(&all);
    let files = state.derived_files();
    assert!(files.contains(&DerivedFile::Decisions(
        tc_journal::ToolId::TranslationNotes,
        BOOK
    )));
    let bytes = state
        .render(&DerivedFile::Decisions(
            tc_journal::ToolId::TranslationNotes,
            BOOK,
        ))
        .expect("renders");
    let text = String::from_utf8(bytes.clone()).expect("utf-8");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("json");
    let decisions = doc["decisions"].as_array().expect("decisions");
    assert_eq!(decisions.len(), 2);
    let good_doc = decisions
        .iter()
        .find(|d| d["reminders"] == true)
        .expect("the good one");
    assert_eq!(good_doc["invalidated"], false);
    assert_eq!(good_doc["verseEdits"], false);
    let stale_doc = decisions
        .iter()
        .find(|d| d["reminders"] == false)
        .expect("stale");
    assert_eq!(stale_doc["verseEdits"], true);
    // One record per line.
    assert_eq!(text.lines().count(), 4, "{text}");
    let decoded = decode_decisions(tc_journal::ToolId::TranslationNotes, Some(BOOK), &bytes)
        .expect("decodes");
    let mut expected = vec![check(&key, good), check(&stale_key, stale)];
    let mut decoded = decoded;
    expected.sort_by_key(|r| format!("{:?}", r.key()));
    decoded.sort_by_key(|r| format!("{:?}", r.key()));
    assert_eq!(decoded, expected);
    let bytes = state
        .render(&DerivedFile::Alignments(BOOK))
        .expect("alignments");
    let decoded = decode_alignments(BOOK, &bytes).expect("decodes");
    assert_eq!(decoded, vec![alignment(1, 1, 1, "x")]);
    let doc: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        doc["chapters"]["1"]["1"]["invalid"], true,
        "made for other text"
    );
}

#[test]
fn metadata_builds_and_passes_for_the_projection() {
    let mut ana = Dev::new(1);
    let mut all = imported_book(&mut ana, &[3]);
    all.push(ana.one(
        Record::ProjectMeta {
            field: tc_journal::MetaField::Name,
            value: Some(obj(&[("en", Json::from("Tito"))])),
        },
        vec![],
    ));
    let bad = ana.one(
        Record::ProjectMeta {
            field: tc_journal::MetaField::Copyright,
            value: Some(Json::from("All rights")),
        },
        vec![],
    );
    all.push(bad.clone());
    let state = fold(&all);
    assert!(state.anomalies().any(|a| matches!(
        a,
        tc_journal::fold::Anomaly::InvalidForBurrito { change, .. } if *change == bad.id()
    )));
    let mut table = Vec::new();
    for file in state.derived_files() {
        let bytes = state.render(&file).expect("renders");
        table.push(tc_burrito::IngredientEntry {
            path: tc_burrito::IngredientPath::parse(&file.path()).expect("path"),
            md5: tc_burrito::Md5::of(&bytes),
            size: bytes.len() as u64,
            role: file.role().map(str::to_owned),
            scope: file.scope(&[]),
        });
    }
    let source = state.metadata_source((1, 0));
    assert_eq!(
        source.fields.get("identification.name"),
        Some(&serde_json::json!({"en": "Tito"}))
    );
    assert!(!source.fields.contains_key("copyright"));
    let bytes = tc_burrito::build_metadata(&source, &table).expect("valid");
    assert!(tc_burrito::validate_metadata_bytes(&bytes).is_empty());
}

#[test]
fn a_chapter_projects_alone() {
    let mut ana = Dev::new(1);
    let book = imported_book(&mut ana, &[2, 3]);
    let state = fold(&book);
    let chapter = state.chapter_usfm(BOOK, 2).expect("chapter");
    assert!(chapter.starts_with("\\c 2"), "{chapter}");
    assert!(chapter.contains("\\v 3 words of 2:3"));
    assert!(!chapter.contains("1:1"));
    let _ = FoldState::new(ProjectKind::Bible);
    let _ = FoldInput::from(book[0].clone());
    let _ = RecordKey::Versification;
}

#[test]
fn a_story_splices_its_frames_and_reference_into_the_template() {
    let mut ana = Dev::new(1);
    let story = tc_journal::StoryNo::new(1).expect("story");
    let frame = tc_journal::FrameNo::new;
    let template = "# The Creation\n\n![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-01.jpg)\n\nThis is how God made everything.\n\n![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-02.jpg)\n\nBut the earth was empty.\n\n_A Bible story from: Genesis 1-2_\n";
    let changes = vec![
        ana.one(
            Record::StoryBase {
                story,
                value: template.into(),
            },
            vec![],
        ),
        ana.one(
            Record::Frame {
                story,
                frame: frame(0),
                value: "La creación".into(),
            },
            vec![],
        ),
        ana.one(
            Record::Frame {
                story,
                frame: frame(1),
                value: "Así es como Dios hizo todo.".into(),
            },
            vec![],
        ),
        ana.one(
            Record::StoryRef {
                story,
                value: Some("Una historia bíblica de: Génesis 1-2".into()),
            },
            vec![],
        ),
    ];
    let state = FoldState::fold(
        ProjectKind::Stories,
        changes.into_iter().map(FoldInput::from),
    );
    let file = DerivedFile::Story(story);
    assert!(state.derived_files().contains(&file));
    let bytes = state.render(&file).expect("story");
    assert_eq!(
        String::from_utf8(bytes).expect("utf-8"),
        "# La creación\n\n![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-01.jpg)\n\nAsí es como Dios hizo todo.\n\n![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-02.jpg)\n\nBut the earth was empty.\n\n_Una historia bíblica de: Génesis 1-2_\n",
        "frame 2 has no record and keeps the template's text"
    );
}

fn story_one_with(template: &str, frame_zero: Option<&str>) -> String {
    let mut ana = Dev::new(1);
    let story = tc_journal::StoryNo::new(1).expect("story");
    let mut changes = vec![ana.one(
        Record::StoryBase {
            story,
            value: template.into(),
        },
        vec![],
    )];
    if let Some(title) = frame_zero {
        changes.push(ana.one(
            Record::Frame {
                story,
                frame: tc_journal::FrameNo::TITLE,
                value: title.into(),
            },
            vec![],
        ));
    }
    let state = FoldState::fold(
        ProjectKind::Stories,
        changes.into_iter().map(FoldInput::from),
    );
    String::from_utf8(state.render(&DerivedFile::Story(story)).expect("story")).expect("utf-8")
}

/// tC4's `TITLE_RE` and `writeTitle`: frame 0 is what follows `# N. `, and the number is the
/// line's own, so a title a person types never loses it (docs/evidence/state-seed-check-2026-10-18.md
/// §4.1, §5.1).
#[test]
fn a_story_title_is_the_text_after_its_number() {
    let template = "# 1. The Creation\n\n![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-01.jpg)\n\nText.\n";
    assert_eq!(
        story_one_with(template, None),
        template,
        "no record: untouched"
    );
    assert_eq!(
        story_one_with(template, Some("La Creación")),
        template.replace("The Creation", "La Creación")
    );
    assert_eq!(
        story_one_with(template, Some("")),
        template.replace("# 1. The Creation", "# 1."),
        "an empty title is `# N.`"
    );
    let seed = template.replace("# 1. The Creation", "# 1.");
    assert_eq!(
        story_one_with(&seed, Some("La Creación")),
        template.replace("The Creation", "La Creación")
    );
}

#[test]
fn a_story_titles_check_text_has_no_number() {
    let mut ana = Dev::new(1);
    let story = tc_journal::StoryNo::new(2).expect("story");
    let template = "# 2. How God made it\n\n![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-02-01.jpg)\n\nText.\n";
    let seed = FoldState::fold(
        ProjectKind::Stories,
        [FoldInput::from(ana.one(
            Record::StoryBase {
                story,
                value: template.into(),
            },
            vec![],
        ))],
    );
    let texts = seed.check_texts(tc_journal::pins::Collection::Stories);
    let target = tc_journal::CheckTarget::Frame {
        story,
        frame: tc_journal::FrameNo::TITLE,
    };
    let title = texts.text(&target).expect("the title has a text");
    assert_eq!(title.content(), "How God made it");
}

/// tC4's `relationshipsFromPins`: a pin whose flavor is not scripture, parascriptural or
/// peripheral (the OBS source text is `gloss/textStories`) has no relationship, since the
/// schema would refuse it and the whole group's relationships would be dropped
/// (docs/evidence/state-seed-check-2026-10-18.md §4.3).
#[test]
fn a_pin_that_is_no_scripture_has_no_relationship_and_makes_no_anomaly() {
    let mut ana = Dev::new(1);
    let slot = |repo: &str, flavor: &str| {
        obj(&[
            ("repoPath", Json::from(repo)),
            ("version", Json::from("v1")),
            ("flavor", Json::from(flavor)),
        ])
    };
    let changes = vec![ana.one(
        support::pins(
            PinGroup::Primary,
            &[
                ("obs", slot("git.door43.org/uW/en_obs", "gloss/textStories")),
                (
                    "obsNotes",
                    slot("git.door43.org/uW/en_obs-tn", "parascriptural/x-obsNotes"),
                ),
                ("bare", slot("git.door43.org/uW/en_bare", "textTranslation")),
            ],
        ),
        vec![],
    )];
    let state = fold(&changes);
    assert_eq!(
        state
            .anomalies()
            .filter(|a| matches!(a, tc_journal::fold::Anomaly::InvalidForBurrito { .. }))
            .count(),
        0
    );
    let source = state.metadata_source((1, 0));
    let ids: Vec<&str> = source
        .relationships
        .iter()
        .map(|r| r["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, ["dcs::uW/en_obs-tn"]);
}

/// A story's check session reads that story's decisions and comments only (R1): the other
/// forty-nine are never touched, however many they hold.
#[test]
fn a_story_reads_its_own_decisions_and_comments() {
    use tc_journal::record::{CommentTarget, UserStatus};
    use tc_journal::{CheckId, CheckKey, CheckTarget, FrameNo, ToolId};
    let mut ana = Dev::new(1);
    let template = |n: u8| {
        format!(
            "# {n}. T\n\n![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-{n:02}-01.jpg)\n\nText.\n"
        )
    };
    let story = |n: u8| tc_journal::StoryNo::new(n).expect("story");
    let mut changes = Vec::new();
    for n in [1, 2] {
        changes.push(ana.one(
            Record::StoryBase {
                story: story(n),
                value: template(n),
            },
            vec![],
        ));
    }
    let key = |n: u8, id: &str| CheckKey {
        tool: ToolId::TranslationNotes,
        check_id: CheckId::parse(id).expect("id"),
        target: CheckTarget::Frame {
            story: story(n),
            frame: FrameNo::new(1),
        },
        occurrence: 1,
    };
    for (n, id) in [(1, "aaaa"), (1, "bbbb"), (2, "cccc")] {
        changes.push(ana.one(
            check(&key(n, id), decision(Some(UserStatus::Valid), false, None)),
            vec![],
        ));
    }
    let on_frame = |n: u8| CommentTarget::Frame {
        story: story(n),
        frame: FrameNo::new(1),
    };
    changes.push(ana.one(comment(1, on_frame(1), "uno"), vec![]));
    changes.push(ana.one(comment(2, on_frame(2), "dos"), vec![]));
    changes.push(ana.one(
        comment(3, CommentTarget::Check(key(1, "aaaa")), "sobre el chequeo"),
        vec![],
    ));
    let state = FoldState::fold(
        ProjectKind::Stories,
        changes.into_iter().map(FoldInput::from),
    );

    let decisions = |n: u8| -> Vec<String> {
        state
            .story_decisions(ToolId::TranslationNotes, story(n))
            .iter()
            .map(|d| d.key.check_id.as_str().to_owned())
            .collect()
    };
    assert_eq!(decisions(1), ["aaaa", "bbbb"]);
    assert_eq!(decisions(2), ["cccc"]);
    assert_eq!(decisions(3), Vec::<String>::new(), "a story with none");
    assert!(
        state
            .story_decisions(ToolId::TranslationWords, story(1))
            .is_empty(),
        "another tool's decisions are not these"
    );
    // The story's own comments: on its frames and on its checks, never another story's.
    assert_eq!(state.story_comments_of(story(1)).len(), 2);
    assert_eq!(state.story_comments_of(story(2)).len(), 1);
    assert!(state.story_comments_of(story(3)).is_empty());
    // The whole collection's read still holds all of them.
    assert_eq!(
        state
            .decisions(
                ToolId::TranslationNotes,
                tc_journal::pins::Collection::Stories
            )
            .len(),
        3
    );
}
