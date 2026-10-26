//! The journal codec: verse split, decompose and recompose (`docs/STORAGE.md` §2.4, §7.5,
//! §13.5).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod census;

use census::{Census, run_with_census};
use proptest::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tc_usfm::codec::{
    BookParts, ChapterParts, DecomposeProblem, LayoutItem, MAX_BOOK_VERSES, MarkerTerm, MarkerWs,
    StructureIssue, VersePart, VerseText, decompose, decompose_bytes, has_verse_or_chapter_marker,
    is_layout_only_line, is_valid_layout, join_verse, recompose, split_verse,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// Every USFM file of tC4's fixtures, including its identity corpus.
fn corpus() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![fixtures().join("tc4")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("usfm" | "sfm" | "txt")
            ) && std::fs::read(&path).unwrap().starts_with(b"\\id")
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn every_fixture_book_round_trips_byte_for_byte() {
    let books = corpus();
    assert!(books.len() >= 15, "found only {} books", books.len());
    for path in books {
        let bytes = std::fs::read(&path).unwrap();
        let parts = decompose_bytes(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let back = recompose(&parts);
        assert!(
            back.issues.is_empty(),
            "{}: {:?}",
            path.display(),
            back.issues
        );
        assert!(
            back.usfm.as_bytes() == bytes.as_slice(),
            "{} does not round-trip",
            path.display()
        );
        for chapter in parts.chapters.values() {
            for part in chapter.verses.values() {
                if let VersePart::Text(text) = part {
                    assert!(!has_verse_or_chapter_marker(&text.words));
                    assert!(is_valid_layout(&text.words, &text.layout));
                }
            }
        }
    }
}

#[test]
fn a_span_is_a_text_and_covered_verses() {
    let parts = decompose("\\id JON\n\\c 1\n\\p\n\\v 1 a\n\\v 2-4 b c\n\\q1\n\\v 5\tz").unwrap();
    assert_eq!(parts.header, "\\id JON\n");
    let chapter = &parts.chapters[&1];
    assert_eq!(chapter.front.as_deref(), Some("\\c 1\n\\p\n"));
    let VersePart::Text(two) = &chapter.verses[&2] else {
        panic!("verse 2 is not a text")
    };
    assert_eq!(
        (two.words.as_str(), two.layout.as_str(), two.last),
        ("b c", "\n\\q1\n", 4)
    );
    assert_eq!(chapter.verses[&3], VersePart::Covered { by: 2 });
    assert_eq!(chapter.verses[&4], VersePart::Covered { by: 2 });
    let VersePart::Text(five) = &chapter.verses[&5] else {
        panic!("verse 5 is not a text")
    };
    assert_eq!(five.marker, MarkerWs::new(" ", MarkerTerm::Tab).unwrap());
    assert_eq!(parts.layout, None);
}

#[test]
fn marker_whitespace_is_kept() {
    for text in [
        "\\c 1\n\\v\t1\na\n",
        "\\c 1\r\n\\v  1\r\nb\r\n",
        "\\c 1\r\\v 1\rc\r",
        "\\c 1\n\\v \t 1 \n",
    ] {
        let parts = decompose(text).unwrap();
        assert_eq!(recompose(&parts).usfm, text);
    }
}

#[test]
fn verse_markers_before_the_first_chapter_stay_in_the_header() {
    let text = "\\id TIT\n\\v 2a odd\n\\v x\n\\c 1\n\\v 1 a\n";
    let parts = decompose(text).unwrap();
    assert_eq!(parts.header, "\\id TIT\n\\v 2a odd\n\\v x\n");
    assert_eq!(recompose(&parts).usfm, text);
}

#[test]
fn out_of_order_verses_and_chapters_are_kept_in_a_layout() {
    let text = "\\c 2\n\\v 1 x\n\\c 1\n\\v 2 b\n\\v 1 a\n";
    let parts = decompose(text).unwrap();
    assert_eq!(
        parts.layout,
        Some(vec![
            LayoutItem::Chapter(2),
            LayoutItem::Verse(2, 1),
            LayoutItem::Chapter(1),
            LayoutItem::Verse(1, 2),
            LayoutItem::Verse(1, 1),
        ])
    );
    assert_eq!(recompose(&parts).usfm, text);
}

#[test]
fn refusals_name_the_line_and_the_problem() {
    let cases: &[(&str, usize, DecomposeProblem)] = &[
        (
            "\\c 1\n\\v 1 a\n\\v 1 b\n",
            3,
            DecomposeProblem::DuplicateVerse {
                chapter: 1,
                verse: 1,
            },
        ),
        (
            "\\c 1\n\\v 1-2 a\n\\v 2 b\n",
            3,
            DecomposeProblem::DuplicateVerse {
                chapter: 1,
                verse: 2,
            },
        ),
        (
            "\\c 1\n\\v 1 a\n\\c 1\n",
            3,
            DecomposeProblem::DuplicateChapter { chapter: 1 },
        ),
        (
            "\\c 1\n\\v 2a x\n",
            2,
            DecomposeProblem::LetterPart { label: "2a".into() },
        ),
        (
            "\\c 1\n\\v 3-2 x\n",
            2,
            DecomposeProblem::SpanNotAscending { first: 3, last: 2 },
        ),
        (
            "\\c 1\n\\v 3-3 x\n",
            2,
            DecomposeProblem::SpanNotAscending { first: 3, last: 3 },
        ),
        (
            "\\c 1\n\\v 01 x\n",
            2,
            DecomposeProblem::LeadingZero { label: "01".into() },
        ),
        ("\\c 1\n\\v 0 x\n", 2, DecomposeProblem::VerseZero),
        (
            "\\c 1\n\\v 70000 x\n",
            2,
            DecomposeProblem::NumberTooLarge {
                label: "70000".into(),
            },
        ),
        ("\\c 1\n\\v 1", 2, DecomposeProblem::VerseMarkerAtEnd),
        ("\\c 1\n\\v\n1 a", 2, DecomposeProblem::VerseLabel),
        ("\\c 1\n\\v x\n", 2, DecomposeProblem::VerseLabel),
        ("\\c 1\n\\v", 2, DecomposeProblem::VerseLabel),
        ("\\c x\n", 1, DecomposeProblem::ChapterLabel),
        ("\\id A\n\\c 1a\n", 2, DecomposeProblem::ChapterLabel),
        ("\\c 0\n", 1, DecomposeProblem::ChapterZero),
        // A `\v` or `\c` right before a marker would end its part, where V3 reads it as a
        // marker.
        ("\\c 1 \\v 1 \\v\\c 2 ", 1, DecomposeProblem::VerseLabel),
        (
            "\\c 1\n\\v 1 a\n\\v\\c 2\n",
            3,
            DecomposeProblem::VerseLabel,
        ),
        ("\\id X \\c\\c 1\n", 1, DecomposeProblem::ChapterLabel),
        (
            "\\c 1\n\\v 1 a \\c\\v 2 b",
            2,
            DecomposeProblem::ChapterLabel,
        ),
        ("\\c 1\n\\v 1 a\u{0}\n", 2, DecomposeProblem::Nul),
    ];
    for (text, line, problem) in cases {
        let error = decompose(text).unwrap_err();
        assert_eq!((error.line, &error.problem), (*line, problem), "{text:?}");
    }
    let error = decompose_bytes(b"\\c 1\n\\v 1 \xff\n").unwrap_err();
    assert_eq!(
        (error.line, error.problem),
        (2, DecomposeProblem::InvalidUtf8)
    );
}

#[test]
fn chapter_numbers_may_have_leading_zeros_because_fronts_are_verbatim() {
    for (text, number) in [
        ("\\c 01\n\\v 1 a\n", 1),
        ("\\id TIT\n\\c 000001\n\\v 1 a\n", 1),
        ("\\c 0000000000000000000002\n\\v 1 a\n", 2),
    ] {
        let parts = decompose(text).unwrap();
        assert_eq!(parts.chapters.keys().copied().collect::<Vec<_>>(), [number]);
        assert_eq!(recompose(&parts).usfm, text);
    }
}

/// Each verse is a record, so a few bytes of `\v 1-65535` must not stand for millions of
/// them: the work and the memory of a decode follow the size of the file, never the length
/// of its spans (§13.5).
#[test]
fn a_book_holds_at_most_65535_verses() {
    let whole = "\\c 1\n\\v 1-65535 x\n";
    let parts = decompose(whole).unwrap();
    assert_eq!(parts.chapters[&1].verses.len(), MAX_BOOK_VERSES);
    assert_eq!(recompose(&parts).usfm, whole);
    let refusal = |book: &str| decompose(book).err().map(|e| (e.line, e.problem));
    let one_more = format!("{whole}\\c 2\n\\v 1 y\n");
    assert_eq!(
        refusal(&one_more),
        Some((4, DecomposeProblem::TooManyVerses))
    );

    // 400 chapters of the longest span are 7.9 KB, and stood for 26 million verses.
    let started = Instant::now();
    let book: String = (1..=400)
        .map(|c| format!("\\c {c}\n\\v 1-65535 x\n"))
        .collect();
    assert_eq!(refusal(&book), Some((4, DecomposeProblem::TooManyVerses)));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );

    // Short spans count verse by verse, too: three chapters of 4,400 spans of five verses
    // are 66,000 verses, and the 4,308th span of the third chapter is one too many.
    let spans: String = (0..4_400)
        .map(|i| format!("\\v {}-{} x\n", 5 * i + 1, 5 * i + 5))
        .collect();
    let book = format!("\\c 1\n{spans}\\c 2\n{spans}\\c 3\n{spans}");
    assert_eq!(
        refusal(&book),
        Some((2 * 4_401 + 1 + 4_308, DecomposeProblem::TooManyVerses))
    );
}

#[test]
fn layout_only_lines_hold_only_paragraph_markers() {
    for line in [
        "",
        "  ",
        "\\p",
        "\\q1",
        "\\Q2",
        "\\pi2 ",
        "\\li1\t\\b",
        "\\ts\\*",
        "\\nb",
        "\\s1",
        "\\ts-s\\*",
    ] {
        assert!(is_layout_only_line(line), "{line:?}");
    }
    for line in [
        "a",
        "\\s1 Heading",
        "\\w a\\w*",
        "\\v 1",
        "\\c 2",
        "\\p a",
        "\\zaln-s",
        "\\*",
        "\\+p",
        "\\p,",
        "\\pfoo",
        "\\p\r",
    ] {
        assert!(!is_layout_only_line(line), "{line:?}");
    }
}

#[test]
fn the_layout_rule_needs_a_line_break_after_words() {
    assert!(is_valid_layout("a", ""));
    assert!(is_valid_layout("a", "\n\\p\n"));
    assert!(is_valid_layout("a", "\r\n\\q1\r\n"));
    assert!(is_valid_layout("", "\\p\n"));
    assert!(!is_valid_layout("a", "\\p\n"));
    assert!(!is_valid_layout("a", "\nmore words\n"));
    assert!(!is_valid_layout("", "\\v 2 x"));
}

#[test]
fn marker_detection_follows_the_v3_pattern() {
    for text in ["\\v 1", "a \\c\n", "x\\v", "\\v\t", "\\c\r"] {
        assert!(has_verse_or_chapter_marker(text), "{text:?}");
    }
    for text in [
        "\\va 1\\va*",
        "\\vp 2\\vp*",
        "\\ca 3\\ca*",
        "\\cl Chapter",
        "v c",
        "\\\\x",
    ] {
        assert!(!has_verse_or_chapter_marker(text), "{text:?}");
    }
}

fn text(words: &str, layout: &str, last: u16) -> VersePart {
    VersePart::Text(VerseText {
        marker: MarkerWs::canonical(),
        words: words.into(),
        layout: layout.into(),
        last,
    })
}

fn chapter(front: Option<&str>, verses: Vec<(u16, VersePart)>) -> ChapterParts {
    ChapterParts {
        front: front.map(Into::into),
        verses: verses.into_iter().collect(),
    }
}

#[test]
fn incoherent_parts_write_every_text_once_and_are_reported() {
    let parts = BookParts {
        header: String::new(),
        chapters: BTreeMap::from([
            (
                1,
                chapter(
                    Some("\\c 1\n"),
                    vec![
                        (4, text("four", "\n", 6)),
                        (5, VersePart::Covered { by: 4 }),
                        (6, text("six", "\n", 6)),
                        (7, VersePart::Covered { by: 2 }),
                    ],
                ),
            ),
            (2, chapter(None, vec![(1, text("one", "\n", 1))])),
        ]),
        layout: None,
    };
    let out = recompose(&parts);
    assert_eq!(
        out.usfm,
        "\\c 1\n\\v 4-5 four\n\\v 6 six\n\\c 2\n\\v 1 one\n"
    );
    assert_eq!(
        out.issues,
        vec![
            StructureIssue::ShortSpan {
                chapter: 1,
                verse: 4,
                last: 6,
                end: 5
            },
            StructureIssue::StrayCovered {
                chapter: 1,
                verse: 7,
                by: 2
            },
            StructureIssue::MissingChapterFront { chapter: 2 },
        ]
    );
}

#[test]
fn records_the_layout_does_not_list_follow_their_numeric_predecessor() {
    let parts = BookParts {
        header: String::new(),
        chapters: BTreeMap::from([
            (
                1,
                chapter(
                    Some("\\c 1\n"),
                    vec![
                        (1, text("a", "\n", 1)),
                        (2, text("b", "\n", 2)),
                        (3, text("c", "\n", 3)),
                        (5, text("e", "\n", 5)),
                    ],
                ),
            ),
            (3, chapter(Some("\\c 3\n"), vec![])),
            (4, chapter(Some("\\c 4\n"), vec![])),
        ]),
        layout: Some(vec![
            LayoutItem::Chapter(4),
            LayoutItem::Chapter(1),
            LayoutItem::Verse(1, 3),
            LayoutItem::Verse(1, 9),
            LayoutItem::Verse(1, 2),
            LayoutItem::Chapter(8),
        ]),
    };
    let out = recompose(&parts);
    assert_eq!(
        out.usfm,
        "\\c 4\n\\c 1\n\\v 1 a\n\\v 3 c\n\\v 5 e\n\\v 2 b\n\\c 3\n"
    );
    assert!(out.issues.is_empty());
}

// ---------------------------------------------------------------------------------------
// Generated books
// ---------------------------------------------------------------------------------------

const WORD_PIECES: &[&str] = &[
    "Ellos",
    "deben",
    "enseñar",
    "אֱלֹהִים",
    "word",
    "\\w a|x-occurrence=\"1\"\\w*",
    "\\f + \\ft note\\f*",
    "\\nd Lord\\nd*",
    " ",
    "  ",
    "\t",
    ",",
    "—",
    "\\s1 Heading",
    "\\vp 3\\vp*",
    "\\ca 2\\ca*",
    "\\zaln-s |x-content=\"x\"\\*",
    "\\zaln-e\\*",
    "\u{a0}",
    "___",
];
const LAYOUT_LINES: &[&str] = &[
    "\\p", "\\q1", "\\q2", "", "\\b", "\\m", "\\ts\\*", "\\li1", "  \\nb",
];
const BREAKS: &[&str] = &["\n", "\r\n"];

fn words_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        (
            prop::sample::select(WORD_PIECES),
            prop::option::weighted(0.25, prop::sample::select(BREAKS)),
            prop::option::weighted(0.15, prop::sample::select(LAYOUT_LINES)),
        ),
        0..6,
    )
    .prop_map(|pieces| {
        let mut out = String::new();
        for (piece, line_break, line) in pieces {
            out.push_str(piece);
            if let Some(line_break) = line_break {
                out.push_str(line_break);
                if let Some(line) = line {
                    out.push_str(line);
                    out.push(' ');
                }
            }
        }
        out
    })
}

