//! Conformance on the real texts the app ships: every USFM book of the ULT v91, UST v91,
//! UHB 3.0.0 and UGNT 0.34 burrito exports in `bundle/en/`, and the scale Psalms in
//! `fixtures/scale/psa/en_ult/19-PSA.usfm`.
//!
//! Each book is cut into verses twice, and every verse is read apart and woven back:
//!
//! - As tC4 cuts a book to align and export it (`decompose` in tC4's
//!   `journal/skeleton.mjs`, the cut the storage codec also makes): a verse runs from after
//!   `\v N ` to the next `\v ` or `\c `. A verse comes back byte for byte unless tC4's own
//!   export does not write it back either, which `fixtures/usfm/export-vectors.json`
//!   records for every verse (made with tC4's code by `tools/gen-export-vectors.mjs`).
//!   Such a verse keeps its text, words and links; it is refused only where tC4's export
//!   cannot write it back with its text and alignment either.
//! - As [`Book`] cuts it, by verse unit bodies. A body is refused only where its verse is
//!   refused in tC4's cut, for the same reason, and otherwise keeps its text, words and
//!   links.
//!
//! In both, every `\zaln-s` of a verse comes back as one source word of its groups, and
//! every Psalm superscription written inside a verse (`\v 1 \d \zaln-s …`) weaves back
//! byte for byte. The counts are pinned, so that a change fails here with the verses it
//! concerns, and any verse that comes out otherwise fails the test with its reason.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::links;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use tc_usfm::raw::{Document, Node};
use tc_usfm::{AlignedVerse, Book, Error, parse_aligned, weave};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The USFM books of a burrito export in `bundle/en/`, named `<archive> <entry>` as the
/// export vectors name them.
fn export(name: &str) -> Vec<(String, String)> {
    let path = repo().join("bundle/en").join(name);
    let file = std::fs::File::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut found = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        if !entry.name().ends_with(".usfm") {
            continue;
        }
        let book = format!("bundle/en/{name} {}", entry.name());
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        found.push((book, text));
    }
    found
}

fn scale_psalms() -> Vec<(String, String)> {
    let name = "fixtures/scale/psa/en_ult/19-PSA.usfm";
    vec![(
        name.to_owned(),
        std::fs::read_to_string(repo().join(name)).unwrap(),
    )]
}

/// What tC4's export writes for one book (`fixtures/usfm/export-vectors.json`).
struct Tc4Export {
    slots: usize,
    aligned: usize,
    /// Verses (`chapter:verse`) the export writes with the same text in other markup.
    rewritten: HashSet<String>,
    /// Verses the export writes with other text.
    changed: HashSet<String>,
    /// Verses whose alignment the export cannot merge, so that it writes them without.
    unaligned: HashSet<String>,
}

impl Tc4Export {
    /// tC4 writes the verse back as published, or with one more space after the paragraph
    /// marker that ends it.
    fn writes_back(&self, key: &str) -> bool {
        !self.rewritten.contains(key)
            && !self.changed.contains(key)
            && !self.unaligned.contains(key)
    }

    /// tC4 cannot write the verse back with its text and alignment.
    fn loses(&self, key: &str) -> bool {
        self.changed.contains(key) || self.unaligned.contains(key)
    }
}

fn tc4_exports() -> HashMap<String, Tc4Export> {
    let path = repo().join("fixtures/usfm/export-vectors.json");
    let vectors: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let keys = |list: &Value| -> HashSet<String> {
        let list = list.as_array().unwrap();
        list.iter()
            .map(|k| k.as_str().unwrap().to_owned())
            .collect()
    };
    let count = |n: &Value| usize::try_from(n.as_u64().unwrap()).unwrap();
    vectors["books"]
        .as_array()
        .unwrap()
        .iter()
        .map(|book| {
            let export = Tc4Export {
                slots: count(&book["slots"]),
                aligned: count(&book["aligned"]),
                rewritten: keys(&book["rewritten"]),
                changed: keys(&book["changed"]),
                unaligned: keys(&book["unaligned"]),
            };
            (book["source"].as_str().unwrap().to_owned(), export)
        })
        .collect()
}

