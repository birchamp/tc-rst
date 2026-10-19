//! What each command writes (`docs/STORAGE.md` §5.1 to §5.9, §5.14 to §5.16), as pure
//! functions of the fold, the token key and the command: step B1 of `commit_batch` (§5.0).
//! A refusal writes nothing; the UI keeps the text and shows the reason.
//!
//! Every value a command changes starts from what its token says the screen displayed, never
//! from the record's current heads (I4): a teammate's version that arrived after the screen
//! was drawn stays a head beside the new change, and the record is a visible conflict.

use crate::reply::{CommandError, Refusal};
use std::collections::{BTreeMap, BTreeSet};
use tc_align::TargetVerse;
use tc_journal::command::{
    AlignmentEdit, Choice, Command, DecisionEdit, Draft, DraftField, PlannedChange, RestoreMode,
    SectionValue, UpgradePlan,
};
use tc_journal::fold::{ConflictId, FoldState, ProjectKind, Resolved};
use tc_journal::json::Json;
use tc_journal::keys::{CheckKey, CheckTarget, PinGroup, RecordKey};
use tc_journal::record::{
    AlignmentValue, BatchReleaseValue, BookPresence, CarriedValue, ChapterFront, ChapterVerse,
    CheckValue, CommentTarget, CommentValue, LayoutItem, PresentHow, Record, ResourceRef, Scope,
    UsfmBytes, VerseValue,
};
use tc_journal::token::{ShownToken, TokenKind};
use tc_journal::{BatchId, BookId, Cause, ChangeId, ChapterNo, CommentId, TokenKey, VerseNo};
use unicode_normalization::UnicodeNormalization;

/// What a command writes.
#[derive(Debug, Default)]
pub(crate) struct Plan {
    pub changes: Vec<PlannedChange>,
    /// Conflicts a bulk choice left for a choice of their own.
    pub skipped: Vec<ConflictId>,
}

/// What the planner reads.
pub(crate) struct Ctx<'a> {
    pub fold: &'a FoldState,
    pub signing: &'a TokenKey,
    pub kind: ProjectKind,
}

type R<T> = Result<T, CommandError>;

/// A record a section editor displayed: its token, the value it showed, and the new value
/// the save sends (`None` when unchanged).
type Displayed<V> = (ShownToken, Option<V>, Option<V>);

fn nfc(text: &str) -> String {
    text.nfc().collect()
}

fn refused(r: Refusal) -> CommandError {
    CommandError::Refused(r)
}

fn change(record: Record, parents: &[ChangeId], cause: Cause) -> PlannedChange {
    let mut parents = parents.to_vec();
    parents.sort();
    parents.dedup();
    PlannedChange {
        record,
        parents,
        cause,
    }
}

impl Ctx<'_> {
    /// The token, checked again under this project's key, of the kind the command takes.
    fn verified(&self, token: &ShownToken, kind: TokenKind) -> R<ShownToken> {
        let checked =
            ShownToken::parse(self.signing, &token.to_text()).map_err(CommandError::BadToken)?;
        if checked.kind() != kind {
            return Err(refused(Refusal::WrongTokenKind));
        }
        Ok(checked)
    }

    fn edit(&self, token: &ShownToken) -> R<ShownToken> {
        self.verified(token, TokenKind::Edit)
    }

    /// The value the token says the screen displayed: the head's value, the merged value of
    /// a merged record, the displayed candidate of a conflict (§5.1 step 2). `None` when
    /// nothing was shown.
    fn displayed(&self, token: &ShownToken) -> R<Option<Record>> {
        let heads = token.heads();
        let mut values: Vec<Record> = Vec::with_capacity(heads.len());
        for head in heads {
            let body = self
                .fold
                .body(head)
                .ok_or(CommandError::Refused(Refusal::Unreadable))?;
            if body.record.key() != *token.key() {
                return Err(CommandError::LookAgain);
            }
            values.push(body.record);
        }
        let Some(first) = values.first().cloned() else {
            return Ok(None);
        };
        if values.iter().all(|v| *v == first) {
            return Ok(Some(first));
        }
        // Several heads with different values stand for one value only as a merge or by
        // the stub rule, which the fold holds for its current heads.
        let state = self.fold.key_state(token.key());
        match state {
            Some(s) if s.heads.as_slice() == heads => match s.state {
                Resolved::Merged { merged, .. } => Ok(Some(*merged)),
                Resolved::SettledByRule { head, .. } | Resolved::Settled { head } => self
                    .fold
                    .body(&head)
                    .map(|b| Some(b.record))
                    .ok_or(CommandError::Refused(Refusal::Unreadable)),
                Resolved::Conflict(_) => Err(CommandError::LookAgain),
            },
            _ => Err(CommandError::LookAgain),
        }
    }

    fn require_kind(&self, kind: ProjectKind) -> R<()> {
        if self.kind == kind {
            Ok(())
        } else {
            Err(refused(Refusal::WrongProject))
        }
    }
}