fn verse_content() -> impl Strategy<Value = String> {
    (
        words_strategy(),
        prop::collection::vec(
            (
                prop::sample::select(BREAKS),
                prop::sample::select(LAYOUT_LINES),
            ),
            0..3,
        ),
        prop::option::of(prop::sample::select(BREAKS)),
    )
        .prop_map(|(words, layout, end)| {
            let mut out = words;
            for (line_break, line) in layout {
                out.push_str(line_break);
                out.push_str(line);
            }
            if let Some(end) = end {
                out.push_str(end);
            }
            out
        })
}

/// One verse unit: (span length, lead, terminator, content).
fn verse_unit() -> impl Strategy<Value = (u16, String, char, String)> {
    (
        prop::sample::select(&[0u16, 0, 0, 1, 2][..]),
        prop::sample::select(&[" ", "  ", "\t", " \t"][..]),
        prop::sample::select(&[' ', ' ', '\t', '\n', '\r'][..]),
        verse_content(),
    )
        .prop_map(|(extra, lead, term, content)| (extra, lead.to_owned(), term, content))
}

/// A book written by hand, not by `recompose`: header, chapters with fronts, verses with
/// spans, poetry, CRLF, tabs and odd marker whitespace, sometimes out of order.
fn book() -> impl Strategy<Value = String> {
    (
        prop::sample::select(
            &[
                "",
                "\\id TIT\n",
                "\u{feff}\\id JON EN\r\n\\h Jonás\r\n\\mt1 Jonás\r\n",
            ][..],
        ),
        prop::collection::vec(
            (
                prop::sample::select(
                    &[
                        "\n",
                        "\r\n",
                        " \n",
                        "\n\\s1 Title\n\\p\n",
                        "\n\\cl Capítulo\n",
                    ][..],
                ),
                prop::collection::vec(verse_unit(), 0..6),
                any::<bool>(),
            ),
            0..4,
        ),
        any::<bool>(),
    )
        .prop_map(|(header, chapters, shuffle_chapters)| {
            let mut out = header.to_owned();
            let mut numbered: Vec<(u16, String)> = Vec::new();
            for (index, (front_tail, verses, reverse)) in chapters.into_iter().enumerate() {
                let number = u16::try_from(index + 1).unwrap();
                let mut chapter = format!("\\c {number}{front_tail}");
                let mut units = Vec::new();
                let mut verse = 1u16;
                for (extra, lead, term, content) in verses {
                    let label = if extra == 0 {
                        verse.to_string()
                    } else {
                        format!("{verse}-{}", verse + extra)
                    };
                    units.push(format!("\\v{lead}{label}{term}{content}"));
                    verse += extra + 1;
                }
                if reverse {
                    units.reverse();
                }
                for unit in units {
                    chapter.push_str(&unit);
                }
                numbered.push((number, chapter));
            }
            if shuffle_chapters {
                numbered.reverse();
            }
            for (_, chapter) in numbered {
                out.push_str(&chapter);
            }
            out
        })
}

