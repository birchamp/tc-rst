//! STORAGE §13.2's state seed (APP-DESIGN AD7): a tC4 project folder with no journal
//! (tC4's "Phase-1 sidecars") turned into the changes of a project in this format, then
//! checked against the folder it came from.
//!
//! **What it makes.** One batch for the project and one per Bible book or OBS story, every
//! change by the device `Importer("tc4-seed")` with `cause: Import`. The project batch holds
//! the free fields of `metadata.json` (`ProjectMeta`), `vrs.json` (`Versification`), the pins
//! of `resources.json` (a `schemaVersion: 1` file migrated as BURRITO-SPEC §5.3 says), the
//! members of `resources.json` no pin group holds, the settings per top-level key and the
//! files the format does not derive (`CarriedFile`, an OBS project's licence and its front
//! and back matter). A book's batch holds the book through the codec, its alignments and its
//! decisions; a story's batch holds the story file as its `StoryBase`, as tC4 keeps a
//! project with no journal, and the decisions on its frames. The changes are sealed into
//! real packs, read back and folded by tc-journal's own fold (AD36).
//!
//! **The check.** What the fold projects must give the folder back: every USFM file,
//! `vrs.json`, every story file and every carried file byte for byte; every sidecar record
//! field for field as tC4 reads it (an absent flag reads as false, a decision's absent
//! `status` as tC4 derives it, an alignment's `invalid` with the I-3 hash rule); every free
//! field of `metadata.json`; and every derived file the folder has must be one the fold
//! derives. Any mismatch refuses the seed, and nothing is made.
//!
//! **Declared differences**, listed in the report and never silent: a decision's
//! `verseEdits` is recomputed from the hash §13.2 takes from the current verse; a decision
//! file's `summary` (a cache tC4 itself drops) is dropped; `resources.json` gains an empty
//! `resources` or `languageSets` member; a `schemaVersion: 1` `resources.json` is migrated;
//! a timestamp is rewritten in tC4's own form.
//!
//! The fixture backend loads this seed (B1b); unit I's importer replaces it once
//! `memory_seed_equals_tc_import_seed` passes (AD7).

mod check;
mod strict;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use serde_json::{Map, Value};
use tc_journal::decision::verse_text_md5_v1;
use tc_journal::fold::{
    DerivedFile, FoldInput, FoldState, ProjectKind, decode_alignments, decode_decisions,
    records_from_usfm,
};
use tc_journal::json::Json;
use tc_journal::pack::{PackSpec, read_pack, seal};
use tc_journal::pins::{Collection, SetSlot};
use tc_journal::record::{
    BookPresence, CarriedValue, CheckValue, PinGroupValue, PresentHow, Scope, ScopeRange,
    ScopeRanges, VersificationValue,
};
use tc_journal::{
    BatchId, BatchRef, BookId, Cause, Change, ChangeBody, CheckTarget, DeviceId, DeviceKind,
    DeviceSeq, FORMAT_MINOR, GitOid, ImportSource, Md5, MetaField, MetaPath, PinGroup, ProjectId,
    Record, RecordKey, RepoPath, ResourcesKey, ResourcesLevel, SettingKey, StoryNo, ToolId,
    Verified, WallTime,
};

use strict::{StrictError, parse_utc_millis, read_json_strict};

const METADATA: &str = "metadata.json";
const INGREDIENTS: &str = "ingredients/";

/// tC4's seed floor (`journal/reconcile.mjs:136`), the time of a change when the project
/// names no `meta.dateCreated`: 2020-01-01T00:00:00.000Z.
const SEED_FLOOR: i64 = 1_577_836_800_000;

// ---------------------------------------------------------------------------------------
// The folder
// ---------------------------------------------------------------------------------------

/// A tC4 project folder read into memory: `metadata.json`, every file under `ingredients/`,
/// and every other entry with the reason it is not imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tc4Folder {
    name: String,
    metadata: Vec<u8>,
    /// Repository paths under `ingredients/`, with their bytes.
    files: BTreeMap<String, Vec<u8>>,
    ignored: Vec<Ignored>,
}

/// A source file the seed does not import, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignored {
    pub path: String,
    pub why: IgnoredWhy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoredWhy {
    /// `.git/`: the project's history, which an import keeps as it is (STORAGE §13).
    GitHistory,
    /// `.gitignore` and `.gitattributes`: files the app writes itself.
    AppOwned,
    /// `.remotes.json`: the remote list of tC4's mock server, not part of a project.
    MockServerRemotes,
    /// `*.bak`: tC4's one-level undo, which the importer keeps in `.tc/external/tc4-bak/`
    /// and never imports (STORAGE §13).
    Tc4Backup,
    /// Any other entry beside `metadata.json` and `ingredients/` (STORAGE §3.1, foreign).
    ForeignRootEntry,
}

impl fmt::Display for IgnoredWhy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::GitHistory => "the project's git history",
            Self::AppOwned => "a file translationCore writes itself",
            Self::MockServerRemotes => "the mock server's remote list",
            Self::Tc4Backup => "a translationCore 4 backup, never imported",
            Self::ForeignRootEntry => "not part of a translationCore project",
        })
    }
}

/// What a path of the folder is, by its place alone.
enum Class {
    Git,
    Ignored(IgnoredWhy),
    Metadata,
    Ingredient,
}

fn classify(path: &str) -> Class {
    if path == ".git" || path.starts_with(".git/") {
        Class::Git
    } else if path.ends_with(".bak") {
        Class::Ignored(IgnoredWhy::Tc4Backup)
    } else if path == ".gitignore" || path == ".gitattributes" {
        Class::Ignored(IgnoredWhy::AppOwned)
    } else if path == ".remotes.json" {
        Class::Ignored(IgnoredWhy::MockServerRemotes)
    } else if path == METADATA {
        Class::Metadata
    } else if path.starts_with(INGREDIENTS) {
        Class::Ingredient
    } else {
        Class::Ignored(IgnoredWhy::ForeignRootEntry)
    }
}

impl Tc4Folder {
    /// Reads the project folder `dir` into memory. It opens files only to read them and
    /// skips `.git/` whole. A link or a special file refuses, since the seed reads only
    /// plain files.
    pub fn read(dir: &Path) -> Result<Self, SeedRefusal> {
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| SeedRefusal::Io {
                path: dir.display().to_string(),
                problem: "the folder's name is not text".into(),
            })?;
        let mut files = Vec::new();
        let mut saw_git = false;
        walk(dir, "", &mut files, &mut saw_git)?;
        Self::assemble(name, files, saw_git)
    }

    /// The same as [`Tc4Folder::read`] over files given in memory, by their `/`-separated
    /// paths relative to the folder, for tests and for the fixture backend.
    pub fn from_files(
        name: &str,
        files: impl IntoIterator<Item = (String, Vec<u8>)>,
    ) -> Result<Self, SeedRefusal> {
        let mut out = Vec::new();
        let mut saw_git = false;
        for (path, bytes) in files {
            let bad = path.is_empty()
                || path.contains('\\')
                || path
                    .split('/')
                    .any(|segment| segment.is_empty() || segment == "." || segment == "..");
            if bad {
                return Err(SeedRefusal::Io {
                    path,
                    problem: "not a relative path with '/' between its parts".into(),
                });
            }
            if matches!(classify(&path), Class::Git) {
                saw_git = true;
            } else {
                out.push((path, bytes));
            }
        }
        Self::assemble(name, out, saw_git)
    }

    fn assemble(
        name: &str,
        files: Vec<(String, Vec<u8>)>,
        saw_git: bool,
    ) -> Result<Self, SeedRefusal> {
        let mut metadata = None;
        let mut ingredients = BTreeMap::new();
        let mut ignored = Vec::new();
        let mut seen = BTreeSet::new();
        for (path, bytes) in files {
            if !seen.insert(path.clone()) {
                return Err(SeedRefusal::Io {
                    path,
                    problem: "the file is given twice".into(),
                });
            }
            match classify(&path) {
                Class::Git => {}
                Class::Ignored(why) => ignored.push(Ignored { path, why }),
                Class::Metadata => metadata = Some(bytes),
                Class::Ingredient => {
                    ingredients.insert(path, bytes);
                }
            }
        }
        if ingredients
            .keys()
            .any(|p| p.starts_with("ingredients/checking/journal/"))
        {
            return Err(SeedRefusal::HasJournal);
        }
        if ingredients.keys().any(|p| p.starts_with("ingredients/tc/")) {
            return Err(SeedRefusal::NewFormat);
        }
        let metadata = metadata.ok_or(SeedRefusal::NoMetadata)?;
        if saw_git {
            ignored.push(Ignored {
                path: ".git/".into(),
                why: IgnoredWhy::GitHistory,
            });
        }
        ignored.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self {
            name: name.to_owned(),
            metadata,
            files: ingredients,
            ignored,
        })
    }

    /// The folder's own name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `metadata.json` as read.
    pub fn metadata(&self) -> &[u8] {
        &self.metadata
    }

    /// Every file under `ingredients/`, by repository path.
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }

    /// The entries not imported, with their reasons, in path order.
    pub fn ignored(&self) -> &[Ignored] {
        &self.ignored
    }
}

