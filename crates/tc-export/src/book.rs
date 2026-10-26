//! The print model of a Bible book (tC4 #20; owner rulings of 2026-09-24).
//!
//! [`printed_items`] says what prints, in chapter order: each chapter with a verse that has
//! text to print, and one line for each run of chapters with none between two such
//! chapters. Chapters before the first or after the last are left out. Inside a printed
//! chapter every verse prints: its text, or its number and "[ verse not yet drafted ]".
//!
//! [`print_items`] writes those items in tc-api's print model (`tc_api::model::print`), which
//! says what each part shows under a page setup; the Community Checking preview splits them
//! across pages there. [`print_book`] is the whole book as one flow, as the PDF prints it.

use tc_api::export::{PrintChapter, PrintItem, PrintVerse};
use tc_api::ids::{IdError, RefPart};
use tc_api::model::print::{FlowBlock, PageSetup, atoms_of, parts_of, print_parts};

use crate::{Direction, Run};

/// One verse of a book as its book model holds it: the fields of tC4's `bookModel` verse
/// entry that a print reads (`src/data/bookModel.js:32-39`). The editor's verse rows carry
/// the same three, so the preview, the PDF and the editor state a verse by one rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookVerse {
    /// The verse's number as the book writes it: `4`, or `4-5` for a bridge.
    pub label: String,
    /// The verse's body is neither empty nor the `___` placeholder.
    pub drafted: bool,
    /// The text the screens show for the verse (tC4's `verseText`); empty when the verse is
    /// not drafted.
    pub text: String,
}

impl BookVerse {
    /// The text the print shows, or `None` when the verse prints as not yet drafted. A
    /// drafted verse with no text to show (only a footnote, say) prints as not yet drafted,
    /// as in tC4.
    pub fn printed_text(&self) -> Option<&str> {
        (self.drafted && !self.text.is_empty()).then_some(self.text.as_str())
    }
}

/// One chapter of a book's model, with its verses in book order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookChapter {
    pub number: u16,
    pub verses: Vec<BookVerse>,
}

impl BookChapter {
    /// Whether any verse of the chapter has text to print.
    pub fn prints_text(&self) -> bool {
        self.verses.iter().any(|v| v.printed_text().is_some())
    }
}

/// A printed chapter, or the line for a run of chapters with no text to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintedItem<'a> {
    Chapter(&'a BookChapter),
    Gap(Run<u16>),
}

/// What the preview and the PDF print of a book, in the order of `chapters` (ascending
/// chapter numbers, as the book model holds them): see the module documentation. Empty
/// when no verse of the book has text to print; the PDF then has nothing to print
/// (`printedItems`, `src/data/bookModel.js:52-64`).
pub fn printed_items(chapters: &[BookChapter]) -> Vec<PrintedItem<'_>> {
    let prints: Vec<bool> = chapters.iter().map(BookChapter::prints_text).collect();
    let (Some(first), Some(last)) = (
        prints.iter().position(|p| *p),
        prints.iter().rposition(|p| *p),
    ) else {
        return Vec::new();
    };
    let mut items: Vec<PrintedItem<'_>> = Vec::new();
    for (chapter, prints) in chapters
        .iter()
        .zip(&prints)
        .skip(first)
        .take(last - first + 1)
    {
        if *prints {
            items.push(PrintedItem::Chapter(chapter));
        } else if let Some(PrintedItem::Gap(run)) = items.last_mut() {
            run.to = chapter.number;
        } else {
            items.push(PrintedItem::Gap(Run::single(chapter.number)));
        }
    }
    items
}

/// Whether the preview says the draft is incomplete ("Incomplete draft. Undrafted verses
/// and chapters show as not yet drafted…"): some verse of the book, printed or not, prints
/// as not yet drafted.
///
/// tC4 asks only whether some verse is not drafted
/// (`src/views/CommunityChecking.jsx:182`), so a book whose only such verse is drafted but
/// has no text to show (a footnote alone) prints "[ verse not yet drafted ]" with no
/// notice. The notice is there to explain those lines, so here it follows the print's own
/// rule (a declared difference, AGENTS.md "Matching tC4").
pub fn has_undrafted(chapters: &[BookChapter]) -> bool {
    chapters
        .iter()
        .flat_map(|c| &c.verses)
        .any(|v| v.printed_text().is_none())
}

/// A verse whose label the wire's [`RefPart`] cannot hold.
#[derive(Debug, thiserror::Error)]
#[error("verse {label}: {source}")]
pub struct LabelError {
    pub label: String,
    pub source: IdError,
}

/// The printed items as tc-api's model holds them: what the preview pages and the PDF prints,
/// so both build their items here (`export/book`, APP-DESIGN §3.8).
pub fn print_items(items: &[PrintedItem<'_>]) -> Result<Vec<PrintItem>, LabelError> {
    items
        .iter()
        .map(|item| {
            Ok(match item {
                PrintedItem::Gap(run) => PrintItem::Gap(tc_api::export::Run {
                    from: run.from,
                    to: run.to,
                }),
                PrintedItem::Chapter(chapter) => PrintItem::Chapter(PrintChapter {
                    chapter: chapter.number,
                    verses: chapter
                        .verses
                        .iter()
                        .map(|verse| {
                            Ok(PrintVerse {
                                label: RefPart::parse(&verse.label).map_err(|source| {
                                    LabelError {
                                        label: verse.label.clone(),
                                        source,
                                    }
                                })?,
                                text: verse.printed_text().map(str::to_owned),
                            })
                        })
                        .collect::<Result<_, LabelError>>()?,
                }),
            })
        })
        .collect()
}

/// A whole book as one flow, as the PDF prints it: the title, then the flow, which the
/// print's columns run through (`PrintBook`, `src/views/print/PrintBook.jsx:60-70`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookView<'a> {
    pub title: &'a str,
    pub dir: Direction,
    pub flow: Vec<FlowBlock<'a>>,
}

/// The book's print: `items` from [`print_items`], each chapter whole, titled `title`.
pub fn print_book<'a>(
    title: &'a str,
    items: &'a [PrintItem],
    setup: &PageSetup,
    dir: Direction,
) -> BookView<'a> {
    BookView {
        title,
        dir,
        flow: print_parts(&parts_of(&atoms_of(items)), setup, false),
    }
}
