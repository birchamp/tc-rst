//! Property tests: lossless parsing, safe replacement and the alignment round trip.

// Helpers outside `#[test]` functions build inputs and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::links;
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use std::collections::HashMap;
use tc_usfm::text::{normalize_whitespace, verse_words};
use tc_usfm::words::{TokenKind, classify, words};
use tc_usfm::{
    AlignmentGroup, Book, Document, Error, SourceWord, TargetWord, display_text, parse_aligned,
    target_words, weave,
};

/// Pieces that exercise the lexer: markers of every shape, separators, attributes,
/// milestones, notes, unknown markers, stray backslashes and non-ASCII text.
const FRAGMENTS: &[&str] = &[
    "\\",
    "\\v",
    "\\v 1 ",
    "\\v 2",
    "\\v 3-4 ",
    "\\v 3a ",
    "\\v\t5\n",
    " ",
    "  ",
    "\n",
    "\r\n",
    "\r",
    "\t",
    "\\c",
    "\\c 1\n",
    "\\c 2 ",
    "\\c x\n",
    "\\p",
    "\\p\n",
    "\\q1 ",
    "\\q2\n",
    "\\b\n",
    "\\s1 ",
    "\\d ",
    "\\mt1 ",
    "\\id TIT ",
    "\\cl ",
    "\\w ",
    "\\w*",
    "\\+w ",
    "\\+w*",
    "\\f + ",
    "\\f*",
    "\\ft ",
    "\\fqa ",
    "\\fqa*",
    "\\x - ",
    "\\x*",
    "\\fig ",
    "\\fig*",
    "|",
    "|x=\"1\"",
    "x-occurrence=\"1\"",
    "\\*",
    "\\zaln-s |x-content=\"α\" x-occurrence=\"1\" x-occurrences=\"1\"",
    "\\zaln-s ",
    "\\zaln-e",
    "\\ts",
    "\\ca 2",
    "\\ca*",
    "\\va 1",
    "\\va*",
    "\\zfoo ",
    "\\zfoo*",
    "*",
    "+",
    "-",
    "=",
    "\"",
    "abc",
    "God",
    "Παῦλος",
    "וַֽ\u{2060}יְהִי֙",
    "\u{FEFF}",
    "\u{A0}",
    "😀",
    "___",
    "1",
    "9-10",
];

fn usfm_like() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        4 => proptest::sample::select(FRAGMENTS).prop_map(str::to_owned),
        1 => ".{0,4}",
    ];
    proptest::collection::vec(piece, 0..80).prop_map(|parts| parts.concat())
}

#[derive(Debug, Clone)]
enum Line {
    /// A verse: whether it is a span, what separates the number from the body, the body.
    Verse(bool, &'static str, String),
    Chapter,
    Other(&'static str),
}

/// A book with chapters, paragraphs, headings and verses whose bodies may hold anything.
/// Chapters and verses are numbered in order, so most keys are unique.
fn book_like() -> impl Strategy<Value = String> {
    let line = prop_oneof![
        3 => (
            any::<bool>(),
            proptest::sample::select(&[" ", " ", " ", "", "\n", "\t", "\r\n"][..]),
            prop_oneof![2 => "[a-z ,.;]{0,20}", 1 => usfm_like(), 1 => "\\\\(p|q1|w a|f \\+ |nd )[a-z ]{0,8}"],
        )
            .prop_map(|(span, separator, body)| Line::Verse(span, separator, body)),
        1 => Just(Line::Chapter),
        1 => proptest::sample::select(&["\\p\n", "\\q1\n", "\\s Head\n", "\\b\n", "\\ts\\*\n", "\\q2 more text\n", "\\d Title\n\\q1 ", "\\ca 2\\ca*\n", "\\cl Psalm\n"][..])
            .prop_map(Line::Other),
    ];
    proptest::collection::vec(line, 0..25).prop_map(|mut lines| {
        if !lines.iter().any(|l| matches!(l, Line::Verse(..))) {
            lines.push(Line::Verse(false, " ", "Last.".to_owned()));
        }
        let mut out = String::from("\\id TIT x\n\\h T\n\\c 1\n");
        let (mut chapter, mut verse) = (1, 0);
        for line in lines {
            match line {
                Line::Verse(span, separator, body) => {
                    verse += 1;
                    let label = if span {
                        verse += 1;
                        format!("{}-{verse}", verse - 1)
                    } else {
                        verse.to_string()
                    };
                    out.push_str(&format!("\\v {label}{separator}{body}\n"));
                }
                Line::Chapter => {
                    chapter += 1;
                    verse = 0;
                    out.push_str(&format!("\\c {chapter}\n"));
                }
                Line::Other(text) => out.push_str(text),
            }
        }
        out
    })
}

/// `PROPTEST_CASES` in the environment overrides `cases`, for longer local runs.
fn config(cases: u32) -> ProptestConfig {
    let default = ProptestConfig::default();
    ProptestConfig {
        cases: if std::env::var_os("PROPTEST_CASES").is_some() {
            default.cases
        } else {
            cases
        },
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource(
            "proptest-regressions",
        ))),
        ..default
    }
}