fn walk(
    root: &Path,
    rel: &str,
    out: &mut Vec<(String, Vec<u8>)>,
    saw_git: &mut bool,
) -> Result<(), SeedRefusal> {
    let io = |path: &str, error: std::io::Error| SeedRefusal::Io {
        path: if path.is_empty() {
            root.display().to_string()
        } else {
            path.to_owned()
        },
        problem: error.to_string(),
    };
    let dir = if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    };
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| io(rel, e))? {
        let entry = entry.map_err(|e| io(rel, e))?;
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(|| SeedRefusal::Io {
            path: format!("{rel}/{}", name.to_string_lossy()),
            problem: "the name is not text".into(),
        })?;
        names.push(name.to_owned());
    }
    names.sort();
    for name in names {
        let path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if rel.is_empty() && name == ".git" {
            *saw_git = true;
            continue;
        }
        let full = root.join(&path);
        let meta = std::fs::symlink_metadata(&full).map_err(|e| io(&path, e))?;
        if meta.file_type().is_symlink() {
            return Err(SeedRefusal::Io {
                path,
                problem: "a link: the seed reads only plain files".into(),
            });
        }
        if meta.is_dir() {
            walk(root, &path, out, saw_git)?;
        } else if meta.is_file() {
            let bytes = std::fs::read(&full).map_err(|e| io(&path, e))?;
            out.push((path, bytes));
        } else {
            return Err(SeedRefusal::Io {
                path,
                problem: "not a plain file".into(),
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// What the seed gives
// ---------------------------------------------------------------------------------------

/// A checked seed: the changes, their fold, the carried files' bytes and what the check
/// declared.
#[derive(Debug, Clone)]
pub struct StateSeed {
    pub project: ProjectId,
    pub kind: ProjectKind,
    /// `Importer("tc4-seed")` of this project.
    pub device: DeviceId,
    /// The project's batch, then one per book in canonical order or per story in number
    /// order.
    pub batches: Vec<SeedBatch>,
    /// The fold of the sealed changes as read back from their packs.
    pub fold: FoldState,
    /// Carried files' bytes by git blob id.
    pub carried: BTreeMap<GitOid, Vec<u8>>,
    pub report: SeedReport,
}

/// One batch of the seed: its changes in record key order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedBatch {
    pub label: BatchLabel,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BatchLabel {
    Project,
    Book(BookId),
    Story(StoryNo),
}

impl BatchLabel {
    /// The text the batch's id is made from: `project`, the book's code, or `story-NN`.
    fn text(self) -> String {
        match self {
            Self::Project => "project".into(),
            Self::Book(book) => book.code().into(),
            Self::Story(story) => format!("story-{:02}", story.get()),
        }
    }
}

/// What the check said besides "it passed".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeedReport {
    pub declared: Vec<DeclaredDifference>,
    pub ignored: Vec<Ignored>,
    /// Files the fold derives that the folder lacked: nothing is lost by them.
    pub added_files: Vec<String>,
    /// Facts worth a line, such as a book file the scope did not list.
    pub notes: Vec<String>,
    pub counts: SeedCounts,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeedCounts {
    pub batches: usize,
    pub changes: usize,
    /// Records by kind name (`verse`, `check`, …).
    pub records: BTreeMap<String, usize>,
}

impl SeedCounts {
    fn of(batches: &[SeedBatch]) -> Self {
        let mut records: BTreeMap<String, usize> = BTreeMap::new();
        for change in batches.iter().flat_map(|b| &b.changes) {
            *records
                .entry(change.body().record.key().kind_name().to_owned())
                .or_default() += 1;
        }
        Self {
            batches: batches.len(),
            changes: batches.iter().map(|b| b.changes.len()).sum(),
            records,
        }
    }
}

/// A difference between the folder and the projection that the format makes by design and
/// that no tC4 screen shows (module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredDifference {
    /// The decision's `verseEdits` as the fold computes it from the current verse.
    VerseEditsRecomputed {
        path: String,
        check: String,
        source: bool,
        projected: bool,
    },
    /// A decision file's `summary`, a cache tC4's own seed drops.
    SummaryDropped { path: String },
    /// An empty member the projection writes where the source had none.
    EmptyMemberAdded { path: String, member: String },
    /// `resources.json` was migrated from this schema version.
    ResourcesMigrated { from: u64 },
    /// The same instant in another text.
    TimestampRewritten {
        path: String,
        check: String,
        source: String,
        projected: String,
    },
}

impl fmt::Display for DeclaredDifference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VerseEditsRecomputed {
                path,
                check,
                source,
                projected,
            } => write!(
                f,
                "{path} {check}: verseEdits {source} is recomputed as {projected}"
            ),
            Self::SummaryDropped { path } => write!(f, "{path}: the summary cache is dropped"),
            Self::EmptyMemberAdded { path, member } => {
                write!(f, "{path}: an empty \"{member}\" member is added")
            }
            Self::ResourcesMigrated { from } => write!(
                f,
                "ingredients/checking/resources.json: schemaVersion {from} is migrated to 2"
            ),
            Self::TimestampRewritten {
                path,
                check,
                source,
                projected,
            } => write!(f, "{path} {check}: {source} is written {projected}"),
        }
    }
}

/// One way the projection does not give the folder back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    /// The repository path of the file (`metadata.json` for its fields).
    pub path: String,
    /// The record, such as `TIT 1:1`, `translationNotes TIT 1:4 gr8c #1` or
    /// `story 3 frame 0`.
    pub record: Option<String>,
    /// The field, such as `invalid`, `resource` or `identification.name`.
    pub field: Option<String>,
    /// What the folder holds: its bytes or compact JSON, with the line for text.
    pub source: String,
    /// What the fold gives, or `absent`.
    pub projected: String,
    pub rule: CheckRule,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path)?;
        if let Some(record) = &self.record {
            write!(f, " {record}")?;
        }
        if let Some(field) = &self.field {
            write!(f, " [{field}]")?;
        }
        write!(
            f,
            " ({}): the folder has {} where the projection has {}",
            self.rule, self.source, self.projected
        )
    }
}

/// The rule of the check a mismatch breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckRule {
    UsfmBytes,
    VrsBytes,
    StoryBytes,
    FrameText,
    CarriedFile,
    FileNotDerived,
    RecordMissing,
    RecordAdded,
    RecordField,
    DocumentField,
    RoundTrip,
    MetadataField,
    FoldAnomaly,
    NotProvisional,
}

