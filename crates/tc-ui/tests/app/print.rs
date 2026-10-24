//! Community Checking's pure parts (unit B8): each preview page's flow is the markup tC4's
//! `PrintParts` renders, byte for byte, on every preview page of
//! `fixtures/export/print-vectors.json`; the sheets have tC4's measures; the export menu lists
//! the producers tC4's table gives the open project; a press asks first while verses are in
//! conflict, and a finished export names at most 20 of the verses it left out.

use serde_json::Value;
use tc_api::export::{
    ExportFacts, Exported, ExportedFile, PrintWords, Producer, SkipReason, SkippedVerse, Subject,
};
use tc_api::export::{PrintVerse, Run};
use tc_api::home::ProjectKind;
use tc_api::ids::RefPart;
use tc_api::ids::{DownloadId, Epoch, ProjectRef, RequestId, Scope};
use tc_api::model::print::{
    ChapterOpening, ChapterPart, PageSetup, Paper, Piece, PrintPart, VerseNumbers, print_parts,
};
use tc_api::shown::Direction;
use tc_core::BookId;
use tc_ui::app::print::{
    ExportOf, Sheet, conflict_question, export_items, export_notice, flow_markup, flow_style,
    page_number_style, print_words, producer_of, px, sheet_style,
};
use tc_ui::i18n::{t, t_dyn};

fn vectors() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/export/print-vectors.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A book case's verses by chapter, as the server sends them.
fn verses(case: &Value) -> Vec<(u16, Vec<PrintVerse>)> {
    case["byChapter"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let chapter = entry["key"].as_str().unwrap().parse::<u16>().unwrap_or(0);
            let verses = entry["verses"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    let text = v["text"].as_str().unwrap();
                    PrintVerse {
                        label: RefPart::parse(v["n"].as_str().unwrap()).unwrap(),
                        text: (v["drafted"].as_bool().unwrap() && !text.is_empty())
                            .then(|| text.to_owned()),
                    }
                })
                .collect();
            (chapter, verses)
        })
        .collect()
}

fn piece<'a>(v: &'a Value) -> Piece<'a> {
    match v["rest"].as_str() {
        Some(rest) => Piece::Continued(rest),
        None => Piece::Start(v["text"].as_str().unwrap()),
    }
}

#[test]
fn each_preview_page_is_the_markup_tc4_renders() {
    let vectors = vectors();
    let mut pages = 0;
    for case in vectors["books"].as_array().unwrap() {
        let by_chapter = verses(case);
        for preview in case["preview"].as_array().unwrap() {
            let setup = PageSetup {
                chapter_opening: if preview["dropCapChapters"].as_bool().unwrap() {
                    ChapterOpening::DropCap
                } else {
                    ChapterOpening::Heading
                },
                verse_numbers: if preview["verseNumbers"].as_bool().unwrap() {
                    VerseNumbers::Shown
                } else {
                    VerseNumbers::Hidden
                },
                ..PageSetup::default()
            };
            for page in preview["pages"].as_array().unwrap() {
                let parts: Vec<PrintPart<'_>> = page["parts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|part| {
                        if let Some(gap) = part["gap"].as_array() {
                            let n = |i: usize| u16::try_from(gap[i].as_u64().unwrap()).unwrap();
                            return PrintPart::Gap(Run {
                                from: n(0),
                                to: n(1),
                            });
                        }
                        let chapter = u16::try_from(part["c"].as_u64().unwrap()).unwrap();
                        let book: Vec<&PrintVerse> = by_chapter
                            .iter()
                            .filter(|(c, _)| *c == chapter)
                            .flat_map(|(_, v)| v)
                            .collect();
                        PrintPart::Chapter(ChapterPart {
                            chapter,
                            head: part["head"].as_bool().unwrap(),
                            verses: part["verses"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|v| {
                                    let label = v["n"].as_str().unwrap();
                                    let verse =
                                        *book.iter().find(|x| x.label.as_str() == label).unwrap();
                                    (verse, piece(v))
                                })
                                .collect(),
                        })
                    })
                    .collect();
                let blocks = print_parts(&parts, &setup, page["continues"].as_bool().unwrap());
                assert_eq!(
                    flow_markup(&blocks, Direction::Ltr, Sheet::Measure),
                    page["html"].as_str().unwrap(),
                    "{} {page}",
                    case["name"]
                );
                pages += 1;
            }
        }
    }
    assert!(pages > 60, "{pages} pages");
}