/// The states `book()` claims to reach, read from a decomposed book.
fn book_states(text: &str, parts: &BookParts, census: &Census) {
    if text.starts_with('\u{feff}') {
        census.saw("byte order mark");
    }
    if text.contains("\r\n") {
        census.saw("CRLF");
    }
    if let Some(layout) = &parts.layout {
        let chapters: Vec<u16> = layout
            .iter()
            .filter_map(|item| match item {
                LayoutItem::Chapter(c) => Some(*c),
                LayoutItem::Verse(..) => None,
            })
            .collect();
        if chapters.windows(2).any(|pair| pair[0] > pair[1]) {
            census.saw("chapters out of order");
        } else {
            census.saw("verses out of order");
        }
    }
    for chapter in parts.chapters.values() {
        if chapter
            .front
            .as_deref()
            .is_some_and(|front| front.contains("\\s1"))
        {
            census.saw("heading");
        }
        if chapter.verses.is_empty() {
            census.saw("chapter without verses");
        }
        for (verse, part) in &chapter.verses {
            let VersePart::Text(text) = part else {
                continue;
            };
            if text.last > *verse {
                census.saw("span");
            }
            if text.marker.lead().contains('\t') || text.marker.term() == MarkerTerm::Tab {
                census.saw("tab in a marker");
            }
            if text.marker.lead() != " " || text.marker.term() != MarkerTerm::Space {
                census.saw("odd marker whitespace");
            }
            if text.words.contains("\\q") || text.layout.contains("\\q") {
                census.saw("poetry");
            }
            if !text.layout.is_empty() {
                census.saw("layout");
            }
        }
    }
}

