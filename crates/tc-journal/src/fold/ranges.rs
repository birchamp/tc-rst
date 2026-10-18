//! Range reads (APP-DESIGN R1): one chapter's records, one (tool, book)'s decisions, one
//! book's alignments and comments, each found from the fold's per-chapter and per-book
//! lists, so a read costs what it returns, never the size of the project. And the facts a
//! store reads of the whole fold once, at open: each device's largest sequence number and
//! newest time.

use super::meta::{ChangeIdx, KeyIdx};
use super::state::FoldState;
use crate::ids::{BatchId, ChangeId, DeviceId, DeviceSeq, ShortId, WallTime};
use crate::keys::{RecordKey, ToolId};
use std::collections::BTreeMap;
use tc_core::BookId;

/// One chapter's records, as the fold files them. Only records with a folded change are
/// listed; each list is in key order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChapterKeys {
    pub front: Option<RecordKey>,
    /// The chapter's verse records.
    pub verses: Vec<RecordKey>,
    /// Alignments keyed by an anchor in this chapter (§7.7).
    pub alignments: Vec<RecordKey>,
    /// Comments on a verse anchor in this chapter, or on a check whose target is in it.
    pub comments: Vec<RecordKey>,
    /// Decisions whose target chapter is this chapter.
    pub checks: Vec<RecordKey>,
}

/// What the fold holds of one device's changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceStat {
    /// The largest sequence number among its changes.
    pub max_seq: DeviceSeq,
    /// The newest `at` among its changes.
    pub max_at: WallTime,
    pub changes: u64,
}

impl FoldState {
    fn keys_listed(&self, keys: impl IntoIterator<Item = KeyIdx>) -> Vec<RecordKey> {
        let mut out: Vec<RecordKey> = keys
            .into_iter()
            .filter(|k| self.entry(*k).active > 0)
            .map(|k| self.entry(k).key.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The chapters of `book` that hold a chapter front or a verse record, ascending.
    pub fn chapters_of(&self, book: BookId) -> Vec<u16> {
        self.books
            .get(&book)
            .map(|b| b.chapters.iter().copied().collect())
            .unwrap_or_default()
    }

    /// One chapter's records (R1): what the editor, the aligner and the Check screen show of
    /// a chapter, found without looking at any other chapter.
    pub fn chapter_keys(&self, book: BookId, chapter: u16) -> ChapterKeys {
        let Some(entry) = self.chapters.get(&(book, chapter)) else {
            return ChapterKeys::default();
        };
        ChapterKeys {
            front: entry
                .front
                .filter(|k| self.entry(*k).active > 0)
                .map(|k| self.entry(k).key.clone()),
            verses: self.keys_listed(entry.verses.values().copied()),
            alignments: self.keys_listed(entry.alignments.values().copied()),
            comments: self.keys_listed(entry.comments.iter().copied()),
            checks: self.keys_listed(entry.checks.iter().copied()),
        }
    }

    /// Every decision record of `tool` on `book` (R1), by its target: the book's chapters'
    /// decisions and those whose target chapter is not a number.
    pub fn decisions_of(&self, tool: ToolId, book: BookId) -> Vec<RecordKey> {
        let Some(entry) = self.books.get(&book) else {
            return Vec::new();
        };
        let of_tool =
            |k: &KeyIdx| matches!(&self.entry(*k).key, RecordKey::Check(c) if c.tool == tool);
        let in_chapters = entry.chapters_with_records().flat_map(|c| {
            self.chapters
                .get(&(book, c))
                .map(|e| e.checks.clone())
                .unwrap_or_default()
        });
        let keys: Vec<KeyIdx> = in_chapters
            .chain(entry.other_checks.iter().copied())
            .filter(of_tool)
            .collect();
        self.keys_listed(keys)
    }

    /// Every alignment record of `book` (R1).
    pub fn alignments_of(&self, book: BookId) -> Vec<RecordKey> {
        let Some(entry) = self.books.get(&book) else {
            return Vec::new();
        };
        let keys: Vec<KeyIdx> = entry
            .chapters_with_records()
            .flat_map(|c| {
                self.chapters
                    .get(&(book, c))
                    .map(|e| e.alignments.values().copied().collect::<Vec<_>>())
                    .unwrap_or_default()
            })
            .collect();
        self.keys_listed(keys)
    }

    /// Every comment record of `book` (R1): on its verses and on its checks.
    pub fn comments_of(&self, book: BookId) -> Vec<RecordKey> {
        let Some(entry) = self.books.get(&book) else {
            return Vec::new();
        };
        let keys: Vec<KeyIdx> = entry
            .chapters_with_records()
            .flat_map(|c| {
                self.chapters
                    .get(&(book, c))
                    .map(|e| e.comments.clone())
                    .unwrap_or_default()
            })
            .chain(entry.other_comments.iter().copied())
            .collect();
        self.keys_listed(keys)
    }

    /// Every record of `book` (§7.6): its text, layout, alignments, checks and comments.
    pub fn book_keys(&self, book: BookId) -> Vec<RecordKey> {
        self.books
            .get(&book)
            .map(|b| self.keys_listed(b.keys.iter().copied()))
            .unwrap_or_default()
    }

    /// The short ids of every head of every record of `book` but its presence: what a
    /// removal holds as seen
    /// (§5.7), sorted, without repeats.
    pub fn book_heads_seen(&self, book: BookId) -> Vec<ShortId> {
        let mut seen: Vec<ShortId> = self
            .books
            .get(&book)
            .map(|b| {
                b.keys
                    .iter()
                    .filter(|k| Some(**k) != b.presence)
                    .flat_map(|k| self.entry(*k).heads.iter())
                    .map(|h| self.meta(*h).id.short())
                    .collect()
            })
            .unwrap_or_default();
        seen.sort();
        seen.dedup();
        seen
    }

    /// The changes of batch `batch` the fold holds, folded or held, in id order: what
    /// *Apply what was recovered* lists (§7.4).
    pub fn batch_changes(&self, batch: BatchId) -> Vec<ChangeId> {
        let mut out: Vec<ChangeId> = self
            .batch_ids
            .find_all(super::state::batch_hash(&batch), |b| {
                self.batches[b as usize].id == batch
            })
            .into_iter()
            .flat_map(|b| self.members(super::meta::BatchIdx(b)).to_vec())
            .map(|c: ChangeIdx| self.meta(c).id)
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Each device's largest sequence number and newest time, over every change the fold
    /// holds (§6.3: the next seq; the ledger check; `ClockAhead`). One pass over the change
    /// table: an open reads it once.
    pub fn device_stats(&self) -> BTreeMap<DeviceId, DeviceStat> {
        let mut by_index: BTreeMap<usize, DeviceStat> = BTreeMap::new();
        for c in 0..self.metas.len() {
            let c = ChangeIdx(c as u32);
            let meta = self.meta(c);
            let seq = self.seq_of(c);
            let stat = by_index
                .entry(meta.device.0 as usize)
                .or_insert(DeviceStat {
                    max_seq: seq,
                    max_at: meta.at,
                    changes: 0,
                });
            stat.max_seq = stat.max_seq.max(seq);
            stat.max_at = stat.max_at.max(meta.at);
            stat.changes += 1;
        }
        by_index
            .into_iter()
            .filter_map(|(i, stat)| Some((*self.devices.get(i)?, stat)))
            .collect()
    }
}
