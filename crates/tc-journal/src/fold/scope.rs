//! What each derived file's projection reads (§6.2): the scope digest an open compares with
//! the manifest entry of the file before it reads or projects it, and the heads file a
//! materialization writes beside each file.
//!
//! A file's scope is the records its projection reads, found from the fold's own lists, never
//! from a heads file: a record that entered the scope since the file was written (the first
//! comment on a book, a teammate's new decision) changes the digest as a new head does. The
//! scope is cut into parts that several files share (a book's text, the pins), each part's
//! records in a fixed order, so an open digests a book's text once for its four or five files.
//!
//! The digest takes **every** folded version of each record, its heads apart: a projection
//! reads more than the displayed candidate (a decision file's newest time over every head,
//! which head is provisional, the bases of a merge, a passage's sides, the newest text of an
//! incoherent verse among its ancestors), and a parent that arrives after its child changes
//! those while the heads stay. A digest that changes when nothing written would change only
//! costs the open one projection of that file. The heads file keeps the
//! token rule's heads: it is the base of an outside edit (§11.1 X3), and an outside editor saw
//! only the provisional text.
//!
//! The property `a_file_whose_scope_digest_is_unchanged_renders_the_same_bytes`
//! (`tests/scope.rs`) checks, over random team histories, that a file whose digest did not
//! change between two folds renders the same bytes in both.

use super::meta::KeyIdx;
use super::project::DerivedFile;
use super::state::{FoldState, Projected};
use super::{Conflict, ConflictKind};
use crate::canon::encode_key;
use crate::ids::{ChangeId, StoryNo};
use crate::keys::{CheckKey, CheckTarget, RecordKey, ToolId};
use crate::record::{CheckValue, CommentTarget, Record};
use crate::sha::Sha256Stream;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use tc_core::BookId;

/// The SHA-256 a file's scope gives (§6.2), kept in the manifest entry of the file.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeDigest(pub [u8; 32]);

impl ScopeDigest {
    pub fn to_hex(&self) -> String {
        crate::text::hex(&self.0)
    }

    pub fn parse(text: &str) -> Option<Self> {
        crate::text::unhex::<32>(text).map(Self)
    }
}

impl fmt::Debug for ScopeDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ScopeDigest({})", self.to_hex())
    }
}

/// One shared piece of a scope.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Part {
    /// A book's presence, header, layout, chapter fronts and verses.
    Text(BookId),
    Alignments(BookId),
    Checks(ToolId, BookId),
    Comments(BookId),
    Pins,
    Settings,
    Versification,
    Story(StoryNo),
    Stories,
    StoryChecks(ToolId),
    StoryComments,
    Conflicts,
    Meta,
}

impl FoldState {
    fn scope_parts(&self, file: &DerivedFile) -> Vec<Part> {
        match file {
            DerivedFile::Usfm(book) => vec![Part::Text(*book)],
            DerivedFile::Alignments(book) => {
                vec![Part::Text(*book), Part::Alignments(*book), Part::Pins]
            }
            DerivedFile::Decisions(tool, book) => {
                vec![Part::Text(*book), Part::Checks(*tool, *book), Part::Pins]
            }
            DerivedFile::Comments(book) => vec![Part::Text(*book), Part::Comments(*book)],
            DerivedFile::Resources => vec![Part::Pins],
            DerivedFile::Settings => vec![Part::Settings],
            DerivedFile::Versification => vec![Part::Versification],
            DerivedFile::Conflicts => vec![Part::Conflicts],
            DerivedFile::Story(story) => vec![Part::Story(*story)],
            DerivedFile::StoryDecisions(tool) => {
                vec![Part::Stories, Part::StoryChecks(*tool), Part::Pins]
            }
            DerivedFile::StoryComments => vec![Part::Stories, Part::StoryComments],
        }
    }

