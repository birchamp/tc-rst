//! The per-change rules V1 to V8 and the size limits (`docs/STORAGE.md` §4.5, §4.6). Each
//! test breaks one rule in otherwise valid bytes and checks the change is quarantined for
//! that rule, and that the writer refuses to produce it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use proptest::prelude::*;
use std::sync::Arc;
use tc_journal::json::{self, Json};
use tc_journal::record::{
    BatchReleaseValue, ChapterFront, CommentTarget, CommentValue, DeviceProfileValue, UsfmBytes,
    VerseValue,
};
use tc_journal::rules::{MAX_LINE_BYTES, MAX_PARENTS, Rule};
use tc_journal::{
    BatchId, BookId, Change, ChangeBody, ChangeError, ChangeId, CommentId, DeviceId, DeviceKind,
    Record, RuleViolation, SettingKey, Verified, parse_change,
};

fn json_of(body: &ChangeBody) -> Json {
    json::parse(Change::new(body.clone()).unwrap().raw()).unwrap()
}

fn read(json: &Json) -> Verified {
    let raw = json.to_canonical();
    parse_change(ChangeId::of(&raw), Arc::from(raw))
}

fn member<'a>(json: &'a mut Json, path: &[&str]) -> &'a mut Json {
    let mut at = json;
    for key in path {
        let Json::Obj(map) = at else {
            panic!("{key} is not inside an object")
        };
        at = map.get_mut(*key).unwrap_or_else(|| panic!("no {key}"));
    }
    at
}

fn set(json: &mut Json, path: &[&str], key: &str, value: Json) {
    let Json::Obj(map) = member(json, path) else {
        panic!("not an object")
    };
    map.insert(key.to_owned(), value);
}

fn quarantined(v: Verified) -> RuleViolation {
    match v {
        Verified::Quarantined(q) => q.rule,
        Verified::Change(c) => panic!("accepted {}", String::from_utf8_lossy(c.raw())),
    }
}

/// Breaks a rule through the writer: `Change::new` must refuse with that rule.
fn refused(body: ChangeBody) -> RuleViolation {
    match Change::new(body) {
        Err(ChangeError::Rule(rule)) => rule,
        other => panic!("not refused by a rule: {other:?}"),
    }
}

fn verse(words: &str, layout: &str, last: u16, v: u16) -> ChangeBody {
    let mut body = verse_body(local(1), 1, v, "x");
    body.record = Record::Verse {
        book: BookId::Titus,
        chapter: chapter(1),
        verse: verse_no(v),
        value: VerseValue::Text {
            marker: tc_journal::record::MarkerWs::canonical(),
            words: words.into(),
            layout: layout.into(),
            last: verse_no(last),
            from: None,
        },
    };
    body
}

// ---------------------------------------------------------------------------------------
// V1
// ---------------------------------------------------------------------------------------

#[test]
fn v1_shape_unknown_missing_and_mistyped_fields() {
    let body = verse_body(local(1), 1, 1, "a");
    let mut json = json_of(&body);
    set(&mut json, &[], "extra", Json::from(1u8));
    assert_eq!(
        quarantined(read(&json)),
        RuleViolation::Shape {
            at: "".into(),
            problem: "unknown field \"extra\"".into()
        }
    );

    let mut json = json_of(&body);
    let Json::Obj(map) = &mut json else { panic!() };
    map.remove("seq");
    assert_eq!(
        quarantined(read(&json)),
        RuleViolation::Shape {
            at: "".into(),
            problem: "missing seq".into()
        }
    );

    let mut json = json_of(&body);
    set(&mut json, &[], "seq", Json::from("1"));
    assert_eq!(quarantined(read(&json)).rule(), Rule::V1);

    let mut json = json_of(&body);
    set(&mut json, &["batch"], "n", Json::from(0u8));
    assert_eq!(
        quarantined(read(&json)),
        RuleViolation::Shape {
            at: "/batch/n".into(),
            problem: "a batch holds at least one change".into()
        }
    );

    let mut json = json_of(&body);
    set(&mut json, &["record", "verse"], "chapter", Json::from(0u8));
    assert_eq!(quarantined(read(&json)).rule(), Rule::V1);

    let mut json = json_of(&body);
    set(&mut json, &[], "cause", Json::from("edit"));
    assert_eq!(quarantined(read(&json)).rule(), Rule::V1);

    let mut json = json_of(&body);
    set(&mut json, &[], "v", Json::from(70_000u32));
    assert_eq!(quarantined(read(&json)).rule(), Rule::V1);

    assert_eq!(
        quarantined(read(&Json::Arr(vec![]))),
        RuleViolation::Shape {
            at: "".into(),
            problem: "expected an object, found an array".into()
        }
    );
}

