//! The fold cache, `.tc/index.bin` (`docs/STORAGE.md` §3.6, §6.1 O8, §14).
//!
//! An open reads every pack and pending file, verifies every line and parses every change
//! (§4.4). For a whole Bible that is most of the open's work, and the same bytes at the same
//! path give the same result every time. The cache keeps, for each file an open read, what
//! the fold takes from it: each change's id, where its JSON lies in the file, the
//! fingerprint of those bytes (§2.6) and the summary the fold keeps
//! ([`FoldInput::prepare_at`]). An open that finds a file whose path, size and
//! [`fingerprint`] equal a section's takes the section's inputs instead of hashing,
//! verifying and parsing the file's lines: those bytes passed P1 to P4 at that path when the
//! section was made, and pass them again. The fingerprint costs a small part of what one
//! SHA-256 pass costs on a processor without SHA instructions (§14).
//!
//! The cache never decides what exists. A section stands for exactly the bytes it was made
//! from, so a new file, a changed file, or a file the cache does not hold is read as before,
//! and a file that is gone is never asked for. A section that does not check (its
//! fingerprint, the version or build that wrote it, a value that breaks its type's grammar)
//! is not used: the open reads that file instead. Only a file whose every line verified and
//! parsed has a section, so a damaged file is read, and reported, at every open.
//!
//! No I/O: `tc-store` reads the file, and writes it with `atomic_write`.
//!
//! ```text
//! index   := "tc-index" | version u32 | minor u16 | project [16] | build str
//!            | count var | count × (len var | section of len bytes)
//! section := fingerprint u64 of the rest | path str | size var | file fingerprint u64
//!            | pack: 0, or 1 and the pack's id [32]
//!            | devices: var n, n × [16] | resources: var n, n × (str | [20])
//!            | entries: var n, n × entry
//! entry   := id [32] | offset var | len var | fingerprint u64 | cause and stub u8
//!            | device var | seq var | at zigzag var | batch [16] | batch n var
//!            | parents: var n, n × [32] | key | shape
//! ```
//!
//! Integers are little-endian; `var` is LEB128; `str` is a `var` length and UTF-8 bytes.

use crate::canon;
use crate::change::Verified;
use crate::fold::FoldInput;
use crate::fold::ValueAt;
use crate::fold::meta::{CauseClass, Shape, Summary, VerseShape, book_of};
use crate::fold::value::Stored;
use crate::format::FORMAT_MINOR;
use crate::ids::{
    BatchId, ChangeId, ChapterNo, CommentId, DeviceId, DeviceSeq, FrameNo, GitOid, PackId,
    ProjectId, StoryNo, VerseNo, WallTime,
};
use crate::json;
use crate::keys::{CheckId, CheckKey, CheckTarget, RecordKey, RefPart, ToolId};
use crate::line::Span;
use crate::pack::PackRead;
use crate::pending::{PendingRead, PendingStatus};
use crate::record::{ChapterVerse, CommentTarget, PresentHow, ResourceRef};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::Arc;
use tc_core::BookId;

/// The version of this encoding and of the summaries it holds. Raise it whenever what
/// [`FoldInput::prepare_at`] keeps of a change, or how it is written here, changes; the
/// test `the_encoding_is_pinned` fails until it is raised. (The build stamp of
/// [`Index::encode`] already keeps a release from reading another release's cache; this
/// keeps two builds of one version apart.)
pub const INDEX_VERSION: u32 = 1;

const MAGIC: &[u8; 8] = b"tc-index";

/// The most parents a change may name (rule V5).
const MAX_PARENTS: u64 = 4096;

/// Why an index, or one of its sections, is not used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IndexError {
    #[error("the file is not an index")]
    NotAnIndex,
    #[error("the index is of version {0}; this app reads version {INDEX_VERSION}")]
    Version(u32),
    #[error("the index was written for format minor {0}")]
    Minor(u16),
    #[error("the index was written by another build of the app")]
    Build,
    #[error("the index is of another project")]
    Project,
    #[error("the index ends early or holds a value out of range")]
    Damaged,
    #[error("a section's fingerprint does not match its bytes")]
    Fingerprint,
    #[error("a section holds a value that breaks its grammar: {0}")]
    Malformed(&'static str),
}

type R<T> = Result<T, IndexError>;

/// The bytes a section stands for: the file's path in the project folder (a pack's
/// repository path, or `.tc/pending/<name>`), its size and the [`fingerprint`] of its
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileKey {
    pub path: Box<str>,
    pub size: u64,
    pub fingerprint: u64,
}

