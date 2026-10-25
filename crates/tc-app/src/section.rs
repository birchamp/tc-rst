//! The section save (unit B4b): the values a section card's text makes of the verses it
//! displayed (STORAGE §5.2), tC4's `saveSection` over the store's value model.
//!
//! The card sends its Type-mode text and the tokens of every verse it displayed, a span's
//! covered verses included. The section's verse keys are not trusted from the card: they are
//! read from the displayed values (a `Text` that reaches a later verse is the span `9-10`).
//! The text is read with the same [`SectionBuffer`] the card runs, and each group the text
//! makes becomes a `Text` with the verse's own marker, anchor and layout, its covered verses
//! `Covered`, and a stub `___` where it has no words. A verse the text leaves as it was is
//! `None`, so a save that changes nothing writes nothing, and a poem's lines, which the card
//! shows as the verse's whole words (#786), come back byte for byte.

use std::collections::BTreeMap;

use tc_api::model::js_trim;
use tc_api::model::section::{ParaFormat, SectionBuffer};
use tc_api::write::SaveRefusal;
use tc_api::{ApiError, NotSaved};
use tc_journal::record::{ChapterVerse, MarkerWs, VerseValue};
use tc_journal::{ChangeId, Record, RecordKey, VerseNo};

use crate::app::App;
use crate::backend::{EditToken, StoreCommand};
use crate::counters::CountingSnapshot;
use crate::dispatch::Handle;
use crate::wire;
use serde_json::json;
use tc_api::editor::SaveSection;
use tc_api::write::Ack;
use tc_journal::command::SectionValue;

/// Why a section's text could not be made into values: the displayed verses do not fit
/// together, an editor bug (STORAGE §5.2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SectionError {
    #[error("{0}")]
    Incoherent(String),
}

fn incoherent<T>(why: impl Into<String>) -> Result<T, SectionError> {
    Err(SectionError::Incoherent(why.into()))
}

/// The first of the lines `\p`, `\q1` and `\q2` a layout holds (tC4's `gapMarkerOf`, #54).
pub fn gap_marker(layout: &str) -> Option<ParaFormat> {
    layout
        .split('\n')
        .find_map(|line| ParaFormat::of_line(line.strip_suffix('\r').unwrap_or(line)))
}

/// `layout` with its `\p`, `\q1` or `\q2` line replaced by `marker`, or removed for none; with
/// none in it, `marker` goes last (tC4's `spliceVerseGap`). Only that one line changes, and it
/// keeps its own line end, so a layout that already says what is asked is returned as it is.
pub fn splice_gap(layout: &str, marker: Option<ParaFormat>) -> String {
    let mut offset = 0;
    for piece in layout.split_inclusive('\n') {
        let end = offset + piece.len();
        let line = piece.strip_suffix('\n').unwrap_or(piece);
        let bare = line.strip_suffix('\r').unwrap_or(line);
        if let Some(found) = ParaFormat::of_line(bare) {
            if marker == Some(found) {
                return layout.to_owned();
            }
            let (before, after) = (
                layout.get(..offset).unwrap_or_default(),
                layout.get(end..).unwrap_or_default(),
            );
            return match marker {
                None => format!("{before}{after}"),
                Some(m) => format!(
                    "{before}\\{}{}{after}",
                    m.name(),
                    piece.get(bare.len()..).unwrap_or_default()
                ),
            };
        }
        offset = end;
    }
    let Some(marker) = marker else {
        return layout.to_owned();
    };
    let newline = if layout.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut out = layout.to_owned();
    // A layout that holds anything starts with a line end (STORAGE §2.4), and the marker
    // starts its own line.
    if !out.ends_with('\n') {
        out.push_str(newline);
    }
    out.push('\\');
    out.push_str(marker.name());
    out.push_str(newline);
    out
}

/// The verse a token displayed and the value it showed, for `book`: the one head's value, or
/// the projected value where the token has several (a conflict, a merge). A token of anything
/// The value a screen showed at `heads`, as the store reads it for a save over that screen's
/// token (`Fold::merged_at`), so a sync that moved the heads since does not change it. Heads
/// in conflict show no one value; the save is refused there, and the projection stands in.
pub fn shown_record(
    read: &CountingSnapshot,
    key: &RecordKey,
    heads: &[ChangeId],
) -> Option<Record> {
    match heads {
        [head] => read.body(head).map(|body| body.record),
        _ => read.merged_at(key, heads).or_else(|| read.projected(key)),
    }
}