/// One verse as tC4 cuts a book: its key `chapter:verse` and its content.
struct Slot<'a> {
    key: String,
    content: &'a str,
}

/// tC4's `decompose`: the verse slots of a book, and the text between them. A verse marker
/// is `\v`, spaces or tabs, an ASCII number or span (`4`, `4-5`) and one space or tab; a
/// slot runs to the next `\v` or `\c` followed by a space or tab. The chapter of a slot is
/// the last `\c N` before it.
fn slots(text: &str) -> (Vec<Slot<'_>>, Vec<&str>) {
    let bytes = text.as_bytes();
    let blank = |at: usize| matches!(bytes.get(at), Some(b' ' | b'\t'));
    let digits = |mut at: usize| {
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        at
    };
    // `\v`, blanks, the key, one blank: the key's range.
    let verse_marker = |at: usize| -> Option<(usize, usize)> {
        if bytes.get(at..at + 2) != Some(b"\\v") || !blank(at + 2) {
            return None;
        }
        let mut start = at + 2;
        while blank(start) {
            start += 1;
        }
        let mut end = digits(start);
        if end == start {
            return None;
        }
        if bytes.get(end) == Some(&b'-') && digits(end + 1) > end + 1 {
            end = digits(end + 1);
        }
        blank(end).then_some((start, end))
    };
    let boundary = |at: usize| {
        bytes[at] == b'\\' && matches!(bytes.get(at + 1), Some(b'v' | b'c')) && blank(at + 2)
    };
    let (mut found, mut between) = (Vec::new(), Vec::new());
    let mut chapter = "0";
    let mut at = 0;
    loop {
        let marker = (at..bytes.len()).find_map(|p| verse_marker(p).map(|key| (p, key)));
        let region = &text[at..marker.map_or(text.len(), |(p, _)| p)];
        for (p, _) in region.match_indices("\\c") {
            let mut start = p + 2;
            while matches!(region.as_bytes().get(start), Some(b' ' | b'\t')) {
                start += 1;
            }
            let end = start
                + region[start..]
                    .bytes()
                    .take_while(u8::is_ascii_digit)
                    .count();
            if start > p + 2 && end > start {
                chapter = &region[start..end];
            }
        }
        between.push(region);
        let Some((_, (key_start, key_end))) = marker else {
            break;
        };
        let start = key_end + 1;
        let end = (start..bytes.len())
            .find(|&p| boundary(p))
            .unwrap_or(text.len());
        found.push(Slot {
            key: format!("{chapter}:{}", &text[key_start..key_end]),
            content: &text[start..end],
        });
        at = end;
    }
    (found, between)
}

/// How one verse came out of `parse_aligned` and `weave`.
#[derive(Debug)]
enum Outcome {
    /// Woven back into the same bytes.
    Same,
    /// Read apart into itself: no alignment, and word markup (Strong's words) that weave
    /// leaves alone.
    Kept,
    /// Woven back with the same text, words and links, but with the occurrence numbers the
    /// verse text gives, where the file wrote others: it counted `May` and `may` as one
    /// word, or kept numbers from an older text, or, in a verse unit body, counted the
    /// words of a heading that leads into the next verse (`\sp`) with this one, as tC4's
    /// cut does.
    Renumbered,
    /// Woven back with the same text, words and links, where only some `\zaln` milestones
    /// stand elsewhere: the file closes a milestone after the punctuation that follows its
    /// last word (`\w Yahweh\w*;\zaln-e\*`) or keeps one run open across a `{`, and weave
    /// closes it at the word and cuts the run, as tC4's export does.
    MilestonesMoved,
    /// Woven back with the same text, words and links, with other layout: a space where the
    /// file has a line break before a word or run, an opening quote mark outside the
    /// milestone that the file puts inside it, or a bare word wrapped in `\w`.
    Relaid,
    /// Refused, with the reason.
    Refused(Error),
}