impl FileKey {
    /// The key of the file at `path` whose bytes are `bytes`.
    pub fn of(path: &str, bytes: &[u8]) -> Self {
        Self {
            path: path.into(),
            size: bytes.len() as u64,
            fingerprint: fingerprint(bytes),
        }
    }
}

/// A fingerprint of a file's bytes or a section's (§2.6): each step maps the state one to
/// one, so bytes that differ in one 8-byte word always differ in it, and other bytes match
/// with a chance of about 2⁻⁶⁴. It detects damage and changed files, not forgery, like every
/// hash of this format (§18); several gigabytes a second on one core. [`Fingerprinter`]
/// takes it of a file read in chunks.
pub fn fingerprint(bytes: &[u8]) -> u64 {
    crate::fold::value::fingerprint(bytes)
}

pub use crate::fold::value::Fingerprinter;

/// One pack or pending file an open read in full, without the cache.
#[derive(Debug)]
pub struct Loaded {
    /// The file's path, size and fingerprint, from the bytes read.
    pub key: FileKey,
    /// Every change line the file holds (a damaged file's salvaged lines, §4.4), prepared
    /// with [`FoldInput::prepare_at`] at the loader's file number.
    pub inputs: Vec<FoldInput>,
    /// What the cache keeps of the file: `None` unless the file is valid and every line is a
    /// change that parsed. A damaged, newer or foreign file, or one with a quarantined line,
    /// is read, and reported, at every open.
    pub section: Option<Section>,
}

/// Reads the pack at repository path `path` (§4.4) as an open does: verifies and salvages
/// it, prepares its changes at file number `file`, and makes its section from the same
/// bytes, so a section always holds every change of exactly the bytes it is keyed by. The
/// pack's entries and spans are moved into [`Loaded::inputs`]; the rest of the read (its
/// status, header and id) is for the open's notices.
pub fn load_pack(path: &str, bytes: &[u8], project: ProjectId, file: u32) -> (PackRead, Loaded) {
    let mut read = crate::pack::read_pack(path, bytes, project);
    let pack = Some(read.id);
    let inputs = prepare(&mut read.entries, &mut read.spans, pack, file);
    let key = FileKey::of(path, bytes);
    let section = read
        .is_valid()
        .then(|| Section::make(key.clone(), pack, file, &inputs))
        .flatten();
    (
        read,
        Loaded {
            key,
            inputs,
            section,
        },
    )
}

/// [`load_pack`] for a pending file at `path` (`.tc/pending/<name>`, §4.2).
pub fn load_pending(
    path: &str,
    bytes: &[u8],
    project: ProjectId,
    file: u32,
) -> (PendingRead, Loaded) {
    let mut read = crate::pending::read_pending(bytes, project);
    let inputs = prepare(&mut read.entries, &mut read.spans, None, file);
    let key = FileKey::of(path, bytes);
    let section = (read.status == PendingStatus::Valid)
        .then(|| Section::make(key.clone(), None, file, &inputs))
        .flatten();
    (
        read,
        Loaded {
            key,
            inputs,
            section,
        },
    )
}

fn prepare(
    entries: &mut Vec<Verified>,
    spans: &mut Vec<Span>,
    pack: Option<PackId>,
    file: u32,
) -> Vec<FoldInput> {
    std::mem::take(entries)
        .into_iter()
        .zip(std::mem::take(spans))
        .map(|(entry, span)| FoldInput::new(entry, pack).prepare_at(ValueAt::new(file, span)))
        .collect()
}

/// One file's section: the prepared inputs of the file named by its key, encoded.
#[derive(Debug, Clone)]
pub struct Section {
    key: FileKey,
    /// The id of the pack the file is, `None` for a pending file.
    pack: Option<PackId>,
    /// The section's bytes are `bytes[start..end]`: the bytes of the index it was read
    /// from, so taking a section copies nothing.
    bytes: Arc<[u8]>,
    start: usize,
    end: usize,
}

