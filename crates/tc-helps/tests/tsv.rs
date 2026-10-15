//! The Door43 TSV tables, against real published files.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

use common::{fixture, resource};
use proptest::prelude::*;
use tc_core::BookId;
use tc_helps::metadata::parse_burrito_metadata;
use tc_helps::tsv::{
    NotesLayout, TsvScope, WordLinkStyle, parse_notes, parse_questions, parse_word_links,
    write_notes, write_questions, write_word_links,
};
use tc_helps::{
    AcademyArticleId, BibleLocus, CheckBlocker, CheckReadiness, Error, HelpRef, Manual, NotACheck,
    Occurrence, TsvTable, WordArticleId, WordCategory,
};

const TIT: TsvScope = TsvScope::Book(BookId::Titus);

fn ready(r: &CheckReadiness) -> bool {
    *r == CheckReadiness::Ready
}

#[test]
fn en_tn_titus_has_157_checks_in_28_groups_all_in_the_academy_toc() {
    let parsed = parse_notes(&resource("en_tn@v86/TIT.tsv"), TIT).unwrap();
    assert_eq!(parsed.layout, NotesLayout::Seven);
    let notes = parsed.notes;
    assert_eq!(notes.len(), 206);
    let checks: Vec<_> = notes
        .iter()
        .filter(|n| ready(&n.check_readiness()))
        .collect();
    assert_eq!(checks.len(), 157);
    let plain = notes
        .iter()
        .filter(|n| n.check_readiness() == CheckReadiness::NotACheck(NotACheck::PlainNote))
        .count();
    assert_eq!(plain, 45);
    let intros = notes
        .iter()
        .filter(|n| n.reference.is_introduction())
        .count();
    assert_eq!(intros, 4);
    assert_eq!(
        notes.iter().filter(|n| n.quote.spans().len() > 1).count(),
        18
    );

    let groups: BTreeSet<String> = checks
        .iter()
        .map(|n| match n.support_reference.as_ref().unwrap() {
            AcademyArticleId::Dcs { manual, slug } => {
                assert_eq!(*manual, Manual::Translate);
                slug.as_str().to_owned()
            }
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(groups.len(), 28);
    let toc = tc_helps::articles::parse_toc(&resource("en_ta@v86/translate-toc.yaml")).unwrap();
    let links: BTreeSet<String> = toc.links().iter().map(|s| s.as_str().to_owned()).collect();
    assert!(
        groups.is_subset(&links),
        "{:?}",
        groups.difference(&links).collect::<Vec<_>>()
    );
}

#[test]
fn notes_keep_quotes_newlines_and_references_as_published() {
    let notes = parse_notes(&resource("en_tn@v86/TIT.tsv"), TIT)
        .unwrap()
        .notes;
    let intro = &notes[0];
    assert_eq!(intro.reference.tsv_text(), "front:intro");
    assert!(
        intro
            .text
            .starts_with("# Introduction to Titus\n\n## Part 1: General Introduction\n")
    );
    assert_eq!(intro.occurrence, Occurrence::NotTied);
    let rtc9 = notes
        .iter()
        .find(|n| n.id.upstream_row_id().unwrap().as_str() == "rtc9")
        .unwrap();
    assert_eq!(
        rtc9.quote.as_str(),
        "κατὰ πίστιν ἐκλεκτῶν Θεοῦ καὶ ἐπίγνωσιν ἀληθείας"
    );
    assert_eq!(rtc9.occurrence, Occurrence::nth(1).unwrap());
    let HelpRef::Bible(r) = &rtc9.reference else {
        panic!()
    };
    assert_eq!(r.to_string(), "TIT 1:1");
    let split = notes
        .iter()
        .find(|n| n.quote.as_str().contains(" & "))
        .unwrap();
    assert_eq!(split.quote.spans(), vec!["ἐφανέρωσεν", "τὸν λόγον αὐτοῦ"]);
}

#[test]
fn hebrew_notes_keep_word_joiners_and_escaped_marks() {
    let notes = parse_notes(
        &resource("en_tn@v86/JON.tsv"),
        TsvScope::Book(BookId::Jonah),
    )
    .unwrap()
    .notes;
    assert_eq!(notes.len(), 196);
    let jdr1 = notes
        .iter()
        .find(|n| n.id.upstream_row_id().unwrap().as_str() == "jdr1")
        .unwrap();
    assert!(jdr1.quote.as_str().contains('\u{2060}'));
    // The file writes `\\*\\*And\\*\\*`: escaped backslashes before markdown stars.
    assert!(jdr1.text.contains(r"\*\*And\*\*"), "{}", jdr1.text);
}

#[test]
fn spanish_notes_count_112_checks_and_link_with_their_language() {
    let notes = parse_notes(&resource("es-419_tn@v66/TIT.tsv"), TIT)
        .unwrap()
        .notes;
    assert_eq!(notes.len(), 216);
    assert_eq!(
        notes.iter().filter(|n| ready(&n.check_readiness())).count(),
        112
    );
    let linked: Vec<_> = notes
        .iter()
        .flat_map(|n| tc_helps::links::find_links(&n.text))
        .filter(|l| l.href.starts_with("rc://es-419/"))
        .collect();
    assert!(!linked.is_empty());
    for l in linked {
        let rc = tc_helps::links::parse_rc_link(&l.href).unwrap();
        assert_eq!(rc.language.as_deref(), Some("es-419"));
    }
}

#[test]
fn word_links_take_their_category_from_the_link() {
    let links = parse_word_links(&resource("en_twl@v86/TIT.tsv"), TIT).unwrap();
    assert_eq!(links.len(), 188);
    let count = |c: WordCategory| {
        links
            .iter()
            .filter(|l| matches!(&l.article, WordArticleId::Dcs { category, .. } if *category == c))
            .count()
    };
    assert_eq!(
        (
            count(WordCategory::KeyTerm),
            count(WordCategory::Other),
            count(WordCategory::Name)
        ),
        (111, 71, 6)
    );
    let second: Vec<_> = links
        .iter()
        .filter(|l| l.occurrence == Occurrence::nth(2).unwrap())
        .collect();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].orig_words.as_str(), "Θεοῦ");
    assert_eq!(second[0].tags, vec!["keyterm".to_owned()]);
    let both = links
        .iter()
        .filter(|l| l.tags == ["keyterm", "name"])
        .count();
    assert_eq!(both, 4);
}

