//! What the screens compute equals what the fold writes (`docs/STORAGE.md` §7.7, §17).
//!
//! The Check screen shows each decision's state with `tc_check::status::CheckState`, and the
//! Align rail each alignment's with `tc_align::verse_status`. The fold writes the same
//! states into the decision files (`invalidated`, `verseEdits`, `status`, `userInvalid`) and
//! the alignment files (`invalid`). Here the screens get their inputs as the screens do, not
//! from the fold's tables: the verse text from the projected USFM file, cut and indexed as
//! tC4's Check screen indexes a book (`verseTextIndex` and `withSpanMembers`, `src/state.jsx`
//! at d310bf3); the resolution from the projected `resources.json`, read by `tc-resources`
//! and resolved on a computer where every pinned resource is installed; an alignment's run
//! from the projected USFM and its original-language pin from `resources.json`. Every
//! decision and alignment of every file must agree, over tC4's sample project, the aligned
//! en_ult Titus, edits that change, join, renumber and clear verses, pins that record no
//! books or name no commit, a set without a gateway language, and the Psalms corpus with its
//! team history.

// Helpers outside `#[test]` functions build known-valid values and may fail loudly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod support;

use serde_json::Value;
use std::collections::HashMap;
use support::*;
use tc_align::{TargetVerse, VerseState, verse_status};
use tc_check::status::CheckState;
use tc_corpus::{Corpus, Spec};
use tc_journal::alignment::{AlignedRun, split_aligned};
use tc_journal::decision::TargetText;
use tc_journal::fold::{
    DerivedFile, FoldInput, FoldState, Placement, ProjectKind, decode_alignments, decode_decisions,
    records_from_usfm,
};
use tc_journal::json::Json;
use tc_journal::pins::{Collection, Coverage, Ladder, Unrecorded};
use tc_journal::record::{PinGroupValue, PresentHow, ResourceRef, UserStatus, VerseValue};
use tc_journal::{
    BookId, Cause, Change, CheckKey, CheckTarget, ImportSource, PinGroup, Record, RecordKey, ToolId,
};
use tc_resources::ProjectPins;
use tc_usfm::codec;

fn fixture(path: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

// ---- What the screens read ----------------------------------------------------------

/// JavaScript's `trim`.
fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}

/// tC4's `indexBook` (`src/data/usfm/indexer.ts` at d310bf3), line by line over the whole
/// book as tC4 runs it, not through the codec: each verse's chapter, key and body range. A
/// body runs from after `\v N` (from the next line when the rest of the marker's line is
/// blank) to the first line that starts with a paragraph-level marker, `\v` or `\c`, without
/// the line breaks before that line. tC4 reads a `\v` only at the start of a line; this
/// port also reads one inside a line, which ends the verse before it there, as §7.7 states
/// (tC4 indexes no such verse, and the ULT's Psalms put every `\v` after a `\q1`).
fn index_book(raw: &str) -> Vec<(String, String, usize, usize)> {
    struct Open {
        chapter: String,
        key: String,
        start: usize,
        marker_end: usize,
    }
    let close = |open: &mut Option<Open>, boundary: usize, out: &mut Vec<_>| {
        let Some(o) = open.take() else { return };
        let mut end = boundary.max(o.start);
        while end > o.start && matches!(raw.as_bytes()[end - 1], b'\n' | b'\r') {
            end -= 1;
        }
        let mut start = o.start;
        if end == start && o.marker_end < start {
            start = o.marker_end;
            end = start;
        }
        out.push((o.chapter, o.key, start, end));
    };
    // `\c` or `\v`, spaces or tabs, then a token: the token and where it ends.
    let token = |text: &str, marker: &str| -> Option<(String, usize)> {
        let rest = text.strip_prefix(marker)?;
        let after = rest.trim_start_matches([' ', '\t']);
        if after.len() == rest.len() {
            return None;
        }
        let len = after
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after.len());
        (len > 0).then(|| (after[..len].to_owned(), text.len() - after.len() + len))
    };
    let mut out = Vec::new();
    let mut chapter: Option<String> = None;
    let mut open: Option<Open> = None;
    let mut line_start = 0;
    while line_start < raw.len() {
        let line_end = raw[line_start..]
            .find('\n')
            .map_or(raw.len(), |at| line_start + at);
        let line = &raw[line_start..line_end];
        // LINE_MARKER: `\`, an optional `+`, a letter, then letters, digits and hyphens.
        let tag = line.strip_prefix('\\').and_then(|rest| {
            let rest = rest.strip_prefix('+').unwrap_or(rest);
            let len = rest
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'-')
                .count();
            rest.as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                .then(|| &rest[..len])
        });
        match tag {
            Some("c") => {
                close(&mut open, line_start, &mut out);
                if let Some((number, _)) = token(line, "\\c") {
                    chapter = Some(number);
                }
            }
            // A `\v` that opens no verse still ends the one before it.
            Some("v") => close(&mut open, line_start, &mut out),
            Some(tag) if codec::is_para_marker(tag) => close(&mut open, line_start, &mut out),
            _ => {}
        }
        // Every `\v` of the line, at its start or not.
        let mut from = 0;
        while let Some(at) = line[from..].find("\\v").map(|at| from + at) {
            from = at + 2;
            let Some((key, after)) = token(&line[at..], "\\v") else {
                continue;
            };
            close(&mut open, line_start + at, &mut out);
            let Some(c) = &chapter else { continue };
            let mut marker_end = line_start + at + after;
            if matches!(raw.as_bytes().get(marker_end), Some(b' ' | b'\t')) {
                marker_end += 1;
            }
            let mut start = marker_end;
            if js_trim(&raw[start.min(line_end)..line_end]).is_empty() {
                start = (line_end + 1).min(raw.len());
            }
            open = Some(Open {
                chapter: c.clone(),
                key,
                start,
                marker_end,
            });
            from = at + after;
        }
        line_start = line_end + 1;
    }
    close(&mut open, raw.len(), &mut out);
    out
}