/// B1 for one command.
pub(crate) fn plan(ctx: &Ctx<'_>, command: &Command) -> R<Plan> {
    let changes = match command {
        Command::SaveVerse { shown, words } => save_verse(ctx, shown, |v| {
            if let VerseValue::Text { words: w, .. } = v {
                *w = nfc(words);
            }
        })?,
        Command::SaveLayout { shown, layout } => save_verse(ctx, shown, |v| {
            if let VerseValue::Text { layout: l, .. } = v {
                *l = nfc(layout);
            }
        })?,
        Command::SaveSection {
            book,
            shown,
            layout,
        } => save_section(ctx, *book, shown, layout.as_ref(), false)?,
        Command::SaveAlignment {
            shown,
            verse_shown,
            value,
        } => save_alignment(ctx, shown, verse_shown, value)?,
        Command::SaveDecision {
            shown,
            verse_shown,
            value,
        } => save_decision(ctx, shown, verse_shown, value)?,
        Command::AddComment { target, text } => add_comment(target, text)?,
        Command::ReviseComment { shown, text } => revise_comment(ctx, shown, text)?,
        Command::SetSetting { shown, value } => set_setting(ctx, shown, value.clone())?,
        Command::SetMeta { shown, value } => set_meta(ctx, shown, value.clone())?,
        Command::SetPins { shown, value } => {
            let shown = ctx.edit(shown)?;
            let RecordKey::Pins(group) = shown.key() else {
                return Err(refused(Refusal::WrongRecord("pins")));
            };
            let record = Record::Pins {
                group: *group,
                value: value.clone(),
            };
            unchanged_or(ctx, &shown, record, Cause::Edit)?
        }
        Command::AddBook { book, scope, seed } => add_book(ctx, *book, scope, &seed.usfm)?,
        Command::RemoveBook {
            book,
            presence_shown,
        } => remove_book(ctx, *book, presence_shown)?,
        Command::RestoreBook {
            book,
            presence_shown,
            mode,
        } => restore_book(ctx, *book, presence_shown, *mode)?,
        Command::SaveFrame { shown, text } => save_frame(ctx, shown, text)?,
        Command::ApplyUpgrade { plan } => apply_upgrade(ctx, plan)?,
        Command::Resolve {
            conflict,
            shown,
            choice,
        } => resolve(ctx, conflict, shown, choice)?,
        Command::ResolveBulk { devices, conflicts } => {
            return resolve_bulk(ctx, devices, conflicts);
        }
        Command::MoveCheck {
            shown,
            to,
            to_shown,
        } => move_check(ctx, shown, to, to_shown)?,
        Command::Restore { shown, from } => restore(ctx, shown, *from)?,
        Command::RemoveCarriedFile { shown } => {
            let shown = ctx.edit(shown)?;
            let RecordKey::CarriedFile(path) = shown.key() else {
                return Err(refused(Refusal::WrongRecord("a carried file")));
            };
            let record = Record::CarriedFile {
                path: path.clone(),
                value: CarriedValue::Removed,
            };
            unchanged_or(ctx, &shown, record, Cause::Edit)?
        }
        Command::SetBookScope {
            presence_shown,
            scope,
        } => set_book_scope(ctx, presence_shown, scope)?,
        Command::ReleaseBatch { batch } => release_batch(ctx, *batch)?,
        Command::Dismiss { shown } => dismiss(ctx, shown)?,
        Command::KeepDraft { draft, section } => keep_draft(ctx, draft, section)?,
        Command::Draft { .. } => Vec::new(),
    };
    Ok(Plan {
        changes,
        skipped: Vec::new(),
    })
}

/// One change of the displayed record, or none when the new value equals it (§5.1 step 4: a
/// no-op save after a sync must not replace a teammate's newer version).
fn unchanged_or(
    ctx: &Ctx<'_>,
    shown: &ShownToken,
    record: Record,
    cause: Cause,
) -> R<Vec<PlannedChange>> {
    if ctx.displayed(shown)?.as_ref() == Some(&record) {
        return Ok(Vec::new());
    }
    Ok(vec![change(record, shown.heads(), cause)])
}

// ---------------------------------------------------------------------------------------
// §5.1 a verse, §5.2 a section
// ---------------------------------------------------------------------------------------

fn save_verse(
    ctx: &Ctx<'_>,
    shown: &ShownToken,
    edit: impl FnOnce(&mut VerseValue),
) -> R<Vec<PlannedChange>> {
    let shown = ctx.edit(shown)?;
    let RecordKey::Verse(book, chapter, verse) = shown.key().clone() else {
        return Err(refused(Refusal::WrongRecord("a verse")));
    };
    let Some(Record::Verse { value: old, .. }) = ctx.displayed(&shown)? else {
        return Err(refused(Refusal::NotText));
    };
    if !matches!(old, VerseValue::Text { .. }) {
        return Err(refused(Refusal::NotText));
    }
    let mut new = old.clone();
    edit(&mut new);
    if new == old {
        return Ok(Vec::new());
    }
    let mut out = vec![change(
        Record::Verse {
            book,
            chapter,
            verse,
            value: new.clone(),
        },
        shown.heads(),
        Cause::Edit,
    )];
    out.extend(reflow(ctx, book, chapter, verse, &old, &new));
    Ok(out)
}

/// The anchor of a verse value: its `from`, else its own number (§7.7).
fn anchor_of(chapter: ChapterNo, verse: VerseNo, value: &VerseValue) -> ChapterVerse {
    match value {
        VerseValue::Text {
            from: Some(from), ..
        } => *from,
        _ => ChapterVerse { chapter, verse },
    }
}

/// The reflowed alignment of a verse whose text changed, in the batch of the text (R25,
/// §5.1): every link that follows its own word into the new text is kept, its occurrence
/// updated, the others go back to the bank, and the value is stamped for the new text (tC4
/// #213, #789, `tc_align::reflow_edit` from the old text). Its parents are the alignment's heads the actor holds, so an
/// alignment edit made elsewhere at the same time is a visible conflict with the reflow. No
/// reflow when the alignment is in conflict, when its span is not the verse's span any more
/// (a span was made or broken: a person aligns again), or when `tc_align::reflow_edit` leaves
/// the value as it is.
fn reflow(
    ctx: &Ctx<'_>,
    book: BookId,
    chapter: ChapterNo,
    verse: VerseNo,
    old: &VerseValue,
    new: &VerseValue,
) -> Option<PlannedChange> {
    let VerseValue::Text {
        words,
        layout,
        last,
        ..
    } = new
    else {
        return None;
    };
    let VerseValue::Text {
        words: old_words,
        layout: old_layout,
        last: old_last,
        ..
    } = old
    else {
        return None;
    };
    if old_last != last {
        return None;
    }
    let anchor = anchor_of(chapter, verse, new);
    let key = RecordKey::Alignment(book, anchor.chapter, anchor.verse);
    let state = ctx.fold.key_state(&key)?;
    if matches!(state.state, Resolved::Conflict(_)) {
        return None;
    }
    let Some(Record::Alignment { value, .. }) = ctx.fold.projected(&key) else {
        return None;
    };
    let span = anchor
        .verse
        .get()
        .checked_add(last.get().checked_sub(verse.get())?)?;
    if value.span_end.get() != span {
        return None;
    }
    let before = TargetVerse::new(&format!("{old_words}{old_layout}"));
    let target = TargetVerse::new(&format!("{words}{layout}"));
    let reflowed = tc_align::reflow_edit(&value, &before, &target).ok()?;
    Some(change(
        Record::Alignment {
            book,
            chapter: anchor.chapter,
            verse: anchor.verse,
            value: reflowed.value,
        },
        &state.heads,
        Cause::Edit,
    ))
}

