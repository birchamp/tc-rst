//! Opening a verse in the aligner (tC4 `openAlign`, `buildAlignmentSession`): the verse's text,
//! the pinned original-language verse its numbering names, the stored alignment, and the
//! tokens the save will carry. Nothing here writes the project.

use std::sync::Arc;

use tc_align::frame::{NoSourceVerse, source_reference};
use tc_align::{Aligner, Basis, SourceBook, TargetVerse, Unavailable};
use tc_api::ApiError;
use tc_api::align::{AlignPlace, Blocked, Testament};
use tc_api::ids::AlignerId;
use tc_api::shown::Direction;
use tc_core::{BookId, Reference, VerseSpan};
use tc_journal::record::{AlignmentValue, ChapterVerse, ResourceRef, VerseValue};
use tc_journal::{ChapterNo, MetaField, Record, RecordKey, VerseNo};
use tc_resources::{Missing, TextRole, TextStatus};

use super::view::{Facts, Gateway};
use crate::app::App;
use crate::backend::EditToken;
use crate::counters::CountingSnapshot;
use crate::session::ProjectSession;
use crate::understand::{LoadedBook, LoadedText, literal_pane};

/// The tokens a save of this verse's alignment carries: the record it replaces and the verse
/// it was made against, both as the screen read them at open (STORAGE §5.3).
#[derive(Clone)]
pub(super) struct Tokens {
    pub alignment: EditToken,
    pub verse: EditToken,
}

pub(super) struct Ready {
    pub aligner: Aligner,
    pub facts: Facts,
    pub tokens: Tokens,
    pub book: BookId,
}

pub(super) enum Prepared {
    Blocked(Blocked),
    Ready(Box<Ready>),
}

/// The original-language pin of a book's testament and its tC4 label, as the store stamps an
/// alignment with them (`dcs::<owner>/<repo>@<version>`).
pub(super) fn original_language(
    read: &CountingSnapshot,
    book: BookId,
) -> (Option<ResourceRef>, String) {
    let slot = match book.testament() {
        tc_core::Testament::Old => "ot",
        tc_core::Testament::New => "nt",
    };
    let Some(Record::Pins { value, .. }) =
        read.projected(&RecordKey::Pins(tc_journal::PinGroup::OriginalLanguage))
    else {
        return (None, String::new());
    };
    let Some(json) = value.slot(slot) else {
        return (None, String::new());
    };
    match tc_journal::pins::SlotPin::read(json)
        .as_ref()
        .and_then(|p| p.usable())
    {
        Some(pin) => {
            let path: Vec<&str> = pin.resource.repo_path.split('/').collect();
            let tail = path[path.len().saturating_sub(2)..].join("/");
            let label = format!("dcs::{tail}@{}", pin.version.as_deref().unwrap_or(""));
            (Some(pin.resource.clone()), label)
        }
        None => (None, String::new()),
    }
}

/// The direction the project's language reads in.
pub(super) fn project_dir(read: &CountingSnapshot) -> Direction {
    let rtl = match read.projected(&RecordKey::ProjectMeta(MetaField::Languages)) {
        Some(Record::ProjectMeta {
            value: Some(tc_journal::Json::Arr(languages)),
            ..
        }) => {
            languages
                .first()
                .and_then(tc_journal::Json::as_obj)
                .and_then(|language| language.get("scriptDirection"))
                .and_then(tc_journal::Json::as_str)
                == Some("rtl")
        }
        _ => false,
    };
    if rtl { Direction::Rtl } else { Direction::Ltr }
}

fn count(n: u16) -> Option<ChapterNo> {
    ChapterNo::new(n)
}

/// The anchor of a verse's text: where its alignment is stored (STORAGE §7.7).
pub(super) fn anchor(
    chapter: ChapterNo,
    verse: VerseNo,
    from: Option<ChapterVerse>,
) -> ChapterVerse {
    from.unwrap_or(ChapterVerse { chapter, verse })
}

/// What the book's original-language text is: the cut book, or why there is none.
pub(super) enum Original {
    Ready { book: Arc<SourceBook> },
    Blocked(Blocked),
}

impl super::AlignCache {
    /// The original-language book cut into verses, once for the session: the first verse
    /// opened pays for the cut, the next ones for themselves.
    async fn original(&self, loaded: &Arc<LoadedBook>, book: BookId) -> Original {
        let sha = match loaded.resources.text(&TextRole::OriginalLanguage) {
            Some(TextStatus::Loaded { pin, .. }) => pin.sha.to_string(),
            None | Some(TextStatus::Missing(Missing::NotPinned)) => {
                return Original::Blocked(Blocked::Unpinned);
            }
            Some(TextStatus::Missing(Missing::Unreadable { .. } | Missing::UnusablePin { .. })) => {
                return Original::Blocked(Blocked::Unreadable);
            }
            Some(TextStatus::Missing(_)) => return Original::Blocked(Blocked::Missing),
        };
        let key = (book, sha);
        let mut cut = self.sources.lock().await;
        if let Some((have, found)) = cut.as_ref()
            && *have == key
        {
            return Original::Ready {
                book: found.clone(),
            };
        }
        let loaded = loaded.clone();
        let parsed = tokio::task::spawn_blocking(move || {
            match loaded.resources.text(&TextRole::OriginalLanguage) {
                Some(TextStatus::Loaded { usfm, .. }) => Some(SourceBook::parse(usfm)),
                _ => None,
            }
        })
        .await;
        match parsed {
            Ok(Some(parsed)) => {
                let parsed = Arc::new(parsed);
                *cut = Some((key, parsed.clone()));
                Original::Ready { book: parsed }
            }
            _ => Original::Blocked(Blocked::Unreadable),
        }
    }
}