    fn part_keys(&self, part: &Part) -> Vec<KeyIdx> {
        match part {
            Part::Text(book) => self.text_keys(*book),
            Part::Alignments(book) => {
                self.book_keys_where(*book, |k| matches!(k, RecordKey::Alignment(..)))
            }
            Part::Checks(tool, book) => self.book_keys_where(
                *book,
                |k| matches!(k, RecordKey::Check(c) if c.tool == *tool),
            ),
            Part::Comments(book) => {
                let comments = self.book_keys_where(*book, |k| matches!(k, RecordKey::Comment(_)));
                self.with_comment_targets(comments)
            }
            Part::Pins => self.by_key(self.pin_keys.iter().copied().collect()),
            Part::Settings => self.by_key(self.setting_keys.iter().copied().collect()),
            Part::Versification => self
                .key_idx(&RecordKey::Versification)
                .into_iter()
                .collect(),
            Part::Story(story) => self.by_key(
                self.story_keys
                    .iter()
                    .copied()
                    .filter(|k| story_of(&self.entry(*k).key) == Some(*story))
                    .collect(),
            ),
            Part::Stories => self.by_key(self.story_keys.iter().copied().collect()),
            Part::StoryChecks(tool) => self.by_key(
                self.story_attached
                    .values()
                    .flat_map(|keys| keys.iter().copied())
                    .filter(
                        |k| matches!(&self.entry(*k).key, RecordKey::Check(c) if c.tool == *tool),
                    )
                    .collect(),
            ),
            Part::StoryComments => {
                let comments: Vec<KeyIdx> = self
                    .story_attached
                    .values()
                    .flat_map(|keys| keys.iter().copied())
                    .filter(|k| matches!(self.entry(*k).key, RecordKey::Comment(_)))
                    .collect();
                self.with_comment_targets(self.by_key(comments))
            }
            Part::Conflicts => Vec::new(),
            Part::Meta => self.by_key(self.meta_choices.keys().copied().collect()),
        }
    }

    /// `keys` in record key order. A key's index is the order the fold first met it, which
    /// differs between two folds of one change set, so no digest may follow it (I1).
    fn by_key(&self, mut keys: Vec<KeyIdx>) -> Vec<KeyIdx> {
        keys.sort_by(|a, b| self.entry(*a).key.cmp(&self.entry(*b).key));
        keys.dedup();
        keys
    }

    /// A book's text records, in a fixed order: presence, header, layout, then each chapter's
    /// front and verses.
    fn text_keys(&self, book: BookId) -> Vec<KeyIdx> {
        let Some(entry) = self.books.get(&book) else {
            return Vec::new();
        };
        let mut out: Vec<KeyIdx> = [entry.presence, entry.header, entry.layout]
            .into_iter()
            .flatten()
            .collect();
        for c in entry.chapters.iter() {
            if let Some(chapter) = self.chapters.get(&(book, *c)) {
                out.extend(chapter.front);
                out.extend(chapter.verses.values().copied());
            }
        }
        out
    }