impl Outcome {
    fn of(body: &str) -> Result<Self, String> {
        let parsed = match parse_aligned(body) {
            Ok(parsed) => parsed,
            Err(e) => return Ok(Self::Refused(e)),
        };
        let milestones = body.matches("\\zaln-s").count();
        let sources: usize = parsed.groups.iter().map(|g| g.sources.len()).sum();
        if sources != milestones {
            return Err(format!(
                "{milestones} \\zaln-s, {sources} source words in the groups"
            ));
        }
        match weave(&parsed.plain, &parsed.groups) {
            Ok(woven) if woven == body => Ok(Self::Same),
            Ok(woven) => {
                same_meaning(body, &woven, &parsed)?;
                Ok(if !parsed.renumbered.is_empty() {
                    Self::Renumbered
                } else if without_milestones(body) == without_milestones(&woven) {
                    Self::MilestonesMoved
                } else {
                    Self::Relaid
                })
            }
            Err(Error::WordMarkup) if parsed.groups.is_empty() && parsed.plain == body => {
                Ok(Self::Kept)
            }
            Err(e) => Err(format!("not woven back: {e}")),
        }
    }
}

/// `woven` reads apart into the same plain verse, words and links as `body` did.
fn same_meaning(body: &str, woven: &str, parsed: &AlignedVerse) -> Result<(), String> {
    let again = parse_aligned(woven).map_err(|e| format!("woven text refused: {e}"))?;
    if again.plain != parsed.plain
        || again.words != parsed.words
        || links(&again.groups) != links(&parsed.groups)
    {
        return Err(format!(
            "woven back with other text, words or links: {}",
            first_difference(body, woven)
        ));
    }
    Ok(())
}