impl fmt::Display for CheckRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UsfmBytes => "USFM bytes",
            Self::VrsBytes => "vrs.json bytes",
            Self::StoryBytes => "story bytes",
            Self::FrameText => "frame text",
            Self::CarriedFile => "carried file",
            Self::FileNotDerived => "file not derived",
            Self::RecordMissing => "record missing",
            Self::RecordAdded => "record added",
            Self::RecordField => "record field",
            Self::DocumentField => "document field",
            Self::RoundTrip => "round trip",
            Self::MetadataField => "metadata field",
            Self::FoldAnomaly => "fold anomaly",
            Self::NotProvisional => "not the shown value",
        })
    }
}

/// Why a folder is not seeded. Nothing is made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SeedRefusal {
    #[error("{path} cannot be read: {problem}")]
    Io { path: String, problem: String },
    #[error("the folder has no metadata.json, so it is not a translationCore project")]
    NoMetadata,
    #[error(
        "the project has a translationCore 4 journal (ingredients/checking/journal/); the \
         translationCore 4 importer imports it (STORAGE §13.1)"
    )]
    HasJournal,
    #[error(
        "the folder has ingredients/tc/: it is a project of the new translationCore or an \
         exchange file, not a translationCore 4 project"
    )]
    NewFormat,
    #[error(
        "metadata.json names the flavor {found}, which is neither a Bible translation nor Open Bible Stories"
    )]
    Flavor { found: String },
    #[error("{path} cannot be read: {problem}")]
    Unreadable { path: String, problem: String },
    #[error("{path} has schemaVersion {found}, which this seed does not read")]
    Schema { path: String, found: String },
    #[error("{path} repeats the key \"{key}\" in the object at \"{at}\"")]
    DuplicateKey {
        path: String,
        at: String,
        key: String,
    },
    #[error("{path} holds two records for {key}")]
    DuplicateRecord { path: String, key: String },
    #[error("{path}: {why}")]
    UnexpectedFile { path: String, why: String },
    #[error("metadata.json lists {book} in its scope, but the project has no {book}.usfm")]
    ScopeWithoutFile { book: String },
    #[error("{path}: {problem}")]
    BadValue { path: String, problem: String },
    #[error("the project would not come back as it is: {}", list_mismatches(.0))]
    Mismatches(Vec<Mismatch>),
    #[error("the seed could not be made (a bug): {0}")]
    Internal(String),
}

fn list_mismatches(mismatches: &[Mismatch]) -> String {
    mismatches
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

// ---------------------------------------------------------------------------------------
// The seed
// ---------------------------------------------------------------------------------------

/// The fixture backend's project id for a folder (D10 of unit B1a): a UUID of version 8
/// from sha256("tc4-state-seed\0" ‖ the folder's name). STORAGE gives §13.2 no rule for a
/// project id; this is the fixture backend's, not an import rule.
pub fn fixture_project_id(folder_name: &str) -> ProjectId {
    let hash = sha256(&[b"tc4-state-seed\0", folder_name.as_bytes()]);
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    ProjectId::from_bytes(bytes)
}

/// Seeds `folder` as project `project` and checks the result (module docs). Returns the
/// seed only when the check passes; otherwise every mismatch, and nothing is made.
pub fn tc4_state_seed(folder: &Tc4Folder, project: ProjectId) -> Result<StateSeed, SeedRefusal> {
    let built = build(folder, project)?;
    let checked = check::check(folder, &built);
    if !checked.mismatches.is_empty() {
        return Err(SeedRefusal::Mismatches(checked.mismatches));
    }
    let mut declared = built.declared;
    declared.extend(checked.declared);
    let counts = SeedCounts::of(&built.batches);
    Ok(StateSeed {
        project: built.project,
        kind: built.kind,
        device: built.device,
        batches: built.batches,
        fold: built.fold,
        carried: built.carried,
        report: SeedReport {
            declared,
            ignored: folder.ignored.clone(),
            added_files: checked.added_files,
            notes: built.notes,
            counts,
        },
    })
}

/// What a source file is to the seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Usfm(BookId),
    Versification,
    Alignments(BookId),
    Decisions(ToolId, BookId),
    StoryDecisions(ToolId),
    Resources,
    Settings,
    Story(StoryNo),
    Carried,
}

impl Role {
    /// The derived file the projection writes at the source file's path.
    pub(crate) fn derived(self) -> Option<DerivedFile> {
        Some(match self {
            Self::Usfm(book) => DerivedFile::Usfm(book),
            Self::Versification => DerivedFile::Versification,
            Self::Alignments(book) => DerivedFile::Alignments(book),
            Self::Decisions(tool, book) => DerivedFile::Decisions(tool, book),
            Self::StoryDecisions(tool) => DerivedFile::StoryDecisions(tool),
            Self::Resources => DerivedFile::Resources,
            Self::Settings => DerivedFile::Settings,
            Self::Story(story) => DerivedFile::Story(story),
            Self::Carried => return None,
        })
    }
}

/// The seed before its check: what [`StateSeed`] holds, and what the check compares with.
pub(crate) struct Built {
    pub(crate) project: ProjectId,
    pub(crate) kind: ProjectKind,
    pub(crate) device: DeviceId,
    pub(crate) batches: Vec<SeedBatch>,
    pub(crate) fold: FoldState,
    pub(crate) carried: BTreeMap<GitOid, Vec<u8>>,
    pub(crate) roles: BTreeMap<String, Role>,
    /// The free fields of `metadata.json`, by dotted path, as the folder holds them.
    pub(crate) meta_fields: Vec<(String, Value)>,
    /// `resources.json` as the projection must give it back: the file, or its migration.
    pub(crate) expected_resources: Option<Value>,
    /// The records seeded from each alignment and decision file, by its path.
    pub(crate) sidecar_records: BTreeMap<String, Vec<Record>>,
    pub(crate) declared: Vec<DeclaredDifference>,
    pub(crate) notes: Vec<String>,
}

/// One record to seed, its time, and the file it came from.
#[derive(Debug, Clone)]
struct Item {
    record: Record,
    at: WallTime,
    origin: String,
}

/// What every change of one seed shares.
struct Ctx {
    project: ProjectId,
    device: DeviceId,
    cause: Cause,
}

/// A book's files.
#[derive(Default)]
struct BookFiles<'a> {
    usfm: Option<(&'a str, &'a [u8])>,
    alignments: Option<(&'a str, &'a [u8])>,
    decisions: BTreeMap<ToolId, (&'a str, &'a [u8])>,
}

