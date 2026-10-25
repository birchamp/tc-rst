//! The one model both Door43 and Aquifer helps are read into.

use std::fmt;

use crate::ids::{AcademyArticleId, HelpItemId, Occurrence, Quote, WordArticleId};
use crate::links::LinkBase;
use crate::refs::{BibleLocus, HelpRef};
use crate::tsv::ReadCells;

/// A translation note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub reference: HelpRef,
    pub id: HelpItemId,
    /// The TSV `Tags` column, split on `;`. Empty in Bible notes; `title` marks the OBS
    /// notes on a story title.
    pub tags: Vec<String>,
    /// The Translation Academy article the note is a check for. A note without one is a
    /// plain note, never a check (except in Open Bible Stories, where every note is one).
    pub support_reference: Option<AcademyArticleId>,
    /// Empty for introductions.
    pub quote: Quote,
    pub occurrence: Occurrence,
    /// The quote in the gateway language, where the source states it: the legacy TSV
    /// `GLQuote` column, or the quoted line of an Aquifer note. Display only; the current
    /// TSV drops it, and the app derives it from the alignment instead.
    pub gl_quote: Option<String>,
    /// The note in the app's markdown dialect (see [`crate::markdown`]).
    pub text: String,
    /// For an item read from a TSV table, the cells its row wrote in a form the writers
    /// would not produce, so writing it back keeps them (see [`crate::tsv`]). `None` for
    /// Aquifer items and items made in the app.
    pub read_from: Option<ReadCells>,
}

/// A link from words of a verse (or a story frame) to a translation words article.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordLink {
    pub reference: HelpRef,
    pub id: HelpItemId,
    /// The TSV `Tags` column, split on `;`: `keyterm`, `name`, or nothing.
    pub tags: Vec<String>,
    /// The original-language words. Empty when the source does not name them (Aquifer).
    pub orig_words: Quote,
    pub occurrence: Occurrence,
    pub article: WordArticleId,
    /// For an item read from a TSV table, the cells its row wrote in a form the writers
    /// would not produce, so writing it back keeps them (see [`crate::tsv`]). `None` for
    /// Aquifer items and items made in the app.
    pub read_from: Option<ReadCells>,
}

/// A comprehension question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub reference: HelpRef,
    pub id: HelpItemId,
    pub tags: Vec<String>,
    pub quote: Quote,
    pub occurrence: Occurrence,
    /// Markdown.
    pub question: String,
    /// Markdown.
    pub response: String,
    /// For an item read from a TSV table, the cells its row wrote in a form the writers
    /// would not produce, so writing it back keeps them (see [`crate::tsv`]). `None` for
    /// Aquifer items and items made in the app.
    pub read_from: Option<ReadCells>,
}

/// A translation words article.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordArticle {
    pub id: WordArticleId,
    pub title: String,
    /// Markdown, without the title heading.
    pub body: String,
}

/// A Translation Academy article.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcademyArticle {
    pub id: AcademyArticleId,
    pub title: String,
    /// The question the article answers (`sub-title.md`). Aquifer does not keep it.
    pub subtitle: Option<String>,
    /// Markdown (`01.md`).
    pub body: String,
}

impl AcademyArticle {
    /// The article as one markdown page: `# title`, `## subtitle`, then the body.
    pub fn page(&self) -> String {
        let mut page = format!("# {}\n", self.title);
        if let Some(sub) = &self.subtitle {
            page.push_str(&format!("## {sub}\n"));
        }
        page.push('\n');
        page.push_str(&self.body);
        page
    }
}

