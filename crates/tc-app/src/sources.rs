//! A source text's book as its Understand pane shows it (unit B3): each chapter's verses with
//! their text, their tokens and the paragraph marks around them, and the chapter's
//! translator's sections. Pure: the open book's resources are read once (`helps::BookCache`)
//! and every chapter is answered from this.
//!
//! The rules are tC4's (`tC4:src/views/sections.js`, `SourceVerse.jsx`, `verseText.js`): the
//! text is [`tc_usfm::display_text`], the tokens [`tc_usfm::display_tokens`], a section
//! starts at a chapter's first verse and at the first verse after each `\ts\*`, and a verse
//! key is a verse number or a bridge (`4-5`).

use std::collections::BTreeMap;

use tc_api::editor::{PaneChapter, PaneVerse};
use tc_api::helps::LitVerse;
use tc_api::ids::RefPart;
use tc_helps::highlight::{AlignedVerse, SourceWord, gateway_quote, highlight_quote};
use tc_helps::{Occurrence, Quote};
use tc_usfm::{Book, DisplayVerse, SegmentKind};

/// One verse of a pane, with what a card's highlight is computed on.
#[derive(Debug, Clone)]
pub struct VerseModel {
    pub key: RefPart,
    /// The first and last verse the key covers.
    pub from: u16,
    pub to: u16,
    pub view: PaneVerse,
    /// The tokens with their alignments, for tc-helps' highlight.
    pub aligned: AlignedVerse,
}

/// One chapter of a pane.
#[derive(Debug, Clone, Default)]
pub struct ChapterModel {
    pub sections: Vec<u16>,
    pub verses: Vec<VerseModel>,
}

impl ChapterModel {
    pub fn view(&self) -> PaneChapter {
        PaneChapter {
            sections: self.sections.clone(),
            verses: self.verses.iter().map(|v| v.view.clone()).collect(),
        }
    }

    /// The pane's rendering of a quote at `verse` (a help's verse part, `1` or `1-3`), and the
    /// tokens of each verse that renders it: tC4's `glTitleFor` and `SourceVerse`. The title
    /// comes from the first verse that carries the help and renders the quote.
    pub fn focus(
        &self,
        verse: &str,
        quote: &Quote,
        occurrence: Occurrence,
    ) -> (Option<String>, Vec<LitVerse>) {
        let Some((from, to)) = verse_range(verse) else {
            return (None, Vec::new());
        };
        let mut title = None;
        let mut lit = Vec::new();
        for model in self.verses.iter().filter(|m| m.from <= to && from <= m.to) {
            if title.is_none() {
                title = gateway_quote(&model.aligned, quote, occurrence);
            }
            let tokens: Vec<u32> = highlight_quote(&model.aligned, quote, occurrence)
                .tokens
                .into_iter()
                .filter_map(|i| u32::try_from(i).ok())
                .collect();
            if !tokens.is_empty() {
                lit.push(LitVerse {
                    verse: model.key.clone(),
                    tokens,
                });
            }
        }
        (title, lit)
    }
}

/// A pane's book, by chapter.
#[derive(Debug, Clone, Default)]
pub struct PaneBook {
    pub chapters: BTreeMap<u16, ChapterModel>,
}

/// `4` → (4, 4), `4-5` → (4, 5); anything else (`intro`, `3a`) is none. tC4's `keyCarries`
/// reads the same two forms.
pub fn verse_range(text: &str) -> Option<(u16, u16)> {
    let (from, to) = text.split_once('-').unwrap_or((text, text));
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(from) || !digits(to) {
        return None;
    }
    Some((from.parse().ok()?, to.parse().ok()?))
}