fn nfc_section_value(value: &SectionValue) -> SectionValue {
    match value {
        SectionValue::Verse(VerseValue::Text {
            marker,
            words,
            layout,
            last,
            from,
        }) => SectionValue::Verse(VerseValue::Text {
            marker: marker.clone(),
            words: nfc(words),
            layout: nfc(layout),
            last: *last,
            from: *from,
        }),
        SectionValue::ChapterFront(ChapterFront::Present(bytes)) => {
            SectionValue::ChapterFront(ChapterFront::Present(UsfmBytes(nfc(&bytes.0))))
        }
        other => other.clone(),
    }
}

/// §5.2. `unverified`: the tokens of a draft this installation can no longer verify (R20):
/// each value is written with no parents, and the section's coherence is judged against the
/// records' current values.
fn save_section(
    ctx: &Ctx<'_>,
    book: BookId,
    shown: &[(ShownToken, Option<SectionValue>)],
    layout: Option<&ShownToken>,
    unverified: bool,
) -> R<Vec<PlannedChange>> {
    ctx.require_kind(ProjectKind::Bible)?;
    let mut verses: BTreeMap<(u16, u16), Displayed<VerseValue>> = BTreeMap::new();
    let mut fronts: BTreeMap<u16, Displayed<ChapterFront>> = BTreeMap::new();
    for (token, new) in shown {
        let token = if unverified {
            token.clone()
        } else {
            ctx.edit(token)?
        };
        // A token this installation cannot verify is no base for a change (R20), but the
        // section is judged as it stands: a span the draft keeps is coherent against the
        // records it covers, which the records hold now.
        let displayed = if unverified {
            ctx.fold.projected(token.key())
        } else {
            ctx.displayed(&token)?
        };
        let new = new.as_ref().map(nfc_section_value);
        match (token.key().clone(), new) {
            (RecordKey::Verse(b, c, v), new) if b == book => {
                let old = match displayed {
                    Some(Record::Verse { value, .. }) => Some(value),
                    _ => None,
                };
                let new = match new {
                    None => None,
                    Some(SectionValue::Verse(value)) => Some(value),
                    Some(SectionValue::ChapterFront(_)) => {
                        return Err(refused(Refusal::WrongRecord("a verse value")));
                    }
                };
                verses.insert((c.get(), v.get()), (token, old, new));
            }
            (RecordKey::ChapterFront(b, c), new) if b == book => {
                let old = match displayed {
                    Some(Record::ChapterFront { value, .. }) => Some(value),
                    _ => None,
                };
                let new = match new {
                    None => None,
                    Some(SectionValue::ChapterFront(value)) => Some(value),
                    Some(SectionValue::Verse(_)) => {
                        return Err(refused(Refusal::WrongRecord("a chapter front")));
                    }
                };
                fronts.insert(c.get(), (token, old, new));
            }
            _ => {
                return Err(refused(Refusal::WrongRecord(
                    "a verse or chapter front of the book",
                )));
            }
        }
    }
    // The section as the author saw it, with the new values applied (§5.2 step 2).
    let view: BTreeMap<(u16, u16), &VerseValue> = verses
        .iter()
        .filter_map(|(at, (_, old, new))| new.as_ref().or(old.as_ref()).map(|v| (*at, v)))
        .collect();
    for (&(c, v), value) in &view {
        match value {
            VerseValue::Text { last, .. } => {
                for covered in v + 1..=last.get() {
                    match view.get(&(c, covered)) {
                        Some(VerseValue::Covered { by }) if by.get() == v => {}
                        _ => {
                            return Err(refused(Refusal::Incoherent(format!(
                                "{c}:{v} spans to {} but {c}:{covered} is not covered by it",
                                last.get()
                            ))));
                        }
                    }
                }
            }
            VerseValue::Covered { by } => match view.get(&(c, by.get())) {
                Some(VerseValue::Text { last, .. }) if last.get() >= v => {}
                _ => {
                    return Err(refused(Refusal::Incoherent(format!(
                        "{c}:{v} is covered by {c}:{} but that verse does not span it",
                        by.get()
                    ))));
                }
            },
            VerseValue::Absent { .. } => {}
        }
    }
    let mut out = Vec::new();
    let mut added: Vec<LayoutItem> = Vec::new();
    for (&c, (token, old, new)) in &fronts {
        let Some(new) = new else { continue };
        if old.as_ref() == Some(new) {
            continue;
        }
        let chapter = ChapterNo::new(c).ok_or(refused(Refusal::WrongRecord("a chapter")))?;
        if old.is_none() && matches!(new, ChapterFront::Present(_)) {
            added.push(LayoutItem::Chapter(chapter));
        }
        let parents: &[ChangeId] = if unverified { &[] } else { token.heads() };
        out.push(change(
            Record::ChapterFront {
                book,
                chapter,
                value: new.clone(),
            },
            parents,
            Cause::Edit,
        ));
    }
    for (&(c, v), (token, old, new)) in &verses {
        let Some(new) = new else { continue };
        if old.as_ref() == Some(new) {
            continue;
        }
        let (Some(chapter), Some(verse)) = (ChapterNo::new(c), VerseNo::new(v)) else {
            return Err(refused(Refusal::WrongRecord("a verse")));
        };
        if old.is_none() && !matches!(new, VerseValue::Absent { .. }) {
            added.push(LayoutItem::Verse(chapter, verse));
        }
        let parents: &[ChangeId] = if unverified { &[] } else { token.heads() };
        out.push(change(
            Record::Verse {
                book,
                chapter,
                verse,
                value: new.clone(),
            },
            parents,
            Cause::Edit,
        ));
        if let Some(old) = old
            && !unverified
        {
            out.extend(reflow(ctx, book, chapter, verse, old, new));
        }
    }
    if !added.is_empty()
        && let Some(change) = layout_change(ctx, book, layout, &added)?
    {
        out.push(change);
    }
    Ok(out)
}