impl Section {
    /// The section of the valid file `key` names, from every input reading it gave, their
    /// values at file number `file`. `None` when the file cannot be cached: a line that is
    /// not a change that parsed (a quarantined change keeps its bytes in memory), or a value
    /// kept in memory (an offset beyond 4 GiB).
    fn make(key: FileKey, pack: Option<PackId>, file: u32, inputs: &[FoldInput]) -> Option<Self> {
        let mut devices: Vec<DeviceId> = Vec::new();
        let mut resources: Vec<Arc<ResourceRef>> = Vec::new();
        let mut entries = Out::default();
        entries.var(inputs.len() as u64);
        for input in inputs {
            let (id, stored, summary) = input.prepared()?;
            let Stored::At {
                file: at,
                offset,
                len,
                fingerprint,
            } = stored
            else {
                return None;
            };
            if *at != file || input.pack() != pack {
                return None;
            }
            entries.bytes(id.as_bytes());
            entries.var(u64::from(*offset));
            entries.var(u64::from(*len));
            entries.bytes(&fingerprint.to_le_bytes());
            entries.u8(cause_code(summary.cause) | (u8::from(summary.stub) << 4));
            let device = match devices.iter().position(|d| *d == summary.device) {
                Some(at) => at,
                None => {
                    devices.push(summary.device);
                    devices.len() - 1
                }
            };
            entries.var(device as u64);
            entries.var(summary.seq.0);
            entries.var(zigzag(summary.at.0));
            entries.bytes(summary.batch.0.as_bytes());
            entries.var(u64::from(summary.batch.1));
            entries.var(summary.parents.len() as u64);
            for parent in &summary.parents {
                entries.bytes(parent.as_bytes());
            }
            put_key(&mut entries, &summary.key)?;
            put_shape(&mut entries, &summary.shape, &mut resources);
        }
        let mut rest = Out::default();
        rest.str(&key.path);
        rest.var(key.size);
        rest.bytes(&key.fingerprint.to_le_bytes());
        match pack {
            None => rest.u8(0),
            Some(pack) => {
                rest.u8(1);
                rest.bytes(pack.as_bytes());
            }
        }
        rest.var(devices.len() as u64);
        for device in &devices {
            rest.bytes(device.as_bytes());
        }
        rest.var(resources.len() as u64);
        for resource in &resources {
            rest.str(&resource.repo_path);
            rest.bytes(resource.sha.as_bytes());
        }
        rest.bytes(&entries.0);
        let mut body = Vec::with_capacity(8 + rest.0.len());
        body.extend_from_slice(&fingerprint(&rest.0).to_le_bytes());
        body.extend_from_slice(&rest.0);
        let end = body.len();
        Some(Self {
            key,
            pack,
            bytes: body.into(),
            start: 0,
            end,
        })
    }

    /// The file this section stands for.
    pub fn key(&self) -> &FileKey {
        &self.key
    }

    /// The id of the pack the file is, `None` for a pending file.
    pub fn pack(&self) -> Option<PackId> {
        self.pack
    }

    fn body(&self) -> &[u8] {
        &self.bytes[self.start..self.end]
    }

    /// The file's prepared inputs, their values at the loader's file number `file`: what
    /// reading the file's lines and [`FoldInput::prepare_at`] give. An error when the
    /// section does not check; the open then reads the file.
    pub fn inputs(&self, file: u32) -> R<Vec<FoldInput>> {
        let body = self.body();
        let (stored, rest) = body.split_first_chunk::<8>().ok_or(IndexError::Damaged)?;
        if u64::from_le_bytes(*stored) != fingerprint(rest) {
            return Err(IndexError::Fingerprint);
        }
        let mut input = In::new(rest);
        let (key, pack) = read_file_key(&mut input)?;
        if key != self.key || pack != self.pack {
            return Err(IndexError::Damaged);
        }
        let devices = input
            .list(|i| DeviceId::from_bytes(i.array()?).ok_or(IndexError::Malformed("a device")))?;
        let resources = input.list(|i| {
            let repo_path = i.str()?;
            if repo_path.is_empty() {
                return Err(IndexError::Malformed("a resource's repository path"));
            }
            Ok(Arc::new(ResourceRef {
                repo_path: repo_path.into(),
                sha: GitOid::from_bytes(i.array()?),
            }))
        })?;
        let count = input.var()?;
        let mut out = Vec::with_capacity(usize::try_from(count.min(1 << 20)).unwrap_or(0));
        for _ in 0..count {
            let id = ChangeId::from_bytes(input.array()?);
            let offset = input.var_u32()?;
            let len = input.var_u32()?;
            let value_fingerprint = u64::from_le_bytes(input.array()?);
            let flags = input.u8()?;
            if flags >> 5 != 0 {
                return Err(IndexError::Malformed("a cause"));
            }
            let cause = cause_of(flags & 0x0f)?;
            let stub = flags & 0x10 != 0;
            let device = usize::try_from(input.var()?)
                .ok()
                .and_then(|d| devices.get(d))
                .copied()
                .ok_or(IndexError::Malformed("a device"))?;
            let seq = DeviceSeq(input.var()?);
            let at = WallTime(unzigzag(input.var()?));
            let batch_id = BatchId::from_bytes(input.array()?);
            let batch_n = input.var_u32()?;
            if batch_n == 0 {
                return Err(IndexError::Malformed("a batch size"));
            }
            let parent_count = input.var()?;
            if parent_count > MAX_PARENTS {
                return Err(IndexError::Malformed("the parents"));
            }
            let mut parents: SmallVec<[ChangeId; 1]> = SmallVec::new();
            for _ in 0..parent_count {
                let parent = ChangeId::from_bytes(input.array()?);
                if parents.last().is_some_and(|last| *last >= parent) {
                    return Err(IndexError::Malformed("the parents"));
                }
                parents.push(parent);
            }
            let key = read_key(&mut input)?;
            let shape = read_shape(&mut input, &resources)?;
            let book = book_of(&key, &shape);
            let summary = Summary {
                key,
                parents,
                device,
                seq,
                at,
                batch: (batch_id, batch_n),
                cause,
                stub,
                shape,
                book,
            };
            let value = Stored::At {
                file,
                offset,
                len,
                fingerprint: value_fingerprint,
            };
            out.push(FoldInput::from_prepared(id, value, summary, pack));
        }
        if !input.is_empty() {
            return Err(IndexError::Damaged);
        }
        Ok(out)
    }
}