pub(crate) fn build(folder: &Tc4Folder, project: ProjectId) -> Result<Built, SeedRefusal> {
    let meta_doc = read_json(METADATA, &folder.metadata)?;
    let Value::Object(meta) = &meta_doc else {
        return Err(SeedRefusal::Unreadable {
            path: METADATA.into(),
            problem: "it is not a JSON object".into(),
        });
    };
    let kind = kind_of(&meta_doc)?;
    let mut roles = BTreeMap::new();
    for path in folder.files.keys() {
        roles.insert(path.clone(), role_of(path, kind)?);
    }
    let mut device_input = b"import\0tc4-seed".to_vec();
    device_input.extend_from_slice(project.as_bytes());
    let ctx = Ctx {
        project,
        device: DeviceId::derived(DeviceKind::Importer, &device_input),
        cause: Cause::Import(ImportSource::Burrito {
            file_name: folder.name.as_str().into(),
        }),
    };
    let project_at = meta
        .get("meta")
        .and_then(|m| m.get("dateCreated"))
        .and_then(Value::as_str)
        .and_then(parse_utc_millis)
        .map_or(WallTime(SEED_FLOOR), WallTime);
    let item = |record: Record, origin: &str| Item {
        record,
        at: project_at,
        origin: origin.to_owned(),
    };

    // The project's batch.
    let mut declared = Vec::new();
    let mut notes = Vec::new();
    let mut project_items = Vec::new();
    let meta_fields = meta_fields(meta)?;
    for (path, value) in &meta_fields {
        project_items.push(item(
            Record::ProjectMeta {
                field: meta_field(path)?,
                value: Some(json_of(METADATA, path, value)?),
            },
            METADATA,
        ));
    }
    let mut expected_resources = None;
    let mut carried = BTreeMap::new();
    let mut books: BTreeMap<BookId, BookFiles<'_>> = BTreeMap::new();
    let mut stories: BTreeMap<StoryNo, (&str, &[u8])> = BTreeMap::new();
    let mut story_decisions: BTreeMap<ToolId, (&str, &[u8])> = BTreeMap::new();
    for (path, role) in &roles {
        let bytes = folder.files.get(path).map_or(&[][..], Vec::as_slice);
        match *role {
            Role::Versification => project_items.push(item(
                Record::Versification {
                    value: VersificationValue {
                        name: None,
                        bytes: text_of(path, bytes)?.to_owned(),
                    },
                },
                path,
            )),
            Role::Resources => {
                let pins = pins_records(path, &read_json(path, bytes)?)?;
                project_items.extend(pins.records.into_iter().map(|r| item(r, path)));
                expected_resources = Some(pins.expected);
                declared.extend(pins.declared);
            }
            Role::Settings => {
                let records = settings_records(path, &read_json(path, bytes)?)?;
                project_items.extend(records.into_iter().map(|r| item(r, path)));
            }
            Role::Carried => {
                let (record, oid) = carried_record(path, bytes)?;
                project_items.push(item(record, path));
                carried.insert(oid, bytes.to_vec());
            }
            Role::Usfm(book) => books.entry(book).or_default().usfm = Some((path, bytes)),
            Role::Alignments(book) => {
                books.entry(book).or_default().alignments = Some((path, bytes));
            }
            Role::Decisions(tool, book) => {
                books
                    .entry(book)
                    .or_default()
                    .decisions
                    .insert(tool, (path, bytes));
            }
            Role::StoryDecisions(tool) => {
                story_decisions.insert(tool, (path, bytes));
            }
            Role::Story(story) => {
                stories.insert(story, (path, bytes));
            }
        }
    }

    // The texts: each book through the codec, each story file as its base. Decisions are
    // stamped with the hash of the text their target names in a fold of these alone.
    let scopes = if kind == ProjectKind::Bible {
        scopes_of(meta)?
    } else {
        BTreeMap::new()
    };
    for book in scopes.keys() {
        if books.get(book).and_then(|b| b.usfm).is_none() {
            return Err(SeedRefusal::ScopeWithoutFile {
                book: book.code().into(),
            });
        }
    }
    let mut texts: BTreeMap<BatchLabel, Vec<Item>> = BTreeMap::new();
    for (book, files) in &books {
        let Some((path, bytes)) = files.usfm else {
            continue;
        };
        let parts =
            tc_usfm::codec::decompose_bytes(bytes).map_err(|e| SeedRefusal::Unreadable {
                path: path.to_owned(),
                problem: format!("line {}: {}", e.line, e.problem),
            })?;
        let scope = match scopes.get(book) {
            Some(scope) => scope.clone(),
            None => {
                notes.push(format!(
                    "{path}: metadata.json's scope does not list {}, so the whole book is in scope",
                    book.code()
                ));
                Scope::Whole
            }
        };
        let items = texts.entry(BatchLabel::Book(*book)).or_default();
        items.push(item(
            Record::BookPresence {
                book: *book,
                value: BookPresence::Present {
                    scope,
                    how: PresentHow::Added,
                },
            },
            path,
        ));
        items.extend(
            records_from_usfm(*book, &parts)
                .into_iter()
                .map(|r| item(r, path)),
        );
    }
    for (story, (path, bytes)) in &stories {
        let text = text_of(path, bytes)?;
        let decoded = tc_obs::decode_story(text).map_err(|e| SeedRefusal::Unreadable {
            path: (*path).to_owned(),
            problem: e.to_string(),
        })?;
        if decoded.records.story.get() != story.get() {
            return Err(SeedRefusal::UnexpectedFile {
                path: (*path).to_owned(),
                why: format!(
                    "the file holds story {}, but its name says story {}",
                    decoded.records.story.get(),
                    story.get()
                ),
            });
        }
        texts
            .entry(BatchLabel::Story(*story))
            .or_default()
            .push(item(
                Record::StoryBase {
                    story: *story,
                    value: text.to_owned(),
                },
                path,
            ));
    }
    let interim = {
        let mut changes = Vec::new();
        for (label, items) in &texts {
            changes.extend(batch(*label, items.clone(), &ctx)?.changes);
        }
        FoldState::fold(kind, changes.into_iter().map(FoldInput::from))
    };

    // Each book's alignments and decisions, each story's decisions.
    let mut sidecar_records = BTreeMap::new();
    let mut attached: BTreeMap<BatchLabel, Vec<Item>> = BTreeMap::new();
    for (book, files) in &books {
        let label = BatchLabel::Book(*book);
        if let Some((path, bytes)) = files.alignments {
            let records = alignment_records(path, *book, bytes)?;
            attached
                .entry(label)
                .or_default()
                .extend(records.iter().cloned().map(|r| item(r, path)));
            sidecar_records.insert(path.to_owned(), records);
        }
        for (tool, (path, bytes)) in &files.decisions {
            let seeded = decision_records(path, *tool, Some(*book), bytes, &interim, project_at)?;
            sidecar_records.insert(
                (*path).to_owned(),
                seeded.iter().map(|i| i.record.clone()).collect(),
            );
            attached.entry(label).or_default().extend(seeded);
        }
    }
    for (tool, (path, bytes)) in &story_decisions {
        let seeded = decision_records(path, *tool, None, bytes, &interim, project_at)?;
        sidecar_records.insert(
            (*path).to_owned(),
            seeded.iter().map(|i| i.record.clone()).collect(),
        );
        for decision in seeded {
            let story = match &decision.record {
                Record::Check {
                    key:
                        tc_journal::CheckKey {
                            target: CheckTarget::Frame { story, .. },
                            ..
                        },
                    ..
                } => *story,
                _ => {
                    return Err(SeedRefusal::BadValue {
                        path: (*path).to_owned(),
                        problem: "a decision of OBS.json targets no story frame".into(),
                    });
                }
            };
            attached
                .entry(BatchLabel::Story(story))
                .or_default()
                .push(decision);
        }
    }

    // The batches, in their order: the project, then the books or the stories.
    let mut labels: BTreeSet<BatchLabel> = texts.keys().copied().collect();
    labels.extend(attached.keys().copied());
    let mut batches = Vec::new();
    if !project_items.is_empty() {
        batches.push(batch(BatchLabel::Project, project_items, &ctx)?);
    }
    for label in labels {
        let mut items = texts.remove(&label).unwrap_or_default();
        items.extend(attached.remove(&label).unwrap_or_default());
        batches.push(batch(label, items, &ctx)?);
    }
    let fold = seal_and_fold(&ctx, kind, project_at, &batches)?;
    Ok(Built {
        project,
        kind,
        device: ctx.device,
        batches,
        fold,
        carried,
        roles,
        meta_fields,
        expected_resources,
        sidecar_records,
        declared,
        notes,
    })
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    for part in parts {
        context.update(part);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(context.finish().as_ref());
    out
}

/// sha256("tc4-seed-batch\0" ‖ projectId ‖ label)[..16].
fn batch_id(project: ProjectId, label: BatchLabel) -> BatchId {
    let hash = sha256(&[
        b"tc4-seed-batch\0",
        project.as_bytes(),
        label.text().as_bytes(),
    ]);
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    BatchId::from_bytes(bytes)
}

/// One batch of `items`, sorted by record key, so the changes do not depend on the order
/// records stand in a source file. Each change's seq is its index in the batch.
fn batch(label: BatchLabel, items: Vec<Item>, ctx: &Ctx) -> Result<SeedBatch, SeedRefusal> {
    let mut keyed: Vec<(RecordKey, Item)> = items
        .into_iter()
        .map(|item| (item.record.key(), item))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    for pair in keyed.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(SeedRefusal::DuplicateRecord {
                path: pair[1].1.origin.clone(),
                key: describe_key(&pair[1].0),
            });
        }
    }
    let n = u32::try_from(keyed.len())
        .map_err(|_| SeedRefusal::Internal("a batch of more than 2³² records".into()))?;
    let batch = BatchRef::new(batch_id(ctx.project, label), n)
        .ok_or_else(|| SeedRefusal::Internal(format!("the batch {} is empty", label.text())))?;
    let mut changes = Vec::with_capacity(keyed.len());
    for (seq, (key, item)) in (0u64..).zip(keyed) {
        let change = Change::new(ChangeBody {
            v: FORMAT_MINOR,
            record: item.record,
            parents: Vec::new(),
            device: ctx.device,
            seq: DeviceSeq(seq),
            at: item.at,
            batch,
            cause: ctx.cause.clone(),
        })
        .map_err(|e| SeedRefusal::BadValue {
            path: item.origin.clone(),
            problem: format!("{}: {e}", describe_key(&key)),
        })?;
        changes.push(change);
    }
    Ok(SeedBatch { label, changes })
}

