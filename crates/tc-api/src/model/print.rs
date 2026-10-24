//! The print model (unit B8, D2): the page setup and what each print part shows
//! (`PrintParts`, `tC4:src/views/print/PrintBook.jsx:37-58`), and the paging the preview does
//! in the page (`tC4:src/views/print/PrintPages.jsx`). It is the one print model: the
//! Community Checking preview and tc-export's PDF both print from it (INV-ONE-PRINT-MODEL).
//!
//! tC4 measures the preview's pages on screen: a hidden sheet takes verses until the next one
//! would overflow, and a verse that does not fit is split between words. The UI does the same
//! with [`paginate`], whose `fits` measures and whose `words` is the browser's word segmenter;
//! here both are inputs, so the paging is a pure function. [`print_parts`] then says what each
//! page's parts show under the page setup. tc-export's `tests/vectors.rs` holds it to tC4's
//! markup on every preview page of `fixtures/export/print-vectors.json` (APP-DESIGN §3.8).
//!
//! [`PageSetup`] holds the choices and tC4's defaults (`DEFAULT_PAGE_SETUP`) and the sheet's
//! measures; tc-export's `page` adds what only the PDF reads.

use serde::{Deserialize, Serialize};

use crate::export::{PrintItem, PrintVerse, Run};

/// The columns a Bible book's text runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Columns {
    #[default]
    One,
    Two,
}

impl Columns {
    pub fn count(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }
}

/// The line spacing: double is twice single.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Spacing {
    #[default]
    Single,
    Double,
}

/// The paper a print is set on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Paper {
    /// 210 × 297 mm.
    #[default]
    A4,
    /// US Letter, 215.9 × 279.4 mm.
    Letter,
}

impl Paper {
    /// The paper's width and height in millimetres (`PAPER_MM`).
    pub fn size_mm(self) -> (f64, f64) {
        match self {
            Self::A4 => (210.0, 297.0),
            Self::Letter => (215.9, 279.4),
        }
    }
}

/// How a chapter of a Bible book opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ChapterOpening {
    /// The chapter's number as a drop cap.
    #[default]
    DropCap,
    /// A "Chapter N" heading.
    Heading,
}

/// Whether a drafted verse shows its number. An undrafted verse always does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum VerseNumbers {
    #[default]
    Shown,
    Hidden,
}

/// Whether an OBS frame shows its picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Pictures {
    #[default]
    Shown,
    Hidden,
}

/// Where an OBS frame's picture sits: above its text, or a quarter of the page wide at the
/// frame's start corner with the text wrapped around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ObsLayout {
    #[default]
    Above,
    Wrapped,
}

/// Every choice of the page setup; the default is tC4's `DEFAULT_PAGE_SETUP`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSetup {
    pub columns: Columns,
    pub chapter_opening: ChapterOpening,
    pub verse_numbers: VerseNumbers,
    pub spacing: Spacing,
    pub paper: Paper,
    pub pictures: Pictures,
    pub obs_layout: ObsLayout,
}

/// Single leading, as a multiple of the verse text's size (`PRINT_LEADING`).
pub const PRINT_LEADING: f64 = 1.4;

/// The page margins in millimetres, top, side and bottom (`PAGE_MARGIN_MM`). The bottom margin
/// holds the page number.
pub const PAGE_MARGIN_MM: (f64, f64, f64) = (18.0, 16.0, 20.0);

impl PageSetup {
    /// `--print-leading`: the line height, as a multiple of the verse text's size.
    pub fn leading(&self) -> f64 {
        match self.spacing {
            Spacing::Single => PRINT_LEADING,
            Spacing::Double => PRINT_LEADING * 2.0,
        }
    }
}

/// What a verse atom shows of its verse's text. `Start` and `Continued` are never empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece<'a> {
    /// The whole verse.
    Whole,
    /// The words before a page break; the number shows when verse numbers are on.
    Start(&'a str),
    /// The words after a page break, up to the next break if it has one; no number shows.
    Continued(&'a str),
}

/// A unit the preview's pages take, in reading order: the line for a run of chapters with
/// nothing to print, or a verse of a chapter (`head` on the chapter's first verse, which
/// carries its drop cap or heading).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atom<'a> {
    Gap(Run),
    Verse {
        chapter: u16,
        head: bool,
        verse: &'a PrintVerse,
        piece: Piece<'a>,
    },
}