/// The sections of an index file, by path.
#[derive(Debug, Clone, Default)]
pub struct Index {
    sections: HashMap<Box<str>, Section>,
}

impl Index {
    /// The bytes of an index of project `project` that holds `sections`, written by the
    /// app build `build`. An index is read only by the build that wrote it, so `build` names
    /// the exact build (its version and source revision): a build that summarizes changes
    /// differently never takes another's sections. Sections are written in path order, so
    /// the same sections give the same bytes.
    pub fn encode<'a>(
        project: ProjectId,
        build: &str,
        sections: impl IntoIterator<Item = &'a Section>,
    ) -> Vec<u8> {
        let mut sections: Vec<&Section> = sections.into_iter().collect();
        sections.sort_by(|a, b| a.key.path.cmp(&b.key.path));
        sections.dedup_by(|a, b| a.key.path == b.key.path);
        let mut out = Out::default();
        out.bytes(MAGIC);
        out.bytes(&INDEX_VERSION.to_le_bytes());
        out.bytes(&FORMAT_MINOR.to_le_bytes());
        out.bytes(project.as_bytes());
        out.str(build);
        out.var(sections.len() as u64);
        for section in sections {
            let body = section.body();
            out.var(body.len() as u64);
            out.bytes(body);
        }
        out.0
    }

    /// Reads an index of project `project` written by the app build `build`. The sections'
    /// fingerprints are checked when their inputs are taken ([`Section::inputs`]), on the
    /// open's reading threads.
    pub fn parse(bytes: impl Into<Arc<[u8]>>, project: ProjectId, build: &str) -> R<Self> {
        let bytes: Arc<[u8]> = bytes.into();
        let mut input = In::new(&bytes);
        if input.take(MAGIC.len()).ok() != Some(MAGIC.as_slice()) {
            return Err(IndexError::NotAnIndex);
        }
        let version = u32::from_le_bytes(input.array()?);
        if version != INDEX_VERSION {
            return Err(IndexError::Version(version));
        }
        let minor = u16::from_le_bytes(input.array()?);
        if minor != FORMAT_MINOR {
            return Err(IndexError::Minor(minor));
        }
        if ProjectId::from_bytes(input.array()?) != project {
            return Err(IndexError::Project);
        }
        if input.str()? != build {
            return Err(IndexError::Build);
        }
        let count = input.var()?;
        let mut sections = HashMap::new();
        for _ in 0..count {
            let len = usize::try_from(input.var()?).map_err(|_| IndexError::Damaged)?;
            let start = input.at;
            let body = input.take(len)?;
            let mut fields = In::new(body.get(8..).ok_or(IndexError::Damaged)?);
            let (key, pack) = read_file_key(&mut fields)?;
            let section = Section {
                key,
                pack,
                bytes: bytes.clone(),
                start,
                end: start + len,
            };
            sections.insert(section.key.path.clone(), section);
        }
        if !input.is_empty() {
            return Err(IndexError::Damaged);
        }
        Ok(Self { sections })
    }

    /// The section that stands for exactly the file `key` names, if the index holds one.
    pub fn section(&self, key: &FileKey) -> Option<&Section> {
        self.sections.get(&key.path).filter(|s| s.key == *key)
    }

    pub fn sections(&self) -> impl Iterator<Item = &Section> {
        self.sections.values()
    }

    pub fn len(&self) -> usize {
        self.sections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
}