#[test]
fn the_preview_pages_carry_tc4s_test_ids_and_escape_the_translators_text() {
    let verse = PrintVerse {
        label: RefPart::parse("1").unwrap(),
        text: Some("a <b> & \"c\"".to_owned()),
    };
    let undrafted = PrintVerse {
        label: RefPart::parse("2").unwrap(),
        text: None,
    };
    let parts = [
        PrintPart::Gap(Run { from: 2, to: 3 }),
        PrintPart::Chapter(ChapterPart {
            chapter: 4,
            head: true,
            verses: vec![(&verse, Piece::Whole), (&undrafted, Piece::Whole)],
        }),
    ];
    let setup = PageSetup {
        chapter_opening: ChapterOpening::Heading,
        ..PageSetup::default()
    };
    assert_eq!(
        flow_markup(
            &print_parts(&parts, &setup, true),
            Direction::Rtl,
            Sheet::Page
        ),
        "<p class=\"print-chapter-gap\" dir=\"rtl\" data-testid=\"cc-chapter-gap\">[ chapters 2–3 not yet drafted ]</p>\
         <section class=\"print-chapter print-chapter-continues\" dir=\"rtl\" data-testid=\"cc-chapter\">\
         <h2 class=\"print-chapter-heading\" data-testid=\"cc-chapter-heading\">Chapter 4</h2>\
         <span><sup class=\"print-verse-number\">1</sup>a &lt;b&gt; &amp; &quot;c&quot;<!-- --> </span>\
         <span class=\"print-undrafted\"><sup class=\"print-verse-number\">2</sup>[ verse not yet drafted ]<!-- --> </span>\
         </section>"
    );
}

/// The values are what tC4's `px` gives in JavaScript (`(mm * 96) / 25.4`).
#[test]
fn a_sheet_is_the_paper_with_the_prints_margins() {
    let a4 = sheet_style(&PageSetup::default(), false).to_style();
    assert!(a4.contains("width: 793.7007874015749px"), "{a4}");
    assert!(a4.contains("height: 1122.5196850393702px"), "{a4}");
    assert!(a4.contains("--print-columns: 1;"), "{a4}");
    assert!(a4.contains("--print-leading: 1.4;"), "{a4}");
    assert!(
        a4.contains("padding: 68.03149606299213px 60.472440944881896px 75.59055118110237px"),
        "{a4}"
    );
    let letter = sheet_style(
        &PageSetup {
            paper: Paper::Letter,
            spacing: tc_api::model::print::Spacing::Double,
            columns: tc_api::model::print::Columns::Two,
            ..PageSetup::default()
        },
        true,
    )
    .to_style();
    assert!(letter.contains("width: 816.0000000000001px"), "{letter}");
    assert!(letter.contains("height: 1056px"), "{letter}");
    assert!(letter.contains("--print-columns: 2;"), "{letter}");
    assert!(letter.contains("--print-leading: 2.8;"), "{letter}");
    assert!(letter.contains("visibility: hidden"), "{letter}");
    assert!((px(25.4) - 96.0).abs() < 1e-9);
    assert!(flow_style(true).to_style().contains("column-fill: balance"));
    assert!(flow_style(false).to_style().contains("column-fill: auto"));
    assert!(
        page_number_style()
            .to_style()
            .contains("bottom: 30.795275590551185px")
    );
}

#[test]
fn the_export_menu_lists_the_producers_that_apply() {
    let words = |kind, bridge| -> Vec<String> {
        export_items(kind, bridge)
            .into_iter()
            .map(|key| t(key, &[]))
            .collect()
    };
    assert_eq!(
        words(ProjectKind::Bible, false),
        ["USFM, aligned", "USFM, plain", "Scripture Burrito (.zip)"]
    );
    assert_eq!(
        words(ProjectKind::Stories, false),
        ["Story Markdown (.zip)", "Scripture Burrito (.zip)"]
    );
    assert_eq!(
        words(ProjectKind::Stories, true),
        [
            "Export PDF",
            "Story Markdown (.zip)",
            "Scripture Burrito (.zip)"
        ]
    );
}