#[test]
fn codec_round_trip() {
    run_with_census(
        512,
        book(),
        &[
            "byte order mark",
            "CRLF",
            "chapters out of order",
            "verses out of order",
            "heading",
            "chapter without verses",
            "span",
            "tab in a marker",
            "odd marker whitespace",
            "poetry",
            "layout",
        ],
        |text, census| {
            let parts = decompose(&text).unwrap();
            book_states(&text, &parts, census);
            let back = recompose(&parts);
            prop_assert!(back.issues.is_empty(), "{:?}", back.issues);
            prop_assert_eq!(back.usfm, text);
            Ok(())
        },
    );
}

fn split_states(words: &str, layout: &str, census: &Census) {
    census.saw(match (words.is_empty(), layout.is_empty()) {
        (false, false) => "words and layout",
        (false, true) => "words only",
        (true, false) => "layout only",
        (true, true) => "empty",
    });
}

#[test]
fn split_and_join_round_trip() {
    run_with_census(
        512,
        verse_content(),
        &["words and layout", "words only", "layout only", "empty"],
        |content, census| {
            let (words, layout) = split_verse(&content);
            split_states(words, layout, census);
            prop_assert_eq!(join_verse(words, layout), content.clone());
            prop_assert!(is_valid_layout(words, layout));
            // Words never end with a layout-only line, so splitting the words alone keeps them.
            if !words.is_empty() {
                prop_assert_eq!(split_verse(words), (words, ""));
            }
            Ok(())
        },
    );
}