/// Opens `chapter`:`verse` of `book`, or says why it cannot be aligned.
pub(super) async fn prepare(
    app: &App,
    session: &Arc<ProjectSession>,
    id: AlignerId,
    book: BookId,
    chapter: u16,
    verse: u16,
) -> Result<Prepared, ApiError> {
    let bad = |what: &str| ApiError::BadRequest {
        field: "verse".to_owned(),
        reason: what.to_owned(),
    };
    let read = session.read();
    let (Some(c), Some(v)) = (count(chapter), VerseNo::new(verse)) else {
        return Err(bad("not a verse"));
    };
    let verse_key = RecordKey::Verse(book, c, v);
    let Some(Record::Verse {
        value:
            VerseValue::Text {
                words,
                layout,
                last,
                from,
                ..
            },
        ..
    }) = read.projected(&verse_key)
    else {
        return Err(bad("not the first verse of a text"));
    };
    if !crate::drafted::words_drafted(&words) {
        return Ok(Prepared::Blocked(Blocked::Undrafted));
    }
    let target = TargetVerse::new(&format!("{words}{layout}"));
    if target.holds_alignment_markup() {
        return Ok(Prepared::Blocked(Blocked::AlignedInText));
    }
    let loaded = app.loaded_book(session, book).await?;
    let original = match session.align().original(&loaded, book).await {
        Original::Ready { book } => book,
        Original::Blocked(why) => return Ok(Prepared::Blocked(why)),
    };
    let last = last.get().max(verse);
    let Some(span) = VerseSpan::range(verse, last) else {
        return Err(bad("a span that ends before it starts"));
    };
    let reference = Reference {
        book,
        chapter,
        verses: span,
    };
    let source_at = match source_reference(loaded.numbering.project_scheme(), reference) {
        Ok(at) => at,
        Err(NoSourceVerse::UnknownFrame) => {
            return Ok(Prepared::Blocked(Blocked::VersificationUnknown));
        }
        Err(NoSourceVerse::Unplaceable(_)) => {
            return Ok(Prepared::Blocked(Blocked::NoCounterpart));
        }
    };
    let source = match original.verse(source_at.chapter, source_at.verses) {
        Ok(source) => source,
        Err(_) => return Ok(Prepared::Blocked(Blocked::Unreadable)),
    };
    let anchor = anchor(c, v, from);
    let alignment_key = RecordKey::Alignment(book, anchor.chapter, anchor.verse);
    let stored: Option<AlignmentValue> = match read.projected(&alignment_key) {
        Some(Record::Alignment { value, .. }) => Some(value),
        _ => None,
    };
    let (pin, source_version) = original_language(&read, book);
    let basis = Basis {
        anchor: anchor.verse,
        run_start: v,
        run_end: VerseNo::new(last).unwrap_or(v),
        source_version,
        source: pin,
    };
    let aligner = match Aligner::open(stored.as_ref(), None, target, &source, basis.clone()) {
        Ok(aligner) => aligner,
        Err(Unavailable::AlignedInText) => return Ok(Prepared::Blocked(Blocked::AlignedInText)),
        Err(Unavailable::NoSource) => return Ok(Prepared::Blocked(Blocked::Missing)),
        Err(Unavailable::NoWords) => return Ok(Prepared::Blocked(Blocked::Undrafted)),
    };
    let token = |key: &RecordKey| {
        read.mint(key, None).map_err(|e| {
            app.internal(
                "an alignment's token could not be made",
                crate::reports::facts(serde_json::json!({ "error": e.to_string() })),
            )
        })
    };
    let tokens = Tokens {
        alignment: token(&alignment_key)?,
        verse: token(&verse_key)?,
    };
    let gateway = gateway(&loaded, chapter, verse, last);
    let facts = Facts {
        id,
        place: AlignPlace {
            chapter,
            verse,
            last,
        },
        testament: match book.testament() {
            tc_core::Testament::Old => Testament::Ot,
            tc_core::Testament::New => Testament::Nt,
        },
        target_dir: project_dir(&read),
        // The Hebrew Bible reads right to left (tC4 `origDir`).
        orig_dir: match book.testament() {
            tc_core::Testament::Old => Direction::Rtl,
            tc_core::Testament::New => Direction::Ltr,
        },
        original: original.display_text(source_at.chapter, source_at.verses),
        gateway,
        marker: crate::translate::marker(&read, &alignment_key),
        basis,
        source,
    };
    Ok(Prepared::Ready(Box::new(Ready {
        aligner,
        facts,
        tokens,
        book,
    })))
}

/// The gateway text of the verse with its own alignment: the literal pane's verse, for a
/// project numbered as its sources are and a pane that carries this very verse or span (tC4's
/// `gatewayTokensFor`, #131).
fn gateway(loaded: &LoadedBook, chapter: u16, verse: u16, last: u16) -> Option<Gateway> {
    if !matches!(loaded.numbering, crate::numbering::Numbering::Same) {
        return None;
    }
    let pane = literal_pane(&loaded.pane_ids())?;
    let LoadedText::Ready(book) = loaded.pane(&pane)? else {
        return None;
    };
    let model = book
        .chapters
        .get(&chapter)?
        .verses
        .iter()
        .find(|m| m.from == verse && m.to == last)?;
    Some(Gateway {
        verse: model.aligned.clone(),
    })
}