/// The draft index the Check screen reads, built as tC4 builds its own (`verseTextIndex`,
/// then `withSpanMembers`, `src/state.jsx` at d310bf3): each verse [`index_book`] finds,
/// under `chapter:key`, and each member verse of a span under its own number when nothing
/// else holds it. A verse is in it
/// when it is drafted by the rule of owner ruling H12, not tC4's: its words on any line,
/// trimmed, are neither empty nor the stub `___` (tC4 trims only the body its indexer finds,
/// so a verse whose words start on a `\q` line is not drafted there). The text the screens
/// assess a drafted verse with is its whole content (the hash and the selections are of it),
/// read here with the codec.
fn draft_index(usfm: &str) -> HashMap<String, String> {
    let parts = codec::decompose(usfm).expect("the projection decomposes");
    let mut contents = HashMap::new();
    for (&c, chapter) in &parts.chapters {
        for (&v, part) in &chapter.verses {
            if let codec::VersePart::Text(text) = part {
                let words = js_trim(&text.words);
                if !words.is_empty() && words != "___" {
                    contents.insert(
                        format!("{c}:{}", codec::label(v, text.last)),
                        codec::join_verse(&text.words, &text.layout),
                    );
                }
            }
        }
    }
    let mut index = HashMap::new();
    let mut spans = Vec::new();
    for (chapter, key, _, _) in index_book(usfm) {
        let at = format!("{chapter}:{key}");
        let Some(content) = contents.get(&at).cloned() else {
            continue;
        };
        if let Some((first, last)) = key.split_once('-') {
            spans.push((
                chapter.clone(),
                first.parse::<u16>().unwrap(),
                last.parse::<u16>().unwrap(),
                content.clone(),
            ));
        }
        index.insert(at, content);
    }
    for (c, first, last, content) in spans {
        for n in first..=last {
            index
                .entry(format!("{c}:{n}"))
                .or_insert_with(|| content.clone());
        }
    }
    index
}

/// The text a Check screen reads for a check: tC4's `verses[textKeyOf(reference)]`.
fn screen_text(index: &HashMap<String, String>, target: &CheckTarget) -> Option<TargetText> {
    let CheckTarget::Verse { chapter, verse, .. } = target else {
        return None;
    };
    index
        .get(&format!("{}:{}", chapter.as_str(), verse.as_str()))
        .map(|content| TargetText::new(content.clone()))
}

/// Each run of a book's USFM by its chapter and first verse: its last verse and content.
fn runs(usfm: &str) -> HashMap<(u16, u16), (u16, String)> {
    let parts = codec::decompose(usfm).expect("the projection decomposes");
    let mut out = HashMap::new();
    for (&c, chapter) in &parts.chapters {
        for (&v, part) in &chapter.verses {
            if let codec::VersePart::Text(text) = part {
                out.insert(
                    (c, v),
                    (text.last, codec::join_verse(&text.words, &text.layout)),
                );
            }
        }
    }
    out
}

/// The project's pins as the resource store reads the projected `resources.json`.
fn screen_pins(state: &FoldState) -> ProjectPins {
    match state.render(&DerivedFile::Resources) {
        Some(bytes) => ProjectPins::from_resources_json(&serde_json::from_slice(&bytes).unwrap())
            .expect("the projected resources.json reads"),
        None => ProjectPins::default(),
    }
}