proptest! {
    #![proptest_config(config(512))]

    #[test]
    fn documents_write_back_byte_for_byte(src in usfm_like()) {
        prop_assert_eq!(Document::parse(&src).to_usfm(), src.clone());
        let book = Book::parse(&src);
        let joined: String = book.segments().iter().map(|s| s.text()).collect();
        prop_assert_eq!(joined, src.clone());
        let tokens: String = classify(&src).iter().map(|t| t.text).collect();
        prop_assert_eq!(tokens, src.clone());
        let text = display_text(&src);
        prop_assert_eq!(normalize_whitespace(&text), text);
        // Any input either reads apart or is refused, and the same for weaving; neither
        // panics, and a woven verse always reads back.
        if let Ok(parsed) = parse_aligned(&src)
            && let Ok(woven) = weave(&parsed.plain, &parsed.groups)
        {
            prop_assert!(parse_aligned(&woven).is_ok(), "{:?}", woven);
        }
        if let Ok(woven) = weave(&src, &[]) {
            prop_assert!(parse_aligned(&woven).is_ok(), "{:?}", woven);
        }
    }

    #[test]
    fn books_cut_into_segments_that_rejoin(src in book_like()) {
        let book = Book::parse(&src);
        let joined: String = book.segments().iter().map(|s| s.text()).collect();
        prop_assert_eq!(joined, src.clone());
        let mut at = 0;
        for segment in book.segments() {
            prop_assert_eq!(segment.range().start, at);
            at = segment.range().end;
            prop_assert_eq!(&src[segment.range()], segment.text());
            if let Some(unit) = segment.as_verse() {
                let rebuilt = format!("{}{}{}", unit.leading(), unit.marker(), unit.body());
                prop_assert_eq!(rebuilt.as_str(), segment.text());
                prop_assert!(unit.marker().starts_with("\\v"));
            }
        }
    }

    #[test]
    fn writing_back_every_body_changes_nothing(src in book_like()) {
        let book = Book::parse(&src);
        for (index, segment) in book.segments().iter().enumerate() {
            let Some(unit) = segment.as_verse() else { continue };
            prop_assert_eq!(book.replace_bodies(&[(index, unit.body())]), Ok(src.clone()));
            prop_assert_eq!(book.replace_units(&[(index, segment.text())]), Ok(src.clone()));
            if let (Some(chapter), Some(verses)) = (unit.chapter(), unit.verses()) {
                match book.replace_body(chapter, verses, unit.body()) {
                    Ok(same) => prop_assert_eq!(same, src.clone()),
                    Err(e) => prop_assert_eq!(e, Error::AmbiguousVerse { chapter, verses }),
                }
            }
        }
        // All at once, read once.
        let all: Vec<(usize, &str)> = book
            .segments()
            .iter()
            .enumerate()
            .filter_map(|(i, s)| Some((i, s.as_verse()?.body())))
            .collect();
        prop_assert_eq!(book.replace_bodies(&all), Ok(src.clone()));
    }

    #[test]
    fn replacing_a_body_touches_only_that_unit(
        src in book_like(),
        pick in any::<prop::sample::Index>(),
        new_body in prop_oneof![usfm_like(), "[a-zA-Z ,.;]{0,30}\n?"],
    ) {
        let book = Book::parse(&src);
        let keyed: Vec<_> = book
            .verses()
            .filter_map(|u| Some((u.chapter()?, u.verses()?)))
            .collect();
        prop_assume!(!keyed.is_empty());
        let (chapter, verses) = keyed[pick.index(keyed.len())];
        // After a marker that ends in a lone CR, a body that starts with LF would join
        // the marker's line end, so it is refused.
        let marker = book.verse(chapter, verses).map(|u| u.marker()).unwrap_or("");
        let safe = !new_body.contains('\\') && !(marker.ends_with('\r') && new_body.starts_with('\n'));
        match book.replace_body(chapter, verses, &new_body) {
            Ok(edited) => {
                let after = Book::parse(&edited);
                prop_assert_eq!(after.segments().len(), book.segments().len());
                for (old, new) in book.segments().iter().zip(after.segments()) {
                    let target = old.as_verse().is_some_and(|u| {
                        (u.chapter(), u.verses()) == (Some(chapter), Some(verses))
                    });
                    if target {
                        let unit = new.as_verse().unwrap();
                        prop_assert_eq!(unit.body(), new_body.as_str());
                        prop_assert_eq!(unit.leading(), old.as_verse().unwrap().leading());
                    } else {
                        prop_assert_eq!(old.text(), new.text());
                    }
                }
                // Every byte before and after the unit's body is the same.
                let unit = book.verse(chapter, verses).unwrap();
                let start = unit.body().as_ptr() as usize - src.as_ptr() as usize;
                let end = start + unit.body().len();
                prop_assert_eq!(&edited[..start], &src[..start]);
                prop_assert_eq!(&edited[edited.len() - (src.len() - end)..], &src[end..]);
            }
            Err(Error::AmbiguousVerse { .. }) => {}
            Err(e) => {
                prop_assert!(!safe, "safe text {:?} was refused: {}", new_body, e);
                prop_assert_eq!(e, Error::StructureChanged { chapter, verses });
            }
        }
    }

    #[test]
    fn replacing_a_unit_touches_only_that_unit(
        src in book_like(),
        pick in any::<prop::sample::Index>(),
        leading in proptest::sample::select(&["", "\\p\n", "\\q2 ", "\\s Heading\n\\p\n", "\\ts\\*\n\\b\n"][..]),
        body in "[a-zA-Z ,.;]{0,30}\n",
    ) {
        let book = Book::parse(&src);
        let keyed: Vec<_> = book
            .verses()
            .filter_map(|u| Some((u.chapter()?, u.verses()?, u.marker())))
            .collect();
        prop_assume!(!keyed.is_empty());
        let (chapter, verses, _) = keyed[pick.index(keyed.len())];
        let unit = format!("{leading}\\v {verses} {body}");
        match book.replace_unit(chapter, verses, &unit) {
            Ok(edited) => {
                let after = Book::parse(&edited);
                prop_assert_eq!(after.segments().len(), book.segments().len());
                for (old, new) in book.segments().iter().zip(after.segments()) {
                    let target = old.as_verse().is_some_and(|u| {
                        (u.chapter(), u.verses()) == (Some(chapter), Some(verses))
                    });
                    if target {
                        prop_assert_eq!(new.text(), unit.as_str());
                    } else {
                        prop_assert_eq!(old.text(), new.text());
                    }
                }
            }
            // A leading heading or paragraph marker after a verse whose body ends in
            // non-text markup may glue onto the previous unit; the edit is refused.
            Err(Error::AmbiguousVerse { .. } | Error::StructureChanged { .. }) => {}
            Err(e) => prop_assert!(false, "unexpected {e}"),
        }
    }

    #[test]
    fn word_occurrences_count_identical_words(text in "[a-c ,.]{0,40}") {
        let found = words(&text);
        for w in &found {
            let same: Vec<_> = found.iter().filter(|o| o.text == w.text).collect();
            prop_assert_eq!(w.occurrences as usize, same.len());
            prop_assert_eq!(same[w.occurrence as usize - 1].start, w.start);
            prop_assert_eq!(&text[w.start..w.start + w.text.len()], w.text);
        }
        let kinds: Vec<TokenKind> = classify(&text).iter().map(|t| t.kind).collect();
        prop_assert!(!kinds.contains(&TokenKind::Other));
    }

    #[test]
    fn verse_words_and_target_words_agree(verse in plain_verse(true)) {
        let a: Vec<(String, u32, u32)> = verse_words(&verse)
            .into_iter()
            .map(|w| (w.text.to_owned(), w.occurrence, w.occurrences))
            .collect();
        let b: Vec<(String, u32, u32)> = target_words(&verse)
            .unwrap()
            .into_iter()
            .map(|w| (w.text, w.occurrence, w.occurrences))
            .collect();
        prop_assert_eq!(a, b);
    }

    #[test]
    fn woven_verses_take_apart_into_what_was_woven(
        verse in plain_verse(false),
        runs in proptest::collection::vec((1usize..4, 0usize..4), 1..12),
        split in any::<bool>(),
    ) {
        let slots = target_words(&verse).unwrap();
        let groups = group_words(&slots, &runs, split);
        let woven = weave(&verse, &groups).unwrap();
        let parsed = parse_aligned(&woven).unwrap();
        prop_assert_eq!(&parsed.plain, &verse);
        prop_assert_eq!(&parsed.words, &slots);
        prop_assert_eq!(links(&parsed.groups), links(&groups));
        // The parsed groups are the runs, so weaving them writes the same bytes.
        prop_assert_eq!(weave(&parsed.plain, &parsed.groups).unwrap(), woven);
    }

    #[test]
    fn line_breaks_before_words_change_only_whitespace(
        verse in plain_verse(true),
        runs in proptest::collection::vec((1usize..4, 0usize..4), 1..12),
    ) {
        let slots = target_words(&verse).unwrap();
        let groups = group_words(&slots, &runs, false);
        let parsed = parse_aligned(&weave(&verse, &groups).unwrap()).unwrap();
        prop_assert_eq!(display_text(&parsed.plain), display_text(&verse));
        prop_assert_eq!(parsed.plain.replace('\n', " "), verse.replace('\n', " "));
        prop_assert_eq!(links(&parsed.groups), links(&groups));
    }

    #[test]
    fn any_aligned_verse_reads_back_after_one_weave(verse in aligned_verse()) {
        let (aligned, plain, expected) = verse;
        let parsed = parse_aligned(&aligned).unwrap();
        prop_assert_eq!(&parsed.plain, &plain);
        prop_assert_eq!(links(&parsed.groups), expected);
        let woven = weave(&parsed.plain, &parsed.groups).unwrap();
        let again = parse_aligned(&woven).unwrap();
        // Markup can leave a line break right before a word in the plain text; the woven
        // form reads that line break back as a space.
        prop_assert_eq!(again.plain.replace('\n', " "), parsed.plain.replace('\n', " "));
        prop_assert_eq!(&again.words, &parsed.words);
        prop_assert_eq!(links(&again.groups), links(&parsed.groups));
        // One weave reaches the canonical form.
        prop_assert_eq!(weave(&again.plain, &again.groups).unwrap(), woven);
    }
}