#[test]
fn v1_shape_lists_that_must_be_sorted_and_maps_that_must_be_omitted() {
    let mut body = verse_body(local(1), 1, 1, "a");
    body.parents.clear();
    body.record = Record::BatchRelease {
        batch: BatchId::from_bytes([3; 16]),
        value: BatchReleaseValue {
            changes: vec![ChangeId::from_bytes([2; 32]), ChangeId::from_bytes([1; 32])],
        },
    };
    assert_eq!(refused(body).rule(), Rule::V1);

    let alignment = r#"{"alignments":[],"dismissed":false,"done":false,"extra":{},"markedInvalid":false,"sourceVersion":"x","spanEnd":1,"targetVerseMd5":"00000000000000000000000000000000","wordBank":[]}"#;
    let mut json = json_of(&verse_body(local(1), 1, 1, "a"));
    set(
        &mut json,
        &[],
        "record",
        json::parse(
            format!(
                r#"{{"alignment":{{"book":"TIT","chapter":1,"value":{alignment},"verse":1}}}}"#
            )
            .as_bytes(),
        )
        .unwrap(),
    );
    assert_eq!(
        quarantined(read(&json)),
        RuleViolation::Shape {
            at: "/record/alignment/value/extra".into(),
            problem: "an empty extra map is left out".into()
        }
    );
    let fixed = alignment.replace(r#""extra":{},"#, "");
    set(
        &mut json,
        &[],
        "record",
        json::parse(
            format!(r#"{{"alignment":{{"book":"TIT","chapter":1,"value":{fixed},"verse":1}}}}"#)
                .as_bytes(),
        )
        .unwrap(),
    );
    assert!(matches!(read(&json), Verified::Change(_)));
}

/// Values that have one valid form: a writer cannot produce the others, and a reader
/// quarantines them, so every value has exactly one byte form and one meaning.
#[test]
fn v1_values_outside_their_grammar_are_refused() {
    use tc_journal::record::{CheckValue, DecisionRecord, Door43Username, Judgement, ResourceRef};
    use tc_journal::{CheckId, CheckKey, CheckTarget, GitOid, Md5, RefPart, ToolId};

    // A verse marker's lead is one or more spaces or tabs: `\v1 ` is no verse marker, so a
    // verse written with it would join the verse before it.
    for lead in ["", "x", " x", "\n"] {
        let mut json = json_of(&verse_body(local(1), 1, 1, "a"));
        set(
            &mut json,
            &["record", "verse", "value", "text", "marker"],
            "lead",
            Json::from(lead),
        );
        assert_eq!(quarantined(read(&json)).rule(), Rule::V1, "{lead:?}");
    }
    assert!(tc_journal::record::MarkerWs::new("", tc_journal::record::MarkerTerm::Space).is_none());

    // A Door43 user name is never empty.
    let mut profile = verse_body(local(1), 1, 1, "a");
    profile.record = Record::DeviceProfile {
        device: local(1),
        value: DeviceProfileValue {
            display_name: None,
            door43_username: Some(Door43Username("maria".into())),
            legacy: None,
        },
    };
    let mut json = json_of(&profile);
    set(
        &mut json,
        &["record", "deviceProfile", "value"],
        "door43Username",
        Json::from(""),
    );
    assert_eq!(quarantined(read(&json)).rule(), Rule::V1);
    if let Record::DeviceProfile { value, .. } = &mut profile.record {
        value.door43_username = Some(Door43Username("".into()));
    }
    assert_eq!(refused(profile).rule(), Rule::V1);

    // A resource pin always names its repository.
    let mut decision = verse_body(local(1), 1, 1, "a");
    let resource = |repo_path: &str| ResourceRef {
        repo_path: repo_path.into(),
        sha: GitOid::from_bytes([1; 20]),
    };
    decision.record = Record::Check {
        key: CheckKey {
            tool: ToolId::TranslationNotes,
            check_id: CheckId::parse("abcd").unwrap(),
            target: CheckTarget::Verse {
                book: BookId::Titus,
                chapter: RefPart::parse("1").unwrap(),
                verse: RefPart::parse("1").unwrap(),
            },
            occurrence: 1,
        },
        value: CheckValue::Decision(DecisionRecord {
            context: Default::default(),
            category: None,
            judgement: Judgement {
                selections: None,
                nothing_to_select: false,
                status: None,
                verse_text_md5: Md5::from_bytes([2; 16]),
            },
            comment: None,
            bookmarked: false,
            resource: resource("en_tn"),
            imported_invalid: false,
            dismissed: false,
            extra: Default::default(),
        }),
    };
    let mut json = json_of(&decision);
    set(
        &mut json,
        &["record", "check", "value", "decision", "resource"],
        "repoPath",
        Json::from(""),
    );
    assert_eq!(quarantined(read(&json)).rule(), Rule::V1);
    if let Record::Check {
        value: CheckValue::Decision(record),
        ..
    } = &mut decision.record
    {
        record.resource = resource("");
    }
    assert_eq!(refused(decision).rule(), Rule::V1);
}

/// A book's passage set is written as SB writes it, and only that text reads back: any
/// text that decodes is the text the value encodes to, so the scope has one byte form.
#[test]
fn a_scope_range_reads_back_only_from_its_own_text() {
    use tc_journal::record::{BookPresence, PresentHow, Scope, ScopeRange, ScopeRanges};
    let mut body = verse_body(local(1), 1, 1, "a");
    body.record = Record::BookPresence {
        book: BookId::Titus,
        value: BookPresence::Present {
            scope: Scope::Ranges(
                ScopeRanges::new(vec!["1:1-9".parse::<ScopeRange>().unwrap()]).unwrap(),
            ),
            how: PresentHow::Added,
        },
    };
    let base = json_of(&body);
    let texts = prop::collection::vec(
        prop::sample::select(vec![
            "0", "1", "2", "9", "10", ":", "-", " ", "+", "\u{661}",
        ]),
        1..8,
    )
    .prop_map(|parts| parts.concat());
    run_with_census(512, texts, &["read", "refused"], |text, census| {
        let mut json = base.clone();
        set(
            &mut json,
            &["record", "bookPresence", "value", "present", "scope"],
            "ranges",
            Json::Arr(vec![Json::from(text.as_str())]),
        );
        match read(&json) {
            Verified::Change(change) => {
                census.saw("read");
                let again = Change::new(change.body().clone()).unwrap();
                prop_assert_eq!(again.raw(), change.raw(), "{}", text);
            }
            Verified::Quarantined(_) => census.saw("refused"),
        }
        Ok(())
    });
}

// ---------------------------------------------------------------------------------------
// V2
// ---------------------------------------------------------------------------------------

#[test]
fn v2_identity_strings_are_refused_not_repaired() {
    let base = verse_body(local(1), 1, 1, "a");
    let with_record = |record: &str| {
        let mut json = json_of(&base);
        set(
            &mut json,
            &[],
            "record",
            json::parse(record.as_bytes()).unwrap(),
        );
        quarantined(read(&json))
    };
    for record in [
        r#"{"setting":{"key":"__proto__","value":1}}"#,
        "{\"setting\":{\"key\":\"cafe\u{301}\",\"value\":1}}",
        r#"{"setting":{"key":"","value":1}}"#,
        r#"{"pins":{"group":"primary","value":{"translation-notes":{}}}}"#,
        r#"{"carriedFile":{"path":"ingredients/a b.wav","value":{"removed":{}}}}"#,
        r#"{"carriedFile":{"path":"ingredients/a&b.wav","value":{"removed":{}}}}"#,
        r#"{"projectMeta":{"field":{"extra":"languages"}}}"#,
        r#"{"check":{"key":{"checkId":"a|b","occurrence":1,"target":{"frame":{"frame":1,"story":1}},"tool":"translationNotes"},"value":{"movedTo":{"to":{"checkId":"a","occurrence":1,"target":{"frame":{"frame":1,"story":1}},"tool":"translationNotes"}}}}}"#,
        r#"{"check":{"key":{"checkId":"a","occurrence":1,"target":{"verse":{"book":"TIT","chapter":"1:1","verse":"1"}},"tool":"translationNotes"},"value":{"movedTo":{"to":{"checkId":"a","occurrence":1,"target":{"frame":{"frame":1,"story":1}},"tool":"translationNotes"}}}}}"#,
    ] {
        let rule = with_record(record);
        assert_eq!(rule.rule(), Rule::V2, "{record}: {rule}");
    }
    // A path in a cause is an identity too.
    let mut json = json_of(&base);
    set(
        &mut json,
        &[],
        "cause",
        json::parse(br#"{"external":{"source":{"localFile":{"path":"ingredients/TIT.usfm ","seenMd5":"00000000000000000000000000000000"}},"unknownBase":false}}"#)
            .unwrap(),
    );
    assert_eq!(quarantined(read(&json)).rule(), Rule::V2);
    assert!(SettingKey::parse("cafe\u{301}").is_err());
}

// ---------------------------------------------------------------------------------------
// V3
// ---------------------------------------------------------------------------------------

#[test]
fn v3_verse_values_never_hold_markers_and_keep_their_layout_rule() {
    assert!(Change::new(verse("a", "\n\\p\n", 1, 1)).is_ok());
    assert!(Change::new(verse("", "\\p\n", 1, 1)).is_ok());
    assert!(Change::new(verse("a \\vp 2\\vp*", "", 3, 2)).is_ok());
    for (words, layout, last, v) in [
        ("a \\v 2 b", "", 1, 1),
        ("a\n\\c 2", "", 1, 1),
        ("a \\v", "", 1, 1),
        ("a", "\n\\v 2 b", 1, 1),
        ("a", "\\p\n", 1, 1),
        ("a", "\nmore words", 1, 1),
        ("a", "", 1, 2),
    ] {
        let rule = refused(verse(words, layout, last, v));
        assert_eq!(rule.rule(), Rule::V3, "{words:?} {layout:?}: {rule}");
    }
    let mut covered = verse_body(local(1), 1, 3, "x");
    covered.record = Record::Verse {
        book: BookId::Titus,
        chapter: chapter(1),
        verse: verse_no(3),
        value: VerseValue::Covered { by: verse_no(3) },
    };
    assert_eq!(refused(covered).rule(), Rule::V3);
}

#[test]
fn v3_headers_and_chapter_fronts_keep_the_book_structure() {
    let with = |record: Record| {
        let mut body = verse_body(local(1), 1, 1, "x");
        body.record = record;
        body
    };
    let header = |text: &str| {
        with(Record::BookHeader {
            book: BookId::Titus,
            value: UsfmBytes(text.into()),
        })
    };
    let front = |n: u16, text: &str| {
        with(Record::ChapterFront {
            book: BookId::Titus,
            chapter: chapter(n),
            value: ChapterFront::Present(UsfmBytes(text.into())),
        })
    };
    assert!(Change::new(header("\\id TIT\n\\v 1 kept in the header\n")).is_ok());
    assert_eq!(refused(header("\\id TIT\n\\c 1\n")).rule(), Rule::V3);
    for ok in [
        "\\c 2\n",
        "\\c 02\r\n\\p\n",
        "\\c 000002\n",
        "\\c 0000000000000000000002\n",
        "\\c\t2",
        "\\c 2 \\ca 3\\ca*\n\\s1 Title\n",
    ] {
        assert!(Change::new(front(2, ok)).is_ok(), "{ok:?}");
    }
    for bad in [
        "\\c 3\n",
        "\\p\n\\c 2\n",
        "\\c2\n",
        "\\c 2x\n",
        "\\c 2\n\\v 1 x",
        "\\c 2\n\\c 3\n",
        "\\c 000003\n",
        "\\c 00000\n",
        "\\c 100002\n",
        "",
    ] {
        assert_eq!(refused(front(2, bad)).rule(), Rule::V3, "{bad:?}");
    }
    assert!(
        Change::new(with(Record::ChapterFront {
            book: BookId::Titus,
            chapter: chapter(2),
            value: ChapterFront::Absent,
        }))
        .is_ok()
    );
}

/// A book with a verse span of `span` extra verses after `first`, written with `lead`,
/// `term`, words and layout from the pools below.
#[derive(Debug, Clone)]
struct GenVerse {
    span: u16,
    lead: &'static str,
    term: &'static str,
    words: &'static str,
    layout: &'static str,
}

fn gen_book() -> impl Strategy<Value = String> {
    let verse = (
        prop_oneof![4 => Just(0u16), 1 => 1u16..4],
        prop::sample::select(vec![" ", "\t", "  "]),
        prop::sample::select(vec![" ", "\t", "\n", "\r"]),
        prop::sample::select(vec![
            "words",
            "line one\n\\q2 line two",
            "",
            "\\add x\\add*",
            "a\n\\s1 Heading",
            "\\w uno|x-occurrence=\"1\"\\w*",
        ]),
        prop::sample::select(vec!["", "\n", "\n\\p\n", "\r\n\\q1\r\n", "\n\\ts\\*\n"]),
    )
        .prop_map(|(span, lead, term, words, layout)| GenVerse {
            span,
            lead,
            term,
            words,
            layout,
        });
    let chapter = (
        0usize..8,
        prop::sample::select(vec![" ", "\t"]),
        prop::sample::select(vec!["\n", "\r\n", " \\ca 3\\ca*\n", "\n\\s1 Title\n\\p\n"]),
        prop::collection::vec(verse, 0..5),
        any::<bool>(),
    );
    (
        prop::sample::select(vec![
            "",
            "\\id TIT\n",
            "\\id TIT x\r\n\\h Titus\r\n",
            "\\id TIT\n\\v 1 before\n",
        ]),
        prop::collection::vec(chapter, 1..4),
    )
        .prop_map(|(header, chapters)| {
            let mut book = header.to_owned();
            for (index, (zeros, blank, after, verses, reversed)) in chapters.into_iter().enumerate()
            {
                book.push_str(&format!(
                    "\\c{blank}{}{}{after}",
                    "0".repeat(zeros),
                    index + 1
                ));
                let mut first = 1u16;
                let mut written: Vec<String> = verses
                    .iter()
                    .map(|v| {
                        let label = if v.span > 0 {
                            format!("{first}-{}", first + v.span)
                        } else {
                            first.to_string()
                        };
                        first += v.span + 1;
                        // A verse ends where the next marker starts, so each one ends its
                        // own line.
                        format!("\\v{}{label}{}{}{}\n", v.lead, v.term, v.words, v.layout)
                    })
                    .collect();
                if reversed {
                    written.reverse();
                }
                book.extend(written);
            }
            book
        })
}

/// Every part the codec cuts from a book passes the per-change rules, so an import, an
/// outside edit or a Door43 web edit never writes a change that readers quarantine (which
/// would drop a chapter's headings and paragraph markers from the projected book).
#[test]
fn every_part_decompose_produces_passes_the_rules() {
    use tc_journal::VerseNo;
    use tc_usfm::codec::{VersePart, decompose};
    run_with_census(
        256,
        gen_book(),
        &[
            "zero-padded chapter",
            "span",
            "verse in header",
            "book layout",
            "CR terminator",
            "layout",
        ],
        |book, census| {
            let parts =
                decompose(&book).map_err(|e| TestCaseError::fail(format!("{e}: {book:?}")))?;
            let with = |record: Record| {
                let mut body = verse_body(local(1), 1, 1, "x");
                body.record = record;
                Change::new(body)
                    .map(|_| ())
                    .map_err(|e| TestCaseError::fail(format!("{e}: {book:?}")))
            };
            if parts.header.contains("\\v ") {
                census.saw("verse in header");
            }
            with(Record::BookHeader {
                book: BookId::Titus,
                value: UsfmBytes(parts.header.clone()),
            })?;
            if let Some(layout) = &parts.layout {
                census.saw("book layout");
                let items = layout
                    .iter()
                    .map(|item| match *item {
                        tc_usfm::codec::LayoutItem::Chapter(c) => {
                            tc_journal::record::LayoutItem::Chapter(chapter(c))
                        }
                        tc_usfm::codec::LayoutItem::Verse(c, v) => {
                            tc_journal::record::LayoutItem::Verse(chapter(c), verse_no(v))
                        }
                    })
                    .collect();
                with(Record::BookLayout {
                    book: BookId::Titus,
                    value: Some(items),
                })?;
            }
            for (number, part) in &parts.chapters {
                let front = part.front.clone().unwrap_or_default();
                if front.starts_with("\\c 0") || front.starts_with("\\c\t0") {
                    census.saw("zero-padded chapter");
                }
                with(Record::ChapterFront {
                    book: BookId::Titus,
                    chapter: chapter(*number),
                    value: ChapterFront::Present(UsfmBytes(front)),
                })?;
                for (verse, value) in &part.verses {
                    let value = match value {
                        VersePart::Text(text) => {
                            if text.last > *verse {
                                census.saw("span");
                            }
                            if text.marker.term() == tc_journal::record::MarkerTerm::Cr {
                                census.saw("CR terminator");
                            }
                            if !text.layout.is_empty() {
                                census.saw("layout");
                            }
                            VerseValue::Text {
                                marker: text.marker.clone(),
                                words: text.words.clone(),
                                layout: text.layout.clone(),
                                last: verse_no(text.last),
                                from: None,
                            }
                        }
                        VersePart::Covered { by } => VerseValue::Covered { by: verse_no(*by) },
                    };
                    with(Record::Verse {
                        book: BookId::Titus,
                        chapter: chapter(*number),
                        verse: VerseNo::new(*verse).unwrap(),
                        value,
                    })?;
                }
            }
            Ok(())
        },
    );
}

// ---------------------------------------------------------------------------------------
// V4 to V6
// ---------------------------------------------------------------------------------------

#[test]
fn v4_only_a_device_or_the_importer_writes_its_profile() {
    let profile = |device: DeviceId, author: DeviceId| {
        let mut body = verse_body(author, 1, 1, "x");
        body.record = Record::DeviceProfile {
            device,
            value: DeviceProfileValue::default(),
        };
        body
    };
    let actor = DeviceId::derived(DeviceKind::Tc4Actor, b"tc4-actor\0maria");
    let importer = DeviceId::derived(DeviceKind::Importer, b"import\0tc4");
    assert!(Change::new(profile(local(1), local(1))).is_ok());
    assert!(Change::new(profile(actor, importer)).is_ok());
    assert_eq!(
        refused(profile(local(1), local(2))),
        RuleViolation::DeviceProfile {
            profile: local(1),
            author: local(2)
        }
    );
    assert_eq!(refused(profile(local(1), importer)).rule(), Rule::V4);
    assert_eq!(refused(profile(actor, local(2))).rule(), Rule::V4);
}

#[test]
fn v5_parents_are_sorted_unique_and_at_most_4096() {
    let ids: Vec<ChangeId> = (0..=MAX_PARENTS)
        .map(|i| ChangeId::of(&i.to_be_bytes()))
        .collect();
    let mut sorted = ids[..MAX_PARENTS].to_vec();
    sorted.sort();
    let mut body = verse_body(local(1), 1, 1, "x");
    body.parents = sorted.clone();
    assert!(Change::new(body.clone()).is_ok());

    let mut too_many = ids.clone();
    too_many.sort();
    body.parents = too_many;
    assert_eq!(refused(body.clone()).rule(), Rule::V5);

    body.parents = vec![sorted[1], sorted[0]];
    assert_eq!(refused(body.clone()).rule(), Rule::V5);
    body.parents = vec![sorted[0], sorted[0]];
    assert_eq!(refused(body).rule(), Rule::V5);
}

#[test]
fn v6_comments_and_releases_are_grow_only() {
    let mut body = verse_body(local(1), 1, 1, "x");
    body.parents = vec![ChangeId::from_bytes([1; 32])];
    body.record = Record::Comment {
        id: CommentId::from_bytes([5; 16]),
        value: CommentValue {
            target: CommentTarget::Verse {
                book: BookId::Titus,
                chapter: chapter(1),
                verse: verse_no(1),
            },
            text: "¿Por qué?".into(),
        },
    };
    assert_eq!(
        refused(body.clone()),
        RuleViolation::GrowOnly {
            kind: "comment".into()
        }
    );
    body.record = Record::BatchRelease {
        batch: BatchId::from_bytes([3; 16]),
        value: BatchReleaseValue { changes: vec![] },
    };
    assert_eq!(refused(body.clone()).rule(), Rule::V6);
    body.parents.clear();
    assert!(Change::new(body).is_ok());
}

// ---------------------------------------------------------------------------------------
// V7 and the size limits
// ---------------------------------------------------------------------------------------

fn setting(value: Json) -> ChangeBody {
    let mut body = verse_body(local(1), 1, 1, "x");
    body.record = Record::Setting {
        key: SettingKey::parse("ui").unwrap(),
        value: Some(value),
    };
    body
}

#[test]
fn v7_nesting_is_at_most_64_levels() {
    // The change, its record and the setting are three levels; the value adds the rest.
    let nested = |levels: usize| (0..levels).fold(Json::Null, |inner, _| Json::Arr(vec![inner]));
    assert!(Change::new(setting(nested(61))).is_ok());
    assert_eq!(refused(setting(nested(62))), RuleViolation::TooDeep);
    let json = json_of(&setting(nested(61)));
    let text = json.to_canonical_string().replace("[null]", "[[null]]");
    let raw = text.into_bytes();
    assert_eq!(
        quarantined(parse_change(ChangeId::of(&raw), Arc::from(raw))),
        RuleViolation::TooDeep
    );
}

#[test]
fn v7_a_line_is_at_most_4_mib() {
    let base = Change::new(setting(Json::from(""))).unwrap();
    let fill = MAX_LINE_BYTES - (base.raw().len() + tc_journal::line::LINE_OVERHEAD);
    let exact = Change::new(setting(Json::from("a".repeat(fill)))).unwrap();
    assert_eq!(
        exact.raw().len() + tc_journal::line::LINE_OVERHEAD,
        MAX_LINE_BYTES
    );
    let over = setting(Json::from("a".repeat(fill + 1)));
    assert_eq!(
        refused(over.clone()),
        RuleViolation::LineTooLong {
            bytes: MAX_LINE_BYTES as u64 + 1
        }
    );
    // A reader quarantines such a line even though it verifies.
    let raw = json_of(&setting(Json::from("")))
        .to_canonical_string()
        .replace(
            "\"value\":\"\"",
            &format!("\"value\":\"{}\"", "a".repeat(fill + 1)),
        );
    let line = tc_journal::line::encode_line(ChangeId::of(raw.as_bytes()), raw.as_bytes());
    assert_eq!(line.len(), MAX_LINE_BYTES + 1);
    assert_eq!(
        quarantined(tc_journal::line::read_line(&line[..line.len() - 1]).unwrap()).rule(),
        Rule::V7
    );
}

// ---------------------------------------------------------------------------------------
// V8
// ---------------------------------------------------------------------------------------

#[test]
fn v8_a_carried_file_is_never_at_a_derived_or_owned_path() {
    let carried = |path: &str| {
        let mut body = verse_body(local(1), 1, 1, "x");
        body.record = Record::CarriedFile {
            path: tc_journal::RepoPath::parse(path).unwrap(),
            value: tc_journal::record::CarriedValue::Removed,
        };
        body
    };
    for ok in [
        "ingredients/audio/01.wav",
        "ingredients/LICENSE.md",
        "ingredients/content/front/title.md",
        "README.md",
        "ingredients/checkingx/a.json",
        "ingredientsx",
        "checking",
        "content/01.md",
        "ingredients/TIT.usfmx/a.wav",
        "metadata.json.bak",
        "docs/metadata.json",
    ] {
        assert!(Change::new(carried(ok)).is_ok(), "{ok}");
    }
    for bad in [
        "ingredients/TIT.usfm",
        "ingredients/tit.USFM",
        "ingredients/vrs.json",
        "ingredients/checking/alignments/TIT.json",
        "Ingredients/Checking/settings.json",
        "ingredients/content/01.md",
        "ingredients/tc/project.json",
        "ingredients/TC/changes/x.ndjson",
        "ingredients/tc/notes/a.json",
        // Folders the app's files are in: a file there could never be written.
        "ingredients",
        "ingredients/tc",
        "Ingredients/TC",
        "ingredients/checking",
        "ingredients/content",
        "ingredients/tc/comments",
        // Inside a file the app writes.
        "ingredients/TIT.usfm/a.wav",
        "ingredients/vrs.json/a.wav",
        "metadata.json/x.wav",
        "ingredients/content/01.md/a/b.txt",
    ] {
        let rule = refused(carried(bad));
        assert_eq!(rule.rule(), Rule::V8, "{bad}: {rule}");
    }
    // metadata.json is no path a record can name at all (V2).
    for bad in ["metadata.json", "Metadata.JSON"] {
        assert!(tc_journal::RepoPath::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn rules_name_themselves_in_their_messages() {
    for (rule, name) in [
        (RuleViolation::TooDeep, "V7"),
        (
            RuleViolation::GrowOnly {
                kind: "comment".into(),
            },
            "V6",
        ),
        (
            RuleViolation::Usfm {
                problem: "x".into(),
            },
            "V3",
        ),
    ] {
        assert!(rule.to_string().starts_with(name), "{rule}");
        assert_eq!(rule.rule().to_string(), name);
    }
}