/// Seals every change into packs of a fixed Local sealer, reads the packs back and folds
/// what they hold (AD36): the fold takes changes exactly as a project's packs give them.
fn seal_and_fold(
    ctx: &Ctx,
    kind: ProjectKind,
    sealed: WallTime,
    batches: &[SeedBatch],
) -> Result<FoldState, SeedRefusal> {
    let mut sealer = [0u8; 15];
    sealer.copy_from_slice(&sha256(&[b"tc4-seed-sealer\0", ctx.project.as_bytes()])[..15]);
    let spec = PackSpec {
        project: ctx.project,
        sealer: DeviceId::local(sealer),
        sealed,
    };
    let lines: Vec<Verified> = batches
        .iter()
        .flat_map(|b| b.changes.iter().cloned().map(Verified::from))
        .collect();
    let packs = seal(&spec, &lines).map_err(|e| SeedRefusal::Internal(e.to_string()))?;
    if !packs.set_aside.is_empty() {
        return Err(SeedRefusal::Internal(format!(
            "{} changes are too long for a pack",
            packs.set_aside.len()
        )));
    }
    let mut inputs = Vec::with_capacity(lines.len());
    let mut read_back = BTreeSet::new();
    for pack in &packs.packs {
        let read = read_pack(&pack.path.to_string(), &pack.bytes, ctx.project);
        if !read.is_valid() {
            return Err(SeedRefusal::Internal(format!(
                "the pack {} does not read back: {:?}",
                pack.path, read.status
            )));
        }
        for entry in read.entries {
            if entry.as_change().is_none() {
                return Err(SeedRefusal::Internal(format!(
                    "the pack {} quarantines change {}",
                    pack.path,
                    entry.id()
                )));
            }
            read_back.insert(entry.id());
            inputs.push(FoldInput::new(entry, Some(read.id)));
        }
    }
    let written: BTreeSet<_> = lines.iter().map(Verified::id).collect();
    if written != read_back || inputs.len() != lines.len() {
        return Err(SeedRefusal::Internal(
            "the packs do not give back every change".into(),
        ));
    }
    Ok(FoldState::fold(kind, inputs))
}

// ---------------------------------------------------------------------------------------
// Reading the folder's files
// ---------------------------------------------------------------------------------------

fn read_json(path: &str, bytes: &[u8]) -> Result<Value, SeedRefusal> {
    read_json_strict(bytes).map_err(|e| match e {
        StrictError::Duplicate(d) => SeedRefusal::DuplicateKey {
            path: path.to_owned(),
            at: d.at,
            key: d.key,
        },
        StrictError::Syntax(problem) => SeedRefusal::Unreadable {
            path: path.to_owned(),
            problem,
        },
    })
}

fn text_of<'a>(path: &str, bytes: &'a [u8]) -> Result<&'a str, SeedRefusal> {
    std::str::from_utf8(bytes).map_err(|e| SeedRefusal::Unreadable {
        path: path.to_owned(),
        problem: format!("not UTF-8 text from byte {}", e.valid_up_to()),
    })
}

fn json_of(path: &str, what: &str, value: &Value) -> Result<Json, SeedRefusal> {
    Json::try_from(value).map_err(|e| SeedRefusal::BadValue {
        path: path.to_owned(),
        problem: format!("{what}: {e}"),
    })
}

/// A Bible translation, or Open Bible Stories (tC4 `isObsMetadata`).
fn kind_of(meta: &Value) -> Result<ProjectKind, SeedRefusal> {
    if tc_obs::ids::is_obs_metadata(meta) {
        return Ok(ProjectKind::Stories);
    }
    let flavor_type = &meta["type"]["flavorType"];
    if flavor_type["name"] == "scripture" && flavor_type["flavor"]["name"] == "textTranslation" {
        return Ok(ProjectKind::Bible);
    }
    let name = |v: &Value| v.as_str().unwrap_or("(none)").to_owned();
    Err(SeedRefusal::Flavor {
        found: format!(
            "{}/{}",
            name(&flavor_type["name"]),
            name(&flavor_type["flavor"]["name"])
        ),
    })
}

/// The book an exact upper-case code names; `BookId`'s parser takes any letter case, and a
/// file in another case would be projected at another path.
fn book_of(code: &str) -> Option<BookId> {
    code.parse::<BookId>()
        .ok()
        .filter(|book| book.code() == code)
}

/// What the file at `path` is to the seed. A file the format derives but the seed does not
/// understand refuses: it could not be kept as a carried file either (rule V8).
fn role_of(path: &str, kind: ProjectKind) -> Result<Role, SeedRefusal> {
    let unexpected = |why: &str| {
        Err(SeedRefusal::UnexpectedFile {
            path: path.to_owned(),
            why: why.to_owned(),
        })
    };
    let rest = path.strip_prefix(INGREDIENTS).unwrap_or(path);
    let bible = kind == ProjectKind::Bible;
    if rest == "vrs.json" {
        return Ok(Role::Versification);
    }
    if let Some(checking) = rest.strip_prefix("checking/") {
        match checking {
            "resources.json" => return Ok(Role::Resources),
            "settings.json" => return Ok(Role::Settings),
            _ => {}
        }
        let parts: Vec<&str> = checking.split('/').collect();
        if let [folder, file] = parts.as_slice()
            && let Some(stem) = file.strip_suffix(".json")
        {
            if *folder == "alignments" {
                return match (bible, book_of(stem)) {
                    (true, Some(book)) => Ok(Role::Alignments(book)),
                    (true, None) => {
                        unexpected("an alignments file is named by its book's upper-case code")
                    }
                    (false, _) => unexpected("an Open Bible Stories project has no alignments"),
                };
            }
            if let Some(tool) = ToolId::parse(folder) {
                return match (bible, stem) {
                    (false, "OBS") => Ok(Role::StoryDecisions(tool)),
                    (true, "OBS") => unexpected("a Bible project has no story decisions"),
                    (true, _) => match book_of(stem) {
                        Some(book) => Ok(Role::Decisions(tool, book)),
                        None => {
                            unexpected("a decisions file is named by its book's upper-case code")
                        }
                    },
                    (false, _) => {
                        unexpected("an Open Bible Stories project keeps its decisions in OBS.json")
                    }
                };
            }
        }
        return unexpected(
            "translationCore writes every file under ingredients/checking/ and does not know \
             this one, so it cannot keep it",
        );
    }
    if !rest.contains('/')
        && let Some(stem) = rest.strip_suffix(".usfm")
    {
        return match (bible, book_of(stem)) {
            (true, Some(book)) => Ok(Role::Usfm(book)),
            (true, None) => {
                unexpected("a book file is named by its upper-case code, such as TIT.usfm")
            }
            (false, _) => unexpected("an Open Bible Stories project has no book files"),
        };
    }
    if let Some(name) = rest.strip_prefix("content/")
        && !name.contains('/')
        && name.ends_with(".md")
    {
        if bible {
            return unexpected("a Bible project has no story files");
        }
        let story = tc_obs::ids::story_number_of(rest).and_then(StoryNo::new);
        return match story {
            Some(story) => Ok(Role::Story(story)),
            None => unexpected("a story file is named 01.md to 50.md"),
        };
    }
    Ok(Role::Carried)
}

