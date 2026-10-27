//! The Door43 TSV tables, against real published files.

// Helpers outside #[test] functions unwrap too; clippy's test exemption covers only
// #[test] bodies.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

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
    AcademyArticleId, BibleLocus, CheckBlocker, CheckReadiness, Error, HelpItemId, HelpRef, Manual,
    NotACheck, Occurrence, RowId, TsvTable, WordArticleId, WordCategory, WriteProblem,
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
            write_notes(&notes, scope, NotesLayout::Seven).unwrap(),
            text,
            "{file}"
        );
    }
    let text = fixture("helps/tn-tsv9/en_tn_57-TIT.tsv");
    let legacy = parse_notes(&text, TIT).unwrap().notes;
    assert_eq!(
        write_notes(&legacy, TIT, NotesLayout::LegacyNine).unwrap(),
        text
    );
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
        assert_eq!(
            write_word_links(&links, scope, style).unwrap(),
            text,
            "{file}"
        );
    }
    let text = resource("en_obs-tq@v10/OBS.tsv");
    let questions = parse_questions(&text, TsvScope::Obs).unwrap();
    assert_eq!(write_questions(&questions, TsvScope::Obs).unwrap(), text);
}

fn write_problem(result: Result<String, Error>) -> WriteProblem {
    match result {
        Err(Error::TsvWrite { problem, .. }) => problem,
        other => panic!("expected a write error, got {other:?}"),
    }
}

#[test]
fn a_table_refuses_items_of_another_book_or_of_the_stories() {
    let jon = TsvScope::Book(BookId::Jonah);
    let tit_notes = parse_notes(&resource("en_tn@v86/TIT.tsv"), TIT)
        .unwrap()
        .notes;
    let jon_notes = parse_notes(&resource("en_tn@v86/JON.tsv"), jon)
        .unwrap()
        .notes;
    let obs_notes = parse_notes(&resource("en_obs-tn@v13/OBS.tsv"), TsvScope::Obs)
        .unwrap()
        .notes;
    let mixed: Vec<_> = tit_notes[..3]
        .iter()
        .chain(&jon_notes[..3])
        .cloned()
        .collect();
    for layout in [NotesLayout::Seven, NotesLayout::LegacyNine] {
        assert_eq!(
            write_problem(write_notes(&mixed, TIT, layout)),
            WriteProblem::WrongBook {
                expected: "TIT".into(),
                found: "JON".into()
            }
        );
    }
    let stories: Vec<_> = tit_notes[..3]
        .iter()
        .chain(&obs_notes[..3])
        .cloned()
        .collect();
    assert_eq!(
        write_problem(write_notes(&stories, TIT, NotesLayout::Seven)),
        WriteProblem::Reference("OBS 1:0".into())
    );
    assert_eq!(
        write_problem(write_notes(&stories, TsvScope::Obs, NotesLayout::Seven)),
        WriteProblem::Reference("TIT front:intro".into())
    );
    assert_eq!(
        write_problem(write_notes(
            &obs_notes,
            TsvScope::Obs,
            NotesLayout::LegacyNine
        )),
        WriteProblem::LegacyObs
    );

    let tit_links = parse_word_links(&resource("en_twl@v86/TIT.tsv"), TIT).unwrap();
    let jon_links = parse_word_links(&resource("en_tw@v87/JON.tsv"), jon).unwrap();
    let mixed: Vec<_> = tit_links[..2]
        .iter()
        .chain(&jon_links[..2])
        .cloned()
        .collect();
    assert!(matches!(
        write_problem(write_word_links(&mixed, TIT, WordLinkStyle::ExportPath)),
        WriteProblem::WrongBook { .. }
    ));
    let obs_links = parse_word_links(&resource("en_obs-twl@v3/OBS.tsv"), TsvScope::Obs).unwrap();
    assert!(matches!(
        write_problem(write_word_links(&obs_links, TIT, WordLinkStyle::Rc)),
        WriteProblem::Reference(_)
    ));
    let questions = parse_questions(&resource("en_obs-tq@v10/OBS.tsv"), TsvScope::Obs).unwrap();
    assert!(matches!(
        write_problem(write_questions(&questions, TIT)),
        WriteProblem::Reference(_)
    ));
}