#[test]
fn split_and_join_round_trip_for_any_string() {
    // Any characters, with the line breaks and paragraph markers a split looks for.
    let text = "(\\PC|[ \t\r\n\\*]|\r\n|\\\\[pqmb][12]?|\\\\s1){0,24}";
    run_with_census(
        512,
        text,
        &["words and layout", "words only", "layout only", "empty"],
        |content, census| {
            let (words, layout) = split_verse(&content);
            split_states(words, layout, census);
            prop_assert_eq!(join_verse(words, layout), content.clone());
            prop_assert!(is_valid_layout(words, layout));
            Ok(())
        },
    );
}

/// Any text either is refused or round-trips, and an accepted text's verse values obey
/// the rules a stored verse value must pass.
#[test]
fn accepted_soup_round_trips() {
    // Well-formed markers often enough that many texts are accepted with verses, and
    // malformed ones that must be refused.
    let well_formed = prop::sample::select(
        &[
            "\\c 1\n",
            "\\c 2 ",
            "\\c 3\r\n",
            "\\v 1 ",
            "\\v 2\n",
            "\\v 2-3 ",
            "\\v 4\t",
            "\\v 5\r",
        ][..],
    );
    let anything = prop::sample::select(
        &[
            "\\c 1",
            "\\c 2",
            "\\c 3",
            "\\v 1",
            "\\v 2",
            "\\v 2-3",
            "\\v 4",
            "\\v 01",
            "\\v",
            "\\c",
            " ",
            "\n",
            "\r\n",
            "\r",
            "\t",
            "\\p",
            "\\q1",
            "a",
            "b c",
            "\\s1 x",
            "\\vp 1\\vp*",
            "\\ts\\*",
            "\\w w\\w*",
            "\\",
            "-",
            "5",
        ][..],
    );
    let pieces = prop::collection::vec(prop_oneof![1 => well_formed, 3 => anything], 0..24);
    run_with_census(
        512,
        pieces,
        &["accepted", "refused", "accepted with verses"],
        |pieces, census| {
            let text: String = pieces.concat();
            let Ok(parts) = decompose(&text) else {
                census.saw("refused");
                return Ok(());
            };
            census.saw("accepted");
            let back = recompose(&parts);
            prop_assert!(back.issues.is_empty());
            prop_assert_eq!(&back.usfm, &text);
            for chapter in parts.chapters.values() {
                for part in chapter.verses.values() {
                    if let VersePart::Text(text) = part {
                        census.saw("accepted with verses");
                        prop_assert!(!has_verse_or_chapter_marker(&text.words));
                        prop_assert!(!has_verse_or_chapter_marker(&text.layout));
                        prop_assert!(is_valid_layout(&text.words, &text.layout));
                    }
                }
            }
            Ok(())
        },
    );
}