/// Every installed resource holds every book: the steady state the coverage backfill
/// (§5.6) brings the pins to, where the screens and the fold resolve alike.
fn everything_installed(_: &ResourceRef) -> Option<Coverage> {
    Some(Coverage::Whole)
}

fn status_name(status: Option<UserStatus>) -> Option<&'static str> {
    status.map(|s| match s {
        UserStatus::Valid => "valid",
        UserStatus::Invalid => "invalid",
        UserStatus::Todo => "todo",
    })
}

// ---- The comparison --------------------------------------------------------------------

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Compared {
    decisions: usize,
    invalidated: usize,
    verse_edits: usize,
    no_text: usize,
    alignments: usize,
    invalid: usize,
    off_run: usize,
    /// Alignments the rail calls invalid and the fold does not: see `compare_alignments`.
    words_not_the_verse: usize,
}

/// Compares every decision file and alignment file of `state` with what the screens
/// compute, and counts what it compared.
fn compare(state: &FoldState) -> Compared {
    let pins = screen_pins(state);
    let mut out = Compared::default();
    for file in state.derived_files() {
        match file {
            DerivedFile::Decisions(tool, book) => {
                compare_decisions(state, &pins, tool, book, &mut out)
            }
            DerivedFile::Alignments(book) => compare_alignments(state, &pins, book, &mut out),
            _ => {}
        }
    }
    out
}

fn compare_decisions(
    state: &FoldState,
    pins: &ProjectPins,
    tool: ToolId,
    book: BookId,
    out: &mut Compared,
) {
    let file = DerivedFile::Decisions(tool, book);
    let bytes = state.render(&file).expect("the decision file");
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    let rendered: Vec<&Value> = doc["decisions"].as_array().unwrap().iter().collect();
    let decoded = decode_decisions(tool, Some(book), &bytes).expect("the file decodes");
    assert_eq!(rendered.len(), decoded.len());
    let by_key: HashMap<CheckKey, &Value> = decoded
        .iter()
        .zip(&rendered)
        .map(|(record, json)| match record {
            Record::Check { key, .. } => (key.clone(), *json),
            other => panic!("not a decision: {other:?}"),
        })
        .collect();

    let usfm = state.book_usfm(book).unwrap_or_default();
    let index = draft_index(&usfm);
    let collection = Collection::Book(book);
    let resolution = pins
        .answer(
            tool,
            collection,
            Unrecorded::Installed(&everything_installed),
        )
        .and_then(|answer| answer.resource().cloned());
    let fold_texts = state.check_texts(collection);
    let projected = state.decisions(tool, collection);
    assert_eq!(projected.len(), rendered.len(), "{}", file.path());
    for decision in &projected {
        let at = format!("{} {:?}", file.path(), decision.key);
        let json = by_key
            .get(&decision.key)
            .unwrap_or_else(|| panic!("{at}: not in its file"));
        let text = screen_text(&index, &decision.key.target);
        // The fold reads the same text a screen reads.
        assert_eq!(
            fold_texts.text(&decision.key.target).map(|t| t.md5()),
            text.as_ref().map(TargetText::md5),
            "{at}"
        );
        let screen = CheckState::of(Some(&decision.record), resolution.as_ref(), text.as_ref());
        let assessment = screen.assessment.unwrap();
        let flag = |name: &str| json.get(name).and_then(Value::as_bool).unwrap_or(false);
        assert_eq!(
            assessment.invalidation.is_some(),
            flag("invalidated"),
            "{at}"
        );
        assert_eq!(assessment.verse_edits, flag("verseEdits"), "{at}");
        assert_eq!(assessment.user_invalid, flag("userInvalid"), "{at}");
        assert_eq!(
            status_name(screen.status()),
            json.get("status").and_then(Value::as_str),
            "{at}"
        );
        // The session shows the fold's state for a decision on its own check.
        assert_eq!(decision.assessment, assessment, "{at}");
        out.decisions += 1;
        out.invalidated += usize::from(assessment.invalidation.is_some());
        out.verse_edits += usize::from(assessment.verse_edits);
        out.no_text += usize::from(text.is_none());
    }
}