#[test]
fn cells_keep_the_form_they_were_read_in() {
    let text = "Reference\tID\tTags\tSupportReference\tQuote\tOccurrence\tNote\n\
        1:1-1:3\tab12\ta;b\trc://es-419/ta/man/translate/figs-metaphor\tx\r\t01\tSee \\v 1.\n\
        1:4\tcd34\t\tfigs-idiom\tΘεοῦ\t1\tOne\\nTwo\n\
        1:5\tef56\t\trc://*/ta/man/translate/figs-you/01.md\t\t0\t\n";
    let notes = parse_notes(text, TIT).unwrap().notes;
    assert_eq!(notes[0].reference.tsv_text(), "1:1-3");
    assert_eq!(notes[0].tags, ["a", "b"]);
    // tC4 drops a CR from the end of every cell (`tsvRows`); the table still writes it back.
    assert_eq!(notes[0].quote.as_str(), "x");
    assert_eq!(notes[0].occurrence, Occurrence::nth(1).unwrap());
    assert_eq!(notes[0].text, "See \\v 1.");
    assert_eq!(
        notes[0].read_from.as_ref().unwrap().get("SupportReference"),
        Some("rc://es-419/ta/man/translate/figs-metaphor")
    );
    // Read and written unchanged, the table keeps its bytes.
    assert_eq!(write_notes(&notes, TIT, NotesLayout::Seven).unwrap(), text);

    // An edit rewrites only the cells it changes.
    let mut edited = notes.clone();
    edited[0].occurrence = Occurrence::nth(2).unwrap();
    edited[1].support_reference = Some(AcademyArticleId::Dcs {
        manual: Manual::Translate,
        slug: tc_helps::Slug::parse("figs-explicit").unwrap(),
    });
    let written = write_notes(&edited, TIT, NotesLayout::Seven).unwrap();
    let rows: Vec<&str> = written.lines().collect();
    assert_eq!(
        rows[1],
        "1:1-1:3\tab12\ta;b\trc://es-419/ta/man/translate/figs-metaphor\tx\r\t2\tSee \\v 1."
    );
    assert_eq!(
        rows[2],
        "1:4\tcd34\t\trc://*/ta/man/translate/figs-explicit\tΘεοῦ\t1\tOne\\nTwo"
    );
    assert_eq!(
        parse_notes(&written, TIT).unwrap().notes[0].occurrence,
        edited[0].occurrence
    );

    let links = "Reference\tID\tTags\tOrigWords\tOccurrence\tTWLink\n\
        1:1\tgh78\tkeyterm;name\tΘεοῦ\t1\trc://en/tw/dict/bible/kt/god.md\n";
    let parsed = parse_word_links(links, TIT).unwrap();
    assert_eq!(
        write_word_links(&parsed, TIT, WordLinkStyle::Rc).unwrap(),
        links
    );
    // Asked for the other style, the link is written in that style.
    assert!(
        write_word_links(&parsed, TIT, WordLinkStyle::ExportPath)
            .unwrap()
            .ends_with("\t./payload/kt/god.md\n")
    );
}

#[test]
fn a_byte_order_mark_and_crlf_line_ends_read_as_the_table() {
    let text = resource("en_tn@v86/TIT.tsv");
    let windows = format!("\u{feff}{}", text.replace('\n', "\r\n"));
    assert_eq!(
        parse_notes(&windows, TIT).unwrap(),
        parse_notes(&text, TIT).unwrap()
    );
    // Written back, the table has neither.
    let notes = parse_notes(&windows, TIT).unwrap().notes;
    assert_eq!(write_notes(&notes, TIT, NotesLayout::Seven).unwrap(), text);
}