    fn book_keys_where(&self, book: BookId, keep: impl Fn(&RecordKey) -> bool) -> Vec<KeyIdx> {
        let mut out: Vec<KeyIdx> = self
            .books
            .get(&book)
            .map(|b| {
                b.keys
                    .iter()
                    .copied()
                    .filter(|k| keep(&self.entry(*k).key))
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| self.entry(*a).key.cmp(&self.entry(*b).key));
        out
    }

    /// `comments`, the device profile of each comment's author (the file names the author),
    /// and every check record a comment on a check is placed by: its target and each check
    /// its `MovedTo` chain passes (§7.7), read from what the fold keeps in memory.
    fn with_comment_targets(&self, comments: Vec<KeyIdx>) -> Vec<KeyIdx> {
        let mut chained: BTreeSet<KeyIdx> = BTreeSet::new();
        for k in &comments {
            for head in self.entry(*k).heads.iter() {
                let device = self.device_of(self.meta(*head));
                if let Some(profile) = self.profile_keys.get(&device) {
                    chained.insert(*profile);
                }
                if let super::meta::Shape::Comment { target } = self.shape(*head)
                    && let CommentTarget::Check(check) = &*target
                {
                    self.move_chain(check, &mut chained);
                }
            }
        }
        let mut out = comments;
        out.extend(self.by_key(chained.into_iter().collect()));
        out
    }

    /// The check records `check`'s `MovedTo` chain visits, `check`'s own first.
    fn move_chain(&self, check: &CheckKey, out: &mut BTreeSet<KeyIdx>) {
        let mut at = check.clone();
        for _ in 0..64 {
            let Some(k) = self.key_idx(&RecordKey::Check(at.clone())) else {
                break;
            };
            if !out.insert(k) {
                break;
            }
            let next = match &self.entry(k).projected {
                Projected::Change(c) => match self.shape(*c) {
                    super::meta::Shape::MovedTo { to } => Some((*to).clone()),
                    _ => None,
                },
                Projected::Merged(record) => match &**record {
                    Record::Check {
                        value: CheckValue::MovedTo { to },
                        ..
                    } => Some(to.clone()),
                    _ => None,
                },
                Projected::Nothing => None,
            };
            match next {
                Some(to) => at = to,
                None => break,
            }
        }
    }

    fn part_digest(&self, part: &Part) -> [u8; 32] {
        let mut stream = Sha256Stream::new();
        if *part == Part::Conflicts {
            for conflict in self.conflicts() {
                conflict_bytes(conflict, &mut stream);
            }
            return stream.finish();
        }
        if let Part::Text(book) = part {
            // Whether and how the book projects reads more than its own presence: the
            // last-book rule reads every book's, and a removal reads whether any record of
            // the book holds work the remover did not see (§7.6). The fold has decided it.
            book_view_bytes(&self.book(*book), &mut stream);
        }
        for k in self.part_keys(part) {
            let entry = self.entry(k);
            let key = encode_key(&entry.key)
                .map(|j| j.to_canonical())
                .unwrap_or_default();
            stream.update(&key);
            stream.update(&[0]);
            // In id order: a change's index is the order the fold met it.
            let mut heads: Vec<ChangeId> = entry.heads.iter().map(|h| self.meta(*h).id).collect();
            heads.sort_unstable();
            stream.update(&(heads.len() as u32).to_le_bytes());
            for h in heads {
                stream.update(h.as_bytes());
            }
            // Every folded version, not only the heads: a parent that arrives after its
            // child changes the bases, passage sides and incoherent-verse fallbacks a
            // projection reads, and leaves the heads as they were.
            let mut folded: Vec<ChangeId> = entry
                .changes
                .iter()
                .filter(|c| self.meta(**c).active())
                .map(|c| self.meta(*c).id)
                .collect();
            folded.sort_unstable();
            stream.update(&(folded.len() as u32).to_le_bytes());
            for id in folded {
                stream.update(id.as_bytes());
            }
        }
        stream.finish()
    }

    fn digest_of_parts(file: &str, parts: impl IntoIterator<Item = [u8; 32]>) -> ScopeDigest {
        let mut stream = Sha256Stream::new();
        stream.update(file.as_bytes());
        stream.update(&[0]);
        for part in parts {
            stream.update(&part);
        }
        ScopeDigest(stream.finish())
    }

    /// The scope digest of `file` (§6.2): what the open compares with the manifest entry.
    pub fn scope_digest(&self, file: &DerivedFile) -> ScopeDigest {
        let parts: Vec<[u8; 32]> = self
            .scope_parts(file)
            .iter()
            .map(|p| self.part_digest(p))
            .collect();
        Self::digest_of_parts(&file.path(), parts)
    }

    /// The scope digests of `files`, each part digested once however many files share it,
    /// on the fold's worker pool.
    pub fn scope_digests(&self, files: &[DerivedFile]) -> Vec<ScopeDigest> {
        use rayon::prelude::*;
        let mut parts: Vec<Part> = files.iter().flat_map(|f| self.scope_parts(f)).collect();
        let mut seen = std::collections::HashSet::new();
        parts.retain(|p| seen.insert(p.clone()));
        let digests: HashMap<Part, [u8; 32]> = parts
            .into_par_iter()
            .map(|p| {
                let d = self.part_digest(&p);
                (p, d)
            })
            .collect();
        files
            .iter()
            .map(|f| {
                let parts = self.scope_parts(f);
                Self::digest_of_parts(
                    &f.path(),
                    parts
                        .iter()
                        .map(|p| digests.get(p).copied().unwrap_or([0; 32])),
                )
            })
            .collect()
    }

    /// The scope digest of `metadata.json`'s records (§6.2): every project detail, book
    /// presence and pin group. The store adds the ingredient table it lists.
    pub fn metadata_scope_digest(&self) -> ScopeDigest {
        Self::digest_of_parts("metadata.json", [self.part_digest(&Part::Meta)])
    }

    /// The records the projection of `file` reads, each with the heads its written bytes
    /// stand for by the token rule of §2.7: the heads file of `file` (§3.6), the base of an
    /// outside edit of it (§11.1 X3). `conflicts.json` is never taken in as an edit, so its
    /// list is empty.
    pub fn file_heads(&self, file: &DerivedFile) -> Vec<(RecordKey, Vec<ChangeId>)> {
        let mut keys: Vec<KeyIdx> = self
            .scope_parts(file)
            .iter()
            .flat_map(|p| self.part_keys(p))
            .collect();
        keys.sort_by(|a, b| self.entry(*a).key.cmp(&self.entry(*b).key));
        keys.dedup();
        keys.into_iter()
            .filter(|k| self.entry(*k).active > 0)
            .map(|k| {
                let key = self.entry(k).key.clone();
                let heads = self.shown_heads(&key, None);
                (key, heads)
            })
            .collect()
    }

    /// The records `metadata.json` takes its project details from (every `ProjectMeta`),
    /// each with the heads its written bytes stand for by the token rule: the base of an
    /// outside edit of the file (§11.1).
    pub fn metadata_heads(&self) -> Vec<(RecordKey, Vec<ChangeId>)> {
        self.by_key(self.part_keys(&Part::Meta))
            .into_iter()
            .filter(|k| self.entry(*k).active > 0)
            .filter(|k| matches!(self.entry(*k).key, RecordKey::ProjectMeta(_)))
            .map(|k| {
                let key = self.entry(k).key.clone();
                let heads = self.shown_heads(&key, None);
                (key, heads)
            })
            .collect()
    }

    /// The records the projection of `file` reads, in key order.
    pub fn scope_keys(&self, file: &DerivedFile) -> Vec<RecordKey> {
        let mut keys: Vec<RecordKey> = self
            .scope_parts(file)
            .iter()
            .flat_map(|p| self.part_keys(p))
            .map(|k| self.entry(k).key.clone())
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }
}

fn story_of(key: &RecordKey) -> Option<StoryNo> {
    match key {
        RecordKey::StoryBase(s) | RecordKey::Frame(s, _) | RecordKey::StoryRef(s) => Some(*s),
        RecordKey::Check(CheckKey {
            target: CheckTarget::Frame { story, .. },
            ..
        }) => Some(*story),
        _ => None,
    }
}

fn book_view_bytes(view: &super::BookView, stream: &mut Sha256Stream) {
    use super::BookView;
    let (tag, scope, unseen) = match view {
        BookView::Unknown => (0u8, None, 0),
        BookView::Present { scope } => (1, Some(*scope), 0),
        BookView::Removed => (2, None, 0),
        BookView::RemovedWhileEdited { scope, unseen } => (3, *scope, *unseen),
        BookView::KeptAsLast { scope } => (4, *scope, 0),
    };
    stream.update(&[tag]);
    stream.update(scope.map_or([0u8; 32], |s| *s.as_bytes()).as_slice());
    stream.update(&unseen.to_le_bytes());
}

/// What `conflicts.json` writes of a conflict (`conflict_record` in `project.rs`).
fn conflict_bytes(c: &Conflict, stream: &mut Sha256Stream) {
    stream.update(c.id.0.as_slice());
    stream.update(c.provisional.as_bytes());
    stream.update(&[c.kind.order()]);
    if let ConflictKind::BookRemovedWhileEdited { unseen } = c.kind {
        stream.update(&unseen.to_le_bytes());
    }
    for key in std::iter::once(&c.key).chain(&c.also_keys) {
        let bytes = encode_key(key)
            .map(|j| j.to_canonical())
            .unwrap_or_default();
        stream.update(&(bytes.len() as u32).to_le_bytes());
        stream.update(&bytes);
    }
    stream.update(&(c.candidates.len() as u32).to_le_bytes());
    for candidate in &c.candidates {
        stream.update(candidate.head.as_bytes());
        stream.update(candidate.device.as_bytes());
        let author = candidate.author.as_deref().unwrap_or("");
        stream.update(&(author.len() as u32).to_le_bytes());
        stream.update(author.as_bytes());
        stream.update(&candidate.at.0.to_le_bytes());
    }
}