fn compare_alignments(state: &FoldState, pins: &ProjectPins, book: BookId, out: &mut Compared) {
    let file = DerivedFile::Alignments(book);
    let bytes = state.render(&file).expect("the alignment file");
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    let decoded = decode_alignments(book, &bytes).expect("the file decodes");
    let mut labels = Vec::new();
    for (chapter, verses) in doc["chapters"].as_object().unwrap() {
        for (label, record) in verses.as_object().unwrap() {
            labels.push((
                chapter.parse::<u16>().unwrap(),
                label.clone(),
                record.clone(),
            ));
        }
    }
    assert_eq!(labels.len(), decoded.len(), "{}", file.path());
    let usfm = state.book_usfm(book).unwrap_or_default();
    let runs = runs(&usfm);
    let source_sha = pins.original_language.sha(book.testament());
    for ((chapter, label, json), record) in labels.into_iter().zip(decoded) {
        let at = format!("{} {chapter}:{label}", file.path());
        let Record::Alignment {
            chapter: key_chapter,
            verse: anchor,
            value,
            ..
        } = record
        else {
            panic!("{at}: not an alignment");
        };
        let invalid = json["invalid"].as_bool().unwrap();
        let start: u16 = label.split('-').next().unwrap().parse().unwrap();
        let key = RecordKey::Alignment(book, key_chapter, anchor);
        // The Align screen shows a record at the run the fold places it at; the file writes
        // it under the label of the span it was made for, from the run's first verse. One
        // the fold places elsewhere, or not at all, is in the file at its own label,
        // invalid.
        let shown_here = state.placement(&key)
            == Placement::Run {
                chapter,
                verse: start,
            };
        let Some((last, content)) = runs.get(&(chapter, start)).filter(|_| shown_here) else {
            assert!(invalid, "{at}: a record no run shows is written invalid");
            out.off_run += 1;
            continue;
        };
        let verse = TargetVerse::new(content);
        let run = AlignedRun {
            anchor: anchor.get(),
            run_start: start,
            run_end: *last,
            text_md5: verse.md5(),
            source_sha,
        };
        let shown = verse_status(Some(&value), Some((&run, &verse)), None);
        // The rail reads `tc_align::needs_review`: the fold's rule, and also whether every
        // word the alignment holds is a word of the verse, which the fold's `invalid` does not
        // read yet (tC4 #796; the corpus stamps a save with the current text's hash over the
        // words of an earlier one). Every other state agrees.
        if shown.state == VerseState::Invalid && !invalid {
            assert!(value.is_valid(&run), "{at}");
            out.words_not_the_verse += 1;
        } else {
            assert_eq!(shown.state == VerseState::Invalid, invalid, "{at}");
        }
        out.alignments += 1;
        out.invalid += usize::from(invalid);
    }
}

// ---- Projects ----------------------------------------------------------------------------

/// A book imported as its USFM is, one batch.
fn import_book(dev: &mut Dev, book: BookId, parts: &codec::BookParts) -> Vec<Change> {
    let mut items = vec![(presence_of(book, PresentHow::Added), vec![])];
    items.extend(
        records_from_usfm(book, parts)
            .into_iter()
            .map(|r| (r, vec![])),
    );
    dev.batch(
        items,
        Cause::Import(ImportSource::Usfm {
            file_name: format!("{}.usfm", book.code()).into(),
            unknown_base: false,
        }),
    )
}

/// The pin groups of a `resources.json`.
fn pin_records(json: &Value) -> Vec<Record> {
    let group = |group: PinGroup, value: &Value| Record::Pins {
        group,
        value: PinGroupValue::from_json(value).expect("a pin group"),
    };
    let mut out = Vec::new();
    for (name, group_of) in [
        ("primary", PinGroup::Primary),
        ("fallback", PinGroup::Fallback),
    ] {
        if let Some(set) = json["languageSets"].get(name) {
            out.push(group(group_of, set));
        }
    }
    for (name, group_of) in [
        ("originalLanguage", PinGroup::OriginalLanguage),
        ("lexicon", PinGroup::Lexicon),
    ] {
        if let Some(set) = json["resources"].get(name) {
            out.push(group(group_of, set));
        }
    }
    if let Some(list) = json.get("extraScripture") {
        out.push(group(
            PinGroup::ExtraScripture,
            &serde_json::json!({ "extraScripture": list }),
        ));
    }
    out
}

const SAMPLE: &str = "tc4/sample-burrito/ingredients";

/// A tC4 decision file as the seed of §13.2 reads it: each decision stamped with the md5 of
/// its verse's current text, `imported_invalid` from `invalidated`, and the person's own
/// status, which tC4's `status` hides once a decision is invalidated.
fn seeded(mut file: Value, index: &HashMap<String, String>) -> Value {
    for decision in file["decisions"].as_array_mut().unwrap() {
        let reference = &decision["contextId"]["reference"];
        let key = format!(
            "{}:{}",
            plain(&reference["chapter"]),
            plain(&reference["verse"])
        );
        let content = index.get(&key).cloned().unwrap_or_default();
        let md5 = tc_journal::decision::verse_text_md5_v1(&content);
        let invalidated = decision["invalidated"].as_bool().unwrap_or(false);
        let status = decision["status"].as_str().map(str::to_owned);
        let user_invalid = decision["userInvalid"].as_bool().unwrap_or(false);
        let user_status = match (invalidated, status.as_deref()) {
            (false, status) => status.map(str::to_owned),
            (true, Some("todo")) => Some("todo".to_owned()),
            (true, _) if user_invalid => Some("invalid".to_owned()),
            (true, _) => None,
        };
        let map = decision.as_object_mut().unwrap();
        map.insert("verseTextMd5".into(), Value::from(md5.to_hex()));
        map.insert("importedInvalid".into(), Value::from(invalidated));
        if let Some(status) = user_status {
            map.insert("userStatus".into(), Value::from(status));
        }
    }
    file
}

