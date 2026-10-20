//! The item detail and the marks (unit B5b): `check/item`, `check/article` and `check/decide`.
//!
//! - **An item** is read from the session's list entry (its check, its decision and state as of
//!   the newest snapshot) and from what one check touches: the verse run its words come from
//!   (a chapter's keys, never the book's text), the loaded book's panes, and its article. It
//!   writes nothing.
//! - **The draft words** are `tc_check::selection::target_words` of the target's shown text, the
//!   words the aligner and the export use, so an index on the screen and an index in a stored
//!   selection are the same word (FIX-text's follow-up).
//! - **A mark** is one `SaveDecision` through the storage seam, over the tokens of what the detail
//!   showed. Mark valid turns the tapped indices into stored selections only against the verse
//!   version the person saw: a verse that changed since is `LookAgain`, never words on the wrong
//!   text. The check is named by its place and its row's id, so a list derived again between the
//!   read and the mark cannot send it to another check.

use tc_api::check::{
    CheckArticle, CheckArticleRead, CheckItemView, CheckNotice, CheckPlace, Decide, DecideEdit,
    DecisionStatus, DecisionView, GatewayText, GatewayView, Item, OriginalView, RailRowPatch,
    TargetWords, Testament, Tool,
};
use tc_api::helps::{ArticleLink, ArticleView};
use tc_api::ids::ViewSeq;
use tc_api::shown::Shown;
use tc_api::write::Ack;
use tc_api::{ApiError, NotSaved};
use tc_check::selection::{self, TargetText};
use tc_check::status::{Edit, Triage, decision_edit};
use tc_check::{CheckItem, CheckTarget, UserStatus};
use tc_core::BookId;
use tc_helps::Occurrence;
use tc_journal::RecordKey;
use tc_journal::record::{Record, VerseValue};

use super::{ListEntry, book_of, count, installed};
use crate::app::App;
use crate::backend::StoreCommand;
use crate::counters::CountingSnapshot;
use crate::dispatch::Handle;
use crate::numbering::Numbering;
use crate::understand::{
    LoadedBook, LoadedText, article_title, article_view, blocks, literal_pane, rung_dir,
};
use crate::wire;

// ---------------------------------------------------------------------------------------
// The draft verse behind a check
// ---------------------------------------------------------------------------------------

/// The verse record whose run holds `target`'s verse, with its content as tC4 reads it
/// (`words` and `layout`, drafted), by the no-text rule of §7.7: none for a chapter's front, a
/// list of verses, a verse nobody drafted.
fn verse_text(read: &CountingSnapshot, target: &CheckTarget) -> Option<(RecordKey, TargetText)> {
    let CheckTarget::Verse {
        book,
        chapter,
        verse,
    } = target
    else {
        return None;
    };
    let keys = read.chapter_keys(*book, chapter.number()?);
    let first = verse.number().map(|n| (n, false)).or_else(|| {
        let (first, _) = verse.span()?;
        Some((first, true))
    })?;
    let start_of = |key: &RecordKey| match key {
        RecordKey::Verse(_, _, start) => Some(start.get()),
        _ => None,
    };
    // The run that starts at the verse, or the last one that starts before it.
    let key = keys
        .verses
        .iter()
        .filter(|key| {
            start_of(key).is_some_and(|start| {
                if first.1 {
                    start == first.0
                } else {
                    start <= first.0
                }
            })
        })
        .max_by_key(|key| start_of(key))?;
    let text = text_at(read, key, target)?;
    Some((key.clone(), text))
}

/// The content of the verse record `key` as the check on `target` reads it: the target's
/// verse is inside the run, or is exactly the run's span, and the run is drafted. The store's
/// `target_text` is the same rule over the displayed version.
fn text_at(read: &CountingSnapshot, key: &RecordKey, target: &CheckTarget) -> Option<TargetText> {
    let CheckTarget::Verse {
        chapter, verse: v, ..
    } = target
    else {
        return None;
    };
    let RecordKey::Verse(_, c, start) = key else {
        return None;
    };
    if chapter.number()? != c.get() {
        return None;
    }
    let Some(Record::Verse {
        value:
            VerseValue::Text {
                marker,
                words,
                layout,
                last,
                ..
            },
        ..
    }) = read.projected(key)
    else {
        return None;
    };
    let inside = match v.number() {
        Some(n) => (start.get()..=last.get()).contains(&n),
        None => {
            let (first, end) = v.span()?;
            first == start.get()
                && end == last.get()
                && tc_usfm::codec::label(first, end) == v.as_str()
        }
    };
    let content = format!("{words}{layout}");
    (inside && tc_usfm::codec::is_drafted(marker.term(), &content))
        .then(|| TargetText::new(content))
}