#[test]
fn a_link_to_an_article_the_words_pin_lacks_is_not_a_check() {
    let links = parse_word_links(
        &resource("en_tw@v91/2SA.tsv"),
        TsvScope::Book(BookId::SecondSamuel),
    )
    .unwrap();
    assert_eq!(links.len(), 3316);
    let carried = parse_burrito_metadata(&resource("en_tw@v91/metadata.json"))
        .unwrap()
        .word_articles();
    assert_eq!(carried.len(), 953);
    let blocked: Vec<_> = links
        .iter()
        .filter(|l| !ready(&l.check_readiness(carried.contains(&l.article))))
        .collect();
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].id.upstream_row_id().unwrap().as_str(), "vwrw");
    assert_eq!(
        blocked[0].check_readiness(false),
        CheckReadiness::Blocked(vec![CheckBlocker::ArticleMissing])
    );
}

#[test]
fn obs_notes_are_all_checks_and_title_notes_are_frame_zero() {
    let notes = parse_notes(&resource("en_obs-tn@v13/OBS.tsv"), TsvScope::Obs)
        .unwrap()
        .notes;
    assert_eq!(notes.len(), 2325);
    let titles: Vec<_> = notes.iter().filter(|n| n.tags == ["title"]).collect();
    assert_eq!(titles.len(), 58);
    assert!(
        titles
            .iter()
            .all(|n| matches!(n.reference, HelpRef::Obs(r) if r.is_title()))
    );
    assert!(
        notes
            .iter()
            .all(|n| n.occurrence == Occurrence::nth(1).unwrap())
    );
    assert_eq!(
        notes.iter().filter(|n| ready(&n.check_readiness())).count(),
        2325
    );
}

#[test]
fn obs_word_links_use_rc_links_and_obs_questions_have_no_quotes() {
    let links = parse_word_links(&resource("en_obs-twl@v3/OBS.tsv"), TsvScope::Obs).unwrap();
    assert_eq!(links.len(), 2381);
    let story1 = links
        .iter()
        .filter(|l| matches!(l.reference, HelpRef::Obs(r) if r.story() == 1))
        .count();
    assert_eq!(story1, 35);
    assert_eq!(links[0].orig_words.as_str(), "God");
    let questions = parse_questions(&resource("en_obs-tq@v10/OBS.tsv"), TsvScope::Obs).unwrap();
    assert_eq!(questions.len(), 672);
    assert!(
        questions
            .iter()
            .all(|q| q.quote.is_empty() && q.occurrence == Occurrence::Unrecorded)
    );
    assert_eq!(
        questions[0].question,
        "Where did everything in the universe come from?"
    );
    assert_eq!(questions[0].response, "God created everything.");
}

