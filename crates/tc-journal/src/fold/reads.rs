//! What screens, commands and projections read from a fold.

use super::attach::{self, Place};
use super::meta::{ChangeIdx, KeyIdx};
use super::state::{BookStatus, FoldState, Local, Projected};
use super::{Anomaly, AutoRule, Conflict, ConflictId, ConflictKind, KeyState, Resolved};
use crate::change::ChangeBody;
use crate::ids::{BatchId, ChangeId, DeviceId, DeviceSeq, PackId, WallTime};
use crate::keys::RecordKey;
use crate::record::Record;
use crate::token::{ShownToken, TokenError, TokenKey};
use std::collections::BTreeMap;
use tc_core::BookId;

/// One change the fold holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeView {
    pub id: ChangeId,
    pub key: RecordKey,
    pub parents: Vec<ChangeId>,
    pub device: DeviceId,
    pub seq: DeviceSeq,
    pub at: WallTime,
    pub batch: BatchId,
    /// Folded: its batch is complete, or a release lists it (§7.4).
    pub folded: bool,
    /// The packs that hold it; empty for a change only in a pending file.
    pub packs: Vec<PackId>,
}

/// How a book projects (§7.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookView {
    /// No folded presence change mentions it (its records, if any, are listed as
    /// `RecordsWithoutBook`).
    Unknown,
    Present {
        scope: ChangeId,
    },
    Removed,
    /// Removed, but kept projected because the removal would hide unseen work.
    RemovedWhileEdited {
        scope: Option<ChangeId>,
        unseen: u32,
    },
    /// The last book of a Bible project, kept projected.
    KeptAsLast {
        scope: Option<ChangeId>,
    },
}

impl BookView {
    pub fn projects(&self) -> bool {
        matches!(
            self,
            Self::Present { .. } | Self::RemovedWhileEdited { .. } | Self::KeptAsLast { .. }
        )
    }
}

/// Where an attached record (an alignment or a comment) shows (§7.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// At the run that starts at this chapter and verse.
    Run { chapter: u16, verse: u16 },
    /// Under Detached.
    Detached,
    /// Not an attached record of a projected book.
    NotAttached,
}

impl FoldState {
    fn key_idx(&self, key: &RecordKey) -> Option<KeyIdx> {
        self.key_ids.get(key).copied()
    }

    /// How many changes the fold holds, folded or held.
    pub fn len(&self) -> usize {
        self.metas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.metas.len() == 0
    }

    /// Every record key with at least one change, in key order.
    pub fn record_keys(&self) -> Vec<RecordKey> {
        let mut keys: Vec<RecordKey> = self
            .keys
            .iter()
            .filter(|e| e.active > 0)
            .map(|e| e.key.clone())
            .collect();
        keys.sort();
        keys
    }

    /// A record's heads and resolution; `None` when no folded change has its key.
    pub fn key_state(&self, key: &RecordKey) -> Option<KeyState> {
        let k = self.key_idx(key)?;
        let entry = self.entry(k);
        if entry.heads.is_empty() {
            return None;
        }
        let heads: Vec<ChangeId> = entry.heads.iter().map(|h| self.meta(*h).id).collect();
        let state = if let Some(g) = entry.group {
            Resolved::Conflict(
                self.groups
                    .get(&g)
                    .map_or_else(|| ConflictId::of(&heads), |group| group.conflict.id),
            )
        } else {
            match &entry.local {
                Local::Empty => return None,
                Local::Settled(c) => Resolved::Settled {
                    head: self.meta(*c).id,
                },
                Local::ByRule(c) => Resolved::SettledByRule {
                    head: self.meta(*c).id,
                    rule: AutoRule::SeedStubs,
                },
                Local::Merged(record, how) => Resolved::Merged {
                    merged: Box::new((**record).clone()),
                    how: how.clone(),
                },
                Local::Conflict { id, .. } => Resolved::Conflict(*id),
                Local::Passage => Resolved::Conflict(ConflictId::of(&heads)),
            }
        };
        Some(KeyState { heads, state })
    }

    /// The heads of a record, sorted.
    pub fn heads(&self, key: &RecordKey) -> Vec<ChangeId> {
        self.key_idx(key).map_or_else(Vec::new, |k| {
            self.entry(k)
                .heads
                .iter()
                .map(|h| self.meta(*h).id)
                .collect()
        })
    }