// ---------------------------------------------------------------------------------------
// check/item
// ---------------------------------------------------------------------------------------

/// What `check/item` takes from the list and the snapshot, before the book's resources.
struct Facts {
    view: ViewSeq,
    item: CheckItem,
    place: CheckPlace,
    title: String,
    target: TargetWords,
    decision: Shown<DecisionView>,
    invalidated: u32,
}

fn status_of(status: UserStatus) -> DecisionStatus {
    match status {
        UserStatus::Valid => DecisionStatus::Valid,
        UserStatus::Invalid => DecisionStatus::Invalid,
        UserStatus::Todo => DecisionStatus::Todo,
    }
}

fn facts(entry: &ListEntry, read: &CountingSnapshot, index: u32) -> Result<Option<Facts>, String> {
    let at = index as usize;
    let (Some(item), Some(state), Some(row)) = (
        entry.derived.items.get(at),
        entry.states.get(at),
        entry.rows.get(at),
    ) else {
        return Ok(None);
    };
    let shown = entry
        .shown
        .get(at)
        .copied()
        .flatten()
        .and_then(|d| entry.decisions.get(d));
    let verse = verse_text(read, &item.key.target);
    let text = verse.as_ref().map(|(_, text)| text.shown());
    let words = text
        .map(|text| {
            selection::target_words(text)
                .iter()
                .map(|w| w.text.to_owned())
                .collect()
        })
        .unwrap_or_default();
    let selected = match (
        text,
        shown.and_then(|d| d.record.judgement.selections.as_deref()),
    ) {
        (Some(text), Some(selections)) => selection::indices(text, selections)
            .into_iter()
            .map(count)
            .collect(),
        _ => Vec::new(),
    };
    let verse_token = verse
        .as_ref()
        .map(|(key, _)| {
            let token = read.mint(key, None).map_err(|e| e.to_string())?;
            wire::token::shown(&token).map_err(|e| e.to_string())
        })
        .transpose()?;
    // A decision shown on this check but stored under another key is edited where it is
    // stored, so it is never copied over or left behind.
    let decision_key = shown.map_or_else(|| item.key.clone(), |d| d.key.clone());
    let decision_token = read
        .mint(&RecordKey::Check(decision_key), None)
        .map_err(|e| e.to_string())
        .and_then(|t| wire::token::shown(&t).map_err(|e| e.to_string()))?;
    let decision = Shown {
        value: DecisionView {
            status: state.status().map(status_of),
            nothing_to_select: state.nothing_to_select,
            bookmarked: state.bookmarked,
            comment: shown
                .and_then(|d| d.record.comment.clone())
                .filter(|c| !c.is_empty()),
            invalidated: state.invalidation().is_some(),
        },
        token: decision_token,
        marker: None,
    };
    Ok(Some(Facts {
        view: super::view_of(read),
        item: item.clone(),
        place: row.place.clone(),
        title: row.title.clone(),
        target: TargetWords {
            words,
            selected,
            verse: verse_token,
        },
        decision,
        // tC4's notice counts the decisions whose words the draft no longer carries; one made
        // against the other rung's resource is the ladder working, and is not announced.
        invalidated: count(
            entry
                .states
                .iter()
                .filter(|s| s.invalidation().is_some_and(|i| i.selections_lost))
                .count(),
        ),
    }))
}

fn testament(book: BookId) -> Testament {
    match book.testament() {
        tc_core::Testament::Old => Testament::Ot,
        tc_core::Testament::New => Testament::Nt,
    }
}