fn read_file_key(input: &mut In<'_>) -> R<(FileKey, Option<PackId>)> {
    let path = input.str()?.into();
    let size = input.var()?;
    let fingerprint = u64::from_le_bytes(input.array()?);
    let pack = match input.u8()? {
        0 => None,
        1 => Some(PackId::from_bytes(input.array()?)),
        _ => return Err(IndexError::Malformed("a pack")),
    };
    Ok((
        FileKey {
            path,
            size,
            fingerprint,
        },
        pack,
    ))
}

// ---------------------------------------------------------------------------------------
// Causes, keys and shapes
// ---------------------------------------------------------------------------------------

fn cause_code(cause: CauseClass) -> u8 {
    match cause {
        CauseClass::Edit => 0,
        CauseClass::Resolve => 1,
        CauseClass::Restore => 2,
        CauseClass::CarryOver => 3,
        CauseClass::External {
            unknown_base: false,
        } => 4,
        CauseClass::External { unknown_base: true } => 5,
        CauseClass::Import => 6,
        CauseClass::Seed => 7,
        CauseClass::StartOver => 8,
        CauseClass::System => 9,
        CauseClass::Newer => 10,
    }
}

fn cause_of(code: u8) -> R<CauseClass> {
    Ok(match code {
        0 => CauseClass::Edit,
        1 => CauseClass::Resolve,
        2 => CauseClass::Restore,
        3 => CauseClass::CarryOver,
        4 => CauseClass::External {
            unknown_base: false,
        },
        5 => CauseClass::External { unknown_base: true },
        6 => CauseClass::Import,
        7 => CauseClass::Seed,
        8 => CauseClass::StartOver,
        9 => CauseClass::System,
        10 => CauseClass::Newer,
        _ => return Err(IndexError::Malformed("a cause")),
    })
}

// The kinds most changes have are written compactly; the rest as their canonical key JSON,
// read back by the same decoder a change's key goes through.
const KEY_PRESENCE: u8 = 0;
const KEY_HEADER: u8 = 1;
const KEY_FRONT: u8 = 2;
const KEY_VERSE: u8 = 3;
const KEY_LAYOUT: u8 = 4;
const KEY_ALIGNMENT: u8 = 5;
const KEY_CHECK: u8 = 6;
const KEY_COMMENT: u8 = 7;
const KEY_JSON: u8 = 8;

fn put_key(out: &mut Out, key: &RecordKey) -> Option<()> {
    match key {
        RecordKey::BookPresence(book) => {
            out.u8(KEY_PRESENCE);
            out.u8(book.ordinal());
        }
        RecordKey::BookHeader(book) => {
            out.u8(KEY_HEADER);
            out.u8(book.ordinal());
        }
        RecordKey::ChapterFront(book, chapter) => {
            out.u8(KEY_FRONT);
            out.u8(book.ordinal());
            out.var(u64::from(chapter.get()));
        }
        RecordKey::Verse(book, chapter, verse) => {
            out.u8(KEY_VERSE);
            out.u8(book.ordinal());
            out.var(u64::from(chapter.get()));
            out.var(u64::from(verse.get()));
        }
        RecordKey::BookLayout(book) => {
            out.u8(KEY_LAYOUT);
            out.u8(book.ordinal());
        }
        RecordKey::Alignment(book, chapter, verse) => {
            out.u8(KEY_ALIGNMENT);
            out.u8(book.ordinal());
            out.var(u64::from(chapter.get()));
            out.var(u64::from(verse.get()));
        }
        RecordKey::Check(check) => {
            out.u8(KEY_CHECK);
            put_check(out, check);
        }
        RecordKey::Comment(id) => {
            out.u8(KEY_COMMENT);
            out.bytes(id.as_bytes());
        }
        other => {
            out.u8(KEY_JSON);
            out.str(&canon::encode_key(other).ok()?.to_canonical_string());
        }
    }
    Some(())
}