/// A reference part as tC4's `textKeyOf` prints it.
fn plain(part: &Value) -> String {
    match part {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// tC4's sample project: Titus and Jonah, its tN and tW decisions on Titus, its Titus
/// alignments and its pins, each as the records an import of its files makes.
fn sample(dev: &mut Dev) -> Vec<Change> {
    let mut changes = Vec::new();
    for book in [BookId::Titus, BookId::Jonah] {
        let usfm = fixture(&format!("{SAMPLE}/{}.usfm", book.code()));
        changes.extend(import_book(dev, book, &codec::decompose(&usfm).unwrap()));
    }
    let resources: Value =
        serde_json::from_str(&fixture(&format!("{SAMPLE}/checking/resources.json"))).unwrap();
    let mut records = pin_records(&resources);
    let index = draft_index(&fixture(&format!("{SAMPLE}/TIT.usfm")));
    for (tool, dir) in [
        (ToolId::TranslationNotes, "translationNotes"),
        (ToolId::TranslationWords, "translationWords"),
    ] {
        let file: Value =
            serde_json::from_str(&fixture(&format!("{SAMPLE}/checking/{dir}/TIT.json"))).unwrap();
        let bytes = serde_json::to_vec(&seeded(file, &index)).unwrap();
        records.extend(decode_decisions(tool, Some(BookId::Titus), &bytes).unwrap());
    }
    let alignments = fixture(&format!("{SAMPLE}/checking/alignments/TIT.json"));
    records.extend(decode_alignments(BookId::Titus, alignments.as_bytes()).unwrap());
    changes.extend(dev.batch(
        records.into_iter().map(|r| (r, vec![])).collect(),
        Cause::Import(ImportSource::Burrito {
            file_name: "sample-burrito".into(),
        }),
    ));
    changes
}

#[test]
fn the_sample_projects_files_are_what_the_screens_compute() {
    let mut ana = Dev::new(1);
    let changes = sample(&mut ana);
    let state = fold(&changes);
    let compared = compare(&state);
    assert_eq!(compared.decisions, 4, "{compared:?}");
    assert_eq!(
        compared.invalidated, 1,
        "tC4 invalidated tN gr8c: {compared:?}"
    );
    assert_eq!(compared.alignments, 1, "{compared:?}");
    // tC4's alignment record comes back as tC4 wrote it, with the journal's `done` beside
    // it (§7.7, fields kept for decoding), which tC4 reads as it reads a record without.
    let mut projected: Value = serde_json::from_slice(
        &state
            .render(&DerivedFile::Alignments(BookId::Titus))
            .unwrap(),
    )
    .unwrap();
    let tc4: Value =
        serde_json::from_str(&fixture(&format!("{SAMPLE}/checking/alignments/TIT.json"))).unwrap();
    let record = &mut projected["chapters"]["1"]["1"];
    assert_eq!(record["done"], false);
    record.as_object_mut().unwrap().remove("done");
    assert_eq!(projected["chapters"], tc4["chapters"]);
}

#[test]
fn edits_joins_renumbers_and_pins_change_files_and_screens_alike() {
    let mut ana = Dev::new(1);
    let mut changes = sample(&mut ana);
    let before = compare(&fold(&changes));

    // Edit 1:4, where tN "gr8c" selected "Gracia" and "paz": the selections are lost.
    // Edit 1:1 keeping "fe", which tN "swi9" selected: the verse changed, the check holds.
    let v4 = on(&changes, &verse_key(1, 4));
    let v1 = on(&changes, &verse_key(1, 1));
    changes.push(ana.one(
        verse(1, 4, text("a Tito, verdadero hijo en la fe común:", 4)),
        vec![v4.id()],
    ));
    changes.push(ana.one(
        verse(
            1,
            1,
            text("Pablo, siervo de Dios, según la fe de los escogidos", 1),
        ),
        vec![v1.id()],
    ));
    // Join 1:5 and 1:6 into one run, and renumber 1:7's text as 1:8 (1:8's own text gone).
    let v5 = on(&changes, &verse_key(1, 5));
    let v6 = on(&changes, &verse_key(1, 6));
    changes.extend(ana.batch(
        vec![
            (verse(1, 5, text("cinco y seis juntos", 6)), vec![v5.id()]),
            (verse(1, 6, covered(5)), vec![v6.id()]),
        ],
        Cause::Edit,
    ));
    let v7 = on(&changes, &verse_key(1, 7));
    let v8 = on(&changes, &verse_key(1, 8));
    changes.extend(ana.batch(
        vec![
            (verse(1, 7, absent(Some((1, 8)))), vec![v7.id()]),
            (
                verse(1, 8, text_from("el texto que era 1:7", 8, (1, 7))),
                vec![v8.id()],
            ),
        ],
        Cause::Edit,
    ));
    // Clear 1:9 to the stub: its checks have no text and give no evidence.
    let v9 = on(&changes, &verse_key(1, 9));
    changes.push(ana.one(verse(1, 9, text("___", 9)), vec![v9.id()]));
    // Lay 1:2 out as poetry whose first line is a `\q1` line of its own, and 1:3 as the stub
    // with a poetry line after it: tC4's indexer reads neither as drafted, but both hold
    // words, so by owner ruling H12 their checks have the verses' text.
    let poetry = |words: &str, last: u16, term: codec::MarkerTerm| VerseValue::Text {
        marker: codec::MarkerWs::new(" ", term).unwrap(),
        words: words.into(),
        layout: "\n".into(),
        last: tc_journal::VerseNo::new(last).unwrap(),
        from: None,
    };
    let v2 = on(&changes, &verse_key(1, 2));
    let v3 = on(&changes, &verse_key(1, 3));
    changes.extend(ana.batch(
        vec![
            (
                verse(
                    1,
                    2,
                    poetry("\\q1 en la esperanza", 2, codec::MarkerTerm::Lf),
                ),
                vec![v2.id()],
            ),
            (
                verse(
                    1,
                    3,
                    poetry("___\n\\q2 a su debido tiempo", 3, codec::MarkerTerm::Space),
                ),
                vec![v3.id()],
            ),
        ],
        Cause::Edit,
    ));
    let edited = fold(&changes);
    let laid_out = edited.check_texts(Collection::Book(BookId::Titus));
    for v in ["2", "3"] {
        let target = CheckTarget::Verse {
            book: BookId::Titus,
            chapter: tc_journal::RefPart::parse("1").unwrap(),
            verse: tc_journal::RefPart::parse(v).unwrap(),
        };
        assert!(laid_out.text(&target).is_some(), "1:{v}");
    }
    let after = compare(&edited);
    assert_eq!(after.decisions, before.decisions);
    // "apóstol" and the second "Dios" are gone from 1:1; "fe" is still there.
    assert_eq!(
        after.invalidated,
        before.invalidated + 2,
        "{before:?} {after:?}"
    );
    assert_eq!(after.verse_edits, 4, "{after:?}");
    let notes = edited.decisions(ToolId::TranslationNotes, Collection::Book(BookId::Titus));
    let fe = notes
        .iter()
        .find(|d| d.key.check_id.as_str() == "swi9")
        .unwrap();
    assert!(fe.assessment.verse_edits && fe.assessment.invalidation.is_none());

    // A tW decision made against the English fallback's words links, which the Spanish
    // primary outranks for Titus: invalidated, and so it stays below.
    let en_twl = ResourceRef {
        repo_path: "git.door43.org/unfoldingWord/en_twl".into(),
        sha: tc_journal::GitOid::parse("7cbb3419f79780ce84b92ad7d42bd476cc5d8a64").unwrap(),
    };
    let tw_key = CheckKey {
        tool: ToolId::TranslationWords,
        check_id: tc_journal::CheckId::parse("zz01").unwrap(),
        target: CheckTarget::Verse {
            book: BookId::Titus,
            chapter: tc_journal::RefPart::parse("1").unwrap(),
            verse: tc_journal::RefPart::parse("2").unwrap(),
        },
        occurrence: 1,
    };
    let mut against_fallback = decision(Some(UserStatus::Valid), false, None);
    against_fallback.context = match Json::try_from(&serde_json::json!({
        "checkId": "zz01",
        "reference": {"bookId": "tit", "chapter": 1, "verse": 2},
        "tool": "translationWords",
        "groupId": "god",
        "quote": "θεοῦ",
        "occurrence": 1
    }))
    .unwrap()
    {
        Json::Obj(map) => map,
        _ => panic!("unexpected"),
    };
    against_fallback.judgement.nothing_to_select = true;
    against_fallback.resource = en_twl;
    changes.push(ana.one(check(&tw_key, against_fallback), vec![]));
    let edited = fold(&changes);

    // The primary words links named by a tag with no commit: every tW decision on Titus
    // is invalidated, on the screen as in the file; the fallback is not used in its place,
    // so the decision made against it stays invalidated.
    let primary = edited
        .projected(&RecordKey::Pins(PinGroup::Primary))
        .unwrap();
    let Record::Pins { mut value, .. } = primary else {
        panic!("a pin group")
    };
    let slot = tc_journal::SlotName::parse("translationWordsLinks").unwrap();
    let mut tag_only = value.0[&slot].0.clone();
    if let Json::Obj(map) = &mut tag_only {
        map.remove("sha");
    }
    value
        .0
        .insert(slot, tc_journal::record::SlotValue(tag_only));
    // A set without a gateway language still resolves its pins, on the screen as in the
    // fold: tC4 treats `gatewayLanguage` as one more slot.
    value
        .0
        .remove(&tc_journal::SlotName::parse("gatewayLanguage").unwrap())
        .unwrap();
    // The fallback's notes record no books: in the fold they cover every book.
    let fallback = edited
        .projected(&RecordKey::Pins(PinGroup::Fallback))
        .unwrap();
    let Record::Pins {
        value: mut fallback_value,
        ..
    } = fallback
    else {
        panic!("a pin group")
    };
    let notes = tc_journal::SlotName::parse("translationNotes").unwrap();
    if let Some(tc_journal::record::SlotValue(Json::Obj(map))) = fallback_value.0.get_mut(&notes) {
        map.remove("books");
    }
    let heads = |key: &RecordKey| edited.shown_heads(key, None);
    changes.extend(ana.batch(
        vec![
            (
                Record::Pins {
                    group: PinGroup::Primary,
                    value,
                },
                heads(&RecordKey::Pins(PinGroup::Primary)),
            ),
            (
                Record::Pins {
                    group: PinGroup::Fallback,
                    value: fallback_value,
                },
                heads(&RecordKey::Pins(PinGroup::Fallback)),
            ),
        ],
        Cause::Edit,
    ));
    let repinned = fold(&changes);
    let compared = compare(&repinned);
    let tw = repinned.decisions(ToolId::TranslationWords, Collection::Book(BookId::Titus));
    assert!(!tw.is_empty());
    assert!(tw.iter().all(|d| {
        d.assessment
            .invalidation
            .is_some_and(|i| i.resource_changed)
    }));
    // The decisions of the edits above, and the one made against the fallback.
    assert_eq!(compared.invalidated, after.invalidated + 1, "{compared:?}");
    assert!(tw.iter().any(|d| d.key == tw_key));
}

/// en_ult Titus as aligned USFM: each verse split into its text and an alignment (§13.5),
/// imported by `ana` in one batch per kind.
fn aligned_titus(ana: &mut Dev) -> Vec<Change> {
    let mut changes = Vec::new();
    for (book, path) in [(BookId::Titus, "tc4/test-fixtures/en_ult/TIT.usfm")] {
        let mut parts = codec::decompose(&fixture(path)).unwrap();
        let mut alignments = Vec::new();
        for (&c, chapter) in parts.chapters.iter_mut() {
            for (&v, part) in chapter.verses.iter_mut() {
                let codec::VersePart::Text(verse) = part else {
                    continue;
                };
                let content = codec::join_verse(&verse.words, &verse.layout);
                let Some((plain, value)) =
                    split_aligned(&content, tc_journal::VerseNo::new(verse.last).unwrap())
                else {
                    continue;
                };
                let (words, layout) = codec::split_verse(&plain);
                verse.words = words.to_owned();
                verse.layout = layout.to_owned();
                alignments.push(Record::Alignment {
                    book,
                    chapter: tc_journal::ChapterNo::new(c).unwrap(),
                    verse: tc_journal::VerseNo::new(v).unwrap(),
                    value,
                });
            }
        }
        changes.extend(import_book(ana, book, &parts));
        changes.extend(ana.batch(
            alignments.into_iter().map(|r| (r, vec![])).collect(),
            Cause::Import(ImportSource::Usfm {
                file_name: format!("{}.usfm", book.code()).into(),
                unknown_base: false,
            }),
        ));
    }
    changes
}

#[test]
fn alignments_follow_their_text_on_the_rail_as_in_the_file() {
    let mut ana = Dev::new(1);
    let mut changes = aligned_titus(&mut ana);
    let state = fold(&changes);
    let imported = compare(&state);
    assert_eq!(imported.alignments, 46, "{imported:?}");
    assert_eq!(
        imported.invalid, 0,
        "an aligned import is valid: {imported:?}"
    );
    assert_eq!(imported.words_not_the_verse, 0, "{imported:?}");

    // Join 1:3 into 1:2's run with new words, edit 2:1, and renumber 3:15 as 3:16 with
    // its text unchanged.
    let at = |c: u16, v: u16| on(&changes, &verse_key(c, v)).id();
    let (v2, v3, v21, v15) = (at(1, 2), at(1, 3), at(2, 1), at(3, 15));
    let last_words = projected_words(&state, 3, 15).unwrap();
    changes.extend(ana.batch(
        vec![
            (
                verse(1, 2, text("in hope of everlasting life", 3)),
                vec![v2],
            ),
            (verse(1, 3, covered(2)), vec![v3]),
            (
                verse(2, 1, text("But you, speak what fits sound teaching.", 1)),
                vec![v21],
            ),
            (verse(3, 15, absent(Some((3, 16)))), vec![v15]),
            (verse(3, 16, text_from(&last_words, 16, (3, 15))), vec![]),
        ],
        Cause::Edit,
    ));
    let state = fold(&changes);
    let edited = compare(&state);
    assert_eq!(edited.alignments + edited.off_run, 46, "{edited:?}");
    assert_eq!(edited.invalid, 2, "the joined run and 2:1: {edited:?}");
    assert_eq!(edited.words_not_the_verse, 0, "{edited:?}");
    assert_eq!(
        edited.off_run, 1,
        "1:3 is shown at no run of its own: {edited:?}"
    );
    assert_eq!(
        state.placement(&RecordKey::Alignment(
            BookId::Titus,
            tc_journal::ChapterNo::new(3).unwrap(),
            tc_journal::VerseNo::new(15).unwrap(),
        )),
        Placement::Run {
            chapter: 3,
            verse: 16
        },
        "a renumbered verse's alignment goes with its text"
    );
    let file: Value = serde_json::from_slice(
        &state
            .render(&DerivedFile::Alignments(BookId::Titus))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        file["chapters"]["3"]["16"]["invalid"], false,
        "and stays valid"
    );
}

/// INV-ONE-REVIEW-RULE: an alignment stamped with the verse's current text but holding a
/// word the verse lacks ("siervo" where 1:1 says "esclavo") needs review in the file as on the
/// Align rail.
#[test]
fn the_alignment_files_and_the_rail_agree_on_invalid() {
    let mut ana = Dev::new(1);
    let mut changes = aligned_titus(&mut ana);
    let state = fold(&changes);
    let key = RecordKey::Alignment(
        BookId::Titus,
        tc_journal::ChapterNo::new(1).unwrap(),
        tc_journal::VerseNo::new(1).unwrap(),
    );
    let old = on(&changes, &key);
    let Record::Alignment { value, .. } = state.projected(&key).unwrap() else {
        panic!("1:1 has an alignment");
    };
    let mut value = value;
    let word = value
        .alignments
        .iter_mut()
        .find_map(|g| g.bottom_words.first_mut())
        .expect("a placed word");
    word.word = "siervo".into();
    word.occurrence = 1;
    word.occurrences = 1;
    changes.extend(ana.batch(
        vec![(
            Record::Alignment {
                book: BookId::Titus,
                chapter: tc_journal::ChapterNo::new(1).unwrap(),
                verse: tc_journal::VerseNo::new(1).unwrap(),
                value,
            },
            vec![old.id()],
        )],
        Cause::Edit,
    ));
    let state = fold(&changes);
    let file: Value = serde_json::from_slice(
        &state
            .render(&DerivedFile::Alignments(BookId::Titus))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        file["chapters"]["1"]["1"]["invalid"], true,
        "INV-ONE-REVIEW-RULE: the file says the alignment needs review"
    );
    let compared = compare(&state);
    assert_eq!(compared.words_not_the_verse, 0, "{compared:?}");
}

#[test]
fn the_psalms_corpus_files_are_what_the_screens_compute() {
    let corpus = Corpus::generate(&Spec::psalms(8).with_history(60, 0.1)).expect("the corpus");
    let state = FoldState::fold(
        ProjectKind::Bible,
        corpus
            .journal
            .lines()
            .into_iter()
            .map(|l| FoldInput::from(l).prepare()),
    );
    let compared = compare(&state);
    assert!(compared.decisions > 1_000, "{compared:?}");
    assert!(compared.alignments > 300, "{compared:?}");
    assert!(compared.verse_edits > 0, "{compared:?}");
    assert!(compared.invalidated > 0, "{compared:?}");
    assert!(compared.no_text > 0, "{compared:?}");
    // The corpus saves alignments with the hash of the text but the words of the ULT; the
    // file and the rail judge those by one rule (INV-ONE-REVIEW-RULE).
    assert_eq!(compared.words_not_the_verse, 0, "{compared:?}");
}
