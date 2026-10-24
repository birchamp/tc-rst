//! The decisions of Translate (unit B4a), the parts of tC4's `Draft.jsx`, `sections.js` and
//! `helpsFocus.js` that are not drawing. Pure, so they are tested on the host.

use tc_api::buffer::{BufferKey, FieldKey, FieldKind, FieldPlace};
use tc_api::editor::{
    ChapterText, CommentTarget, CommentView, PaneChapter, RowSource, SourceFrame, VerseRow,
};
use tc_api::ids::{ProjectId, ProjectRef};
use tc_api::model::section::SectionVerse;
use tc_api::shown::{PersonLabel, Who};
use tc_api::token::ShownToken;
use tc_core::BookId;

use crate::app::shell::{Editing, js_trim};
use crate::app::understand::{Unit, key_carries, leading_num};
use crate::i18n::{keys, t};

/// Comments keep their durable target when a source section or tab groups verses differently.
pub fn comment_key(
    project: &ProjectRef,
    id: &ProjectId,
    target: &CommentTarget,
) -> Option<BufferKey> {
    let place = match target {
        CommentTarget::Verse {
            book,
            chapter,
            verse,
        } => verse_place(*book, *chapter, &verse.to_string()),
        CommentTarget::Frame { story, frame } => format!("{:02}-{frame:02}", story.get()),
    };
    Some(BufferKey {
        project: project.clone(),
        id: id.clone(),
        field: FieldKey {
            kind: FieldKind::Comment,
            place: FieldPlace::parse(&place).ok()?,
        },
    })
}

/// A restored comment has no mounted box; its field place still names its full save target.
pub fn comment_target(place: &str) -> Option<CommentTarget> {
    if let Some((book, chapter, verse)) = parse_verse_place(place) {
        return Some(CommentTarget::Verse {
            book,
            chapter,
            verse: verse.parse().ok()?,
        });
    }
    let (story, frame) = crate::app::story::parse_frame_place(place)?;
    Some(CommentTarget::Frame { story, frame })
}

/// The verse keys of the target chapter, in order.
pub fn verse_keys(text: &ChapterText) -> Vec<String> {
    text.verses
        .iter()
        .map(|v| v.key.as_str().to_owned())
        .collect()
}

/// tC4's `sectionRanges`: ordered verse keys split where a section starts; keys before the
/// first start are a range of their own, so no verse is ever dropped from the page.
pub fn section_ranges(starts: &[u16], keys: &[String]) -> Vec<Vec<String>> {
    if starts.is_empty() {
        return if keys.is_empty() {
            Vec::new()
        } else {
            vec![keys.to_vec()]
        };
    }
    let mut ranges: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for key in keys {
        let n = leading_num(key);
        let starts_here = n.is_some_and(|n| starts.iter().any(|s| u32::from(*s) == n));
        let head = current.first().and_then(|k| leading_num(k));
        if !current.is_empty() && starts_here && n != head {
            ranges.push(std::mem::take(&mut current));
        }
        current.push(key.clone());
    }
    if !current.is_empty() {
        ranges.push(current);
    }
    ranges
}

/// tC4's `chapterSections`: in another numbering one section per verse (tC4's D30 rule, as in
/// Understand); otherwise the source pane's `\ts\*` sections, else the translation's own, else
/// the whole chapter.
pub fn sections(
    pane: Option<&PaneChapter>,
    text: &ChapterText,
    cross_frame: bool,
) -> Vec<Vec<String>> {
    let keys = verse_keys(text);
    if cross_frame {
        return keys.into_iter().map(|k| vec![k]).collect();
    }
    let marked = |starts: &[u16]| starts.len() > 1;
    let starts: &[u16] = match pane {
        Some(pane) if marked(&pane.sections) => &pane.sections,
        _ if marked(&text.sections) => &text.sections,
        _ => &[],
    };
    section_ranges(starts, &keys)
}

/// Where a section's source verse is in a project of another numbering (#131): Translate
/// shows the source verse the project's numbering names, never the one with the same number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcePlace {
    /// The same numbering: the section's own keys.
    Same,
    /// The source verse at `chapter:verse`, which the pane carries under the project's key; its
    /// number is the one the passage shows.
    Verse { label: String, number: String },
    /// Another book (#121): stated, never shown as this book's text.
    OtherBook { at: String, to: String },
    /// No place in the sources' numbering: stated.
    Unmapped { at: String },
    /// The project's numbering could not be established: no passage.
    Unknown,
}

/// The source place of a project verse `key` of `chapter` under `frame`.
pub fn source_place(book: BookId, chapter: u16, key: &str, frame: &SourceFrame) -> SourcePlace {
    let rows = match frame {
        SourceFrame::Same => return SourcePlace::Same,
        SourceFrame::Unknown => return SourcePlace::Unknown,
        SourceFrame::Mapped { rows } => rows,
    };
    let at = format!("{chapter}:{key}");
    match rows
        .iter()
        .find(|r| r.verse.as_str() == key)
        .map(|r| &r.source)
    {
        Some(RowSource::Verse { chapter, verse }) => SourcePlace::Verse {
            label: format!("{} {chapter}:{verse}", book.english_name()),
            number: verse.as_str().to_owned(),
        },
        Some(RowSource::OtherBook { to }) => SourcePlace::OtherBook { at, to: to.clone() },
        Some(RowSource::Unmapped) | None => SourcePlace::Unmapped { at },
    }
}