    /// The value projection uses for a record: its settled head, its merged value, or its
    /// provisional value while it is in conflict (§7.5).
    pub fn projected(&self, key: &RecordKey) -> Option<Record> {
        let k = self.key_idx(key)?;
        if let Some(shown) = self.shown_after_restore(k) {
            return match shown {
                attach::ShownVerse::Change(_, c) => super::record_of(self, c),
                attach::ShownVerse::Merged(_, record) => Some((*record).clone()),
            };
        }
        match &self.entry(k).projected {
            Projected::Nothing => None,
            Projected::Change(c) => super::record_of(self, *c),
            Projected::Merged(record) => Some((**record).clone()),
        }
    }

    /// The change a record's projection comes from, when it is one change.
    pub fn projected_change(&self, key: &RecordKey) -> Option<ChangeId> {
        let k = self.key_idx(key)?;
        if let Some(shown) = self.shown_after_restore(k) {
            return match shown {
                attach::ShownVerse::Change(_, c) => Some(self.meta(c).id),
                attach::ShownVerse::Merged(..) => None,
            };
        }
        match &self.entry(k).projected {
            Projected::Change(c) => Some(self.meta(*c).id),
            _ => None,
        }
    }

    /// For a verse whose projected change is a `StartOver` of a *Start empty* that lost
    /// the presence race to *Restore the text*: what the projection shows instead (§7.6).
    fn shown_after_restore(&self, k: KeyIdx) -> Option<attach::ShownVerse> {
        let RecordKey::Verse(book, ..) = &self.entry(k).key else {
            return None;
        };
        let Projected::Change(c) = &self.entry(k).projected else {
            return None;
        };
        let batch = self.batch(self.meta(*c).batch).id;
        let rewound = super::books::rewound_batches(self, *book);
        if !rewound.contains(&batch) {
            return None;
        }
        attach::shown_verse(self, k, &rewound)
    }

    /// The conflict list, sorted by key: canon book order, then the key, then the kind.
    pub fn conflicts(&self) -> impl Iterator<Item = &Conflict> {
        self.conflicts.values().map(|c| &**c)
    }

    pub fn conflict(&self, id: &ConflictId) -> Option<&Conflict> {
        let sort = self.conflict_ids.get(id)?;
        self.conflicts.get(sort).map(|c| &**c)
    }

