//! Building the check list of a book or of the stories from translation helps (tC4
//! `derive.ts`).
//!
//! A Bible list is built in tC4's order, which is a requirement and not a preference:
//! every row is made into a check, its reference is mapped into the project's
//! versification, then the list is filtered to the project's scope. The mapped
//! reference is the check's identity, and identities are written once, so mapping comes
//! before anything filters on it or stores it.
//!
//! What counts as a check is tC4's rule: a Bible note with a SupportReference, every
//! words link whose article the resolving set's translation words carry, and every Open
//! Bible Stories note and link. A row whose identity cannot be stored, such as one with
//! no recorded occurrence (Aquifer), is listed in [`CheckList::skipped`] with the reason;
//! a check with no verse in the project's numbering is listed in [`CheckList::dropped`].
//! Neither is ever silently lost.
//!
//! The place of a check is tC4's too. A row on the front of a chapter (`N:front`, a
//! Psalm's superscription) is a check on the verse `front` of that chapter, not on the
//! first verse. A project in another versification drops it as
//! [`Unplaceable::MalformedReference`], because only numbers can be mapped (tC4 D60).

use std::collections::{BTreeSet, HashSet};

use tc_burrito::ScopeRange;
use tc_core::BookId;
use tc_helps::{
    BibleLocus, BibleRef, HelpItemId, HelpRef, Note, ObsRef, Occurrence, WordArticleId, WordLink,
};
use tc_versification::{BookCode, MapOutcome, RowReference, Scheme, Unplaceable, map_reference};

use crate::item::{Category, CheckItem, NoteCategory};
use crate::key::{CheckId, CheckKey, CheckTarget, RefPart, ToolId};
use crate::scope;

/// What a Bible check list is built for.
#[derive(Debug, Clone, Copy)]
pub struct BookSpec<'a> {
    pub book: BookId,
    /// The versification the resource is numbered in. The unfoldingWord helps are all
    /// `eng`.
    pub resource_frame: &'a Scheme,
    /// The project's versification; `None` when it could not be established, and then
    /// no check can be placed.
    pub project_frame: Option<&'a Scheme>,
    /// The book's scope ranges; empty is the whole book.
    pub scope: &'a [ScopeRange],
}

/// A derived check list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckList {
    /// The checks, in the resource's row order.
    pub items: Vec<CheckItem>,
    /// Checks the project's versification has no place for, with the reason. Only those
    /// that would be inside the scope are listed. The screens state their number.
    pub dropped: Vec<Dropped>,
    /// Rows that are not checks, with the reason.
    pub skipped: Vec<Skipped>,
    /// True when a reference was converted between versifications.
    pub mapped: bool,
}

/// A check the project's numbering has no verse for. Its target is in the resource's
/// numbering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dropped {
    pub item: CheckItem,
    pub reason: DropReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropReason {
    /// The project's versification is not known.
    UnknownFrame,
    Unplaceable(Unplaceable),
    /// The mapping lands in another book.
    OtherBook(BookCode),
}

/// A help row that does not become a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub id: HelpItemId,
    pub source: HelpRef,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// A Bible note without a SupportReference: reading material, not a check.
    PlainNote,
    /// The row belongs to another book, or to the stories in a Bible list (or the
    /// reverse), or to another story than the one asked for.
    OutsideList,
    /// The resolving set's translation words do not carry the linked article (tC4
    /// #643).
    ArticleMissing,
    /// The source does not record which occurrence of the quote the row is about
    /// (Aquifer), so the check cannot tell repeated words apart.
    OccurrenceUnrecorded,
    /// Occurrence `-1` (every occurrence) cannot be a check's identity.
    EveryOccurrence,
    /// The row's id is not a valid check id.
    InvalidCheckId(String),
    /// The row's reference has no verse part.
    InvalidReference(String),
    /// Another row of the list already has this identity; the first row is the check.
    DuplicateKey(CheckKey),
}

