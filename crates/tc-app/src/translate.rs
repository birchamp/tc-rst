//! Translate (unit B4a): the translation's text of the open book, every chapter in one read,
//! so a chapter switch reads nothing of the translation (AD16). Each verse comes with the
//! words its box edits and their token (AD37), and the paragraph marks the grid draws, by the
//! rules the source panes follow ([`crate::sources`]). Nothing here writes the project.

use tc_api::ApiError;
use tc_api::editor::{ChapterText, Chapters, ChaptersView, VerseRow};
use tc_api::ids::{ConflictRef, RefPart};
use tc_api::shown::{ConflictMarker, Shown};
use tc_journal::fold::Resolved;
use tc_journal::record::VerseValue;
use tc_journal::{Record, RecordKey};

use crate::app::App;
use crate::counters::CountingSnapshot;
use crate::dispatch::Handle;
use crate::sources::{is_para_mark, level};
use crate::wire;

/// The highest order of a verse conflict kind (`ConflictKind::is_verse`: verse text, verse
/// structure, a passage, a structure issue).
const LAST_VERSE_KIND: u8 = 3;

/// One verse of a chapter as the store holds it.
struct Text {
    key: RecordKey,
    verse: u16,
    last: u16,
    words: String,
    layout: String,
}

/// The paragraph marks of a verse's content: the levels of those before its first text and
/// after its last, in order.
fn marks(body: &str) -> (Vec<u8>, Vec<u8>) {
    let (lead, tail) = tc_usfm::edge_markers(body);
    let levels = |names: Vec<&str>| {
        names
            .into_iter()
            .filter(|m| is_para_mark(m))
            .map(level)
            .collect::<Vec<u8>>()
    };
    (levels(lead), levels(tail))
}

fn texts(read: &CountingSnapshot, book: tc_core::BookId, chapter: u16) -> Vec<Text> {
    let mut texts: Vec<Text> = read
        .chapter_keys(book, chapter)
        .verses
        .into_iter()
        .filter_map(|key| match read.projected(&key) {
            Some(Record::Verse {
                verse,
                value:
                    VerseValue::Text {
                        words,
                        layout,
                        last,
                        ..
                    },
                ..
            }) => Some(Text {
                key,
                verse: verse.get(),
                last: last.get().max(verse.get()),
                words,
                layout,
            }),
            _ => None,
        })
        .collect();
    texts.sort_unstable_by_key(|t| t.verse);
    texts
}

/// The conflict a verse is in, with how many versions it has.
fn marker(read: &CountingSnapshot, key: &RecordKey) -> Option<ConflictMarker> {
    let state = read.key_state(key)?;
    let Resolved::Conflict(id) = state.state else {
        return None;
    };
    Some(ConflictMarker {
        conflict: ConflictRef::parse(&id.to_hex()).ok()?,
        versions: u8::try_from(state.heads.len()).unwrap_or(u8::MAX),
    })
}

impl App {
    fn chapter_text(
        &self,
        read: &CountingSnapshot,
        book: tc_core::BookId,
        chapter: u16,
    ) -> Result<ChapterText, ApiError> {
        let mut sections = Vec::new();
        let mut verses = Vec::new();
        // The marks after the previous verse's text, and whether a `\ts\*` came in it.
        let mut trailing: Vec<u8> = Vec::new();
        let mut section_mark = false;
        for text in texts(read, book, chapter) {
            let body = format!("{}{}", text.words, text.layout);
            let (lead, tail) = marks(&body);
            let label = if text.last > text.verse {
                format!("{}-{}", text.verse, text.last)
            } else {
                text.verse.to_string()
            };
            let key = RefPart::parse(&label).map_err(|e| {
                self.internal(
                    "a verse's number could not be written",
                    crate::reports::facts(
                        serde_json::json!({"verse": label, "reason": e.to_string()}),
                    ),
                )
            })?;
            if verses.is_empty() || section_mark {
                sections.push(text.verse);
            }
            let minted = read
                .mint(&text.key, None)
                .map_err(|e| e.to_string())
                .and_then(|t| wire::token::shown(&t).map_err(|e| e.to_string()))
                .map_err(|reason| {
                    self.internal(
                        "a verse's token could not be made",
                        crate::reports::facts(
                            serde_json::json!({"verse": label, "reason": reason}),
                        ),
                    )
                })?;
            let record = wire::scope::record_ref(&text.key).ok_or_else(|| {
                self.internal(
                    "a verse's record could not be named",
                    crate::reports::facts(serde_json::json!({"verse": label})),
                )
            })?;
            let drafted = crate::drafted::words_drafted(&text.words);
            verses.push(VerseRow {
                key,
                record,
                drafted,
                text: if drafted {
                    tc_usfm::display_text(&text.words)
                } else {
                    String::new()
                },
                lead: lead.first().copied(),
                before: trailing.last().copied(),
                words: Shown {
                    marker: marker(read, &text.key),
                    value: text.words,
                    token: minted,
                },
            });
            trailing = tail;
            section_mark = body.contains("\\ts\\*");
        }
        Ok(ChapterText {
            chapter,
            sections,
            verses,
        })
    }
}

impl Handle<Chapters> for App {
    async fn handle(&self, req: Chapters) -> Result<ChaptersView, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
        let numbers = read.chapters_of(req.book);
        if numbers.is_empty() {
            return Err(ApiError::BadRequest {
                field: "book".to_owned(),
                reason: format!("{} is not in the project", req.book.code()),
            });
        }
        let chapters = numbers
            .into_iter()
            .map(|chapter| self.chapter_text(&read, req.book, chapter))
            .collect::<Result<Vec<_>, _>>()?;
        let conflicts = read
            .conflict_counts()
            .iter()
            .filter(|((_, kind), _)| *kind <= LAST_VERSE_KIND)
            .map(|(_, n)| *n)
            .sum();
        Ok(ChaptersView {
            view: wire::scope::view(read.seq()),
            chapters,
            conflicts,
        })
    }
}