fn read_key(input: &mut In<'_>) -> R<RecordKey> {
    Ok(match input.u8()? {
        KEY_PRESENCE => RecordKey::BookPresence(input.book()?),
        KEY_HEADER => RecordKey::BookHeader(input.book()?),
        KEY_FRONT => RecordKey::ChapterFront(input.book()?, input.chapter()?),
        KEY_VERSE => RecordKey::Verse(input.book()?, input.chapter()?, input.verse()?),
        KEY_LAYOUT => RecordKey::BookLayout(input.book()?),
        KEY_ALIGNMENT => RecordKey::Alignment(input.book()?, input.chapter()?, input.verse()?),
        KEY_CHECK => RecordKey::Check(read_check(input)?),
        KEY_COMMENT => RecordKey::Comment(CommentId::from_bytes(input.array()?)),
        KEY_JSON => {
            let text = input.str()?;
            let value = json::parse(text.as_bytes()).map_err(|_| IndexError::Malformed("a key"))?;
            if value.to_canonical_string() != text {
                return Err(IndexError::Malformed("a key"));
            }
            let key = canon::decode_key(&value).map_err(|_| IndexError::Malformed("a key"))?;
            // The compact kinds are always written compactly, so each key has one form.
            if matches!(
                key,
                RecordKey::BookPresence(_)
                    | RecordKey::BookHeader(_)
                    | RecordKey::ChapterFront(..)
                    | RecordKey::Verse(..)
                    | RecordKey::BookLayout(_)
                    | RecordKey::Alignment(..)
                    | RecordKey::Check(_)
                    | RecordKey::Comment(_)
            ) {
                return Err(IndexError::Malformed("a key"));
            }
            key
        }
        _ => return Err(IndexError::Malformed("a key kind")),
    })
}

fn put_check(out: &mut Out, check: &CheckKey) {
    out.u8(match check.tool {
        ToolId::TranslationWords => 0,
        ToolId::TranslationNotes => 1,
        ToolId::TranslationQuestions => 2,
    });
    out.str(check.check_id.as_str());
    match &check.target {
        CheckTarget::Verse {
            book,
            chapter,
            verse,
        } => {
            out.u8(0);
            out.u8(book.ordinal());
            out.str(chapter.as_str());
            out.str(verse.as_str());
        }
        CheckTarget::Frame { story, frame } => {
            out.u8(1);
            out.u8(story.get());
            out.var(u64::from(frame.0));
        }
    }
    out.var(u64::from(check.occurrence));
}

fn read_check(input: &mut In<'_>) -> R<CheckKey> {
    let tool = match input.u8()? {
        0 => ToolId::TranslationWords,
        1 => ToolId::TranslationNotes,
        2 => ToolId::TranslationQuestions,
        _ => return Err(IndexError::Malformed("a tool")),
    };
    let check_id = CheckId::parse(input.str()?).map_err(|_| IndexError::Malformed("a check id"))?;
    let target = match input.u8()? {
        0 => CheckTarget::Verse {
            book: input.book()?,
            chapter: input.ref_part()?,
            verse: input.ref_part()?,
        },
        1 => CheckTarget::Frame {
            story: input.story()?,
            frame: input.frame()?,
        },
        _ => return Err(IndexError::Malformed("a check target")),
    };
    Ok(CheckKey {
        tool,
        check_id,
        target,
        occurrence: input.var_u32()?,
    })
}

const SHAPE_OTHER: u8 = 0;
const SHAPE_TEXT: u8 = 1;
const SHAPE_COVERED: u8 = 2;
const SHAPE_ABSENT: u8 = 3;
const SHAPE_FRONT: u8 = 4;
const SHAPE_REMOVED: u8 = 5;
const SHAPE_PRESENT: u8 = 6;
const SHAPE_ALIGNMENT: u8 = 7;
const SHAPE_DECISION: u8 = 8;
const SHAPE_MOVED_TO: u8 = 9;
const SHAPE_COMMENT: u8 = 10;
const SHAPE_CARRIED_REMOVED: u8 = 11;
const SHAPE_CARRIED: u8 = 12;
const SHAPE_RELEASE: u8 = 13;