/// but a verse of `book` is a forged or misrouted one.
pub fn displayed(
    read: &CountingSnapshot,
    token: &EditToken,
    book: tc_core::BookId,
) -> Result<(u16, VerseValue), ApiError> {
    let RecordKey::Verse(of, _, verse) = token.key() else {
        return Err(ApiError::NotSaved(NotSaved::BadToken));
    };
    if *of != book {
        return Err(ApiError::NotSaved(NotSaved::BadToken));
    }
    let record = shown_record(read, token.key(), token.heads());
    match record {
        Some(Record::Verse { value, .. }) => Ok((verse.get(), value)),
        _ => Err(ApiError::NotSaved(NotSaved::Refused(
            SaveRefusal::IncoherentView,
        ))),
    }
}

/// One verse or span of the section as it was displayed.
struct Old {
    head: u16,
    last: u16,
    marker: MarkerWs,
    words: String,
    layout: String,
    from: Option<ChapterVerse>,
}

fn label(head: u16, last: u16) -> String {
    if last > head {
        format!("{head}-{last}")
    } else {
        head.to_string()
    }
}

/// The text of a verse as the card shows it: trimmed, and empty for the stub.
fn shown_body(words: &str) -> &str {
    let words = js_trim(words);
    if words == "___" { "" } else { words }
}

fn verse_no(n: u16) -> Result<VerseNo, SectionError> {
    VerseNo::new(n).map_or_else(|| incoherent("verse 0"), Ok)
}

/// The displayed verses read as the spans they make, in verse order.
fn spans(displayed: &[(u16, VerseValue)]) -> Result<Vec<Old>, SectionError> {
    let mut sorted: Vec<&(u16, VerseValue)> = displayed.iter().collect();
    sorted.sort_by_key(|(verse, _)| *verse);
    let mut olds: Vec<Old> = Vec::new();
    let mut present: Vec<u16> = Vec::new();
    for (verse, value) in sorted {
        if present.last() == Some(verse) {
            return incoherent(format!("verse {verse} is displayed twice"));
        }
        present.push(*verse);
        match value {
            VerseValue::Text {
                marker,
                words,
                layout,
                last,
                from,
            } => {
                if olds.last().is_some_and(|o| *verse <= o.last) {
                    return incoherent(format!("verse {verse} begins inside a span"));
                }
                olds.push(Old {
                    head: *verse,
                    last: last.get().max(*verse),
                    marker: marker.clone(),
                    words: words.clone(),
                    layout: layout.clone(),
                    from: *from,
                });
            }
            VerseValue::Covered { by } => {
                if !olds
                    .last()
                    .is_some_and(|o| o.head == by.get() && *verse <= o.last)
                {
                    return incoherent(format!(
                        "verse {verse} is covered by {by} but not shown with it"
                    ));
                }
            }
            VerseValue::Absent { .. } => {
                return incoherent(format!("verse {verse} is not in the translation"));
            }
        }
    }
    for old in &olds {
        if (old.head..=old.last).any(|n| !present.contains(&n)) {
            return incoherent(format!(
                "the span {} is not displayed whole",
                label(old.head, old.last)
            ));
        }
    }
    Ok(olds)
}

fn number(text: &str) -> Result<u16, SectionError> {
    text.parse()
        .or_else(|_| incoherent(format!("{text} is not a verse")))
}