/// The free fields of `metadata.json` (every path `tc_burrito::is_derived_field` does not
/// name), with `meta`, `idAuthorities`, `identification` and `identification.primary`
/// broken into their members, as the project's records hold them (STORAGE §3.4).
fn meta_fields(meta: &Map<String, Value>) -> Result<Vec<(String, Value)>, SeedRefusal> {
    use tc_burrito::is_derived_field;
    let not_object = |path: &str| SeedRefusal::BadValue {
        path: METADATA.into(),
        problem: format!("{path} is not an object"),
    };
    let mut fields: Vec<(Vec<String>, Value)> = Vec::new();
    for (key, value) in meta {
        if is_derived_field(key) {
            continue;
        }
        if !matches!(key.as_str(), "meta" | "idAuthorities" | "identification") {
            fields.push((vec![key.clone()], value.clone()));
            continue;
        }
        let Value::Object(members) = value else {
            return Err(not_object(key));
        };
        for (member, value) in members {
            let path = format!("{key}.{member}");
            if is_derived_field(&path) {
                continue;
            }
            if path != "identification.primary" {
                fields.push((vec![key.clone(), member.clone()], value.clone()));
                continue;
            }
            let Value::Object(primary) = value else {
                return Err(not_object(&path));
            };
            for (authority, value) in primary {
                if !is_derived_field(&format!("{path}.{authority}")) {
                    fields.push((
                        vec![key.clone(), member.clone(), authority.clone()],
                        value.clone(),
                    ));
                }
            }
        }
    }
    // A key with a dot in it would be stored at a dotted path that names another place.
    let mut nested = Map::new();
    let mut dotted = Map::new();
    for (segments, value) in &fields {
        set_segments(&mut nested, segments, value.clone());
        let path = segments.join(".");
        let split: Vec<String> = path.split('.').map(str::to_owned).collect();
        set_segments(&mut dotted, &split, value.clone());
    }
    if nested != dotted {
        return Err(SeedRefusal::BadValue {
            path: METADATA.into(),
            problem: "a key holds a dot, so its field cannot be kept at its own path".into(),
        });
    }
    Ok(fields
        .into_iter()
        .map(|(segments, value)| (segments.join("."), value))
        .collect())
}

fn set_segments(root: &mut Map<String, Value>, segments: &[String], value: Value) {
    let Some((last, parents)) = segments.split_last() else {
        return;
    };
    let mut at = root;
    for segment in parents {
        let next = at
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !next.is_object() {
            *next = Value::Object(Map::new());
        }
        match next {
            Value::Object(map) => at = map,
            _ => return,
        }
    }
    at.insert(last.clone(), value);
}

/// The record a free field of `metadata.json` is: one of the five modelled fields, or
/// `Extra` at its dotted path.
fn meta_field(path: &str) -> Result<MetaField, SeedRefusal> {
    Ok(match path {
        "identification.name" => MetaField::Name,
        "identification.abbreviation" => MetaField::Abbreviation,
        "identification.description" => MetaField::Description,
        "languages" => MetaField::Languages,
        "copyright" => MetaField::Copyright,
        _ => MetaField::Extra(MetaPath::parse(path).map_err(|e| SeedRefusal::BadValue {
            path: METADATA.into(),
            problem: e.to_string(),
        })?),
    })
}

/// Each book's scope from `type.flavorType.currentScope`: `[]` is the whole book, ranges a
/// passage set (D26).
fn scopes_of(meta: &Map<String, Value>) -> Result<BTreeMap<BookId, Scope>, SeedRefusal> {
    let bad = |problem: String| SeedRefusal::BadValue {
        path: METADATA.into(),
        problem,
    };
    let Some(scope) = meta
        .get("type")
        .and_then(|t| t.get("flavorType"))
        .and_then(|t| t.get("currentScope"))
    else {
        return Ok(BTreeMap::new());
    };
    let Value::Object(books) = scope else {
        return Err(bad("currentScope is not an object".into()));
    };
    let mut out = BTreeMap::new();
    for (code, ranges) in books {
        let book =
            book_of(code).ok_or_else(|| bad(format!("currentScope names no book {code:?}")))?;
        let Value::Array(ranges) = ranges else {
            return Err(bad(format!("the scope of {code} is not a list")));
        };
        let mut parsed = Vec::with_capacity(ranges.len());
        for range in ranges {
            let text = range
                .as_str()
                .ok_or_else(|| bad(format!("a scope range of {code} is not text")))?;
            let read: ScopeRange = text
                .parse()
                .map_err(|e| bad(format!("{code} {text:?}: {e}")))?;
            // A range written another way would come back in another form.
            if read.to_string() != text {
                return Err(bad(format!(
                    "{code} {text:?} would be written {:?}",
                    read.to_string()
                )));
            }
            parsed.push(read);
        }
        let scope = match ScopeRanges::new(parsed) {
            Some(ranges) => Scope::Ranges(ranges),
            None => Scope::Whole,
        };
        out.insert(book, scope);
    }
    Ok(out)
}

/// The pin records of a `resources.json`, the document the projection must give back, and
/// the migration when there was one.
struct PinsSeed {
    records: Vec<Record>,
    expected: Value,
    declared: Option<DeclaredDifference>,
}