fn put_shape(out: &mut Out, shape: &Shape, resources: &mut Vec<Arc<ResourceRef>>) {
    match shape {
        Shape::Other => out.u8(SHAPE_OTHER),
        Shape::Verse(VerseShape::Text { last, from }) => {
            out.u8(SHAPE_TEXT);
            out.var(u64::from(*last));
            put_place(out, *from);
        }
        Shape::Verse(VerseShape::Covered { by }) => {
            out.u8(SHAPE_COVERED);
            out.var(u64::from(*by));
        }
        Shape::Verse(VerseShape::Absent { moved_to }) => {
            out.u8(SHAPE_ABSENT);
            put_place(out, *moved_to);
        }
        Shape::Front { present } => {
            out.u8(SHAPE_FRONT);
            out.u8(u8::from(*present));
        }
        Shape::Presence(None) => out.u8(SHAPE_REMOVED),
        Shape::Presence(Some(how)) => {
            out.u8(SHAPE_PRESENT);
            out.u8(match how {
                PresentHow::Added => 0,
                PresentHow::RestoredText => 1,
                PresentHow::StartedEmpty => 2,
            });
        }
        Shape::Alignment { dismissed } => {
            out.u8(SHAPE_ALIGNMENT);
            out.u8(u8::from(*dismissed));
        }
        Shape::Decision {
            resource,
            dismissed,
        } => {
            let at = match resources.iter().position(|r| **r == **resource) {
                Some(at) => at,
                None => {
                    resources.push(resource.clone());
                    resources.len() - 1
                }
            };
            out.u8(SHAPE_DECISION);
            out.var(at as u64);
            out.u8(u8::from(*dismissed));
        }
        Shape::MovedTo { to } => {
            out.u8(SHAPE_MOVED_TO);
            put_check(out, to);
        }
        Shape::Comment { target } => {
            out.u8(SHAPE_COMMENT);
            match &**target {
                CommentTarget::Verse {
                    book,
                    chapter,
                    verse,
                } => {
                    out.u8(0);
                    out.u8(book.ordinal());
                    out.var(u64::from(chapter.get()));
                    out.var(u64::from(verse.get()));
                }
                CommentTarget::Check(check) => {
                    out.u8(1);
                    put_check(out, check);
                }
                CommentTarget::Frame { story, frame } => {
                    out.u8(2);
                    out.u8(story.get());
                    out.var(u64::from(frame.0));
                }
            }
        }
        Shape::Carried { oid: None } => out.u8(SHAPE_CARRIED_REMOVED),
        Shape::Carried { oid: Some(oid) } => {
            out.u8(SHAPE_CARRIED);
            out.bytes(oid.as_bytes());
        }
        Shape::Release { changes } => {
            out.u8(SHAPE_RELEASE);
            out.var(changes.len() as u64);
            for change in changes.iter() {
                out.bytes(change.as_bytes());
            }
        }
    }
}

fn read_shape(input: &mut In<'_>, resources: &[Arc<ResourceRef>]) -> R<Shape> {
    Ok(match input.u8()? {
        SHAPE_OTHER => Shape::Other,
        SHAPE_TEXT => Shape::Verse(VerseShape::Text {
            last: input.verse()?.get(),
            from: read_place(input)?,
        }),
        SHAPE_COVERED => Shape::Verse(VerseShape::Covered {
            by: input.verse()?.get(),
        }),
        SHAPE_ABSENT => Shape::Verse(VerseShape::Absent {
            moved_to: read_place(input)?,
        }),
        SHAPE_FRONT => Shape::Front {
            present: input.flag()?,
        },
        SHAPE_REMOVED => Shape::Presence(None),
        SHAPE_PRESENT => Shape::Presence(Some(match input.u8()? {
            0 => PresentHow::Added,
            1 => PresentHow::RestoredText,
            2 => PresentHow::StartedEmpty,
            _ => return Err(IndexError::Malformed("how a book is present")),
        })),
        SHAPE_ALIGNMENT => Shape::Alignment {
            dismissed: input.flag()?,
        },
        SHAPE_DECISION => Shape::Decision {
            resource: usize::try_from(input.var()?)
                .ok()
                .and_then(|at| resources.get(at))
                .cloned()
                .ok_or(IndexError::Malformed("a resource"))?,
            dismissed: input.flag()?,
        },
        SHAPE_MOVED_TO => Shape::MovedTo {
            to: Arc::new(read_check(input)?),
        },
        SHAPE_COMMENT => Shape::Comment {
            target: Arc::new(match input.u8()? {
                0 => CommentTarget::Verse {
                    book: input.book()?,
                    chapter: input.chapter()?,
                    verse: input.verse()?,
                },
                1 => CommentTarget::Check(read_check(input)?),
                2 => CommentTarget::Frame {
                    story: input.story()?,
                    frame: input.frame()?,
                },
                _ => return Err(IndexError::Malformed("a comment target")),
            }),
        },
        SHAPE_CARRIED_REMOVED => Shape::Carried { oid: None },
        SHAPE_CARRIED => Shape::Carried {
            oid: Some(GitOid::from_bytes(input.array()?)),
        },
        SHAPE_RELEASE => {
            let count = input.var()?;
            let mut changes: Vec<ChangeId> = Vec::new();
            for _ in 0..count {
                let change = ChangeId::from_bytes(input.array()?);
                if changes.last().is_some_and(|last| *last >= change) {
                    return Err(IndexError::Malformed("a release's changes"));
                }
                changes.push(change);
            }
            Shape::Release {
                changes: Arc::new(changes),
            }
        }
        _ => return Err(IndexError::Malformed("a value's shape")),
    })
}