#[test]
fn every_published_table_writes_back_byte_for_byte() {
    for (file, scope) in [
        ("en_tn@v86/TIT.tsv", TIT),
        ("en_tn@v86/JON.tsv", TsvScope::Book(BookId::Jonah)),
        ("en_tn@v91/TIT.tsv", TIT),
        ("es-419_tn@v66/TIT.tsv", TIT),
        ("en_obs-tn@v13/OBS.tsv", TsvScope::Obs),
    ] {
        let text = resource(file);
        let notes = parse_notes(&text, scope).unwrap().notes;
        assert_eq!(
            write_notes(&notes, NotesLayout::Seven).unwrap(),
            text,
            "{file}"
        );
    }
    for (file, scope, style) in [
        ("en_twl@v86/TIT.tsv", TIT, WordLinkStyle::ExportPath),
        ("en_tw@v87/TIT.tsv", TIT, WordLinkStyle::ExportPath),
        (
            "en_tw@v87/JON.tsv",
            TsvScope::Book(BookId::Jonah),
            WordLinkStyle::ExportPath,
        ),
        ("en_tw@v91/TIT.tsv", TIT, WordLinkStyle::ExportPath),
        (
            "en_tw@v91/2SA.tsv",
            TsvScope::Book(BookId::SecondSamuel),
            WordLinkStyle::ExportPath,
        ),
        ("en_obs-twl@v3/OBS.tsv", TsvScope::Obs, WordLinkStyle::Rc),
    ] {
        let text = resource(file);
        let links = parse_word_links(&text, scope).unwrap();
        assert_eq!(write_word_links(&links, style).unwrap(), text, "{file}");
    }
    let text = resource("en_obs-tq@v10/OBS.tsv");
    let questions = parse_questions(&text, TsvScope::Obs).unwrap();
    assert_eq!(write_questions(&questions).unwrap(), text);
}

#[test]
fn the_legacy_nine_column_table_reads_into_the_same_notes() {
    let legacy = parse_notes(&fixture("helps/tn-tsv9/en_tn_57-TIT.tsv"), TIT).unwrap();
    assert_eq!(legacy.layout, NotesLayout::LegacyNine);
    assert_eq!(legacy.notes.len(), 12);
    let current = parse_notes(&resource("en_tn@v86/TIT.tsv"), TIT)
        .unwrap()
        .notes;
    for (old, new) in legacy.notes.iter().zip(&current) {
        assert_eq!(old.reference, new.reference);
        assert_eq!(old.id, new.id);
        // A bare slug in the old table names the same translate-manual article.
        assert_eq!(old.support_reference, new.support_reference);
        assert_eq!(old.quote, new.quote);
        assert_eq!(old.occurrence, new.occurrence);
        assert_eq!(old.text, new.text);
    }
    let rtc9 = &legacy.notes[2];
    assert_eq!(
        rtc9.gl_quote.as_deref(),
        Some("for the faith of the chosen people of God and knowledge of the truth")
    );
    assert_eq!(legacy.notes[4].gl_quote, None);
    // The current table has no GLQuote column; writing it there would lose the value.
    assert!(write_notes(&legacy.notes, NotesLayout::Seven).is_err());
    let again = write_notes(&legacy.notes, NotesLayout::LegacyNine).unwrap();
    assert_eq!(parse_notes(&again, TIT).unwrap(), legacy);
}