/// The tN check list of a book from its notes.
pub fn notes_for_book(notes: &[Note], spec: &BookSpec<'_>) -> CheckList {
    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    for note in notes {
        let skip = |reason| Skipped {
            id: note.id.clone(),
            source: note.reference.clone(),
            reason,
        };
        let HelpRef::Bible(place) = &note.reference else {
            skipped.push(skip(SkipReason::OutsideList));
            continue;
        };
        if place.book != spec.book {
            skipped.push(skip(SkipReason::OutsideList));
            continue;
        }
        let Some(support) = &note.support_reference else {
            skipped.push(skip(SkipReason::PlainNote));
            continue;
        };
        let group_id = support.slug().unwrap_or_default().to_owned();
        let row = Row {
            tool: ToolId::TranslationNotes,
            id: &note.id,
            occurrence: note.occurrence,
            group_id: group_id.clone(),
            category: Category::Note(NoteCategory::of(&group_id)),
            quote: note.quote.clone(),
            note: Some(note_cell(note)),
            source: &note.reference,
        };
        match bible_target(spec.book, place, note.read_from.as_ref()) {
            Ok(target) => rows.push((row, target)),
            Err(reason) => skipped.push(skip(reason)),
        }
    }
    finish_book(rows, skipped, spec)
}

/// The tW check list of a book from its word links. `articles` is the set of articles
/// the resolving language set's translation words carry; a link to an article outside
/// it is not a check. `None` applies no such test.
pub fn word_links_for_book(
    links: &[WordLink],
    articles: Option<&BTreeSet<WordArticleId>>,
    spec: &BookSpec<'_>,
) -> CheckList {
    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    for link in links {
        let skip = |reason| Skipped {
            id: link.id.clone(),
            source: link.reference.clone(),
            reason,
        };
        let HelpRef::Bible(place) = &link.reference else {
            skipped.push(skip(SkipReason::OutsideList));
            continue;
        };
        if place.book != spec.book {
            skipped.push(skip(SkipReason::OutsideList));
            continue;
        }
        if articles.is_some_and(|set| !set.contains(&link.article)) {
            skipped.push(skip(SkipReason::ArticleMissing));
            continue;
        }
        let row = word_row(link);
        match bible_target(spec.book, place, link.read_from.as_ref()) {
            Ok(target) => rows.push((row, target)),
            Err(reason) => skipped.push(skip(reason)),
        }
    }
    finish_book(rows, skipped, spec)
}

/// The tN check list of Open Bible Stories: every note is a check, the story titles'
/// notes on frame 0 included. `story` keeps one story's notes.
pub fn notes_for_stories(notes: &[Note], story: Option<u8>) -> CheckList {
    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    for note in notes {
        let group_id = note
            .support_reference
            .as_ref()
            .and_then(|s| s.slug())
            .unwrap_or_default()
            .to_owned();
        let row = Row {
            tool: ToolId::TranslationNotes,
            id: &note.id,
            occurrence: note.occurrence,
            category: Category::Note(NoteCategory::of(&group_id)),
            group_id,
            quote: note.quote.clone(),
            note: Some(note_cell(note)),
            source: &note.reference,
        };
        story_row(row, story, &mut rows, &mut skipped);
    }
    finish_stories(rows, skipped)
}

/// The tW check list of Open Bible Stories. `story` keeps one story's links.
pub fn word_links_for_stories(links: &[WordLink], story: Option<u8>) -> CheckList {
    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    for link in links {
        story_row(word_row(link), story, &mut rows, &mut skipped);
    }
    finish_stories(rows, skipped)
}

/// A help row on its way to becoming a check.
struct Row<'a> {
    tool: ToolId,
    id: &'a HelpItemId,
    occurrence: Occurrence,
    group_id: String,
    category: Category,
    quote: tc_helps::Quote,
    note: Option<String>,
    source: &'a HelpRef,
}