#[test]
fn tags_that_would_read_back_as_other_tags_are_refused() {
    let mut notes = parse_notes(&resource("en_tn@v86/TIT.tsv"), TIT)
        .unwrap()
        .notes;
    for bad in ["a;b", " a", ""] {
        notes[5].tags = vec![bad.to_owned()];
        assert_eq!(
            write_problem(write_notes(&notes, TIT, NotesLayout::Seven)),
            WriteProblem::Tag(bad.to_owned())
        );
    }
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
    assert_eq!(
        write_problem(write_notes(&legacy.notes, TIT, NotesLayout::Seven)),
        WriteProblem::NoColumn { column: "GLQuote" }
    );
    let again = write_notes(&legacy.notes, TIT, NotesLayout::LegacyNine).unwrap();
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
    // A Bible Reference never refuses the table: `reference_forms.rs` reads `1:x` and
    // `0:1` as written.
    let cases = [
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
    // A Bible reference is not an OBS reference.
    assert_eq!(
        cell_error(parse_notes(&resource("en_tn@v86/TIT.tsv"), TsvScope::Obs)),
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
}

fn locus() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("front:intro".to_owned()),
        (1u16..200).prop_map(|c| format!("{c}:intro")),
        (1u16..200).prop_map(|c| format!("{c}:front")),
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

/// A SupportReference cell in each form the tables use.
fn support() -> impl Strategy<Value = String> {
    let slug = "[a-z][a-z-]{0,15}";
    prop_oneof![
        Just(String::new()),
        slug.prop_map(|s| format!("rc://*/ta/man/translate/{s}")),
        (prop_oneof![Just("en"), Just("es-419"), Just("*")], slug)
            .prop_map(|(l, s)| format!("rc://{l}/ta/man/checking/{s}")),
        slug.prop_map(|s| format!("rc://*/ta/man/translate/{s}/01.md")),
        slug.prop_map(|s: String| s),
    ]
}

fn occurrence() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("0".to_owned()),
        Just("-1".to_owned()),
        (1u32..9).prop_map(|n| n.to_string()),
        (1u32..9).prop_map(|n| format!("0{n}")),
    ]
}

fn tag_cell() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("keyterm".to_owned()),
        Just("keyterm; name".to_owned()),
        Just("keyterm;name".to_owned()),
        Just(" name ".to_owned()),
    ]
}

/// A Reference cell, sometimes in a longer form than the writer's (`1:1-1:3`, `01:02`,
/// `03:front`), sometimes in a form kept as written (`1:1a`, `0:front`, any cell text).
fn reference_cell() -> impl Strategy<Value = String> {
    prop_oneof![
        6 => locus(),
        2 => (1u16..50, 1u16..50, 1u16..9).prop_map(|(c, v, n)| format!("{c}:{v}-{c}:{}", v + n)),
        2 => (1u16..9, 1u16..9).prop_map(|(c, v)| format!("0{c}:0{v}")),
        2 => (1u16..9).prop_map(|c| format!("0{c}:front")),
        1 => (0u16..200, "[a-z]{1,2}").prop_map(|(c, v)| format!("{c}:1{v}")),
        1 => (0u16..9, prop_oneof![Just("front"), Just("Front"), Just("front "), Just("intro")])
            .prop_map(|(c, v)| format!("{c}:{v}")),
        1 => "[^\t\n]{0,12}",
    ]
}

/// The rows of a table as tC4's `tsvRows` (`derive.ts`) reads them: lines split on LF,
/// those whose `trim()` is empty dropped, the header dropped, cells split on tabs, and one CR
/// dropped from the end of every cell.
fn tc4_rows(text: &str) -> Vec<Vec<&str>> {
    text.split('\n')
        .filter(|line| !line.trim().is_empty())
        .skip(1)
        .map(|line| {
            line.split('\t')
                .map(|cell| cell.strip_suffix('\r').unwrap_or(cell))
                .collect()
        })
        .collect()
}

/// Up to `max` CRs, to end a cell with.
fn crs(max: usize) -> impl Strategy<Value = String> {
    (0..=max).prop_map(|n| "\r".repeat(n))
}