/// Whether a help item can drive a tC check, and why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckReadiness {
    Ready,
    /// The item is reading material, not a check.
    NotACheck(NotACheck),
    /// The item would be a check, but its source lacks data the check needs.
    Blocked(Vec<CheckBlocker>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotACheck {
    Introduction,
    /// A Bible note without a SupportReference.
    PlainNote,
    /// The item names no words of the verse.
    NoQuote,
    /// Occurrence `0`: the item is about no particular words.
    NotTiedToWords,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckBlocker {
    /// The source does not say which occurrence of the quote in the verse the item is
    /// about.
    OccurrenceUnrecorded,
    /// The source links the article to whole verses and names no original-language words.
    NoOriginalWords,
    /// The pinned translation words resource has no article for the link.
    ArticleMissing,
    /// No translation words resource is loaded, so whether the article exists is unknown.
    WordsNotLoaded,
}

impl fmt::Display for CheckBlocker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CheckBlocker::OccurrenceUnrecorded => {
                "the source does not record which occurrence of the quote in the verse the item \
                 is about (Aquifer drops the TSV Occurrence column), so the check cannot tell \
                 repeated words apart"
            }
            CheckBlocker::NoOriginalWords => {
                "the source links the article to whole verses and names no original-language \
                 words (Aquifer has no TWL OrigWords), so there is nothing to select"
            }
            CheckBlocker::ArticleMissing => {
                "the pinned translation words resource has no article for this link"
            }
            CheckBlocker::WordsNotLoaded => {
                "no translation words resource is loaded, so the linked article cannot be found"
            }
        })
    }
}

impl Note {
    /// The base the note's relative links resolve against. Door43 notes, and the Aquifer
    /// notes copied from them, link `../05/03.md` as if each note lived in
    /// `<book>/<chapter>/<verse>.md`.
    pub fn link_base(&self) -> LinkBase {
        match &self.reference {
            HelpRef::Bible(r) => LinkBase::Note {
                book: r.book,
                chapter: match &r.locus {
                    BibleLocus::BookIntro => None,
                    BibleLocus::ChapterIntro(c) | BibleLocus::ChapterFront(c) => Some(*c),
                    BibleLocus::Verses(list) => Some(list.start().chapter().get()),
                    BibleLocus::AsWritten(text) => text.chapter(),
                },
            },
            HelpRef::Obs(_) => LinkBase::ObsNote,
        }
    }

    /// tC's rule: a Bible note is a check when it has a SupportReference; every OBS note is
    /// one. Then it must name words and the occurrence of those words.
    pub fn check_readiness(&self) -> CheckReadiness {
        if self.reference.is_introduction() {
            return CheckReadiness::NotACheck(NotACheck::Introduction);
        }
        if matches!(self.reference, HelpRef::Bible(_)) && self.support_reference.is_none() {
            return CheckReadiness::NotACheck(NotACheck::PlainNote);
        }
        if self.quote.is_empty() {
            return CheckReadiness::NotACheck(NotACheck::NoQuote);
        }
        occurrence_readiness(self.occurrence, Vec::new())
    }
}

impl WordLink {
    /// `article_exists` is whether the pinned translation words resource carries the
    /// linked article (tC drops links to missing articles).
    pub fn check_readiness(&self, article_exists: bool) -> CheckReadiness {
        self.readiness((!article_exists).then_some(CheckBlocker::ArticleMissing))
    }

    /// The readiness with `article`, what blocks the linked article, if anything.
    pub(crate) fn readiness(&self, article: Option<CheckBlocker>) -> CheckReadiness {
        let mut blockers = Vec::new();
        if self.orig_words.is_empty() {
            blockers.push(CheckBlocker::NoOriginalWords);
        }
        blockers.extend(article);
        occurrence_readiness(self.occurrence, blockers)
    }
}

fn occurrence_readiness(occurrence: Occurrence, mut blockers: Vec<CheckBlocker>) -> CheckReadiness {
    if occurrence == Occurrence::Unrecorded {
        blockers.push(CheckBlocker::OccurrenceUnrecorded);
    }
    if !blockers.is_empty() {
        return CheckReadiness::Blocked(blockers);
    }
    if occurrence == Occurrence::NotTied {
        return CheckReadiness::NotACheck(NotACheck::NotTiedToWords);
    }
    CheckReadiness::Ready
}