/// The verse `place` names in a source book, by its exact key as tC4 indexes a source's
/// chapter (`chapters[c][v]`): a verse inside a bridge is not found.
fn pane_verse<'a>(
    book: &'a crate::sources::PaneBook,
    place: &CheckPlace,
) -> Option<&'a crate::sources::VerseModel> {
    let CheckPlace::Verse { chapter, verse, .. } = place else {
        return None;
    };
    let chapter = chapter.as_str().parse::<u16>().ok()?;
    book.chapters
        .get(&chapter)?
        .verses
        .iter()
        .find(|m| m.key.as_str() == verse.as_str())
}

fn original(loaded: &LoadedBook, book: BookId, place: &CheckPlace) -> OriginalView {
    let testament = testament(book);
    if !matches!(loaded.numbering, Numbering::Same) {
        return OriginalView::CrossFrame { testament };
    }
    match loaded.original() {
        LoadedText::Ready(text) => OriginalView::Ready {
            testament,
            verse: pane_verse(text, place).map(|m| m.view.text.clone()),
        },
        LoadedText::NotInstalled | LoadedText::Missing => OriginalView::Unavailable { testament },
        LoadedText::Error(reason) => OriginalView::Error {
            testament,
            reason: reason.clone(),
        },
    }
}

fn gateway(loaded: &LoadedBook, place: &CheckPlace, item: &CheckItem) -> GatewayView {
    let ids = loaded.pane_ids();
    if ids.is_empty() {
        return GatewayView::NoPane;
    }
    if !matches!(loaded.numbering, Numbering::Same) {
        return GatewayView::CrossFrame;
    }
    let Some(pane) = literal_pane(&ids) else {
        return GatewayView::NoPane;
    };
    let text = match loaded.pane(&pane) {
        Some(LoadedText::Ready(book)) => match pane_verse(book, place) {
            Some(model) => {
                let occurrence =
                    Occurrence::nth(item.key.occurrence).unwrap_or(Occurrence::NotTied);
                let lit =
                    tc_helps::highlight::highlight_quote(&model.aligned, &item.quote, occurrence)
                        .tokens
                        .into_iter()
                        .filter_map(|i| u32::try_from(i).ok())
                        .collect();
                GatewayText::Verse {
                    tokens: one_space_tokens(&model.view.tokens),
                    lit,
                }
            }
            None => GatewayText::Absent,
        },
        Some(LoadedText::NotInstalled) => GatewayText::NotInstalled,
        Some(LoadedText::Missing) | None => GatewayText::Missing,
        Some(LoadedText::Error(reason)) => GatewayText::Error {
            reason: reason.clone(),
        },
    };
    GatewayView::Pane { pane, text }
}

/// A verse's tokens as tC4's usfm-js hands them to the pane: the USFM breaks a line between the
/// words of an aligned verse, which are single spaces there, and the line break that ends the
/// verse stays.
fn one_space_tokens(tokens: &[String]) -> Vec<String> {
    let mut out: Vec<String> = tokens.iter().map(|t| one_space(t)).collect();
    if let (Some(last), Some(shown)) = (tokens.last(), out.last_mut())
        && last.ends_with('\n')
        && shown.ends_with(' ')
    {
        shown.pop();
        shown.push('\n');
    }
    out
}