/// The book's `BookLayout` with `added` placed after their numeric predecessors (§5.2), when
/// the book has a layout that does not list them.
fn layout_change(
    ctx: &Ctx<'_>,
    book: BookId,
    layout: Option<&ShownToken>,
    added: &[LayoutItem],
) -> R<Option<PlannedChange>> {
    let key = RecordKey::BookLayout(book);
    let Some(Record::BookLayout {
        value: Some(items), ..
    }) = ctx.fold.projected(&key)
    else {
        return Ok(None);
    };
    let missing: Vec<LayoutItem> = added
        .iter()
        .filter(|a| !items.contains(a))
        .copied()
        .collect();
    if missing.is_empty() {
        return Ok(None);
    }
    let layout = layout.ok_or(refused(Refusal::WrongRecord(
        "the book's layout token, to place a new verse",
    )))?;
    let layout = ctx.edit(layout)?;
    if *layout.key() != key {
        return Err(refused(Refusal::WrongRecord("the book's layout")));
    }
    let base = match ctx.displayed(&layout)? {
        Some(Record::BookLayout {
            value: Some(items), ..
        }) => items,
        _ => items,
    };
    let mut out = base;
    for item in missing {
        let at = out
            .iter()
            .rposition(|existing| precedes(existing, &item))
            .map_or(0, |i| i + 1);
        out.insert(at, item);
    }
    Ok(Some(change(
        Record::BookLayout {
            book,
            value: Some(out),
        },
        layout.heads(),
        Cause::Edit,
    )))
}

/// Whether `existing` is a numeric predecessor `item` goes after.
fn precedes(existing: &LayoutItem, item: &LayoutItem) -> bool {
    match (existing, item) {
        (LayoutItem::Chapter(a), LayoutItem::Chapter(b)) => a < b,
        (LayoutItem::Verse(c1, _), LayoutItem::Chapter(c2)) => c1 < c2,
        (LayoutItem::Chapter(a), LayoutItem::Verse(c, _)) => a <= c,
        (LayoutItem::Verse(c1, v1), LayoutItem::Verse(c2, v2)) => (c1, v1) < (c2, v2),
    }
}

// ---------------------------------------------------------------------------------------
// §5.3 an alignment, §5.4 a decision
// ---------------------------------------------------------------------------------------

/// The original-language pin of a book's testament and its tC4 label
/// (`dcs::<owner>/<repo>@<version>`, `src/state.jsx:1188` at d310bf3).
fn original_language(fold: &FoldState, book: BookId) -> (Option<ResourceRef>, String) {
    let slot = match book.testament() {
        tc_core::Testament::Old => "ot",
        tc_core::Testament::New => "nt",
    };
    let Some(Record::Pins { value, .. }) =
        fold.projected(&RecordKey::Pins(PinGroup::OriginalLanguage))
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

fn save_alignment(
    ctx: &Ctx<'_>,
    shown: &ShownToken,
    verse_shown: &ShownToken,
    edit: &AlignmentEdit,
) -> R<Vec<PlannedChange>> {
    let shown = ctx.edit(shown)?;
    let verse_shown = ctx.edit(verse_shown)?;
    let displayed_verse = ctx.displayed(&verse_shown)?;
    let displayed = match ctx.displayed(&shown)? {
        Some(Record::Alignment { value, .. }) => Some(value),
        _ => None,
    };
    let record = alignment_record(
        ctx,
        shown.key(),
        verse_shown.key(),
        displayed_verse,
        displayed,
        edit,
    )?;
    unchanged_or(ctx, &shown, record, Cause::Edit)
}

/// The alignment record the aligner's `edit` makes (§5.3), stamped from the verse value the
/// aligner displayed, not the current one: if the verse changed meanwhile, the saved
/// alignment is stale at once and shown so, never re-stamped silently.
fn alignment_record(
    ctx: &Ctx<'_>,
    key: &RecordKey,
    verse_key: &RecordKey,
    verse_value: Option<Record>,
    displayed: Option<AlignmentValue>,
    edit: &AlignmentEdit,
) -> R<Record> {
    let RecordKey::Alignment(book, chapter, anchor) = key.clone() else {
        return Err(refused(Refusal::WrongRecord("an alignment")));
    };
    let RecordKey::Verse(vbook, _, run_start) = verse_key.clone() else {
        return Err(refused(Refusal::WrongRecord("the aligned verse")));
    };
    if vbook != book {
        return Err(refused(Refusal::WrongRecord(
            "a verse of the alignment's book",
        )));
    }
    let Some(Record::Verse {
        value:
            VerseValue::Text {
                words,
                layout,
                last,
                ..
            },
        ..
    }) = verse_value
    else {
        return Err(refused(Refusal::NotText));
    };
    let content = format!("{words}{layout}");
    let target = TargetVerse::new(&content);
    // The run's end in the anchor's numbering (§5.3, §7.7): a renumbered verse's alignment
    // covers as many verses as it did where it was made.
    let length = last.get().saturating_sub(run_start.get());
    let span_end = VerseNo::new(anchor.get().saturating_add(length)).unwrap_or(anchor);
    let (source, source_version) = original_language(ctx.fold, book);
    let mut value = AlignmentValue {
        span_end,
        alignments: edit.alignments.clone(),
        word_bank: edit.word_bank.clone(),
        target_verse_md5: target.md5(),
        source_version: source_version.into(),
        source,
        done: edit.done,
        marked_invalid: false,
        dismissed: displayed.as_ref().is_some_and(|d| d.dismissed),
        extra: displayed.map(|d| d.extra).unwrap_or_default(),
    };
    value.put_in_verse_order(
        target
            .words()
            .iter()
            .map(|w| (w.word.as_str(), w.occurrence)),
    );
    Ok(Record::Alignment {
        book,
        chapter,
        verse: anchor,
        value,
    })
}

/// The target's text by the no-text rule of §7.7, from the verse the screen displayed: the
/// run's content when the target names a verse of the run (or the run's exact span) and the
/// run is drafted; `None` otherwise (`front`, a list of verses, an undrafted verse).
fn target_text(check: &CheckKey, verse: &RecordKey, value: Option<&Record>) -> Option<String> {
    let CheckTarget::Verse {
        chapter, verse: v, ..
    } = &check.target
    else {
        return None;
    };
    let RecordKey::Verse(_, c, start) = verse else {
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
    }) = value
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
    (inside && tc_usfm::codec::is_drafted(marker.term(), &content)).then_some(content)
}