const WORDS: &[&str] = &[
    "a",
    "of",
    "God",
    "Παῦλος",
    "וַֽ\u{2060}יְהִי֙",
    "123",
    "self",
    "x",
];

/// A plain verse: words (repeated, so occurrences exceed 1), punctuation, spaces, notes
/// and markers. With `line_breaks`, a line break may come right before a word, which the
/// aligned form cannot tell apart from a space.
fn plain_verse(line_breaks: bool) -> impl Strategy<Value = String> {
    let word = proptest::sample::select(WORDS);
    let mut gaps = vec![
        " ",
        ", ",
        "; ",
        "—",
        " {",
        "} ",
        "-",
        ". ",
        " “",
        "” ",
        ".” ",
        " \\f + \\ft note\\f* ",
        "\n\\q2 ",
        ", \\nd ",
        "\\nd* ",
        "\r\n",
        " $ ",
        "\r ",
        ",\r ",
        " \r ",
    ];
    if line_breaks {
        gaps.extend(["\n", ",\n", "  \n"]);
    }
    let gap = proptest::sample::select(gaps);
    let end = proptest::sample::select(&["", ".", ".\n", ",\n\n", "\n\\p\n"][..]);
    (
        proptest::sample::select(&["", "{", "“"][..]),
        proptest::collection::vec((word.clone(), gap), 1..20),
        word,
        end,
    )
        .prop_map(|(start, middle, last, end)| {
            let mut out = start.to_owned();
            for (w, g) in middle {
                out.push_str(w);
                out.push_str(g);
            }
            out.push_str(last);
            out.push_str(end);
            out
        })
}