impl Row<'_> {
    fn skip(&self, reason: SkipReason) -> Skipped {
        Skipped {
            id: self.id.clone(),
            source: self.source.clone(),
            reason,
        }
    }

    fn item(&self, target: CheckTarget) -> Result<CheckItem, SkipReason> {
        let occurrence = match self.occurrence {
            Occurrence::NotTied => 0,
            Occurrence::Nth(n) => n.get(),
            Occurrence::All => return Err(SkipReason::EveryOccurrence),
            Occurrence::Unrecorded => return Err(SkipReason::OccurrenceUnrecorded),
        };
        let id = match self.id {
            HelpItemId::Row(id) => id.as_str(),
            HelpItemId::Aquifer(id) => id.as_str(),
        };
        let check_id = CheckId::parse(id).map_err(|_| SkipReason::InvalidCheckId(id.to_owned()))?;
        Ok(CheckItem {
            key: CheckKey {
                tool: self.tool,
                check_id,
                target,
                occurrence,
            },
            group_id: self.group_id.clone(),
            category: self.category,
            quote: self.quote.clone(),
            note: self.note.clone(),
            source: self.source.clone(),
            help_id: self.id.clone(),
        })
    }
}

fn word_row(link: &WordLink) -> Row<'_> {
    let (group_id, category) = match &link.article {
        WordArticleId::Dcs { category, slug } => (slug.as_str().to_owned(), Some(*category)),
        WordArticleId::Aquifer(_) => (String::new(), link.article.category()),
    };
    Row {
        tool: ToolId::TranslationWords,
        id: &link.id,
        occurrence: link.occurrence,
        group_id,
        category: Category::Word(category.unwrap_or(tc_helps::WordCategory::Other)),
        quote: link.orig_words.clone(),
        note: None,
        source: &link.reference,
    }
}

/// The note's TSV cell as tC4 keeps it: the raw cell, escapes included.
fn note_cell(note: &Note) -> String {
    if let Some(raw) = note.read_from.as_ref().and_then(|r| r.get("Note")) {
        return raw.to_owned();
    }
    note.text.replace('\\', "\\\\").replace('\n', "\\n")
}

/// The check's place as tC4 reads it from the row's `Reference` cell (`derive.ts`:
/// `ref.split(':').map(refPart)`): `front:intro` is chapter `front`, verse `intro`;
/// `N:intro` and `N:front` are verse `intro` and verse `front` of chapter N. A verse list
/// keeps the first two `:`-separated parts of the cell as it was written, so `1:3,5` is
/// verse `3,5` and `1:30-2:2` is verse `30-2`.
fn bible_target(
    book: BookId,
    place: &BibleRef,
    read_from: Option<&tc_helps::tsv::ReadCells>,
) -> Result<CheckTarget, SkipReason> {
    let text = |part: &str| {
        RefPart::from_cell(part).map_err(|_| SkipReason::InvalidReference(part.to_owned()))
    };
    let (chapter, verse) = match &place.locus {
        BibleLocus::BookIntro => (text("front")?, text("intro")?),
        BibleLocus::ChapterIntro(c) => (RefPart::number(*c), text("intro")?),
        BibleLocus::ChapterFront(c) => (RefPart::number(*c), text("front")?),
        BibleLocus::Verses(_) => {
            let written = place.to_string();
            let cell = read_from
                .and_then(|r| r.get("Reference"))
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    written
                        .split_once(' ')
                        .map_or(written.clone(), |(_, rest)| rest.to_owned())
                });
            let mut parts = cell.split(':');
            let bad = || SkipReason::InvalidReference(cell.clone());
            let chapter =
                RefPart::from_cell(parts.next().unwrap_or_default()).map_err(|_| bad())?;
            let verse = RefPart::from_cell(parts.next().ok_or_else(bad)?).map_err(|_| bad())?;
            (chapter, verse)
        }
    };
    Ok(CheckTarget::Verse {
        book,
        chapter,
        verse,
    })
}

fn story_row<'a>(
    row: Row<'a>,
    story: Option<u8>,
    rows: &mut Vec<(Row<'a>, CheckTarget)>,
    skipped: &mut Vec<Skipped>,
) {
    let HelpRef::Obs(at) = row.source else {
        skipped.push(row.skip(SkipReason::OutsideList));
        return;
    };
    if story.is_some_and(|s| s != at.story()) {
        skipped.push(row.skip(SkipReason::OutsideList));
        return;
    }
    let at: ObsRef = *at;
    rows.push((row, CheckTarget::Frame(at)));
}