impl Atom<'_> {
    /// Whether the atom carries on the chapter before it onto a new page (`carriesOn`).
    fn carries_on(&self) -> bool {
        matches!(self, Self::Verse { head: false, .. })
    }
}

/// The printed items as atoms (`atomsOf`).
pub fn atoms_of(items: &[PrintItem]) -> Vec<Atom<'_>> {
    let mut atoms = Vec::new();
    for item in items {
        match item {
            PrintItem::Gap(run) => atoms.push(Atom::Gap(*run)),
            PrintItem::Chapter(chapter) => {
                atoms.extend(
                    chapter
                        .verses
                        .iter()
                        .enumerate()
                        .map(|(i, verse)| Atom::Verse {
                            chapter: chapter.chapter,
                            head: i == 0,
                            verse,
                            piece: Piece::Whole,
                        }),
                );
            }
        }
    }
    atoms
}

/// The largest `n` in `0..=max` with `ok(n)`, for an `ok` true up to some `n` and false after
/// (`largest`): doubling, then halving. `ok(0)` is never asked.
fn largest(max: usize, mut ok: impl FnMut(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, 1);
    while hi <= max && ok(hi) {
        lo = hi;
        hi *= 2;
    }
    hi = (hi - 1).min(max);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// The text a verse atom takes words from: what it has left to print.
fn left_of<'a>(atom: &Atom<'a>) -> Option<&'a str> {
    let Atom::Verse { verse, piece, .. } = atom else {
        return None;
    };
    let text = verse.text.as_deref().filter(|t| !t.is_empty())?;
    Some(match piece {
        Piece::Whole => text,
        Piece::Start(part) | Piece::Continued(part) => part,
    })
}

/// The atom's first `cut` bytes of what it has left (`pieceOf`): a whole verse keeps its
/// number; a continuation stays one.
fn first_of<'a>(atom: &Atom<'a>, left: &'a str, cut: usize) -> Atom<'a> {
    let mut out = *atom;
    if let Atom::Verse { piece, .. } = &mut out {
        let words = left.get(..cut).unwrap_or(left);
        *piece = match piece {
            Piece::Whole | Piece::Start(_) => Piece::Start(words),
            Piece::Continued(_) => Piece::Continued(words),
        };
    }
    out
}

/// The rest of a verse after a page break (`restOf`): no head, no number.
fn rest_of<'a>(atom: &Atom<'a>, left: &'a str, cut: usize) -> Atom<'a> {
    let mut out = *atom;
    if let Atom::Verse { head, piece, .. } = &mut out {
        *head = false;
        *piece = Piece::Continued(left.get(cut..).unwrap_or_default());
    }
    out
}