/// tC4's `isParaMark`, by marker name: `\p`, `\m`, `\pi`, `\pm`, `\pmo`, `\nb`, `\b`, `\q`,
/// `\li`, `\lh`, `\lf`, `\lim`, with a level digit where tC4 allows one.
fn is_para_mark(name: &str) -> bool {
    let with_level = |base: &str| {
        name.strip_prefix(base).is_some_and(|rest| {
            rest.is_empty() || (rest.len() == 1 && rest.as_bytes()[0].is_ascii_digit())
        })
    };
    matches!(name, "p" | "m" | "pm" | "pmo" | "nb" | "b" | "lh" | "lf")
        || with_level("pi")
        || with_level("q")
        || with_level("li")
        || with_level("lim")
}

/// tC4's `levelOfTag`: `\q2` is 2, `\q` 1, any other marker 0.
fn level(name: &str) -> u8 {
    match name.strip_prefix('q') {
        Some("") => 1,
        Some(digits) => digits.parse().unwrap_or(0),
        None => 0,
    }
}

/// The paragraph markers named in `text` (a unit's leading markers), in order.
fn para_marks(text: &str) -> Vec<&str> {
    let (before, after) = tc_usfm::edge_markers(text);
    before
        .into_iter()
        .chain(after)
        .filter(|m| is_para_mark(m))
        .collect()
}

fn aligned(display: &DisplayVerse) -> AlignedVerse {
    let mut verse = AlignedVerse::new();
    let ids: Vec<_> = display
        .groups
        .iter()
        .map(|group| {
            verse.add_alignment(
                group
                    .iter()
                    .map(|s| SourceWord::new(s.content.clone(), s.occurrence))
                    .collect(),
            )
        })
        .collect();
    for token in &display.tokens {
        if token.word {
            // Each group was added above, so its id is known to the verse.
            let _ = verse.push_word(
                token.text.clone(),
                token.group.and_then(|g| ids.get(g).copied()),
            );
        } else {
            verse.push_text(token.text.clone());
        }
    }
    verse
}

/// A source book cut into what its pane shows.
pub fn pane_book(usfm: &str) -> PaneBook {
    let book = Book::parse(usfm);
    let mut out = PaneBook::default();
    let mut chapter: Option<u16> = None;
    // The paragraph marks after the previous verse's text, and whether a `\ts\*` came since
    // the previous verse marker.
    let mut trailing: Vec<String> = Vec::new();
    let mut section_mark = false;
    for segment in book.segments() {
        match segment.kind() {
            SegmentKind::Chapter(mark) => {
                chapter = mark.number();
                trailing.clear();
                section_mark = false;
            }
            SegmentKind::Verse(unit) => {
                let Some(c) = unit.chapter().or(chapter) else {
                    continue;
                };
                let label = unit.label();
                let body = unit.body();
                let leading = unit.leading();
                let (lead, tail) = tc_usfm::edge_markers(body);
                let leading_marks = para_marks(leading);
                let before = leading_marks
                    .last()
                    .map(|m| level(m))
                    .or_else(|| trailing.last().map(|m| level(m)));
                let starts_section = section_mark || leading.contains("\\ts\\*");
                trailing = tail
                    .into_iter()
                    .filter(|m| is_para_mark(m))
                    .map(str::to_owned)
                    .collect();
                section_mark = body.contains("\\ts\\*");
                let (Some((from, to)), Ok(key)) = (verse_range(label), RefPart::parse(label))
                else {
                    continue;
                };
                let display = tc_usfm::display_tokens(body);
                let model = out.chapters.entry(c).or_default();
                if (model.verses.is_empty() || starts_section) && !model.sections.contains(&from) {
                    model.sections.push(from);
                    model.sections.sort_unstable();
                }
                model.verses.push(VerseModel {
                    key: key.clone(),
                    from,
                    to,
                    view: PaneVerse {
                        key,
                        text: tc_usfm::display_text(body),
                        tokens: display.tokens.iter().map(|t| t.text.clone()).collect(),
                        lead: lead.iter().find(|m| is_para_mark(m)).map(|m| level(m)),
                        before,
                    },
                    aligned: aligned(&display),
                });
            }
            _ => {}
        }
    }
    for model in out.chapters.values_mut() {
        model.verses.sort_by_key(|v| v.from);
    }
    out
}