/// A token with each run of white space as one space.
fn one_space(token: &str) -> String {
    let mut out = String::with_capacity(token.len());
    let mut space = false;
    for c in token.chars() {
        if c.is_whitespace() {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out
}

/// A note's cell as blocks. The cell keeps its line breaks escaped (`\n` written as a backslash
/// and an n), and tC4's `ExpandableNote` reads them as breaks before it renders.
fn note_blocks(cell: &str) -> Vec<tc_api::model::ArticleBlock> {
    blocks(&cell.replace("\\n", "\n"))
}

/// The article a check links to: a note's Translation Academy article, a word's article.
fn link_of(item: &CheckItem) -> ArticleLink {
    match item.category {
        tc_check::Category::Note(_) => ArticleLink::Academy {
            slug: item.group_id.clone(),
        },
        tc_check::Category::Word(folder) => ArticleLink::Words {
            category: Some(folder.folder().to_owned()),
            slug: item.group_id.clone(),
        },
    }
}

impl Handle<Item> for App {
    async fn handle(&self, req: Item) -> Result<CheckItemView, ApiError> {
        let session = self.read(&req.at)?;
        let book = book_of(&req.list)?;
        let installed = installed(self)?;
        let facts = self
            .with_list(
                &session,
                req.list.tool,
                req.list.unit,
                &installed,
                |entry, read| facts(entry, read, req.index),
            )
            .await?
            .map_err(|_| ApiError::ToolNotReady)?
            .map_err(|reason| {
                self.internal(
                    "a check could not be shown",
                    crate::reports::facts(serde_json::json!({ "reason": reason })),
                )
            })?;
        let Some(facts) = facts else {
            return Err(ApiError::BadRequest {
                field: "index".to_owned(),
                reason: format!("the list has no check {}", req.index),
            });
        };
        let loaded = self.loaded_book_for_checks(&session, book).await?;
        let link = link_of(&facts.item);
        let part = match req.list.tool {
            Tool::TranslationNotes => tc_helps::HelpsPart::Notes,
            Tool::TranslationWords => tc_helps::HelpsPart::WordLinks,
        };
        let rung = match loaded.resources.parts.get(&part) {
            Some(tc_resources::PartStatus::Loaded { rung, .. }) => *rung,
            _ => None,
        };
        Ok(CheckItemView {
            index: req.index,
            view: facts.view,
            check_id: facts.item.key.check_id.as_str().to_owned(),
            original: original(&loaded, book, &facts.place),
            gateway: gateway(&loaded, &facts.place, &facts.item),
            place: facts.place,
            title: facts.title,
            note: facts
                .item
                .note
                .as_deref()
                .map(note_blocks)
                .unwrap_or_default(),
            helps_dir: rung_dir(loaded.pins(), rung),
            article: CheckArticle {
                title: article_title(&loaded, &link),
                category: super::category(facts.item.category),
                link,
            },
            target: facts.target,
            decision: facts.decision,
            notices: if facts.invalidated > 0 {
                vec![CheckNotice::Invalidated {
                    count: facts.invalidated,
                }]
            } else {
                Vec::new()
            },
        })
    }
}

// ---------------------------------------------------------------------------------------
// check/article
// ---------------------------------------------------------------------------------------

impl Handle<CheckArticleRead> for App {
    async fn handle(&self, req: CheckArticleRead) -> Result<ArticleView, ApiError> {
        let session = self.read(&req.at)?;
        let book = book_of(&req.list)?;
        let installed = installed(self)?;
        let link = self
            .with_list(
                &session,
                req.list.tool,
                req.list.unit,
                &installed,
                |entry, _| entry.derived.items.get(req.index as usize).map(link_of),
            )
            .await?
            .map_err(|_| ApiError::ToolNotReady)?
            .ok_or_else(|| ApiError::BadRequest {
                field: "index".to_owned(),
                reason: format!("the list has no check {}", req.index),
            })?;
        let loaded = self.loaded_book_for_checks(&session, book).await?;
        Ok(article_view(&loaded, &link))
    }
}

// ---------------------------------------------------------------------------------------
// check/decide
// ---------------------------------------------------------------------------------------

/// The check a mark names, found in the list by its place and its row's id.
fn find(entry: &ListEntry, place: &CheckPlace, check_id: &str) -> Option<usize> {
    entry
        .derived
        .places
        .iter()
        .zip(&entry.derived.items)
        .position(|(at, item)| at == place && item.key.check_id.as_str() == check_id)
}

fn edit_of(
    edit: &DecideEdit,
    read: &CountingSnapshot,
    verse: Option<&crate::backend::EditToken>,
    item: &CheckItem,
) -> Result<Edit, ApiError> {
    Ok(match edit {
        DecideEdit::Valid { words } if words.is_empty() => Edit::Triage(Triage::Valid(Vec::new())),
        DecideEdit::Valid { words } => {
            let bad = |reason: &str| ApiError::BadRequest {
                field: "edit".to_owned(),
                reason: reason.to_owned(),
            };
            let verse =
                verse.ok_or_else(|| bad("words were tapped in a check with no verse text"))?;
            // The indices are words of the version the person saw. A verse that changed since
            // has other words at them.
            if read.heads(verse.key()) != verse.heads() {
                return Err(ApiError::NotSaved(NotSaved::LookAgain));
            }
            let text = text_at(read, verse.key(), &item.key.target)
                .ok_or_else(|| bad("the verse has no text to select words from"))?;
            let indices: Vec<usize> = words.iter().map(|&i| i as usize).collect();
            Edit::Triage(Triage::Valid(
                selection::from_indices(&text, &indices).map_err(|e| bad(&e.to_string()))?,
            ))
        }
        DecideEdit::Invalid => Edit::Triage(Triage::Invalid),
        DecideEdit::Todo => Edit::Triage(Triage::Todo),
        DecideEdit::Bookmark { on } => Edit::Bookmark(*on),
        DecideEdit::Comment { text } => Edit::Comment(text.clone()),
    })
}

impl Handle<Decide> for App {
    /// One act on a check, saved through the session's store as this request's own save
    /// (AD37). No lock is held across the wait: the write's lease keeps the session open.
    async fn handle(&self, req: Decide) -> Result<Ack, ApiError> {
        let session = self
            .inner
            .sessions
            .reader(&req.at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))?;
        let handle = session.handle().clone();
        book_of(&req.list)?;
        let installed = installed(self)?;
        let shown = wire::token::edit_token(handle.as_ref(), &req.decision)?;
        let verse = req
            .verse
            .as_ref()
            .map(|token| wire::token::edit_token(handle.as_ref(), token))
            .transpose()?;
        let (item, resource, index) = self
            .with_list(
                &session,
                req.list.tool,
                req.list.unit,
                &installed,
                |entry, _| {
                    let at = find(entry, &req.place, &req.check_id)?;
                    Some((
                        entry.derived.items.get(at)?.clone(),
                        entry.derived.resolution.clone(),
                        at,
                    ))
                },
            )
            .await?
            .map_err(|_| ApiError::ToolNotReady)?
            // The list was derived again and no longer holds the check the detail showed.
            .ok_or(ApiError::NotSaved(NotSaved::LookAgain))?;
        let edit = edit_of(&req.edit, &session.read(), verse.as_ref(), &item)?;
        let comment = match &req.edit {
            DecideEdit::Comment { text } => text.clone(),
            _ => String::new(),
        };
        let me = handle.facts().device;
        let command = StoreCommand::save_decision(
            shown.clone(),
            verse.unwrap_or_else(|| shown.clone()),
            decision_edit(&item, edit, &resource),
        )
        .requested_by(req.request);
        match handle.submit(command).await {
            Ok(reply) => {
                // The check's row as the newest snapshot shows it: what the write changed.
                let rows = self
                    .with_list(
                        &session,
                        req.list.tool,
                        req.list.unit,
                        &installed,
                        |entry, _| {
                            entry
                                .rows
                                .get(index)
                                .map(|row| vec![RailRowPatch { row: row.clone() }])
                        },
                    )
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .flatten()
                    .unwrap_or_default();
                wire::write::ack_decision(&reply, &session.read(), &shown, &comment, me, rows)
                    .map_err(|e| {
                        self.internal(
                            "a save's reply could not be written",
                            crate::reports::facts(serde_json::json!({"reason": e.to_string()})),
                        )
                    })
            }
            Err(error) => Err(match wire::error::command_error(error) {
                Ok(api) => api,
                Err(unworded) => self.internal(
                    "the store refused a save no screen has words for yet",
                    crate::reports::facts(serde_json::json!({"reason": unworded.0})),
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verses_line_breaks_between_words_are_single_spaces() {
        assert_eq!(one_space(",\n"), ", ");
        assert_eq!(one_space("\n"), " ");
        assert_eq!(one_space("a \n b"), "a b");
        assert_eq!(one_space("Paul"), "Paul");
        let verse = [",\n".to_owned(), "Christ".to_owned(), ",\n".to_owned()];
        assert_eq!(one_space_tokens(&verse), [", ", "Christ", ",\n"]);
    }

    #[test]
    fn a_notes_escaped_line_breaks_are_breaks() {
        let blocks = note_blocks("## Abstract nouns\\nThe words **faith** and *hope*.");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text(), "Abstract nouns");
        assert_eq!(blocks[1].text(), "The words faith and hope.");
    }
}