#[test]
fn every_export_item_but_the_burrito_zip_runs_its_producer() {
    let runs = |kind| -> Vec<(String, Option<Producer>)> {
        export_items(kind, true)
            .into_iter()
            .map(|key| (t(key, &[]), producer_of(key)))
            .collect()
    };
    assert_eq!(
        runs(ProjectKind::Bible),
        [
            ("Export PDF".to_owned(), Some(Producer::PrintHtml)),
            ("USFM, aligned".to_owned(), Some(Producer::AlignedUsfm)),
            ("USFM, plain".to_owned(), Some(Producer::PlainUsfm)),
            ("Scripture Burrito (.zip)".to_owned(), None),
        ]
    );
    assert_eq!(
        runs(ProjectKind::Stories),
        [
            ("Export PDF".to_owned(), Some(Producer::PrintHtml)),
            (
                "Story Markdown (.zip)".to_owned(),
                Some(Producer::StoryMarkdown)
            ),
            ("Scripture Burrito (.zip)".to_owned(), None),
        ]
    );
}

#[test]
fn a_press_with_verses_in_conflict_asks_first() {
    assert_eq!(conflict_question(0), None);
    assert_eq!(
        conflict_question(1).as_deref(),
        Some("1 verse has two versions. The export uses the version shown. Review first?")
    );
    assert_eq!(
        conflict_question(3).as_deref(),
        Some("3 verses have two versions. The export uses the version shown. Review first?")
    );
}

#[test]
fn a_press_sends_one_request_and_only_a_print_carries_the_page_setup() {
    let of = ExportOf {
        at: Scope {
            project: ProjectRef::parse("en_ult_tit_book").unwrap(),
            epoch: Epoch::new(1),
        },
        subject: Subject::Book {
            book: BookId::Titus,
        },
        title: "Titus".to_owned(),
    };
    let request = RequestId::from_bytes([7; 16]);
    let setup = PageSetup {
        paper: Paper::Letter,
        ..PageSetup::default()
    };
    let usfm = of.run(request, Producer::AlignedUsfm, setup);
    assert_eq!(usfm.request, request);
    assert_eq!(usfm.print, None);
    let print = of.run(request, Producer::PrintHtml, setup).print.unwrap();
    assert_eq!((print.setup, print.title.as_str()), (setup, "Titus"));
    // The words are the catalog's, placeholders and all, so the server words the PDF as the
    // preview does.
    for key in PrintWords::KEYS {
        assert_eq!(
            print.words.message(key),
            Some(t_dyn(key, &[], None).as_str())
        );
    }
    assert_eq!(print.words, print_words());
}

fn exported(skipped: u16, total: u32) -> Exported {
    Exported {
        file: ExportedFile::Download {
            download: DownloadId::parse(&"a".repeat(32)).unwrap(),
            file_name: "TIT-2026-10-24.usfm".to_owned(),
        },
        facts: ExportFacts {
            skipped: (1..=skipped)
                .map(|v| SkippedVerse {
                    chapter: 1,
                    verse: RefPart::parse(&v.to_string()).unwrap(),
                    why: SkipReason::NeedsReview,
                })
                .collect(),
            skipped_total: total,
        },
    }
}

#[test]
fn the_toast_names_at_most_20_skipped_verses() {
    let none = export_notice(&exported(0, 0));
    assert_eq!(none.ready, "Export ready: TIT-2026-10-24.usfm");
    assert_eq!(
        none.download,
        Some(("a".repeat(32), "TIT-2026-10-24.usfm".to_owned()))
    );
    assert_eq!(none.skipped, None);

    let two = export_notice(&exported(2, 2));
    assert_eq!(
        two.skipped.as_deref(),
        Some("Your alignment of these verses is not in the file. Check them in Align: 1:1, 1:2")
    );

    let many = export_notice(&exported(20, 23)).skipped.unwrap();
    assert!(many.ends_with("1:19, 1:20, …"), "{many}");
    assert_eq!(many.matches("1:").count(), 20, "{many}");

    // Facts that kept more than 20 still show 20.
    let over = export_notice(&exported(25, 25)).skipped.unwrap();
    assert_eq!(over.matches("1:").count(), 20, "{over}");
    assert!(over.ends_with("1:20, …"), "{over}");

    let print = export_notice(&Exported {
        file: ExportedFile::Print {
            file_name: "TIT-2026-10-24.pdf".to_owned(),
        },
        facts: ExportFacts::default(),
    });
    assert_eq!(print.ready, "Export ready: TIT-2026-10-24.pdf");
    assert_eq!(print.download, None);
}