fn without_milestones(usfm: &str) -> String {
    fn write(nodes: &[Node<'_>], out: &mut String) {
        for node in nodes {
            match node {
                Node::Milestone(m) if m.name() == "zaln-s" || m.name() == "zaln-e" => {}
                Node::Span(span) => {
                    out.push_str(span.open.raw());
                    write(&span.children, out);
                    out.push_str(span.attributes.map_or("", |a| a.raw()));
                    out.push_str(span.close.map_or("", |c| c.raw()));
                }
                other => other.write_to(out),
            }
        }
    }
    let mut out = String::with_capacity(usfm.len());
    write(Document::parse(usfm).nodes(), &mut out);
    out
}

/// Where two texts part, with some context.
fn first_difference(expected: &str, actual: &str) -> String {
    let same = expected
        .bytes()
        .zip(actual.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let from = expected.floor_char_boundary(same.saturating_sub(40));
    let to = |s: &str| s.ceil_char_boundary(same + 40).min(s.len());
    format!(
        "from byte {same}, {:?} became {:?}",
        &expected[from..to(expected)],
        &actual[from..to(actual)]
    )
}

/// A verse marker that tC4's cut does not take: `\v`, blanks, a number and a line break.
fn has_untaken_verse(region: &str) -> bool {
    region.match_indices("\\v").any(|(at, _)| {
        let rest = &region[at + 2..];
        let number = rest.trim_start_matches([' ', '\t']);
        number.len() < rest.len()
            && number.starts_with(|c: char| c.is_ascii_digit())
            && number
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .starts_with(['\n', '\r'])
    })
}

/// A short name for a refusal.
fn reason(e: &Error) -> &'static str {
    match e {
        Error::NotOneWord(_) => "not one word",
        Error::InvalidSourceWord(_) => "invalid source word",
        Error::UnbalancedMilestone => "unbalanced milestone",
        Error::UnclosedMilestone => "unclosed milestone",
        Error::InvalidAlignedWord(_) => "invalid aligned word",
        Error::BrokenWordMarkup(_) => "broken word markup",
        Error::AlignmentOutsideVerseText(_) => "alignment outside verse text",
        _ => "other",
    }
}

/// The outcomes of every verse of some books, in one cut.
#[derive(Debug, Default, PartialEq, Eq)]
struct Census {
    verses: usize,
    same: usize,
    kept: usize,
    renumbered: usize,
    milestones_moved: usize,
    relaid: usize,
    /// The refused verses as `BOOK chapter:verse` and a short reason.
    refused: Vec<(String, &'static str)>,
    /// Verses whose text starts with a Psalm superscription (`\v 1 \d …`), each woven back
    /// byte for byte.
    superscriptions: usize,
    /// The `\zaln-s` outside every verse. Each stands in a Psalm's superscription before
    /// its `\v 1` (`\c 3`, `\d \zaln-s …`, `\q1`, `\v 1`): tC4's cut leaves it in the
    /// chapter's front, which tC4 does not align, and [`Book`] gives it to verse 1 as a
    /// heading that leads into it.
    outside: usize,
    /// The `\zaln-s` of verses that tC4's cut does not take as verses, because a line break
    /// follows the verse number (`\v 20` LF in UST DAN 3:20 and JOL 3:15): tC4 leaves them
    /// with the text between verses, and neither aligns nor exports them.
    untaken: usize,
    /// Every verse that came out in none of the ways [`Outcome`] names, or otherwise than
    /// the module documentation allows, with the reason.
    failures: Vec<String>,
}

impl Census {
    fn count(&mut self, at: &str, body: &str, outcome: &Result<Outcome, String>) {
        self.verses += 1;
        if body.starts_with("\\d ") {
            self.superscriptions += 1;
            if !matches!(outcome, Ok(Outcome::Same)) {
                self.failures.push(format!(
                    "{at}: a superscription not woven back: {outcome:?}"
                ));
            }
        }
        match outcome {
            Ok(Outcome::Same) => self.same += 1,
            Ok(Outcome::Kept) => self.kept += 1,
            Ok(Outcome::Renumbered) => self.renumbered += 1,
            Ok(Outcome::MilestonesMoved) => self.milestones_moved += 1,
            Ok(Outcome::Relaid) => self.relaid += 1,
            Ok(Outcome::Refused(e)) => self.refused.push((at.to_owned(), reason(e))),
            Err(e) => self.failures.push(format!("{at}: {e}")),
        }
    }
}

/// The census of some books in tC4's cut, and in [`Book`]'s.
fn census(books: &[(String, String)]) -> (Census, Census) {
    let exports = tc4_exports();
    let (mut tc4_cut, mut book_cut) = (Census::default(), Census::default());
    for (name, text) in books {
        let code = name.rsplit('/').next().unwrap_or(name);
        let code = code.trim_end_matches(".usfm");
        let code = code.split_once('-').map_or(code, |(_, c)| c);
        let export = exports.get(name.as_str());

        let (found, between) = slots(text);
        let mut refused: HashMap<String, Error> = HashMap::new();
        for region in between {
            let outside = region.matches("\\zaln-s").count();
            if outside == 0 {
                continue;
            }
            if has_untaken_verse(region) {
                tc4_cut.untaken += outside;
            } else if region.contains("\\c ") && region.contains("\\d ") {
                tc4_cut.outside += outside;
            } else {
                tc4_cut.failures.push(format!(
                    "{name}: \\zaln-s outside a verse and a superscription"
                ));
            }
        }
        if let Some(export) = export {
            let aligned = found
                .iter()
                .filter(|s| s.content.contains("\\zaln-s"))
                .count();
            if (export.slots, export.aligned) != (found.len(), aligned) {
                tc4_cut.failures.push(format!(
                    "{name}: tC4 cut {} verses, {} aligned; here {}, {aligned}",
                    export.slots,
                    export.aligned,
                    found.len()
                ));
            }
        }
        for slot in &found {
            let at = format!("{code} {}", slot.key);
            let outcome = Outcome::of(slot.content);
            let writes_back = export.is_none_or(|e| e.writes_back(&slot.key));
            let loses = export.is_some_and(|e| e.loses(&slot.key));
            match &outcome {
                Ok(Outcome::Same | Outcome::Kept) | Err(_) => {}
                Ok(Outcome::Refused(e)) => {
                    if !loses {
                        tc4_cut.failures.push(format!(
                            "{at}: refused, where tC4's export writes its text and alignment: {e}"
                        ));
                    }
                    refused.insert(slot.key.clone(), e.clone());
                }
                Ok(other) if writes_back => tc4_cut.failures.push(format!(
                    "{at}: {other:?}, where tC4's export writes it back as published"
                )),
                Ok(_) => {}
            }
            tc4_cut.count(&at, slot.content, &outcome);
        }

        let mut in_bodies = 0;
        for unit in Book::parse(text).verses() {
            let chapter = unit
                .chapter()
                .map_or_else(|| "?".to_owned(), |c| c.to_string());
            let key = format!("{chapter}:{}", unit.label());
            let at = format!("{code} {key}");
            in_bodies += unit.body().matches("\\zaln-s").count();
            let outcome = Outcome::of(unit.body());
            if let Ok(Outcome::Refused(e)) = &outcome
                && refused.get(&key) != Some(e)
            {
                book_cut.failures.push(format!(
                    "{at}: refused, where tC4's cut reads it otherwise: {e}"
                ));
            }
            book_cut.count(&at, unit.body(), &outcome);
        }
        // What no body holds leads into a verse 1 as its superscription.
        let outside = text.matches("\\zaln-s").count() - in_bodies;
        let leading: usize = Book::parse(text)
            .verses()
            .filter(|u| u.label() == "1" && u.leading().contains("\\d "))
            .map(|u| u.leading().matches("\\zaln-s").count())
            .sum();
        if outside != leading {
            book_cut.failures.push(format!(
                "{name}: {outside} \\zaln-s outside verse bodies, {leading} of them in superscriptions"
            ));
        }
        book_cut.outside += outside;
    }
    (tc4_cut, book_cut)
}

fn listed(list: &[(&str, &'static str)]) -> Vec<(String, &'static str)> {
    list.iter()
        .map(|&(at, what)| (at.to_owned(), what))
        .collect()
}

/// The ULT's refusals in either cut: what tC4's export cannot write back either.
fn ult_refused() -> Vec<(String, &'static str)> {
    listed(&[
        ("2SA 23:8", "unbalanced milestone"),
        // `ever-flowing`, which tC4's tokenizer reads as two words.
        ("AMO 5:24", "not one word"),
        ("MIC 7:6", "not one word"),
        ("NEH 2:13", "unbalanced milestone"),
    ])
}

#[test]
fn every_verse_of_the_ult_v91_reads_apart_and_weaves_back() {
    let (tc4_cut, book_cut) = census(&export("en_ult_v91.sb.zip"));
    assert_eq!(
        tc4_cut,
        Census {
            verses: 23_900,
            same: 23_631,
            kept: 0,
            renumbered: 3,
            milestones_moved: 180,
            // 63 of them are the verses of Lamentations that end with the next verse's
            // acrostic letter, which the file leaves bare (`\qa BETH` before `\v 2`): weave
            // writes it as a `\w` word (`tests/superscriptions.rs`). For 27 of them that is
            // the only change.
            relaid: 82,
            refused: ult_refused(),
            superscriptions: 50,
            outside: 391,
            untaken: 0,
            failures: Vec::new(),
        }
    );
    assert_eq!(
        book_cut,
        Census {
            verses: 23_900,
            same: 23_658,
            kept: 0,
            renumbered: 3,
            milestones_moved: 216,
            relaid: 19,
            refused: ult_refused(),
            superscriptions: 50,
            outside: 391,
            untaken: 0,
            failures: Vec::new(),
        }
    );
}

/// ULT MIC 5:7, 5:8 and 5:13 align occurrence 2 of a source word that their milestone
/// counts once (`x-occurrence="2" x-occurrences="1"`; UHB 3.0.0 MIC 5:7 holds the word
/// once). tC4 reads such a milestone as written, and its export writes these verses back
/// with their alignment, closing each milestone before the comma that the file keeps
/// inside it. So each verse reads apart with those source words reported, and weaves back
/// with the same text, words and links and its milestones closed where tC4 closes them.
#[test]
fn source_words_beyond_their_count_are_read_as_tc4_reads_them() {
    let name = "bundle/en/en_ult_v91.sb.zip en_ult/ingredients/MIC.usfm";
    let (_, micah) = export("en_ult_v91.sb.zip")
        .into_iter()
        .find(|(book, _)| book == name)
        .unwrap();
    let rewritten = &tc4_exports()[name].rewritten;
    let (found, _) = slots(&micah);
    let mut beyond = Vec::new();
    for key in ["5:7", "5:8", "5:13"] {
        let slot = found.iter().find(|s| s.key == key).unwrap();
        let parsed = parse_aligned(slot.content).unwrap_or_else(|e| panic!("MIC {key}: {e}"));
        for source in &parsed.sources_beyond_count {
            assert!(
                parsed.groups.iter().any(|g| g.sources.contains(source)),
                "MIC {key}"
            );
            let strong = source.strong.clone().unwrap();
            beyond.push((key, strong, source.occurrence, source.occurrences));
        }
        assert!(rewritten.contains(key), "MIC {key}");
        assert!(
            matches!(Outcome::of(slot.content), Ok(Outcome::MilestonesMoved)),
            "MIC {key}"
        );
    }
    let strong = |s: &str| s.to_owned();
    assert_eq!(
        beyond,
        [
            ("5:7", strong("c:H1961"), 2, 1),
            ("5:8", strong("c:H1961"), 2, 1),
            ("5:13", strong("c:H3808"), 2, 1),
            ("5:13", strong("H7812"), 2, 1),
        ]
    );
}

/// The UST's refusals in either cut. Each is a word tC4's tokenizer splits, such as
/// `Solomon’s` or `Ben-Hadad`, or `s` and `uch` written as two words, but for the
/// unbalanced milestone in 2SA 23:8.
fn ust_refused() -> Vec<(String, &'static str)> {
    [
        "1KI 9:20",
        "1KI 10:12",
        "1KI 11:41",
        "2SA 23:8",
        "AMO 1:3",
        "AMO 1:4",
        "AMO 1:11",
        "AMO 2:2",
        "AMO 2:3",
        "AMO 3:6",
        "AMO 5:26",
        "AMO 6:10",
        "AMO 7:1",
        "AMO 7:13",
        "AMO 7:14",
        "AMO 8:5",
        "HAB 3:14",
        "HAB 3:19",
        "HOS 2:6",
        "LAM 1:2",
        "LAM 1:5",
        "LAM 1:8",
        "LAM 2:7",
        "LAM 4:7",
        "MIC 6:6",
        "MIC 7:6",
        "ZEC 6:12",
        "ZEC 6:13",
        "ZEC 6:15",
        "ZEC 7:3",
        "ZEC 8:23",
        "ZEC 9:13",
        "ZEC 10:11",
        "ZEC 11:6",
        "ZEC 11:7",
        "ZEC 12:8",
        "ZEC 12:12",
        "ZEC 13:5",
        "ZEC 14:10",
    ]
    .into_iter()
    .map(|at| {
        let why = if at == "2SA 23:8" {
            "unbalanced milestone"
        } else {
            "not one word"
        };
        (at.to_owned(), why)
    })
    .collect()
}

#[test]
fn every_verse_of_the_ust_v91_reads_apart_and_weaves_back() {
    let (tc4_cut, book_cut) = census(&export("en_ust_v91.sb.zip"));
    assert_eq!(
        tc4_cut,
        Census {
            verses: 23_821,
            same: 23_560,
            kept: 0,
            renumbered: 3,
            milestones_moved: 157,
            // 47 in Lamentations, as in the ULT; for 25 of them the letter is the only change.
            relaid: 62,
            refused: ust_refused(),
            superscriptions: 52,
            outside: 416,
            untaken: 26,
            failures: Vec::new(),
        }
    );
    assert_eq!(
        book_cut,
        Census {
            verses: 23_823,
            same: 23_563,
            kept: 0,
            // The book cut gives a speaker heading (`\sp`) that leads into the next verse to
            // that verse, where the file counted its words with this one, as tC4 does.
            renumbered: 27,
            milestones_moved: 179,
            relaid: 15,
            refused: ust_refused(),
            superscriptions: 52,
            outside: 416,
            untaken: 0,
            failures: Vec::new(),
        }
    );
}

/// tC4's cut takes no verse of the original-language texts: their verse markers end with
/// a line break (`\v 1` LF). tC4 reads them with usfm-js to align against, and never cuts
/// or exports them.
#[test]
fn every_verse_of_the_uhb_reads_apart_into_itself() {
    let (tc4_cut, book_cut) = census(&export("hbo_uhb_v3.0.0.sb.zip"));
    assert_eq!(tc4_cut, Census::default());
    assert_eq!(
        book_cut,
        Census {
            verses: 23_145,
            kept: 23_145,
            ..Census::default()
        }
    );
}

#[test]
fn every_verse_of_the_ugnt_reads_apart_into_itself() {
    let (tc4_cut, book_cut) = census(&export("el-x-koine_ugnt_v0.34.sb.zip"));
    assert_eq!(tc4_cut, Census::default());
    assert_eq!(
        book_cut,
        Census {
            verses: 7_958,
            kept: 7_958,
            ..Census::default()
        }
    );
}

/// The scale Psalms are the same bytes as the ULT v91 Psalms.
#[test]
fn every_verse_of_the_scale_psalms_reads_apart_and_weaves_back() {
    let (tc4_cut, book_cut) = census(&scale_psalms());
    let expected = Census {
        verses: 2_461,
        same: 2_426,
        milestones_moved: 35,
        superscriptions: 50,
        outside: 391,
        untaken: 0,
        ..Census::default()
    };
    assert_eq!(tc4_cut, expected);
    assert_eq!(book_cut, expected);
}

/// tC4 #812: a layout line inside a verse (`\p`, `\ts\*`, `\b`, `\q3`, `\s`) never cuts the
/// verse off. Over every book of the ULT and UST v91, the words part the storage codec keeps
/// of each verse ([`tc_usfm::codec::decompose`]) holds every target word of the verse, and
/// the layout after it holds none.
#[test]
fn every_bundled_ult_and_ust_verse_body_holds_every_target_word() {
    let mut verses = 0;
    let mut cut = Vec::new();
    for archive in ["en_ult_v91.sb.zip", "en_ust_v91.sb.zip"] {
        for (name, text) in export(archive) {
            let parts = tc_usfm::codec::decompose(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            for (c, chapter) in &parts.chapters {
                for (v, part) in &chapter.verses {
                    let tc_usfm::codec::VersePart::Text(verse) = part else {
                        continue;
                    };
                    verses += 1;
                    let whole = format!("{}{}", verse.words, verse.layout);
                    if tc_usfm::target_words(&verse.words) != tc_usfm::target_words(&whole) {
                        cut.push(format!("{name} {c}:{v}"));
                    }
                }
            }
        }
    }
    assert_eq!(verses, 47_723);
    assert!(cut.is_empty(), "{} verses cut off: {cut:?}", cut.len());
}