fn save_decision(
    ctx: &Ctx<'_>,
    shown: &ShownToken,
    verse_shown: &ShownToken,
    edit: &DecisionEdit,
) -> R<Vec<PlannedChange>> {
    let shown = ctx.edit(shown)?;
    let verse_shown = ctx.edit(verse_shown)?;
    let RecordKey::Check(check) = shown.key().clone() else {
        return Err(refused(Refusal::WrongRecord("a check decision")));
    };
    let displayed = match ctx.displayed(&shown)? {
        Some(Record::Check {
            value: CheckValue::Decision(d),
            ..
        }) => Some(d),
        Some(_) => {
            return Err(refused(Refusal::WrongRecord(
                "a decision, not a moved check",
            )));
        }
        None => None,
    };
    let verse_value = ctx.displayed(&verse_shown)?;
    let text = target_text(&check, verse_shown.key(), verse_value.as_ref());
    let next = edit
        .apply(displayed.as_ref(), text.as_deref())
        .map_err(|e| refused(Refusal::Text(e.to_string())))?;
    let record = Record::Check {
        key: check,
        value: CheckValue::Decision(next),
    };
    unchanged_or(ctx, &shown, record, Cause::Edit)
}

// ---------------------------------------------------------------------------------------
// §5.5 comments, §5.6 settings, details and pins
// ---------------------------------------------------------------------------------------

fn add_comment(target: &CommentTarget, text: &str) -> R<Vec<PlannedChange>> {
    let text = nfc(text);
    if text.trim().is_empty() {
        return Err(refused(Refusal::Text("a comment needs text".into())));
    }
    let id = CommentId::from_bytes(crate::random::bytes::<16>());
    Ok(vec![change(
        Record::Comment {
            id,
            value: CommentValue {
                target: target.clone(),
                text,
            },
        },
        &[],
        Cause::Edit,
    )])
}

/// A new version of the comment the box showed (R24): same id and target, the earlier text
/// kept in History as its parent.
fn revise_comment(ctx: &Ctx<'_>, shown: &ShownToken, text: &str) -> R<Vec<PlannedChange>> {
    let shown = ctx.edit(shown)?;
    let RecordKey::Comment(id) = shown.key().clone() else {
        return Err(refused(Refusal::WrongRecord("a comment")));
    };
    let Some(Record::Comment { value, .. }) = ctx.displayed(&shown)? else {
        return Err(refused(Refusal::WrongRecord("a comment that exists")));
    };
    let text = nfc(text);
    if text.trim().is_empty() {
        return Err(refused(Refusal::Text("a comment needs text".into())));
    }
    let record = Record::Comment {
        id,
        value: CommentValue {
            target: value.target,
            text,
        },
    };
    unchanged_or(ctx, &shown, record, Cause::Edit)
}

fn set_setting(ctx: &Ctx<'_>, shown: &ShownToken, value: Option<Json>) -> R<Vec<PlannedChange>> {
    let shown = ctx.edit(shown)?;
    let RecordKey::Setting(key) = shown.key().clone() else {
        return Err(refused(Refusal::WrongRecord("a setting")));
    };
    unchanged_or(ctx, &shown, Record::Setting { key, value }, Cause::Edit)
}

fn burrito_kind(kind: ProjectKind) -> tc_burrito::ProjectKind {
    match kind {
        ProjectKind::Bible => tc_burrito::ProjectKind::BibleText,
        ProjectKind::Stories => tc_burrito::ProjectKind::OpenBibleStories,
    }
}

fn set_meta(ctx: &Ctx<'_>, shown: &ShownToken, value: Option<Json>) -> R<Vec<PlannedChange>> {
    let shown = ctx.edit(shown)?;
    let RecordKey::ProjectMeta(field) = shown.key().clone() else {
        return Err(refused(Refusal::WrongRecord("a project detail")));
    };
    // A value that would make metadata.json invalid is refused in words (§3.4).
    if let Some(json) = &value {
        let as_value: serde_json::Value =
            serde_json::from_slice(&json.to_canonical()).unwrap_or(serde_json::Value::Null);
        tc_burrito::check_field(burrito_kind(ctx.kind), &field.path(), &as_value)
            .map_err(|why| refused(Refusal::NotValidInBurrito(why)))?;
    }
    unchanged_or(
        ctx,
        &shown,
        Record::ProjectMeta { field, value },
        Cause::Edit,
    )
}

// ---------------------------------------------------------------------------------------
// §5.7 books
// ---------------------------------------------------------------------------------------

fn check_scope(ctx: &Ctx<'_>, book: BookId, scope: &Scope) -> R<()> {
    let ranges: Vec<String> = match scope {
        Scope::Whole => Vec::new(),
        Scope::Ranges(ranges) => ranges.as_slice().iter().map(|r| r.to_string()).collect(),
    };
    tc_burrito::check_scope(burrito_kind(ctx.kind), book.code(), &ranges)
        .map_err(|why| refused(Refusal::NotValidInBurrito(why)))
}