/// Fills pages in order (`paginate`). `fits(page, atoms, continues)` says whether `atoms`
/// fit on page `page` (0 is the first, which has the title); `continues` is true when the
/// page's last chapter goes on to the next page. `words(text)` gives the byte offset where
/// each word segment of `text` ends, in order, the last being `text.len()`: the segments join
/// back to the text exactly. A verse that does not fit is split between words; an empty page
/// takes at least one word, so a verse longer than a page is split, never cut, and an atom
/// that fits no page gets a page to itself.
pub fn paginate<'a>(
    atoms: &[Atom<'a>],
    words: &mut dyn FnMut(&str) -> Vec<usize>,
    fits: &mut dyn FnMut(usize, &[Atom<'a>], bool) -> bool,
) -> Vec<Vec<Atom<'a>>> {
    let mut pages: Vec<Vec<Atom<'a>>> = Vec::new();
    let mut queue: Vec<Atom<'a>> = atoms.to_vec();
    let mut start = 0;
    while start < queue.len() {
        let p = pages.len();
        let rest = queue.get(start..).unwrap_or_default();
        let n = largest(rest.len(), |k| {
            let carries = rest.get(k).is_some_and(Atom::carries_on);
            fits(p, rest.get(..k).unwrap_or_default(), carries)
        });
        let mut page: Vec<Atom<'a>> = rest.get(..n).unwrap_or_default().to_vec();
        let mut next_start = start + n;
        if let Some(next) = queue.get(next_start).copied()
            && let Some(left) = left_of(&next)
        {
            let ends = words(left);
            let mut k = largest(ends.len().saturating_sub(1), |m| {
                let cut = ends.get(m - 1).copied().unwrap_or(0);
                let mut trial = page.clone();
                trial.push(first_of(&next, left, cut));
                fits(p, &trial, true)
            });
            if k == 0 && page.is_empty() && ends.len() > 1 {
                k = 1;
            }
            if k > 0 {
                let cut = ends.get(k - 1).copied().unwrap_or(0);
                page.push(first_of(&next, left, cut));
                if let Some(slot) = queue.get_mut(next_start) {
                    *slot = rest_of(&next, left, cut);
                }
            }
        }
        if page.is_empty() {
            page.extend(queue.get(start).copied());
            next_start = start + 1;
        }
        pages.push(page);
        start = next_start;
    }
    pages
}

/// Whether the page after `page` starts by carrying on `page`'s last chapter: the `continues`
/// of `page`'s parts.
pub fn continues(next: Option<&[Atom<'_>]>) -> bool {
    next.and_then(|page| page.first())
        .is_some_and(Atom::carries_on)
}

/// A run of verses of one chapter. `head` is true when the part starts the chapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterPart<'a> {
    pub chapter: u16,
    pub head: bool,
    pub verses: Vec<(&'a PrintVerse, Piece<'a>)>,
}

/// A page's content: the line for a run of chapters, or a run of a chapter's verses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintPart<'a> {
    Gap(Run),
    Chapter(ChapterPart<'a>),
}

/// A page's atoms as print parts: consecutive verses of one chapter share a part (`partsOf`).
pub fn parts_of<'a>(atoms: &[Atom<'a>]) -> Vec<PrintPart<'a>> {
    let mut parts: Vec<PrintPart<'a>> = Vec::new();
    for atom in atoms {
        match *atom {
            Atom::Gap(run) => parts.push(PrintPart::Gap(run)),
            Atom::Verse {
                chapter,
                head,
                verse,
                piece,
            } => match parts.last_mut() {
                Some(PrintPart::Chapter(last)) if last.chapter == chapter && !head => {
                    last.verses.push((verse, piece));
                }
                _ => parts.push(PrintPart::Chapter(ChapterPart {
                    chapter,
                    head,
                    verses: vec![(verse, piece)],
                })),
            },
        }
    }
    parts
}

/// How a printed chapter opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opening {
    /// The chapter's number as a drop cap.
    DropCap,
    /// The heading "Chapter N".
    Heading,
}

/// A verse as the print shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerseLine<'a> {
    /// Text, after the verse's number when `number` is set.
    Text {
        number: Option<&'a str>,
        text: &'a str,
    },
    /// The verse's number, then "[ verse not yet drafted ]".
    NotDrafted { number: &'a str },
}

/// A chapter as the print shows it. `continues` is true when the chapter goes on to the next
/// page, so it ends with no space below it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterBlock<'a> {
    pub chapter: u16,
    pub opening: Option<Opening>,
    pub verses: Vec<VerseLine<'a>>,
    pub continues: bool,
}

/// A block of a page's flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowBlock<'a> {
    /// The line for a run of chapters with nothing to print.
    Gap(Run),
    Chapter(ChapterBlock<'a>),
}

/// What `parts` show under `setup` (`PrintParts`). `continues` marks the last part as going on
/// to the next page; only a chapter takes it.
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
            PrintPart::Gap(run) => FlowBlock::Gap(*run),
            PrintPart::Chapter(part) => FlowBlock::Chapter(ChapterBlock {
                chapter: part.chapter,
                opening: part.head.then_some(match setup.chapter_opening {
                    ChapterOpening::DropCap => Opening::DropCap,
                    ChapterOpening::Heading => Opening::Heading,
                }),
                verses: part
                    .verses
                    .iter()
                    .map(|(verse, piece)| verse_line(verse, *piece, setup.verse_numbers))
                    .collect(),
                continues: continues && Some(i) == last,
            }),
        })
        .collect()
}

fn verse_line<'a>(verse: &'a PrintVerse, piece: Piece<'a>, numbers: VerseNumbers) -> VerseLine<'a> {
    let number = verse.label.as_str();
    let Some(text) = verse.text.as_deref().filter(|t| !t.is_empty()) else {
        return VerseLine::NotDrafted { number };
    };
    let numbered = numbers == VerseNumbers::Shown;
    let (number, text) = match piece {
        Piece::Whole => (numbered.then_some(number), text),
        Piece::Start(part) => (numbered.then_some(number), part),
        Piece::Continued(part) => (None, part),
    };
    VerseLine::Text { number, text }
}