/// tC4's `rangeSpan`: "9–10", "9", or "9-10" for a single span key.
pub fn range_span(keys: &[String]) -> String {
    match keys {
        [] => String::new(),
        [only] => only.clone(),
        [from, .., to] => {
            let from = leading_num(from).map_or_else(String::new, |n| n.to_string());
            let to = to.rsplit('-').next().unwrap_or(to);
            format!("{from}–{to}")
        }
    }
}

/// tC4's `sourceKeysFor`: the source pane's keys for a section's verse keys. A span key the
/// source keeps as separate verses becomes those verses; a key inside a source span becomes
/// that span's key.
pub fn source_keys_for(keys: &[String], pane: &PaneChapter) -> Vec<String> {
    let has = |k: &str| pane.verses.iter().any(|v| v.key.as_str() == k);
    let mut expanded: Vec<String> = Vec::new();
    for key in keys {
        if has(key) {
            expanded.push(key.clone());
            continue;
        }
        let span = key
            .split_once('-')
            .and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)));
        match span {
            Some((from, to)) => {
                let inside: Vec<String> = (from..=to)
                    .map(|n| n.to_string())
                    .filter(|k| has(k))
                    .collect();
                if inside.is_empty() {
                    expanded.push(key.clone());
                } else {
                    expanded.extend(inside);
                }
            }
            None => expanded.push(key.clone()),
        }
    }
    let mut out: Vec<String> = Vec::new();
    for key in expanded {
        let mapped = if has(&key) {
            key
        } else {
            pane.verses
                .iter()
                .find(|v| key_carries(v.key.as_str(), &key))
                .map_or(key, |v| v.key.as_str().to_owned())
        };
        if !out.contains(&mapped) {
            out.push(mapped);
        }
    }
    out
}

/// A section's source verses as a unit, for Understand's paragraph rule.
pub fn source_unit(keys: &[String], pane: &PaneChapter) -> Unit {
    let verses = source_keys_for(keys, pane);
    Unit {
        key: String::new(),
        head: verses.first().cloned().unwrap_or_default(),
        label: String::new(),
        verses,
        number: None,
        notice: None,
    }
}

/// A target paragraph's indent (tC4's `paragraphLevel` of a model verse: `q1` 1, `q2` 2,
/// anything else 0).
fn indent(level: Option<u8>) -> u8 {
    match level {
        Some(1) => 1,
        Some(2) => 2,
        _ => 0,
    }
}

/// tC4's `paragraphsOf` over model verses: a verse opens a paragraph when it is the section's
/// first or a paragraph marker stands before its text. Each paragraph with its indent.
pub fn target_paragraphs<'a>(verses: &[&'a VerseRow]) -> Vec<(Vec<&'a VerseRow>, u8)> {
    let mut out: Vec<(Vec<&VerseRow>, u8)> = Vec::new();
    for (i, verse) in verses.iter().enumerate() {
        let opens = i == 0 || verse.lead.is_some() || verse.before.is_some();
        match out.last_mut() {
            Some((para, _)) if !opens => para.push(verse),
            _ => out.push((vec![verse], indent(verse.lead.or(verse.before)))),
        }
    }
    out
}

/// The words a drafted verse shows, one span each (tC4 splits the text on space runs).
pub fn shown_words(text: &str) -> Vec<&str> {
    text.split(crate::app::shell::js_space)
        .filter(|w| !w.is_empty())
        .collect()
}

/// What the verse box shows of a verse's words: all of them, as the server sends them (poem
/// lines as tabs since tC4 rc.2, #786, #810, Q-20); the stub `___` of an undrafted verse shows as
/// an empty box.
pub fn box_text(words: &str) -> &str {
    if js_trim(words) == "___" { "" } else { words }
}

/// A verse field's place in the draft buffer: `TIT 1:4`.
pub fn verse_place(book: BookId, chapter: u16, key: &str) -> String {
    format!("{} {chapter}:{key}", book.code())
}

/// The chapter and verse key of a verse field's place.
pub fn parse_verse_place(place: &str) -> Option<(BookId, u16, String)> {
    let (book, rest) = place.split_once(' ')?;
    let (chapter, verse) = rest.split_once(':')?;
    Some((
        book.parse::<BookId>().ok()?,
        chapter.parse().ok()?,
        verse.to_owned(),
    ))
}