fn pins_records(path: &str, doc: &Value) -> Result<PinsSeed, SeedRefusal> {
    let bad = |problem: String| SeedRefusal::BadValue {
        path: path.to_owned(),
        problem,
    };
    let Value::Object(top) = doc else {
        return Err(bad("it is not a JSON object".into()));
    };
    let (expected, declared) = match top.get("schemaVersion").and_then(Value::as_u64) {
        Some(2) => (doc.clone(), None),
        Some(1) => (
            migrate_pins_v1(path, top)?,
            Some(DeclaredDifference::ResourcesMigrated { from: 1 }),
        ),
        _ => {
            return Err(SeedRefusal::Schema {
                path: path.to_owned(),
                found: top
                    .get("schemaVersion")
                    .map_or_else(|| "(none)".into(), Value::to_string),
            });
        }
    };
    let Value::Object(top) = &expected else {
        return Err(SeedRefusal::Internal("a migrated resources.json".into()));
    };
    let mut records = Vec::new();
    let group = |name: &str, group: PinGroup, value: &Value| -> Result<Record, SeedRefusal> {
        let value = PinGroupValue::from_json(value)
            .ok_or_else(|| bad(format!("{name} is not a group of pins tC4 could write")))?;
        Ok(Record::Pins { group, value })
    };
    let extra = |level: ResourcesLevel, name: &str, value: &Value| -> Result<Record, SeedRefusal> {
        Ok(Record::ResourcesExtra {
            key: ResourcesKey::new(level, name).map_err(|e| bad(e.to_string()))?,
            value: Some(json_of(path, name, value)?),
        })
    };
    for (name, value) in top {
        match name.as_str() {
            "schemaVersion" => {}
            "languageSets" | "resources" => {
                let Value::Object(members) = value else {
                    return Err(bad(format!("{name} is not an object")));
                };
                let level = if name == "languageSets" {
                    ResourcesLevel::LanguageSets
                } else {
                    ResourcesLevel::Resources
                };
                for (member, value) in members {
                    let at = format!("{name}.{member}");
                    records.push(match (level, member.as_str()) {
                        (ResourcesLevel::LanguageSets, "primary") => {
                            group(&at, PinGroup::Primary, value)?
                        }
                        (ResourcesLevel::LanguageSets, "fallback") => {
                            group(&at, PinGroup::Fallback, value)?
                        }
                        (ResourcesLevel::Resources, "originalLanguage") => {
                            group(&at, PinGroup::OriginalLanguage, value)?
                        }
                        (ResourcesLevel::Resources, "lexicon") => {
                            group(&at, PinGroup::Lexicon, value)?
                        }
                        _ => extra(level, member, value)?,
                    });
                }
            }
            "extraScripture" => records.push(group(
                name,
                PinGroup::ExtraScripture,
                &serde_json::json!({ "extraScripture": value }),
            )?),
            _ => records.push(extra(ResourcesLevel::Top, name, value)?),
        }
    }
    Ok(PinsSeed {
        records,
        expected,
        declared,
    })
}

/// BURRITO-SPEC §5.3's migration of a `schemaVersion: 1` file, the only definition of one
/// (no public tC4 commit wrote it): its `gatewayLanguage` and the set slots under
/// `resources` become the fallback set, `primary` equals `fallback`, a `translationWordsLinks`
/// pin is added when the file has none (the installed English suite's `en_twl`, D97), and
/// `originalLanguage`, `lexicon`, `extraScripture` and every other member carry over.
fn migrate_pins_v1(path: &str, top: &Map<String, Value>) -> Result<Value, SeedRefusal> {
    let bad = |problem: &str| SeedRefusal::BadValue {
        path: path.to_owned(),
        problem: format!("schemaVersion 1: {problem}"),
    };
    if top.contains_key("languageSets") {
        return Err(bad("the file already has languageSets"));
    }
    let resources = match top.get("resources") {
        Some(Value::Object(r)) => r.clone(),
        None => Map::new(),
        Some(_) => return Err(bad("resources is not an object")),
    };
    let gateway = match (top.get("gatewayLanguage"), resources.get("gatewayLanguage")) {
        (Some(a), Some(b)) if a != b => {
            return Err(bad("two different gatewayLanguage values"));
        }
        (Some(a), _) | (None, Some(a)) => a.clone(),
        (None, None) => return Err(bad("the file names no gatewayLanguage")),
    };
    let mut set = Map::new();
    set.insert("gatewayLanguage".into(), gateway);
    let mut kept = Map::new();
    for (name, value) in resources {
        if name == "gatewayLanguage" {
            continue;
        }
        if SetSlot::parse(&name).is_some() {
            set.insert(name, value);
        } else {
            kept.insert(name, value);
        }
    }
    if !set.contains_key(SetSlot::TranslationWordsLinks.name()) {
        let twl = serde_json::to_value(tc_resources::suite::english_twl())
            .map_err(|e| SeedRefusal::Internal(e.to_string()))?;
        set.insert(SetSlot::TranslationWordsLinks.name().into(), twl);
    }
    let mut out = Map::new();
    out.insert("schemaVersion".into(), Value::from(2));
    let mut sets = Map::new();
    sets.insert("primary".into(), Value::Object(set.clone()));
    sets.insert("fallback".into(), Value::Object(set));
    out.insert("languageSets".into(), Value::Object(sets));
    if !kept.is_empty() {
        out.insert("resources".into(), Value::Object(kept));
    }
    for (name, value) in top {
        if !matches!(
            name.as_str(),
            "schemaVersion" | "gatewayLanguage" | "resources"
        ) {
            out.insert(name.clone(), value.clone());
        }
    }
    Ok(Value::Object(out))
}

/// One `Setting` record per top-level key of `settings.json` but `schemaVersion`, which
/// must be 1.
fn settings_records(path: &str, doc: &Value) -> Result<Vec<Record>, SeedRefusal> {
    let Value::Object(top) = doc else {
        return Err(SeedRefusal::BadValue {
            path: path.to_owned(),
            problem: "it is not a JSON object".into(),
        });
    };
    check_schema(path, top)?;
    let mut out = Vec::new();
    for (name, value) in top {
        if name == "schemaVersion" {
            continue;
        }
        out.push(Record::Setting {
            key: SettingKey::parse(name).map_err(|e| SeedRefusal::BadValue {
                path: path.to_owned(),
                problem: e.to_string(),
            })?,
            value: Some(json_of(path, name, value)?),
        });
    }
    Ok(out)
}

/// Every sidecar and `settings.json` of tC4 is `schemaVersion` 1.
fn check_schema(path: &str, top: &Map<String, Value>) -> Result<(), SeedRefusal> {
    match top.get("schemaVersion") {
        Some(v) if v.as_u64() == Some(1) => Ok(()),
        other => Err(SeedRefusal::Schema {
            path: path.to_owned(),
            found: other.map_or_else(|| "(none)".into(), Value::to_string),
        }),
    }
}

/// A file the format does not derive, kept by its git blob id (STORAGE §11.4).
fn carried_record(path: &str, bytes: &[u8]) -> Result<(Record, GitOid), SeedRefusal> {
    let repo_path = RepoPath::parse(path).map_err(|e| SeedRefusal::BadValue {
        path: path.to_owned(),
        problem: format!("the file cannot be kept under this name: {e}"),
    })?;
    let mut context = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
    context.update(format!("blob {}\0", bytes.len()).as_bytes());
    context.update(bytes);
    let mut oid = [0u8; 20];
    oid.copy_from_slice(context.finish().as_ref());
    let oid = GitOid::from_bytes(oid);
    let record = Record::CarriedFile {
        path: repo_path,
        value: CarriedValue::Present {
            oid,
            size: bytes.len() as u64,
            md5: Md5::of(bytes),
        },
    };
    Ok((record, oid))
}

/// The records of an alignments sidecar. tC4's `invalid: true` becomes `markedInvalid`
/// (§13.2: `marked_invalid` from `invalid`), which the decoder reads; the decoder ignores
/// `invalid` itself, a flag the fold computes.
fn alignment_records(path: &str, book: BookId, bytes: &[u8]) -> Result<Vec<Record>, SeedRefusal> {
    let mut doc = read_json(path, bytes)?;
    let Value::Object(top) = &mut doc else {
        return Err(SeedRefusal::BadValue {
            path: path.to_owned(),
            problem: "it is not a JSON object".into(),
        });
    };
    check_schema(path, top)?;
    if let Some(Value::Object(chapters)) = top.get_mut("chapters") {
        for verses in chapters.values_mut() {
            let Value::Object(verses) = verses else {
                continue;
            };
            for record in verses.values_mut() {
                if let Value::Object(record) = record
                    && record.get("invalid") == Some(&Value::Bool(true))
                    && !record.contains_key("markedInvalid")
                {
                    record.insert("markedInvalid".into(), Value::Bool(true));
                }
            }
        }
    }
    let rewritten = serde_json::to_vec(&doc).map_err(|e| SeedRefusal::Internal(e.to_string()))?;
    let records = decode_alignments(book, &rewritten).map_err(|e| SeedRefusal::Unreadable {
        path: path.to_owned(),
        problem: e.to_string(),
    })?;
    no_two_on_one_key(path, &records)?;
    Ok(records)
}