fn source(n: usize, s: usize) -> SourceWord {
    SourceWord {
        content: format!("λ{n}"),
        strong: Some(format!("G{n:04}{s}")),
        lemma: Some("λόγος".to_owned()),
        morph: s.is_multiple_of(2).then(|| "Gr,N,,,,,NMS,".to_owned()),
        occurrence: 1,
        occurrences: 1,
        extra: if s == 2 {
            vec![("x-tw".to_owned(), "rc://*/tw/dict/bible/kt/god".to_owned())]
        } else {
            Vec::new()
        },
    }
}

/// Cut the words into consecutive runs; a run with zero sources stays unaligned. With
/// `split`, the first group also takes the last word, which forces a second run.
fn group_words(slots: &[TargetWord], runs: &[(usize, usize)], split: bool) -> Vec<AlignmentGroup> {
    let mut groups = Vec::new();
    let mut at = 0;
    for (n, &(len, sources)) in runs.iter().enumerate() {
        if at >= slots.len() {
            break;
        }
        let end = (at + len).min(slots.len());
        if sources > 0 {
            groups.push(AlignmentGroup {
                sources: (0..sources).map(|s| source(n, s)).collect(),
                targets: slots[at..end].to_vec(),
            });
        }
        at = end;
    }
    if split && slots.len() > 2 {
        let last = slots[slots.len() - 1].clone();
        let owner = groups.iter().position(|g| g.targets.contains(&last));
        if let Some(first) = groups.first_mut()
            && !first.targets.contains(&last)
        {
            first.targets.push(last.clone());
            if let Some(owner) = owner {
                groups[owner].targets.retain(|t| *t != last);
            }
        }
        groups.retain(|g| !g.targets.is_empty());
    }
    groups
}