/// The value each displayed verse is to have after the card's `text` is saved: `None` for a
/// verse that stays as it was, in the order of `displayed`. `displayed` is each verse number of
/// the section with the value the author saw, every verse of a span included.
pub fn section_values(
    text: &str,
    displayed: &[(u16, VerseValue)],
) -> Result<Vec<Option<VerseValue>>, SectionError> {
    let olds = spans(displayed)?;
    let keys: Vec<String> = olds.iter().map(|o| label(o.head, o.last)).collect();
    let buffer = SectionBuffer::parse(text, &keys);
    let texts: BTreeMap<String, String> = buffer.verses().into_iter().collect();
    // A save that leaves the section with no words changes the verse bodies only: the lines
    // between the verses stay (#600).
    let formats: BTreeMap<String, Option<ParaFormat>> = if buffer.is_empty() {
        BTreeMap::new()
    } else {
        buffer.formats().into_iter().collect()
    };
    // The layout that follows each verse number: a span's belongs to its last verse.
    let mut layout_after: BTreeMap<u16, &str> = BTreeMap::new();
    for old in &olds {
        for n in old.head..old.last {
            layout_after.insert(n, "\n");
        }
        layout_after.insert(old.last, &old.layout);
    }
    let mut new: BTreeMap<u16, VerseValue> = BTreeMap::new();
    for group in buffer.groups() {
        let (Some(first), Some(last)) = (group.members.first(), group.members.last()) else {
            continue;
        };
        let (head, last) = (number(first)?, number(last)?);
        let body = texts.get(&group.key).map_or("", String::as_str);
        let at_head = olds.iter().find(|o| o.head == head);
        let holding = olds.iter().find(|o| (o.head..=o.last).contains(&head));
        // The card shows poem lines as tabs and breaks as empty lines (tC4 #810, REF1b): a
        // verse's text goes back to its words by the words it was shown from.
        let words = match at_head {
            Some(o) if tc_usfm::poem::editor_text(shown_body(&o.words)) == body => o.words.clone(),
            _ if body.is_empty() => "___".to_owned(),
            Some(o) => tc_usfm::poem::words_from_editor_text(body, shown_body(&o.words)),
            None => tc_usfm::poem::words_from_editor_text(body, ""),
        };
        let layout = layout_after.get(&last).copied().unwrap_or("\n");
        let layout = match formats.get(&group.key) {
            Some(format) => splice_gap(layout, *format),
            None => layout.to_owned(),
        };
        new.insert(
            head,
            VerseValue::Text {
                marker: holding.map_or_else(MarkerWs::canonical, |o| o.marker.clone()),
                words,
                layout,
                last: verse_no(last)?,
                from: at_head.and_then(|o| o.from),
            },
        );
        for covered in head + 1..=last {
            new.insert(
                covered,
                VerseValue::Covered {
                    by: verse_no(head)?,
                },
            );
        }
    }
    Ok(displayed
        .iter()
        .map(|(verse, old)| new.get(verse).filter(|value| *value != old).cloned())
        .collect())
}

impl Handle<SaveSection> for App {
    /// The card's text over the verses it displayed (STORAGE §5.2): the values are made from
    /// the text and the values the tokens stood for, and the store checks the section whole.
    async fn handle(&self, req: SaveSection) -> Result<Ack, ApiError> {
        let session = self
            .inner
            .sessions
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let handle = session.handle().clone();
        let tokens = req
            .displayed
            .iter()
            .map(|token| wire::token::edit_token(handle.as_ref(), token))
            .collect::<Result<Vec<_>, _>>()?;
        let read = session.read();
        let displayed = tokens
            .iter()
            .map(|token| crate::section::displayed(&read, token, req.book))
            .collect::<Result<Vec<_>, _>>()?;
        let refused = |_| ApiError::NotSaved(NotSaved::Refused(SaveRefusal::IncoherentView));
        let values = crate::section::section_values(&req.text, &displayed).map_err(refused)?;
        let me = handle.facts().device;
        let command = StoreCommand::save_section(
            req.book,
            tokens
                .iter()
                .cloned()
                .zip(values.into_iter().map(|v| v.map(SectionValue::Verse)))
                .collect(),
            None,
        )
        .requested_by(req.request);
        match handle.submit(command).await {
            Ok(reply) => {
                // The card names the section's first verse, with that verse's own text (#831).
                if let Some(first) = tokens.first() {
                    let words = match session.read().projected(first.key()) {
                        Some(tc_journal::Record::Verse {
                            value: tc_journal::record::VerseValue::Text { words, .. },
                            ..
                        }) => words,
                        _ => String::new(),
                    };
                    self.record_translate_edit(&session, first, &words, self.inner.clock.now());
                }
                wire::write::section_ack(&reply, &session.read(), &tokens, &req.text, me).map_err(
                    |e| {
                        self.internal(
                            "a section save's reply could not be written",
                            crate::reports::facts(json!({"reason": e.to_string()})),
                        )
                    },
                )
            }
            Err(error) => Err(match wire::error::command_error(error) {
                Ok(api) => api,
                Err(unworded) => self.internal(
                    "the store refused a section save no screen has words for yet",
                    crate::reports::facts(json!({"reason": unworded.0})),
                ),
            }),
        }
    }
}
