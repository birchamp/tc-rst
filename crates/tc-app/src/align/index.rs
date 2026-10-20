//! The Align rail (tC4 `alignIndexItems`): every verse of the book with the state of its
//! alignment, from the same rule the open verse and the picker's Align card use (tC4 #796).
//! One probe of the verse and one of its alignment per verse, so the cost is the book's.

use tc_align::{TargetVerse, VerseState, verse_status};
use tc_api::align::{AlignPlace, IndexItem, RailStatus};
use tc_core::BookId;
use tc_journal::alignment::AlignedRun;
use tc_journal::record::VerseValue;
use tc_journal::{Record, RecordKey};

use super::open::{anchor, original_language};
use super::view::rail_state;
use crate::counters::CountingSnapshot;
use crate::drafted::words_drafted;

/// The rail's rows, in the book's order. A verse another verse's span covers has no row of its
/// own.
pub(crate) fn items(read: &CountingSnapshot, book: BookId) -> Vec<IndexItem> {
    let source_sha = original_language(read, book).0.map(|r| r.sha);
    let mut out = Vec::new();
    for chapter in read.chapters_of(book) {
        for key in read.chapter_keys(book, chapter).verses {
            let Some(Record::Verse {
                chapter: c,
                verse,
                value:
                    VerseValue::Text {
                        words,
                        layout,
                        last,
                        from,
                        ..
                    },
                ..
            }) = read.projected(&key)
            else {
                continue;
            };
            let place = AlignPlace {
                chapter,
                verse: verse.get(),
                last: last.get().max(verse.get()),
            };
            if !words_drafted(&words) {
                out.push(IndexItem {
                    place,
                    text: String::new(),
                    status: RailStatus {
                        state: rail_state(VerseState::Undrafted),
                        placed: 0,
                        total: 0,
                    },
                });
                continue;
            }
            let content = format!("{words}{layout}");
            let at = anchor(c, verse, from);
            let stored = match read.projected(&RecordKey::Alignment(book, at.chapter, at.verse)) {
                Some(Record::Alignment { value, .. }) => value,
                // Nothing linked: to do, with no word counted. Most verses of a book are so, and
                // the row needs only the text.
                _ => {
                    out.push(IndexItem {
                        place,
                        text: preview(&tc_usfm::shown_text_v1(&content)),
                        status: RailStatus {
                            state: rail_state(VerseState::Todo),
                            placed: 0,
                            total: 0,
                        },
                    });
                    continue;
                }
            };
            let target = TargetVerse::new(&content);
            let run = AlignedRun {
                anchor: at.verse.get(),
                run_start: verse.get(),
                run_end: place.last,
                text_md5: target.md5(),
                source_sha,
            };
            let status = verse_status(Some(&stored), Some((&run, &target)), None);
            out.push(IndexItem {
                place,
                text: preview(target.text()),
                status: RailStatus {
                    state: rail_state(status.state),
                    placed: u32::try_from(status.placed).unwrap_or(u32::MAX),
                    total: u32::try_from(status.total).unwrap_or(u32::MAX),
                },
            });
        }
    }
    out
}

/// The start of the verse for its row, which shows one line of the rail's width. tC4 puts the
/// whole verse in the row and lets the line hide the rest; 240 characters hold nearly every
/// verse whole, so the row's text is tC4's, and the rare longer verse still fills the line.
fn preview(text: &str) -> String {
    const SHOWN: usize = 240;
    text.chars().take(SHOWN).collect()
}

/// The Align card's count (tC4 `alignPickerEntry`): every drafted verse counts, one is done
/// when its alignment is valid, and the next is the first drafted verse not done.
pub(crate) fn progress(read: &CountingSnapshot, book: BookId) -> tc_api::check::AlignProgress {
    let mut done = 0;
    let mut total = 0;
    let mut next = None;
    for item in items(read, book) {
        if item.status.state == tc_api::align::RailState::Undrafted {
            continue;
        }
        total += 1;
        if item.status.state == tc_api::align::RailState::Valid {
            done += 1;
        } else if next.is_none() {
            next = Some(item.place.label());
        }
    }
    tc_api::check::AlignProgress { done, total, next }
}