/// Replaces the cell at `column` of the data row on `line` (counting the header as 1).
fn with_cell(text: &str, line: usize, column: usize, value: &str) -> String {
    text.split('\n')
        .enumerate()
        .map(|(i, l)| {
            if i + 1 == line {
                let mut cells: Vec<&str> = l.split('\t').collect();
                cells[column] = value;
                cells.join("\t")
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn cell_error(result: Result<impl std::fmt::Debug, Error>) -> (usize, &'static str) {
    match result {
        Err(Error::TsvCell { line, column, .. }) => (line, column),
        other => panic!("expected a cell error, got {other:?}"),
    }
}

#[test]
fn malformed_rows_are_refused_naming_line_and_column() {
    let tn = resource("en_tn@v86/TIT.tsv");
    let cases = [
        (0, "1:x", "Reference"),
        (0, "0:1", "Reference"),
        (1, "r t9", "ID"),
        (3, "rc://*/ta/man/nomanual/figs-x", "SupportReference"),
        (3, "rc://*/tw/dict/bible/kt/god", "SupportReference"),
        (5, "one", "Occurrence"),
        (5, "-2", "Occurrence"),
    ];
    for (column, value, name) in cases {
        let broken = with_cell(&tn, 40, column, value);
        assert_eq!(
            cell_error(parse_notes(&broken, TIT)),
            (40, name),
            "{value:?}"
        );
    }

    let short = with_cell(&tn, 7, 6, "x\tonly-in-a-bad-file");
    assert!(matches!(
        parse_notes(&short, TIT),
        Err(Error::TsvColumnCount {
            line: 7,
            found: 8,
            expected: 7,
            ..
        })
    ));
    let header = tn.replacen("SupportReference", "Support", 1);
    assert!(matches!(
        parse_notes(&header, TIT),
        Err(Error::TsvHeader {
            table: TsvTable::Notes,
            ..
        })
    ));
    // An OBS reference is not a Bible reference.
    assert_eq!(
        cell_error(parse_notes(&resource("en_obs-tn@v13/OBS.tsv"), TIT)),
        (2, "Reference")
    );

    let twl = resource("en_twl@v86/TIT.tsv");
    let broken = with_cell(&twl, 3, 5, "./payload/animals/dog.md");
    assert_eq!(cell_error(parse_word_links(&broken, TIT)), (3, "TWLink"));
    let obs = resource("en_obs-twl@v3/OBS.tsv");
    let broken = with_cell(&obs, 9, 0, "51:1");
    assert_eq!(
        cell_error(parse_word_links(&broken, TsvScope::Obs)),
        (9, "Reference")
    );

    let legacy = fixture("helps/tn-tsv9/en_tn_57-TIT.tsv");
    assert_eq!(
        cell_error(parse_notes(&with_cell(&legacy, 5, 0, "JON"), TIT)),
        (5, "Book")
    );
    assert_eq!(
        cell_error(parse_notes(&with_cell(&legacy, 5, 1, "one"), TIT)),
        (5, "Chapter")
    );
    assert_eq!(
        cell_error(parse_notes(&with_cell(&legacy, 5, 2, "x"), TIT)),
        (5, "Verse")
    );
}

fn locus() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("front:intro".to_owned()),
        (1u16..200).prop_map(|c| format!("{c}:intro")),
        (1u16..200, 1u16..200).prop_map(|(c, v)| format!("{c}:{v}")),
        (1u16..200, 1u16..100, 0u16..20).prop_map(|(c, v, n)| format!("{c}:{v}-{}", v + n + 1)),
        (1u16..100, 1u16..100, 1u16..50, 1u16..100)
            .prop_map(|(c, v, dc, w)| format!("{c}:{v}-{}:{w}", c + dc)),
        (1u16..100, 1u16..50, 1u16..50).prop_map(|(c, v, n)| format!("{c}:{v},{}", v + n)),
    ]
}

fn cell_text() -> impl Strategy<Value = String> {
    "[^\t\n\r]{0,30}"
}

fn row(columns: Vec<String>) -> String {
    columns.join("\t")
}

proptest! {
    #[test]
    fn notes_tables_round_trip(rows in prop::collection::vec(
        (locus(), "[a-z0-9]{4}", prop::option::of("[a-z-]{1,20}"), cell_text(),
         prop_oneof![Just(String::new()), Just("0".to_owned()), Just("-1".to_owned()), (1u32..9).prop_map(|n| n.to_string())],
         "[^\t\r]{0,60}"),
        0..20,
    )) {
        let mut text = "Reference\tID\tTags\tSupportReference\tQuote\tOccurrence\tNote\n".to_owned();
        for (locus, id, slug, quote, occ, note) in &rows {
            let support = slug.as_ref().map(|s| format!("rc://*/ta/man/translate/{s}")).unwrap_or_default();
            let note = note.replace('\\', "\\\\").replace('\n', "\\n");
            text.push_str(&row(vec![locus.clone(), id.clone(), String::new(), support, quote.clone(), occ.clone(), note]));
            text.push('\n');
        }
        let notes = parse_notes(&text, TIT).unwrap().notes;
        prop_assert_eq!(notes.len(), rows.len());
        let written = write_notes(&notes, NotesLayout::Seven).unwrap();
        prop_assert_eq!(&written, &text);
        prop_assert_eq!(parse_notes(&written, TIT).unwrap().notes, notes);
    }

    #[test]
    fn word_link_tables_round_trip(rows in prop::collection::vec(
        (locus(), "[a-z0-9]{4}", prop_oneof![Just("kt"), Just("names"), Just("other")], "[a-z0-9]{1,15}",
         cell_text(), 1u32..5, prop_oneof![Just(""), Just("keyterm"), Just("name"), Just("keyterm; name")]),
        0..20,
    ), rc in any::<bool>()) {
        let mut text = "Reference\tID\tTags\tOrigWords\tOccurrence\tTWLink\n".to_owned();
        for (locus, id, cat, slug, words, occ, tags) in &rows {
            let link = if rc { format!("rc://*/tw/dict/bible/{cat}/{slug}") } else { format!("./payload/{cat}/{slug}.md") };
            text.push_str(&row(vec![locus.clone(), id.clone(), (*tags).to_owned(), words.clone(), occ.to_string(), link]));
            text.push('\n');
        }
        let links = parse_word_links(&text, TIT).unwrap();
        let style = if rc { WordLinkStyle::Rc } else { WordLinkStyle::ExportPath };
        prop_assert_eq!(write_word_links(&links, style).unwrap(), text);
    }

    #[test]
    fn question_tables_round_trip(rows in prop::collection::vec(
        ((1u8..=50, 0u16..30), "[a-z0-9]{4}", "[^\t\r]{0,40}", "[^\t\r]{0,40}"),
        0..20,
    )) {
        let mut text = "Reference\tID\tTags\tQuote\tOccurrence\tQuestion\tResponse\n".to_owned();
        for ((story, frame), id, q, a) in &rows {
            let esc = |s: &str| s.replace('\\', "\\\\").replace('\n', "\\n");
            text.push_str(&row(vec![format!("{story}:{frame}"), id.clone(), String::new(), String::new(), String::new(), esc(q), esc(a)]));
            text.push('\n');
        }
        let questions = parse_questions(&text, TsvScope::Obs).unwrap();
        prop_assert_eq!(&questions.iter().map(|q| q.question.clone()).collect::<Vec<_>>(),
                        &rows.iter().map(|r| r.2.clone()).collect::<Vec<_>>());
        prop_assert_eq!(write_questions(&questions).unwrap(), text);
    }

    #[test]
    fn parsing_never_panics(text in "[\\PC\t\n]{0,300}") {
        let _ = parse_notes(&text, TIT);
        let _ = parse_notes(&format!("Book\tChapter\tVerse\tID\tSupportReference\tOrigQuote\tOccurrence\tGLQuote\tOccurrenceNote\n{text}"), TIT);
        let _ = parse_notes(&format!("Reference\tID\tTags\tSupportReference\tQuote\tOccurrence\tNote\n{text}"), TsvScope::Obs);
        let _ = parse_word_links(&format!("Reference\tID\tTags\tOrigWords\tOccurrence\tTWLink\n{text}"), TIT);
        let _ = parse_questions(&format!("Reference\tID\tTags\tQuote\tOccurrence\tQuestion\tResponse\n{text}"), TIT);
    }
}

#[test]
fn intro_rows_are_introductions_not_checks() {
    let notes = parse_notes(&resource("en_tn@v91/TIT.tsv"), TIT)
        .unwrap()
        .notes;
    let chapter_intros: Vec<u16> = notes
        .iter()
        .filter_map(|n| match &n.reference {
            HelpRef::Bible(r) => match r.locus {
                BibleLocus::ChapterIntro(c) => Some(c),
                _ => None,
            },
            HelpRef::Obs(_) => None,
        })
        .collect();
    assert_eq!(chapter_intros, vec![1, 2, 3]);
    assert!(
        notes
            .iter()
            .filter(|n| n.reference.is_introduction())
            .all(|n| n.check_readiness() == CheckReadiness::NotACheck(NotACheck::Introduction))
    );
}