proptest! {
    /// A cell that ends in a CR, a CR before a tab, reads as tC4 reads it, without that CR,
    /// and the table writes back with it.
    #[test]
    fn cells_ending_in_a_cr_read_as_tc4_reads_them(rows in prop::collection::vec(
        ((reference_cell(), crs(2)), ("[a-z0-9]{4}", crs(1)), (tag_cell(), crs(2)),
         (cell_text(), crs(2)), (occurrence(), crs(1)), "[a-z0-9]{1,15}"),
        1..12,
    ), crlf in any::<bool>()) {
        let mut text = "Reference\tID\tTags\tOrigWords\tOccurrence\tTWLink\n".to_owned();
        for ((reference, r), (id, i), (tags, t), (words, w), (occ, o), slug) in &rows {
            text.push_str(&row(vec![
                format!("{reference}{r}"), format!("{id}{i}"), format!("{tags}{t}"),
                format!("{words}{w}"), format!("{occ}{o}"),
                format!("rc://*/tw/dict/bible/kt/{slug}"),
            ]));
            text.push('\n');
        }
        let links = parse_word_links(&text, TIT).unwrap();
        let tc4 = tc4_rows(&text);
        prop_assert_eq!(links.len(), tc4.len());
        for (link, cells) in links.iter().zip(&tc4) {
            let reference = link.read_from.as_ref().and_then(|r| r.get("Reference"))
                .map_or_else(|| link.reference.tsv_text(), str::to_owned);
            prop_assert_eq!(&reference, cells[0]);
            prop_assert_eq!(&link.id, &HelpItemId::Row(RowId::parse(cells[1]).unwrap()));
            prop_assert_eq!(link.orig_words.as_str(), cells[3]);
        }
        prop_assert_eq!(&write_word_links(&links, TIT, WordLinkStyle::Rc).unwrap(), &text);
        if crlf {
            let windows = text.replace('\n', "\r\n");
            prop_assert_eq!(parse_word_links(&windows, TIT).unwrap(), links);
        }
    }

    #[test]
    fn notes_tables_round_trip(rows in prop::collection::vec(
        (reference_cell(), "[a-z0-9]{4}", tag_cell(), support(), cell_text(), occurrence(),
         "[^\t\r]{0,60}", any::<bool>()),
        0..20,
    )) {
        let mut text = "Reference\tID\tTags\tSupportReference\tQuote\tOccurrence\tNote\n".to_owned();
        for (reference, id, tags, support, quote, occ, note, escape_all) in &rows {
            // Published tables double every backslash; a hand-made one may leave some single.
            let note = if *escape_all { note.replace('\\', "\\\\") } else { note.clone() };
            let note = note.replace('\n', "\\n");
            text.push_str(&row(vec![reference.clone(), id.clone(), tags.clone(), support.clone(), quote.clone(), occ.clone(), note]));
            text.push('\n');
        }
        let notes = parse_notes(&text, TIT).unwrap().notes;
        prop_assert_eq!(notes.len(), rows.len());
        let written = write_notes(&notes, TIT, NotesLayout::Seven).unwrap();
        prop_assert_eq!(&written, &text);
        prop_assert_eq!(parse_notes(&written, TIT).unwrap().notes, notes.clone());
        // Every value survives a rewrite in the writer's own form too.
        let mut fresh = notes.clone();
        for n in &mut fresh {
            n.read_from = None;
        }
        let canonical = write_notes(&fresh, TIT, NotesLayout::Seven).unwrap();
        let again = parse_notes(&canonical, TIT).unwrap().notes;
        for (a, b) in again.iter().zip(&notes) {
            prop_assert_eq!(&a.reference, &b.reference);
            prop_assert_eq!(&a.tags, &b.tags);
            prop_assert_eq!(&a.support_reference, &b.support_reference);
            prop_assert_eq!(&a.quote, &b.quote);
            prop_assert_eq!(a.occurrence, b.occurrence);
            prop_assert_eq!(&a.text, &b.text);
        }
    }

    #[test]
    fn word_link_tables_round_trip(rows in prop::collection::vec(
        (reference_cell(), "[a-z0-9]{4}", prop_oneof![Just("kt"), Just("names"), Just("other")], "[a-z0-9]{1,15}",
         cell_text(), occurrence(), tag_cell(), prop_oneof![Just("*"), Just("en")], any::<bool>()),
        0..20,
    ), rc in any::<bool>()) {
        let mut text = "Reference\tID\tTags\tOrigWords\tOccurrence\tTWLink\n".to_owned();
        for (reference, id, cat, slug, words, occ, tags, lang, md) in &rows {
            let suffix = if *md { ".md" } else { "" };
            let link = if rc { format!("rc://{lang}/tw/dict/bible/{cat}/{slug}{suffix}") } else { format!("./payload/{cat}/{slug}.md") };
            text.push_str(&row(vec![reference.clone(), id.clone(), tags.clone(), words.clone(), occ.clone(), link]));
            text.push('\n');
        }
        let links = parse_word_links(&text, TIT).unwrap();
        let style = if rc { WordLinkStyle::Rc } else { WordLinkStyle::ExportPath };
        prop_assert_eq!(write_word_links(&links, TIT, style).unwrap(), text);
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
        prop_assert_eq!(write_questions(&questions, TsvScope::Obs).unwrap(), text);
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