/// The project's name as `\id` lines carry it: the first text of its localized name.
fn project_name(fold: &FoldState) -> String {
    match fold.projected(&RecordKey::ProjectMeta(tc_journal::MetaField::Name)) {
        Some(Record::ProjectMeta {
            value: Some(Json::Obj(map)),
            ..
        }) => map
            .values()
            .find_map(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
        Some(Record::ProjectMeta {
            value: Some(Json::Str(name)),
            ..
        }) => name,
        _ => String::new(),
    }
}

fn add_book(ctx: &Ctx<'_>, book: BookId, scope: &Scope, source: &str) -> R<Vec<PlannedChange>> {
    ctx.require_kind(ProjectKind::Bible)?;
    if !ctx.fold.heads(&RecordKey::BookPresence(book)).is_empty() {
        return Err(refused(Refusal::Book(format!(
            "{} is already in the project, or was removed: restore it instead",
            book.english_name()
        ))));
    }
    check_scope(ctx, book, scope)?;
    let seeded = crate::seed::seed_book(
        source,
        book.code(),
        book.english_name(),
        &project_name(ctx.fold),
    )
    .ok_or_else(|| {
        refused(Refusal::Book(
            "the source book has no chapters or verses to copy".into(),
        ))
    })?;
    let parts = tc_usfm::codec::decompose(&seeded)
        .map_err(|e| refused(Refusal::Book(format!("the seeded book does not read: {e}"))))?;
    let mut out = vec![change(
        Record::BookPresence {
            book,
            value: BookPresence::Present {
                scope: scope.clone(),
                how: PresentHow::Added,
            },
        },
        &[],
        Cause::Seed,
    )];
    for record in tc_journal::fold::records_from_usfm(book, &parts) {
        out.push(change(record, &[], Cause::Seed));
    }
    Ok(out)
}

fn presence_token(ctx: &Ctx<'_>, book: BookId, shown: &ShownToken) -> R<ShownToken> {
    let shown = ctx.edit(shown)?;
    if *shown.key() != RecordKey::BookPresence(book) {
        return Err(refused(Refusal::WrongRecord("the book's presence")));
    }
    Ok(shown)
}

fn remove_book(ctx: &Ctx<'_>, book: BookId, shown: &ShownToken) -> R<Vec<PlannedChange>> {
    let shown = presence_token(ctx, book, shown)?;
    if ctx.kind == ProjectKind::Bible {
        let others = ctx
            .fold
            .projected_books()
            .into_iter()
            .filter(|b| *b != book)
            .count();
        if others == 0 {
            return Err(refused(Refusal::LastBook));
        }
    }
    Ok(vec![change(
        Record::BookPresence {
            book,
            value: BookPresence::Removed {
                seen: ctx.fold.book_heads_seen(book),
            },
        },
        shown.heads(),
        Cause::Edit,
    )])
}

/// The scope of the newest `Present` value of a book's presence history.
fn last_scope(fold: &FoldState, book: BookId) -> Scope {
    let key = RecordKey::BookPresence(book);
    for id in fold.history(&key).iter().rev() {
        if let Some(Record::BookPresence {
            value: BookPresence::Present { scope, .. },
            ..
        }) = fold.body(id).map(|b| b.record)
        {
            return scope;
        }
    }
    Scope::Whole
}

fn restore_book(
    ctx: &Ctx<'_>,
    book: BookId,
    shown: &ShownToken,
    mode: RestoreMode,
) -> R<Vec<PlannedChange>> {
    let shown = presence_token(ctx, book, shown)?;
    if ctx.fold.book(book).projects() {
        return Err(refused(Refusal::Book(format!(
            "{} is in the project",
            book.english_name()
        ))));
    }
    let scope = last_scope(ctx.fold, book);
    let how = match mode {
        RestoreMode::KeepText => PresentHow::RestoredText,
        RestoreMode::StartEmpty => PresentHow::StartedEmpty,
    };
    let mut out = vec![change(
        Record::BookPresence {
            book,
            value: BookPresence::Present { scope, how },
        },
        shown.heads(),
        Cause::Edit,
    )];
    if mode == RestoreMode::StartEmpty {
        for chapter in ctx.fold.chapters_of(book) {
            for key in ctx.fold.chapter_keys(book, chapter).verses {
                let Some(Record::Verse {
                    chapter,
                    verse,
                    value:
                        VerseValue::Text {
                            marker,
                            layout,
                            last,
                            from,
                            ..
                        },
                    ..
                }) = ctx.fold.projected(&key)
                else {
                    continue;
                };
                out.push(change(
                    Record::Verse {
                        book,
                        chapter,
                        verse,
                        value: VerseValue::Text {
                            marker,
                            words: "___".into(),
                            layout,
                            last,
                            from,
                        },
                    },
                    &ctx.fold.heads(&key),
                    Cause::StartOver,
                ));
            }
        }
    }
    Ok(out)
}

fn set_book_scope(ctx: &Ctx<'_>, shown: &ShownToken, scope: &Scope) -> R<Vec<PlannedChange>> {
    let shown = ctx.edit(shown)?;
    let RecordKey::BookPresence(book) = shown.key().clone() else {
        return Err(refused(Refusal::WrongRecord("a book's presence")));
    };
    let Some(Record::BookPresence {
        value: BookPresence::Present { how, .. },
        ..
    }) = ctx.displayed(&shown)?
    else {
        return Err(refused(Refusal::Book(
            "the book is not in the project".into(),
        )));
    };
    check_scope(ctx, book, scope)?;
    let record = Record::BookPresence {
        book,
        value: BookPresence::Present {
            scope: scope.clone(),
            how,
        },
    };
    unchanged_or(ctx, &shown, record, Cause::Edit)
}

// ---------------------------------------------------------------------------------------
// §5.15 OBS frames
// ---------------------------------------------------------------------------------------

fn save_frame(ctx: &Ctx<'_>, shown: &ShownToken, text: &str) -> R<Vec<PlannedChange>> {
    ctx.require_kind(ProjectKind::Stories)?;
    let shown = ctx.edit(shown)?;
    let text = nfc(text);
    let record = match shown.key().clone() {
        RecordKey::Frame(story, frame) => {
            let problem = if frame.0 == 0 {
                tc_obs::story::title_text_problem(&text)
            } else {
                tc_obs::story::frame_text_problem(&text)
            };
            if let Some(problem) = problem {
                return Err(refused(Refusal::Text(problem.to_string())));
            }
            Record::Frame {
                story,
                frame,
                value: text,
            }
        }
        RecordKey::StoryRef(story) => {
            if let Some(problem) = tc_obs::story::reference_text_problem(&text) {
                return Err(refused(Refusal::Text(problem.to_string())));
            }
            Record::StoryRef {
                story,
                value: (!text.is_empty()).then_some(text),
            }
        }
        _ => {
            return Err(refused(Refusal::WrongRecord(
                "a frame or a story reference",
            )));
        }
    };
    unchanged_or(ctx, &shown, record, Cause::Edit)
}

// ---------------------------------------------------------------------------------------
// §5.8 upgrades, §5.9 resolutions and restores
// ---------------------------------------------------------------------------------------

/// §5.8 step 3: every record the plan touches still has the heads it had in the snapshot the
/// plan was made from; otherwise the plan is made again.
fn apply_upgrade(ctx: &Ctx<'_>, plan: &UpgradePlan) -> R<Vec<PlannedChange>> {
    let mut keys = BTreeSet::new();
    for planned in &plan.changes {
        let key = planned.record.key();
        let mut want = planned.parents.clone();
        want.sort();
        want.dedup();
        if ctx.fold.heads(&key) != want || !keys.insert(key) {
            return Err(CommandError::PlanStale);
        }
    }
    Ok(plan.changes.clone())
}

/// The records of a conflict: its key and its other keys.
fn conflict_keys(fold: &FoldState, id: &ConflictId) -> Option<Vec<RecordKey>> {
    let conflict = fold.conflict(id)?;
    Some(
        std::iter::once(conflict.key.clone())
            .chain(conflict.also_keys.iter().cloned())
            .collect(),
    )
}

/// §5.9 step 1: one review token per record of the conflict, each listing every current
/// head; otherwise "A newer change just arrived. Look again."
fn check_review_tokens(ctx: &Ctx<'_>, id: &ConflictId, shown: &[ShownToken]) -> R<()> {
    let keys = conflict_keys(ctx.fold, id).ok_or(CommandError::LookAgain)?;
    let mut tokens: BTreeMap<RecordKey, ShownToken> = BTreeMap::new();
    for token in shown {
        let token = ctx.verified(token, TokenKind::Review)?;
        tokens.insert(token.key().clone(), token);
    }
    for key in keys {
        let token = tokens.get(&key).ok_or(CommandError::LookAgain)?;
        if token.heads() != ctx.fold.heads(&key).as_slice() {
            return Err(CommandError::LookAgain);
        }
    }
    Ok(())
}

fn resolve_error(e: tc_journal::fold::ResolveError) -> CommandError {
    match e {
        tc_journal::fold::ResolveError::UnknownConflict => CommandError::LookAgain,
        other => refused(Refusal::Resolve(other.to_string())),
    }
}

fn resolve(
    ctx: &Ctx<'_>,
    id: &ConflictId,
    shown: &[ShownToken],
    choice: &Choice,
) -> R<Vec<PlannedChange>> {
    check_review_tokens(ctx, id, shown)?;
    ctx.fold.resolution(id, choice).map_err(resolve_error)
}

/// A bulk choice (§5.9 step 3), one batch. A conflict whose tokens are stale is left out,
/// with those the fold leaves for a choice of their own.
fn resolve_bulk(
    ctx: &Ctx<'_>,
    devices: &BTreeSet<tc_journal::DeviceId>,
    conflicts: &[(ConflictId, Vec<ShownToken>)],
) -> R<Plan> {
    let mut fresh = Vec::new();
    let mut skipped = Vec::new();
    for (id, shown) in conflicts {
        match check_review_tokens(ctx, id, shown) {
            Ok(()) => fresh.push(*id),
            Err(CommandError::LookAgain) => skipped.push(*id),
            Err(e) => return Err(e),
        }
    }
    let (resolved, left) = ctx.fold.bulk_resolution(devices, &fresh);
    skipped.extend(left);
    let mut seen = BTreeSet::new();
    let mut changes = Vec::new();
    for (id, planned) in resolved {
        if planned.iter().any(|p| seen.contains(&p.record.key())) {
            skipped.push(id);
            continue;
        }
        for p in planned {
            seen.insert(p.record.key());
            changes.push(p);
        }
    }
    Ok(Plan { changes, skipped })
}

/// Re-keys a decision (R7): `MovedTo` on the old key, the decision on the new one, each with
/// the heads its token lists.
fn move_check(
    ctx: &Ctx<'_>,
    shown: &ShownToken,
    to: &CheckKey,
    to_shown: &ShownToken,
) -> R<Vec<PlannedChange>> {
    let shown = ctx.edit(shown)?;
    let to_shown = ctx.edit(to_shown)?;
    let RecordKey::Check(from) = shown.key().clone() else {
        return Err(refused(Refusal::WrongRecord("a check decision")));
    };
    if *to_shown.key() != RecordKey::Check(to.clone()) || from == *to {
        return Err(refused(Refusal::WrongRecord("the check it moves to")));
    }
    let Some(Record::Check {
        value: CheckValue::Decision(decision),
        ..
    }) = ctx.displayed(&shown)?
    else {
        return Err(refused(Refusal::WrongRecord("a decision")));
    };
    Ok(vec![
        change(
            Record::Check {
                key: from,
                value: CheckValue::MovedTo { to: to.clone() },
            },
            shown.heads(),
            Cause::Edit,
        ),
        change(
            Record::Check {
                key: to.clone(),
                value: CheckValue::Decision(decision),
            },
            to_shown.heads(),
            Cause::Edit,
        ),
    ])
}

fn restore(ctx: &Ctx<'_>, shown: &ShownToken, from: ChangeId) -> R<Vec<PlannedChange>> {
    let shown = ctx.edit(shown)?;
    let body = ctx.fold.body(&from).ok_or(refused(Refusal::Unreadable))?;
    if body.record.key() != *shown.key() {
        return Err(refused(Refusal::WrongRecord("a version of this record")));
    }
    Ok(vec![change(
        body.record,
        shown.heads(),
        Cause::Restore { from },
    )])
}

fn release_batch(ctx: &Ctx<'_>, batch: BatchId) -> R<Vec<PlannedChange>> {
    let held = ctx.fold.anomalies().any(
        |a| matches!(a, tc_journal::fold::Anomaly::IncompleteBatch { batch: b, .. } if *b == batch),
    );
    let changes = ctx.fold.batch_changes(batch);
    if !held || changes.is_empty() {
        return Err(refused(Refusal::NothingToRelease));
    }
    Ok(vec![change(
        Record::BatchRelease {
            batch,
            value: BatchReleaseValue { changes },
        },
        &[],
        Cause::Edit,
    )])
}

fn dismiss(ctx: &Ctx<'_>, shown: &ShownToken) -> R<Vec<PlannedChange>> {
    let shown = ctx.edit(shown)?;
    let record = match ctx.displayed(&shown)? {
        Some(Record::Alignment {
            book,
            chapter,
            verse,
            mut value,
        }) => {
            value.dismissed = true;
            Record::Alignment {
                book,
                chapter,
                verse,
                value,
            }
        }
        Some(Record::Check {
            key,
            value: CheckValue::Decision(mut decision),
        }) => {
            decision.dismissed = true;
            Record::Check {
                key,
                value: CheckValue::Decision(decision),
            }
        }
        _ => return Err(refused(Refusal::NotDismissable)),
    };
    unchanged_or(ctx, &shown, record, Cause::Edit)
}

// ---------------------------------------------------------------------------------------
// §5.14 drafts
// ---------------------------------------------------------------------------------------

/// *Keep* a draft offered at open: the save its field makes, with its tokens. A token this
/// installation cannot verify any more (R20) is no base: the value is the record's current
/// value with the draft's text in it, written with no parents, so it stands as a conflict
/// with what the record holds unless the two are equal.
fn keep_draft(
    ctx: &Ctx<'_>,
    draft: &Draft,
    section: &[(ShownToken, Option<SectionValue>)],
) -> R<Vec<PlannedChange>> {
    let verifies = |t: &ShownToken| ShownToken::parse(ctx.signing, &t.to_text()).is_ok();
    let verified = draft.shown.iter().all(verifies);
    if let DraftField::Section { book } = &draft.field {
        let verified = verified && section.iter().all(|(t, _)| verifies(t));
        return save_section(ctx, *book, section, None, !verified);
    }
    if verified {
        let token = draft.shown.first();
        let command = match (&draft.field, token) {
            (DraftField::Words, Some(t)) => Command::SaveVerse {
                shown: t.clone(),
                words: draft.text.clone(),
            },
            (DraftField::Layout, Some(t)) => Command::SaveLayout {
                shown: t.clone(),
                layout: draft.text.clone(),
            },
            (DraftField::Frame | DraftField::StoryRef, Some(t)) => Command::SaveFrame {
                shown: t.clone(),
                text: draft.text.clone(),
            },
            (DraftField::UserComment { .. }, Some(t)) => Command::ReviseComment {
                shown: t.clone(),
                text: draft.text.clone(),
            },
            (DraftField::UserComment { target }, None) => Command::AddComment {
                target: target.clone(),
                text: draft.text.clone(),
            },
            (DraftField::CheckComment, Some(t)) => {
                return keep_check_comment(ctx, t, &draft.text, true);
            }
            (DraftField::Alignment, Some(_)) => {
                return keep_alignment(ctx, &draft.shown, &draft.text, true);
            }
            _ => return Err(refused(Refusal::WrongRecord("a draft with its token"))),
        };
        return Ok(plan(ctx, &command)?.changes);
    }
    // R20: unverifiable. What the token says is read, never trusted as a base.
    let token = draft
        .shown
        .first()
        .ok_or(refused(Refusal::WrongRecord("a draft with its token")))?;
    let key = token.key().clone();
    let current = ctx.fold.projected(&key);
    let text = nfc(&draft.text);
    let record = match (&draft.field, key.clone(), current) {
        (
            DraftField::Words | DraftField::Layout,
            RecordKey::Verse(book, chapter, verse),
            Some(Record::Verse {
                value:
                    VerseValue::Text {
                        marker,
                        words,
                        layout,
                        last,
                        from,
                    },
                ..
            }),
        ) => {
            let (words, layout) = match draft.field {
                DraftField::Words => (text, layout),
                _ => (words, text),
            };
            Record::Verse {
                book,
                chapter,
                verse,
                value: VerseValue::Text {
                    marker,
                    words,
                    layout,
                    last,
                    from,
                },
            }
        }
        (DraftField::Frame, RecordKey::Frame(story, frame), _) => Record::Frame {
            story,
            frame,
            value: text,
        },
        (DraftField::StoryRef, RecordKey::StoryRef(story), _) => Record::StoryRef {
            story,
            value: (!text.is_empty()).then_some(text),
        },
        (DraftField::UserComment { target }, RecordKey::Comment(id), _) => Record::Comment {
            id,
            value: CommentValue {
                target: target.clone(),
                text,
            },
        },
        (DraftField::CheckComment, RecordKey::Check(_), _) => {
            return keep_check_comment(ctx, token, &draft.text, false);
        }
        (DraftField::Alignment, RecordKey::Alignment(..), _) => {
            return keep_alignment(ctx, &draft.shown, &draft.text, false);
        }
        _ => return Err(refused(Refusal::WrongRecord("a draft its record can take"))),
    };
    Ok(vec![change(record, &[], Cause::Edit)])
}

fn keep_check_comment(
    ctx: &Ctx<'_>,
    token: &ShownToken,
    text: &str,
    verified: bool,
) -> R<Vec<PlannedChange>> {
    let RecordKey::Check(key) = token.key().clone() else {
        return Err(refused(Refusal::WrongRecord("a check")));
    };
    let base = if verified {
        ctx.displayed(token)?
    } else {
        ctx.fold.projected(token.key())
    };
    let Some(Record::Check {
        value: CheckValue::Decision(mut decision),
        ..
    }) = base
    else {
        return Err(refused(Refusal::WrongRecord("a decision")));
    };
    let text = nfc(text);
    decision.comment = (!text.is_empty()).then_some(text);
    let record = Record::Check {
        key,
        value: CheckValue::Decision(decision),
    };
    if verified {
        unchanged_or(ctx, token, record, Cause::Edit)
    } else {
        Ok(vec![change(record, &[], Cause::Edit)])
    }
}

/// *Keep* an aligner's draft (R23): the links as an alignment save with the draft's two
/// tokens, the alignment's and the verse's it displayed, so a first alignment of a verse is
/// kept as well as an edited one. An unverifiable draft (R20) is stamped from what the
/// records hold now and written with no parents. A draft with the alignment's token alone
/// (an older draft) keeps the links on the alignment it showed.
fn keep_alignment(
    ctx: &Ctx<'_>,
    tokens: &[ShownToken],
    text: &str,
    verified: bool,
) -> R<Vec<PlannedChange>> {
    let token = tokens
        .first()
        .ok_or(refused(Refusal::WrongRecord("a draft with its token")))?;
    let edit = AlignmentEdit::from_json(text).ok_or(refused(Refusal::Text(
        "the draft's links do not read".into(),
    )))?;
    match (tokens.get(1), verified) {
        (Some(verse_token), true) => return save_alignment(ctx, token, verse_token, &edit),
        (Some(verse_token), false) => {
            let displayed = match ctx.fold.projected(token.key()) {
                Some(Record::Alignment { value, .. }) => Some(value),
                _ => None,
            };
            let record = alignment_record(
                ctx,
                token.key(),
                verse_token.key(),
                ctx.fold.projected(verse_token.key()),
                displayed,
                &edit,
            )?;
            return Ok(vec![change(record, &[], Cause::Edit)]);
        }
        (None, _) => {}
    }
    let RecordKey::Alignment(book, chapter, verse) = token.key().clone() else {
        return Err(refused(Refusal::WrongRecord("an alignment")));
    };
    let base = if verified {
        ctx.displayed(token)?
    } else {
        ctx.fold.projected(token.key())
    };
    let Some(Record::Alignment { mut value, .. }) = base else {
        return Err(refused(Refusal::WrongRecord(
            "an alignment that exists, or the aligned verse's token",
        )));
    };
    value.alignments = edit.alignments;
    value.word_bank = edit.word_bank;
    value.done = edit.done;
    let record = Record::Alignment {
        book,
        chapter,
        verse,
        value,
    };
    if verified {
        unchanged_or(ctx, token, record, Cause::Edit)
    } else {
        Ok(vec![change(record, &[], Cause::Edit)])
    }
}