    /// How many conflicts the list holds, by kind (the kind's order, §8.1).
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }

    /// Conflict counts per book and kind (`None` for records of no book), kept up to date by
    /// every step: the Home card and the book tiles.
    pub fn conflict_counts(&self) -> &BTreeMap<(Option<BookId>, u8), u32> {
        &self.counts
    }

    /// The anomalies, sorted by kind, then key.
    pub fn anomalies(&self) -> impl Iterator<Item = &Anomaly> {
        self.anomalies.iter()
    }

    /// Records with no place on any screen (§7.7), sorted.
    pub fn detached(&self) -> impl Iterator<Item = &RecordKey> {
        self.detached.iter()
    }

    pub fn change(&self, id: &ChangeId) -> Option<ChangeView> {
        let c = *self.ids.get(id)?;
        Some(self.view(c))
    }

    pub(crate) fn view(&self, c: ChangeIdx) -> ChangeView {
        let meta = self.meta(c);
        ChangeView {
            id: meta.id,
            key: self.entry(meta.key).key.clone(),
            parents: meta.parents.to_vec(),
            device: meta.device,
            seq: meta.seq,
            at: meta.at,
            batch: self.batch(meta.batch).id,
            folded: meta.active,
            packs: meta.packs.to_vec(),
        }
    }

    /// A change's body, decoded from its stored bytes.
    pub fn body(&self, id: &ChangeId) -> Option<ChangeBody> {
        let c = *self.ids.get(id)?;
        super::meta::decode(&self.meta(c).raw)
    }

    /// A change's canonical bytes, exactly as stored.
    pub fn raw(&self, id: &ChangeId) -> Option<std::sync::Arc<[u8]>> {
        let c = *self.ids.get(id)?;
        Some(self.meta(c).raw.clone())
    }

    /// Every folded version of a record, oldest first by (at, id): its History.
    pub fn history(&self, key: &RecordKey) -> Vec<ChangeId> {
        let Some(k) = self.key_idx(key) else {
            return Vec::new();
        };
        let mut out: Vec<(WallTime, ChangeId)> = self
            .entry(k)
            .changes
            .iter()
            .filter(|c| self.meta(**c).active)
            .map(|c| (self.meta(*c).at, self.meta(*c).id))
            .collect();
        out.sort();
        out.into_iter().map(|(_, id)| id).collect()
    }

    /// The heads a shown token lists for a record (§2.7): every head of a settled or merged
    /// record; for a record in conflict, only the displayed candidate (the provisional one
    /// unless `displayed` names another head) and the heads with its identical value.
    pub fn shown_heads(&self, key: &RecordKey, displayed: Option<ChangeId>) -> Vec<ChangeId> {
        let Some(k) = self.key_idx(key) else {
            return Vec::new();
        };
        let entry = self.entry(k);
        let in_conflict =
            entry.group.is_some() || matches!(entry.local, Local::Conflict { .. } | Local::Passage);
        if !in_conflict {
            return entry.heads.iter().map(|h| self.meta(*h).id).collect();
        }
        let shown = displayed
            .and_then(|id| self.ids.get(&id).copied())
            .filter(|c| entry.heads.contains(c))
            .or_else(|| match &entry.projected {
                Projected::Change(c) if entry.heads.contains(c) => Some(*c),
                _ => entry
                    .heads
                    .iter()
                    .copied()
                    .max_by_key(|h| self.meta(*h).order()),
            });
        let Some(shown) = shown else {
            return Vec::new();
        };
        let value = super::value_of(self, shown);
        let mut out: Vec<ChangeId> = entry
            .heads
            .iter()
            .copied()
            .filter(|h| *h == shown || super::value_of(self, *h) == value)
            .map(|h| self.meta(h).id)
            .collect();
        out.sort();
        out
    }

    /// A token for a record as a screen displays it.
    pub fn token(
        &self,
        signing: &TokenKey,
        key: &RecordKey,
        displayed: Option<ChangeId>,
    ) -> Result<ShownToken, TokenError> {
        ShownToken::mint(signing, key.clone(), self.shown_heads(key, displayed))
    }

    /// How a book projects.
    pub fn book(&self, book: BookId) -> BookView {
        match self.books.get(&book).map(|b| &b.status) {
            None | Some(BookStatus::Unknown) => BookView::Unknown,
            Some(BookStatus::Present { scope }) => BookView::Present {
                scope: self.meta(*scope).id,
            },
            Some(BookStatus::Removed) => BookView::Removed,
            Some(BookStatus::RemovedWhileEdited { scope, unseen }) => {
                BookView::RemovedWhileEdited {
                    scope: scope.map(|s| self.meta(s).id),
                    unseen: *unseen,
                }
            }
            Some(BookStatus::KeptAsLast { scope }) => BookView::KeptAsLast {
                scope: scope.map(|s| self.meta(s).id),
            },
        }
    }

    /// Every book with a record, and how it projects, in canon order.
    pub fn books(&self) -> Vec<(BookId, BookView)> {
        self.books.keys().map(|b| (*b, self.book(*b))).collect()
    }

    /// The books that project, in canon order.
    pub fn projected_books(&self) -> Vec<BookId> {
        self.books
            .iter()
            .filter(|(_, b)| b.status.projects())
            .map(|(id, _)| *id)
            .collect()
    }

    /// Where an alignment or a verse comment shows (§7.7).
    pub fn placement(&self, key: &RecordKey) -> Placement {
        let (book, anchor) = match key {
            RecordKey::Alignment(book, c, v) => (*book, (c.get(), v.get())),
            RecordKey::Comment(_) => match self.projected(key) {
                Some(Record::Comment { value, .. }) => match value.target {
                    crate::record::CommentTarget::Verse {
                        book,
                        chapter,
                        verse,
                    } => (book, (chapter.get(), verse.get())),
                    _ => return Placement::NotAttached,
                },
                _ => return Placement::NotAttached,
            },
            _ => return Placement::NotAttached,
        };
        if !self.book(book).projects() {
            return Placement::NotAttached;
        }
        match attach::place(self, book, anchor).0 {
            Place::Run(chapter, verse) => Placement::Run { chapter, verse },
            Place::Nowhere => Placement::Detached,
        }
    }

    /// Conflicts of a kind, for the review's tabs.
    pub fn conflicts_of(&self, kind: ConflictKind) -> impl Iterator<Item = &Conflict> {
        self.conflicts()
            .filter(move |c| c.kind.order() == kind.order())
    }
}