/// The buffer key of a verse of the open project.
pub fn verse_key(
    project: &ProjectRef,
    id: &ProjectId,
    book: BookId,
    chapter: u16,
    key: &str,
) -> Option<BufferKey> {
    Some(BufferKey {
        project: project.clone(),
        id: id.clone(),
        field: FieldKey {
            kind: FieldKind::Verse,
            place: FieldPlace::parse(&verse_place(book, chapter, key)).ok()?,
        },
    })
}

/// The verse an open verse card edits, when it is in `book` and `chapter`.
pub fn editing_verse(editing: &Editing, book: BookId, chapter: u16) -> Option<String> {
    let Editing::Verse(key) = editing else {
        return None;
    };
    let (b, c, verse) = parse_verse_place(key.field.place.as_str())?;
    (b == book && c == chapter).then_some(verse)
}

/// tC4's `editingFocus`: the helps follow the open card's verses, a section card's whole
/// section; none when no card is open in this chapter.
pub fn editing_focus(editing: &Editing, book: BookId, chapter: u16) -> Option<Vec<String>> {
    editing_section(editing, book, chapter)
        .map(<[String]>::to_vec)
        .or_else(|| editing_verse(editing, book, chapter).map(|v| vec![v]))
}

/// A section field's place in the draft buffer: `TIT 2:9-10`, from the first verse to the last
/// of the keys, whatever spans they hold, so a section is the same place before and after a
/// join.
pub fn section_place(book: BookId, chapter: u16, keys: &[String]) -> String {
    let first = keys
        .first()
        .and_then(|k| leading_num(k))
        .map_or_else(String::new, |n| n.to_string());
    let last = keys
        .last()
        .map(|k| k.rsplit('-').next().unwrap_or(k))
        .unwrap_or_default();
    format!("{} {chapter}:{first}-{last}", book.code())
}

/// The buffer key of the section card over `keys` of the open project.
pub fn section_key(
    project: &ProjectRef,
    id: &ProjectId,
    book: BookId,
    chapter: u16,
    keys: &[String],
) -> Option<BufferKey> {
    Some(BufferKey {
        project: project.clone(),
        id: id.clone(),
        field: FieldKey {
            kind: FieldKind::Section,
            place: FieldPlace::parse(&section_place(book, chapter, keys)).ok()?,
        },
    })
}

/// The verse keys of the section card open in `book` and `chapter`.
pub fn editing_section(editing: &Editing, book: BookId, chapter: u16) -> Option<&[String]> {
    let Editing::Section { key, keys } = editing else {
        return None;
    };
    let (b, c, _) = parse_verse_place(key.field.place.as_str())?;
    (b == book && c == chapter).then_some(keys.as_slice())
}

/// The verses a section card opens on, from the rows of its keys: what it types, tC4's
/// `verses` of `SectionEditor` (`body` is the verse's words trimmed, none for the stub).
pub fn card_verses(rows: &[&VerseRow]) -> Vec<SectionVerse> {
    rows.iter()
        .map(|row| SectionVerse {
            key: row.key.as_str().to_owned(),
            drafted: row.drafted,
            body: if row.drafted {
                js_trim(&row.words.value).to_owned()
            } else {
                String::new()
            },
            format: row.format,
        })
        .collect()
}

/// The tokens a section card is based on: each verse's, then those of the verses it covers,
/// in the order the card shows them (STORAGE §5.2 asks for every verse it displays).
pub fn card_tokens(rows: &[&VerseRow]) -> Vec<ShownToken> {
    rows.iter()
        .flat_map(|row| std::iter::once(row.words.token.clone()).chain(row.covered.iter().cloned()))
        .collect()
}

/// "Juan changed this verse while you were typing. Your text will be kept as a second
/// version." (STORAGE §9.9), naming the author when the change names one.
pub fn changed_while_typing(by: Option<&PersonLabel>) -> String {
    let name = by.and_then(|label| match &label.who {
        Who::Username(name) | Who::DisplayName(name) => Some(name.clone()),
        Who::Teammate(n) => Some(t(keys::conflicts::TEAMMATE, &[("n", n)])),
        Who::Mine => None,
    });
    match name {
        Some(name) => t(keys::translate::CHANGED_WHILE_TYPING, &[("name", &name)]),
        None => t(keys::translate::CHANGED_WHILE_TYPING_UNNAMED, &[]),
    }
}

/// The Comments tab (tC4's `CommentsTab`): the newest comment of each verse of the chapter in
/// focus, in verse order.
pub fn comments_in_focus<'a>(
    comments: &'a [CommentView],
    chapter: u16,
    focus: Option<&[String]>,
) -> Vec<&'a CommentView> {
    let mut newest: Vec<&CommentView> = Vec::new();
    for comment in comments.iter().filter(|c| c.chapter == chapter) {
        match newest.iter_mut().find(|kept| kept.verse == comment.verse) {
            Some(kept) if kept.at <= comment.at => *kept = comment,
            Some(_) => {}
            None => newest.push(comment),
        }
    }
    newest.retain(|c| crate::app::understand::in_focus(&c.verse.to_string(), focus));
    newest.sort_by_key(|c| c.verse);
    newest
}