fn finish_stories(rows: Vec<(Row<'_>, CheckTarget)>, mut skipped: Vec<Skipped>) -> CheckList {
    let mut items = Vec::new();
    for (row, target) in rows {
        match row.item(target) {
            Ok(item) => items.push(item),
            Err(reason) => skipped.push(row.skip(reason)),
        }
    }
    let items = unique(items, &mut skipped);
    CheckList {
        items,
        dropped: Vec::new(),
        skipped,
        mapped: false,
    }
}

fn finish_book(
    rows: Vec<(Row<'_>, CheckTarget)>,
    mut skipped: Vec<Skipped>,
    spec: &BookSpec<'_>,
) -> CheckList {
    let mut items = Vec::new();
    let mut dropped = Vec::new();
    let mut mapped = false;
    for (row, target) in rows {
        let item = match row.item(target) {
            Ok(item) => item,
            Err(reason) => {
                skipped.push(row.skip(reason));
                continue;
            }
        };
        match place(&item.key.target, spec) {
            Placement::Placed {
                target,
                mapped: converted,
            } => {
                mapped |= converted;
                let inside = match &target {
                    CheckTarget::Verse { chapter, verse, .. } => {
                        scope::in_scope(spec.scope, chapter, verse)
                    }
                    CheckTarget::Frame(_) => true,
                };
                if inside {
                    items.push(CheckItem {
                        key: CheckKey { target, ..item.key },
                        ..item
                    });
                }
            }
            Placement::Dropped { reason, in_scope } => {
                if in_scope {
                    dropped.push(Dropped { item, reason });
                }
            }
        }
    }
    let items = unique(items, &mut skipped);
    CheckList {
        items,
        dropped,
        skipped,
        mapped,
    }
}

enum Placement {
    /// `mapped` is true when the numbering was converted.
    Placed {
        target: CheckTarget,
        mapped: bool,
    },
    Dropped {
        reason: DropReason,
        in_scope: bool,
    },
}

/// Maps a check's target into the project's numbering (tC4 `deriveForProject`). A
/// failure is reported only when it concerns the scope: always for a whole book,
/// otherwise when a verse the mapping reached is inside a scope range.
fn place(target: &CheckTarget, spec: &BookSpec<'_>) -> Placement {
    let kept = || Placement::Placed {
        target: target.clone(),
        mapped: false,
    };
    let CheckTarget::Verse {
        book,
        chapter,
        verse,
    } = target
    else {
        return kept();
    };
    let whole = spec.scope.is_empty();
    let Some(to) = spec.project_frame else {
        return Placement::Dropped {
            reason: DropReason::UnknownFrame,
            in_scope: whole,
        };
    };
    let row = RowReference {
        book: BookCode::from(*book),
        chapter: chapter.to_versification(),
        verse: verse.to_versification(),
    };
    match map_reference(spec.resource_frame, to, &row) {
        MapOutcome::SameFrame(_) => kept(),
        MapOutcome::Mapped(at) => {
            if at.book.book_id() != Some(*book) {
                return Placement::Dropped {
                    reason: DropReason::OtherBook(at.book),
                    in_scope: whole,
                };
            }
            Placement::Placed {
                target: CheckTarget::Verse {
                    book: *book,
                    chapter: RefPart::number(at.chapter),
                    verse: RefPart::from(&tc_versification::RefPart::from(at.verses)),
                },
                mapped: true,
            }
        }
        MapOutcome::Unplaceable { reason, candidates } => Placement::Dropped {
            reason: DropReason::Unplaceable(reason),
            in_scope: whole
                || candidates
                    .iter()
                    .any(|c| scope::in_scope_numbers(spec.scope, c.chapter, c.verse)),
        },
    }
}

/// Keeps the first check of each identity; a later row with the same identity is
/// reported, because one decision cannot belong to two checks.
fn unique(items: Vec<CheckItem>, skipped: &mut Vec<Skipped>) -> Vec<CheckItem> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if seen.insert(item.key.clone()) {
            out.push(item);
        } else {
            skipped.push(Skipped {
                id: item.help_id,
                source: item.source,
                reason: SkipReason::DuplicateKey(item.key),
            });
        }
    }
    out
}