#[derive(Debug, Clone)]
enum Item {
    /// Plain text between words, written as is.
    Text(&'static str),
    Word(&'static str),
    /// Open a milestone for this source word.
    Open(usize),
    /// Close the innermost open milestone.
    Close,
}

/// An aligned verse in any shape the markup allows: milestones nested and closed in any
/// order, words inside and outside them, text anywhere. Returns the aligned text, the
/// plain text it stands for, and what it means.
#[allow(clippy::type_complexity)]
fn aligned_verse() -> impl Strategy<
    Value = (
        String,
        String,
        std::collections::BTreeMap<(String, u32), Vec<(String, u32)>>,
    ),
> {
    let item = prop_oneof![
        3 => proptest::sample::select(&[" ", ", ", "\n", " {", "} ", ".", "—", "\\f + \\ft a\\f*", "\n\\q2 ", " \\zaln-s |x-content=\"no\" x-occurrence=\"1\" x-occurrences=\"1\"\\*\\zaln-e\\*"][..]).prop_map(Item::Text),
        4 => proptest::sample::select(WORDS).prop_map(Item::Word),
        2 => (0usize..6).prop_map(Item::Open),
        2 => Just(Item::Close),
    ];
    proptest::collection::vec(item, 0..40).prop_map(|items| {
        // Occurrences are counted over the whole verse first.
        let mut totals: HashMap<&str, u32> = HashMap::new();
        for item in &items {
            if let Item::Word(w) = item {
                *totals.entry(w).or_default() += 1;
            }
        }
        let mut seen: HashMap<&str, u32> = HashMap::new();
        let mut aligned = String::new();
        let mut plain = String::new();
        let mut open: Vec<usize> = Vec::new();
        let mut meaning = std::collections::BTreeMap::new();
        // Whether the plain text so far ends in a line break written as text, which reads
        // back as a space before a word or a milestone.
        let mut last_text_ends_in_lf = false;
        // Two words with nothing between them would read back as one word.
        let mut last_was_word = false;
        for item in items {
            match item {
                Item::Text(t) => {
                    aligned.push_str(t);
                    if t.starts_with(" \\zaln-s") {
                        // An empty milestone pair: markup after one space.
                        plain.push(' ');
                        last_text_ends_in_lf = false;
                    } else {
                        plain.push_str(t);
                        last_text_ends_in_lf = t.ends_with('\n');
                    }
                    last_was_word = false;
                }
                Item::Word(w) => {
                    if last_was_word {
                        aligned.push('—');
                        plain.push('—');
                    }
                    if last_text_ends_in_lf {
                        plain.pop();
                        plain.push(' ');
                    }
                    last_text_ends_in_lf = false;
                    last_was_word = true;
                    let n = seen.entry(w).or_default();
                    *n += 1;
                    aligned.push_str(&format!(
                        "\\w {w}|x-occurrence=\"{n}\" x-occurrences=\"{}\"\\w*",
                        totals[w]
                    ));
                    plain.push_str(w);
                    if !open.is_empty() {
                        meaning.insert(
                            (w.to_owned(), *n),
                            open.iter().map(|s| (format!("λ{s}"), 1)).collect(),
                        );
                    }
                }
                Item::Open(s) => {
                    if last_text_ends_in_lf {
                        plain.pop();
                        plain.push(' ');
                    }
                    last_text_ends_in_lf = false;
                    open.push(s);
                    aligned.push_str(&format!(
                        "\\zaln-s |x-strong=\"G{s:04}0\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"λ{s}\"\\*"
                    ));
                }
                Item::Close => {
                    if open.pop().is_some() {
                        aligned.push_str("\\zaln-e\\*");
                        last_text_ends_in_lf = false;
                    }
                }
            }
        }
        for _ in open.drain(..) {
            aligned.push_str("\\zaln-e\\*");
        }
        (aligned, plain, meaning)
    })
}