fn put_place(out: &mut Out, place: Option<ChapterVerse>) {
    match place {
        None => out.u8(0),
        Some(place) => {
            out.u8(1);
            out.var(u64::from(place.chapter.get()));
            out.var(u64::from(place.verse.get()));
        }
    }
}

fn read_place(input: &mut In<'_>) -> R<Option<ChapterVerse>> {
    Ok(match input.u8()? {
        0 => None,
        1 => Some(ChapterVerse {
            chapter: input.chapter()?,
            verse: input.verse()?,
        }),
        _ => return Err(IndexError::Malformed("a chapter and verse")),
    })
}

fn zigzag(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

fn unzigzag(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

// ---------------------------------------------------------------------------------------
// Bytes
// ---------------------------------------------------------------------------------------

#[derive(Default)]
struct Out(Vec<u8>);

impl Out {
    fn u8(&mut self, byte: u8) {
        self.0.push(byte);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }

    fn var(&mut self, mut n: u64) {
        while n >= 0x80 {
            self.0.push((n as u8 & 0x7f) | 0x80);
            n >>= 7;
        }
        self.0.push(n as u8);
    }

    fn str(&mut self, text: &str) {
        self.var(text.len() as u64);
        self.bytes(text.as_bytes());
    }
}

struct In<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> In<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn is_empty(&self) -> bool {
        self.at == self.bytes.len()
    }

    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(IndexError::Damaged)?;
        let out = self.bytes.get(self.at..end).ok_or(IndexError::Damaged)?;
        self.at = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> R<[u8; N]> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }

    fn flag(&mut self) -> R<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(IndexError::Malformed("a flag")),
        }
    }

    /// LEB128, in its shortest form, at most 64 bits.
    fn var(&mut self) -> R<u64> {
        let mut n: u64 = 0;
        for shift in (0..64).step_by(7) {
            let byte = self.u8()?;
            let bits = u64::from(byte & 0x7f);
            if shift == 63 && bits > 1 {
                return Err(IndexError::Damaged);
            }
            n |= bits << shift;
            if byte & 0x80 == 0 {
                if byte == 0 && shift > 0 {
                    return Err(IndexError::Damaged);
                }
                return Ok(n);
            }
        }
        Err(IndexError::Damaged)
    }

    fn var_u32(&mut self) -> R<u32> {
        u32::try_from(self.var()?).map_err(|_| IndexError::Damaged)
    }

    fn var_u16(&mut self) -> R<u16> {
        u16::try_from(self.var()?).map_err(|_| IndexError::Damaged)
    }

    fn str(&mut self) -> R<&'a str> {
        let len = usize::try_from(self.var()?).map_err(|_| IndexError::Damaged)?;
        std::str::from_utf8(self.take(len)?).map_err(|_| IndexError::Malformed("a string"))
    }

    fn list<T>(&mut self, mut item: impl FnMut(&mut Self) -> R<T>) -> R<Vec<T>> {
        let count = self.var()?;
        let mut out = Vec::new();
        for _ in 0..count {
            out.push(item(self)?);
        }
        Ok(out)
    }

    fn book(&mut self) -> R<BookId> {
        BookId::from_ordinal(self.u8()?).ok_or(IndexError::Malformed("a book"))
    }

    fn chapter(&mut self) -> R<ChapterNo> {
        ChapterNo::new(self.var_u16()?).ok_or(IndexError::Malformed("a chapter"))
    }

    fn verse(&mut self) -> R<VerseNo> {
        VerseNo::new(self.var_u16()?).ok_or(IndexError::Malformed("a verse"))
    }

    fn story(&mut self) -> R<StoryNo> {
        StoryNo::new(self.u8()?).ok_or(IndexError::Malformed("a story"))
    }

    fn frame(&mut self) -> R<FrameNo> {
        Ok(FrameNo(self.var_u16()?))
    }

    fn ref_part(&mut self) -> R<RefPart> {
        RefPart::parse(self.str()?).map_err(|_| IndexError::Malformed("a reference part"))
    }
}
