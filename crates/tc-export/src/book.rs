//! The print model of a Bible book (tC4 #20; owner rulings of 2026-09-24).
//!
//! [`printed_items`] says what prints, in chapter order: each chapter with a verse that has
//! text to print, and one line for each run of chapters with none between two such
//! chapters. Chapters before the first or after the last are left out. Inside a printed
//! chapter every verse prints: its text, or its number and "[ verse not yet drafted ]".
//!
//! [`parts_of_items`] turns those items into print parts, and [`print_parts`] says what each
//! part shows under a page setup: the chapter's drop cap or heading, and each verse with or
//! without its number. The Community Checking preview splits the parts across pages; a
//! verse split by a page break is two [`Piece`]s, and its second piece shows no number.
//! [`print_book`] is the whole book as one flow, as the PDF prints it.

use crate::page::{ChapterOpening, PageSetup, VerseNumbers};
use crate::{Direction, PrintText, Run};

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

/// What a verse of a print part shows of the verse's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece<'a> {
    /// The whole verse.
    Whole,
    /// The words of the verse before its first page break, never empty; its number shows
    /// when verse numbers are on.
    Start(&'a str),
    /// The words of the verse after a page break, up to the next break if it has one, never
    /// empty; no number shows.
    Continued(&'a str),
}

/// A verse of a print part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersePiece<'a> {
    pub verse: &'a BookVerse,
    pub piece: Piece<'a>,
}

/// A run of verses of one chapter. `head` is true when the part starts the chapter, so it
/// carries the chapter's drop cap or heading; the rest of a chapter that a page break split
/// has none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterPart<'a> {
    pub chapter: u16,
    pub head: bool,
    pub verses: Vec<VersePiece<'a>>,
}

/// The flow's content: the line for a run of chapters, or a run of a chapter's verses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintPart<'a> {
    Gap(Run<u16>),
    Chapter(ChapterPart<'a>),
}

/// The printed items as print parts: each chapter whole, with its head (`partsOfItems`,
/// `src/views/print/PrintBook.jsx:33-35`).
pub fn parts_of_items<'a>(items: &[PrintedItem<'a>]) -> Vec<PrintPart<'a>> {
    items
        .iter()
        .map(|item| match *item {
            PrintedItem::Gap(run) => PrintPart::Gap(run),
            PrintedItem::Chapter(chapter) => PrintPart::Chapter(ChapterPart {
                chapter: chapter.number,
                head: true,
                verses: chapter
                    .verses
                    .iter()
                    .map(|verse| VersePiece {
                        verse,
                        piece: Piece::Whole,
                    })
                    .collect(),
            }),
        })
        .collect()
}

/// How a printed chapter opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opening {
    /// The chapter's number as a drop cap.
    DropCap,
    /// A heading line, [`PrintText::ChapterHeading`].
    Heading(PrintText),
}

/// A verse as a print shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerseLine<'a> {
    /// Text, after the verse's number when `number` is set.
    Text {
        number: Option<&'a str>,
        text: &'a str,
    },
    /// The verse's number, then [`PrintText::VerseNotDrafted`].
    NotDrafted { number: &'a str },
}

/// A chapter as a print shows it: its opening when the part starts the chapter, and its
/// verses. `continues` is true when the chapter goes on to the next page, so it ends with
/// no space below it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterBlock<'a> {
    pub chapter: u16,
    pub opening: Option<Opening>,
    pub verses: Vec<VerseLine<'a>>,
    pub continues: bool,
}

/// A block of a print's flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowBlock<'a> {
    /// The line for a run of undrafted chapters, [`PrintText::ChaptersNotDrafted`].
    Gap(PrintText),
    Chapter(ChapterBlock<'a>),
}

/// What `parts` show under `setup` (`PrintParts`, `src/views/print/PrintBook.jsx:37-58`).
/// `continues` marks the last part as going on to the next page; only a chapter takes it.
pub fn print_parts<'a>(
    parts: &[PrintPart<'a>],
    setup: &PageSetup,
    continues: bool,
) -> Vec<FlowBlock<'a>> {
    let last = parts.len().checked_sub(1);
    parts
        .iter()
        .enumerate()
        .map(|(i, part)| match part {
            PrintPart::Gap(run) => FlowBlock::Gap(PrintText::ChaptersNotDrafted(*run)),
            PrintPart::Chapter(part) => FlowBlock::Chapter(ChapterBlock {
                chapter: part.chapter,
                opening: part.head.then_some(match setup.chapter_opening {
                    ChapterOpening::DropCap => Opening::DropCap,
                    ChapterOpening::Heading => Opening::Heading(PrintText::ChapterHeading {
                        chapter: part.chapter,
                    }),
                }),
                verses: part
                    .verses
                    .iter()
                    .map(|piece| verse_line(piece, setup.verse_numbers))
                    .collect(),
                continues: continues && Some(i) == last,
            }),
        })
        .collect()
}

fn verse_line<'a>(piece: &VersePiece<'a>, numbers: VerseNumbers) -> VerseLine<'a> {
    let verse = piece.verse;
    let number = verse.label.as_str();
    if verse.printed_text().is_none() {
        return VerseLine::NotDrafted { number };
    }
    let numbered = numbers == VerseNumbers::Shown;
    let (number, text) = match piece.piece {
        Piece::Whole => (numbered.then_some(number), verse.text.as_str()),
        Piece::Start(text) => (numbered.then_some(number), text),
        Piece::Continued(text) => (None, text),
    };
    VerseLine::Text { number, text }
}

/// A whole book as one flow, as the PDF prints it: the title, then the flow, which the
/// print's columns run through (`PrintBook`, `src/views/print/PrintBook.jsx:60-70`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookView<'a> {
    pub title: &'a str,
    pub dir: Direction,
    pub flow: Vec<FlowBlock<'a>>,
}

/// The book's print: `items` from [`printed_items`], titled `title`.
pub fn print_book<'a>(
    title: &'a str,
    items: &[PrintedItem<'a>],
    setup: &PageSetup,
    dir: Direction,
) -> BookView<'a> {
    BookView {
        title,
        dir,
        flow: print_parts(&parts_of_items(items), setup, false),
    }
}