/// tC4 refuses "co-present records on one identity key", and the fold would show only one
/// of them.
fn no_two_on_one_key(path: &str, records: &[Record]) -> Result<(), SeedRefusal> {
    let mut seen = BTreeSet::new();
    for record in records {
        let key = record.key();
        if !seen.insert(key.clone()) {
            return Err(SeedRefusal::DuplicateRecord {
                path: path.to_owned(),
                key: describe_key(&key),
            });
        }
    }
    Ok(())
}

/// The decision records of a decisions sidecar, each with its time: the decision's own
/// `modifiedTimestamp` (tC4 R-8.8.2), which the projection writes back from it.
///
/// Each tC4 record is first rewritten into the fields the decoder reads (the rule of
/// `crates/tc-journal/tests/screens_and_fold.rs`, `seeded`): `importedInvalid` from
/// `invalidated`; `userStatus` from `status` when the record is not invalidated, and on an
/// invalidated one `todo` if tC4 says To do, `invalid` if `userInvalid` says the Invalid is
/// the person's own, else nothing, since tC4's `status` then only repeats the invalidation.
/// `verseTextMd5` is then the hash of the text the target names now (§13.2), read from
/// `interim`, a fold of the project's texts, as the projection reads it.
fn decision_records(
    path: &str,
    tool: ToolId,
    book: Option<BookId>,
    bytes: &[u8],
    interim: &FoldState,
    project_at: WallTime,
) -> Result<Vec<Item>, SeedRefusal> {
    let mut doc = read_json(path, bytes)?;
    let Value::Object(top) = &mut doc else {
        return Err(SeedRefusal::BadValue {
            path: path.to_owned(),
            problem: "it is not a JSON object".into(),
        });
    };
    check_schema(path, top)?;
    let placeholder = verse_text_md5_v1("").to_hex();
    let mut times = Vec::new();
    let mut own_hash = Vec::new();
    if let Some(Value::Array(decisions)) = top.get_mut("decisions") {
        for decision in decisions.iter_mut() {
            let Value::Object(m) = decision else {
                times.push(project_at);
                own_hash.push(false);
                continue;
            };
            let invalidated = m.get("invalidated") == Some(&Value::Bool(true));
            let user_invalid = m.get("userInvalid") == Some(&Value::Bool(true));
            let status = m.get("status").and_then(Value::as_str).map(str::to_owned);
            let user_status = match (invalidated, status.as_deref()) {
                (false, status) => status.map(str::to_owned),
                (true, Some("todo")) => Some("todo".to_owned()),
                (true, _) if user_invalid => Some("invalid".to_owned()),
                (true, _) => None,
            };
            m.entry("importedInvalid")
                .or_insert(Value::Bool(invalidated));
            if let Some(user_status) = user_status {
                m.entry("userStatus").or_insert(Value::from(user_status));
            }
            own_hash.push(m.contains_key("verseTextMd5"));
            m.entry("verseTextMd5")
                .or_insert(Value::from(placeholder.clone()));
            times.push(
                m.get("modifiedTimestamp")
                    .and_then(Value::as_str)
                    .and_then(parse_utc_millis)
                    .map_or(project_at, WallTime),
            );
        }
    }
    let rewritten = serde_json::to_vec(&doc).map_err(|e| SeedRefusal::Internal(e.to_string()))?;
    let mut records =
        decode_decisions(tool, book, &rewritten).map_err(|e| SeedRefusal::Unreadable {
            path: path.to_owned(),
            problem: e.to_string(),
        })?;
    if records.len() != times.len() {
        return Err(SeedRefusal::Internal(format!(
            "{path}: {} decisions decoded from {}",
            records.len(),
            times.len()
        )));
    }
    let book_texts = book.map(|b| interim.check_texts(Collection::Book(b)));
    let story_texts = interim.check_texts(Collection::Stories);
    for (record, keep) in records.iter_mut().zip(&own_hash) {
        let Record::Check {
            key,
            value: CheckValue::Decision(decision),
        } = record
        else {
            continue;
        };
        let texts = match (&key.target, book) {
            (CheckTarget::Verse { book: target, .. }, Some(file)) if *target == file => {
                book_texts.as_ref()
            }
            (CheckTarget::Frame { .. }, None) => Some(&story_texts),
            _ => {
                return Err(SeedRefusal::BadValue {
                    path: path.to_owned(),
                    problem: format!("{key} does not belong in this file"),
                });
            }
        };
        if !keep {
            decision.judgement.verse_text_md5 = texts
                .and_then(|t| t.text(&key.target))
                .map_or_else(|| verse_text_md5_v1(""), |t| t.md5());
        }
    }
    no_two_on_one_key(path, &records)?;
    Ok(records
        .into_iter()
        .zip(times)
        .map(|(record, at)| Item {
            record,
            at,
            origin: path.to_owned(),
        })
        .collect())
}

// ---------------------------------------------------------------------------------------
// Words for keys and files
// ---------------------------------------------------------------------------------------

/// A record key in words, such as `TIT 1:1`, `translationNotes TIT 1:4 gr8c #1` or
/// `story 3`.
pub(crate) fn describe_key(key: &RecordKey) -> String {
    match key {
        RecordKey::BookPresence(book) => format!("{} presence", book.code()),
        RecordKey::BookHeader(book) => format!("{} header", book.code()),
        RecordKey::ChapterFront(book, c) => format!("{} {c} front", book.code()),
        RecordKey::Verse(book, c, v) => format!("{} {c}:{v}", book.code()),
        RecordKey::BookLayout(book) => format!("{} layout", book.code()),
        RecordKey::Alignment(book, c, v) => format!("alignment {} {c}:{v}", book.code()),
        RecordKey::Check(check) => check.to_string(),
        RecordKey::Pins(group) => format!("pins {group:?}"),
        RecordKey::ResourcesExtra(key) => format!("resources {:?} {}", key.level(), key.name()),
        RecordKey::Setting(name) => format!("setting {name}"),
        RecordKey::ProjectMeta(field) => field.path(),
        RecordKey::Versification => "versification".into(),
        RecordKey::StoryBase(story) => format!("story {}", story.get()),
        RecordKey::Frame(story, frame) => format!("story {} frame {}", story.get(), frame.0),
        RecordKey::StoryRef(story) => format!("story {} reference", story.get()),
        RecordKey::CarriedFile(path) => path.to_string(),
        other => format!("{} {other:?}", other.kind_name()),
    }
}

/// The source file a record comes from.
pub(crate) fn path_of_key(key: &RecordKey) -> String {
    match key {
        RecordKey::BookPresence(_) | RecordKey::ProjectMeta(_) => METADATA.into(),
        RecordKey::BookHeader(book)
        | RecordKey::ChapterFront(book, _)
        | RecordKey::Verse(book, ..)
        | RecordKey::BookLayout(book) => DerivedFile::Usfm(*book).path(),
        RecordKey::Alignment(book, ..) => DerivedFile::Alignments(*book).path(),
        RecordKey::Check(check) => match check.book() {
            Some(book) => DerivedFile::Decisions(check.tool, book).path(),
            None => DerivedFile::StoryDecisions(check.tool).path(),
        },
        RecordKey::Pins(_) | RecordKey::ResourcesExtra(_) => DerivedFile::Resources.path(),
        RecordKey::Setting(_) => DerivedFile::Settings.path(),
        RecordKey::Versification => DerivedFile::Versification.path(),
        RecordKey::StoryBase(story) | RecordKey::Frame(story, _) | RecordKey::StoryRef(story) => {
            DerivedFile::Story(*story).path()
        }
        RecordKey::CarriedFile(path) => path.to_string(),
        other => other.kind_name().to_owned(),
    }
}

#[cfg(test)]
mod tests;
