//! The fixture backend (APP-DESIGN §4.2, AD6, AD35; feature `fixture-backend`, never in a
//! release build): projects held as folds in memory, opened through the same seam as the
//! store.
//!
//! It runs no command: a command answers `Unavailable` unless a test queued an outcome for it
//! ([`Scripted`]), so the save behaviour of the UI is testable before tc-store runs saves
//! without a second copy of STORAGE's rules. Tokens are signed with a fixed installation
//! secret, so every run mints the same text. Teammates' changes are made by tc-journal's own
//! encoder and folded as an intake would fold them (AD36).
//!
//! Like tc-store, it refuses a second open of a folder whose handle is live (STORAGE §6.1
//! O1), so the session registry's sharing is tested against a backend that would refuse it.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tc_api::event::{CheckpointOutcome, SessionMode};
use tc_api::ids::ProjectRef;
use tc_api::write::ReadOnlyReason;
use tc_journal::command::{Command, DraftField};
use tc_journal::fold::{Delta, FoldInput, FoldState, ProjectKind};
use tc_journal::token::TokenError;
use tc_journal::{BookId, Change, ChangeId, DeviceId, GitOid, RecordKey, ShownToken, TokenKey};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::seed::StateSeed;
use super::{
    BoxFuture, CheckpointReason, CommandError, CopyFailure, DraftToken, EditToken, Listing,
    OfferedDraft, OpenFailure, ProjectEntry, ProjectFacts, ProjectHandle, ProjectStore, Published,
    ReviewToken, Snapshot, StoreCause, StoreCommand, StoreCounters, StoreError, StoreEvent,
    StoreReply, Stored, SummaryFiles, SummaryStamp, SummaryWatch,
};
use crate::counters::CountingSnapshot;
use crate::jobs::ProgressSink;

/// The installation secret of every memory backend: tokens are the same on every run and in
/// every backend, so a test can name one.
const MEMORY_INSTALLATION_SECRET: [u8; 32] = *b"tc4-memory-backend-test-secret!!";

/// The memory backend's own device in every project: "this computer", whose person is mine.
pub const MEMORY_DEVICE: [u8; 15] = [0xB1; 15];

pub use super::Door43Remote;

/// A project the memory backend holds.
#[derive(Debug, Clone)]
pub struct MemoryProject {
    pub id: tc_journal::ProjectId,
    pub kind: ProjectKind,
    pub fold: FoldState,
    /// The bytes of carried files, by git blob id.
    pub carried: BTreeMap<GitOid, Vec<u8>>,
    pub remote: Option<Door43Remote>,
}

impl MemoryProject {
    /// A checked seed of a tC4 project (AD7).
    pub fn from_seed(seed: StateSeed, remote: Option<Door43Remote>) -> Self {
        Self {
            id: seed.project,
            kind: seed.kind,
            fold: seed.fold,
            carried: seed.carried,
            remote,
        }
    }

    /// A project with no change.
    pub fn empty(id: tc_journal::ProjectId, kind: ProjectKind) -> Self {
        Self {
            id,
            kind,
            fold: FoldState::new(kind),
            carried: BTreeMap::new(),
            remote: None,
        }
    }

    /// A project of exactly `changes`, folded by tc-journal's fold.
    pub fn from_changes(
        id: tc_journal::ProjectId,
        kind: ProjectKind,
        changes: Vec<Change>,
    ) -> Self {
        Self {
            id,
            kind,
            fold: FoldState::fold(kind, changes.into_iter().map(FoldInput::from)),
            carried: BTreeMap::new(),
            remote: None,
        }
    }
}

/// What a command submitted to an open handle does instead of running (AD6).
#[derive(Debug, Clone)]
pub enum Scripted {
    /// A fresh edit token for each displayed value, naming the heads the command's token
    /// named; the fold does not change; durable.
    Written,
    /// As `Written`, with the disk's warning.
    WrittenNotDurable(String),
    /// The disk refused it.
    NotWritten(String),
    /// The next outcome, after this long. The reply waits; the caller never does.
    After(Duration, Box<Scripted>),
    /// The project becomes read-only (a `Mode` event): this command and every later one is
    /// refused.
    ReadOnly(ReadOnlyReason),
    /// These changes reach the fold (a new snapshot, a `Committed` event), then the next
    /// outcome.
    TeammateHeads(Vec<Change>, Box<Scripted>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MemoryError {
    #[error("the memory backend holds no project {0}")]
    UnknownFolder(ProjectRef),
    #[error("the memory backend already holds a project {0}")]
    FolderTaken(ProjectRef),
    #[error("{0} shows no text verse to edit")]
    NotATextVerse(String),
    #[error("a teammate's change could not be made: {0}")]
    Change(String),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Every change under these locks is a store of whole values.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A fold with the key its tokens are signed with. The fold is shared: a snapshot of each open
/// and of each step is an `Arc` clone, never a copy of the fold.
struct SignedFold {
    fold: Arc<FoldState>,
    seq: u64,
    signing: Arc<TokenKey>,
}

impl Published for SignedFold {
    fn fold(&self) -> &FoldState {
        &self.fold
    }

    fn seq(&self) -> u64 {
        self.seq
    }

    fn token(
        &self,
        key: &RecordKey,
        displayed: Option<ChangeId>,
    ) -> Result<ShownToken, TokenError> {
        self.fold.token(&self.signing, key, displayed)
    }

    fn review_token(&self, key: &RecordKey) -> Result<ShownToken, TokenError> {
        self.fold.review_token(&self.signing, key)
    }
}

/// One folder's project and what happens to it.
struct ProjectState {
    seeded: MemoryProject,
    signing: Arc<TokenKey>,
    live: Mutex<Live>,
}

struct Live {
    /// The project now: what the next open gets, and an open handle's newest snapshot.
    current: Arc<FoldState>,
    /// The open handle, by the number of its open; `None` while closed.
    open: Option<u64>,
    opens: u64,
    published: Snapshot,
    seq: u64,
    mode: SessionMode,
    scripts: VecDeque<Scripted>,
    subscribers: Vec<UnboundedSender<StoreEvent>>,
    submitted: Vec<Command>,
    /// How far each teammate's sequence has gone in this project.
    teammates: teammates::Teammates,
    /// Raised by every change of `current`: the project's Home summary stamp (B2 D8).
    version: u64,
}

impl ProjectState {
    fn new(project: MemoryProject) -> Self {
        let mut project = project;
        // A fold cloned from a cached corpus shares its counters with every clone; each
        // project counts its own work, or parallel tests count into each other's.
        project.fold.own_counters();
        let signing = Arc::new(TokenKey::derive(&MEMORY_INSTALLATION_SECRET, project.id));
        let current = Arc::new(project.fold.clone());
        let published = Snapshot::new(SignedFold {
            fold: current.clone(),
            seq: 0,
            signing: signing.clone(),
        });
        Self {
            seeded: project,
            signing,
            live: Mutex::new(Live {
                current,
                open: None,
                opens: 0,
                published,
                seq: 0,
                mode: SessionMode::Writable,
                scripts: VecDeque::new(),
                subscribers: Vec::new(),
                submitted: Vec::new(),
                teammates: teammates::Teammates::new(),
                version: 0,
            }),
        }
    }

    fn publish(&self, live: &mut Live) {
        live.published = Snapshot::new(SignedFold {
            fold: live.current.clone(),
            seq: live.seq,
            signing: self.signing.clone(),
        });
    }

    fn send(live: &mut Live, event: &StoreEvent) {
        live.subscribers.retain(|tx| tx.send(event.clone()).is_ok());
    }

    /// Folds `changes` as an intake would: an open handle publishes a new snapshot and tells
    /// its subscribers.
    fn intake(&self, changes: Vec<Change>) -> Delta {
        let devices: Vec<DeviceId> = changes
            .iter()
            .map(|c| c.body().device)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut live = lock(&self.live);
        // Snapshots taken before keep the fold they had: the step works on a copy of the
        // shared state, which shares every part it does not touch.
        let delta = Arc::make_mut(&mut live.current).add(changes.into_iter().map(FoldInput::from));
        live.version += 1;
        if live.open.is_some() {
            live.seq += 1;
            self.publish(&mut live);
            let event = StoreEvent::Committed {
                seq: live.seq,
                delta: delta.clone(),
                cause: StoreCause::Intake { devices },
            };
            Self::send(&mut live, &event);
        }
        delta
    }

    fn close(&self, open: u64) {
        let mut live = lock(&self.live);
        if live.open == Some(open) {
            live.open = None;
            // The session's event pump ends with the channel.
            live.subscribers.clear();
        }
    }
}

/// Projects in memory, by folder. Clones share the projects.
#[derive(Clone, Default)]
pub struct MemoryBackend {
    projects: Arc<Mutex<BTreeMap<ProjectRef, Arc<ProjectState>>>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self, folder: &ProjectRef) -> Result<Arc<ProjectState>, MemoryError> {
        lock(&self.projects)
            .get(folder)
            .cloned()
            .ok_or_else(|| MemoryError::UnknownFolder(folder.clone()))
    }

    /// Adds a project under `folder`. Its fold counts its own work from here.
    pub fn add(&self, folder: ProjectRef, project: MemoryProject) -> Result<(), MemoryError> {
        let mut projects = lock(&self.projects);
        if projects.contains_key(&folder) {
            return Err(MemoryError::FolderTaken(folder));
        }
        projects.insert(folder, Arc::new(ProjectState::new(project)));
        Ok(())
    }

    /// Queues an outcome for the next command submitted to `folder` (first in, first out).
    pub fn script(&self, folder: &ProjectRef, outcome: Scripted) -> Result<(), MemoryError> {
        lock(&self.state(folder)?.live).scripts.push_back(outcome);
        Ok(())
    }

    /// Teammates' edits of `key` in `folder`'s project as it is now
    /// ([`teammates::Teammates::edit_verse`]), each continuing its teammate's sequence in this
    /// project, so a teammate who edits again never repeats a sequence number. Not folded:
    /// hand them to [`Self::teammate_edits`], or script them as [`Scripted::TeammateHeads`].
    pub fn teammates_edit_verse(
        &self,
        folder: &ProjectRef,
        key: &RecordKey,
        texts: &[&str],
    ) -> Result<Vec<Change>, MemoryError> {
        let state = self.state(folder)?;
        let mut live = lock(&state.live);
        let fold = live.current.clone();
        live.teammates.edit_verse(&fold, key, texts)
    }

    /// Teammate 1's join of verses in `folder`'s project as it is now
    /// ([`teammates::Teammates::join_verses`]). Not folded, as [`Self::teammates_edit_verse`].
    pub fn teammates_join_verses(
        &self,
        folder: &ProjectRef,
        book: BookId,
        chapter: tc_journal::ChapterNo,
        first: tc_journal::VerseNo,
        last: tc_journal::VerseNo,
    ) -> Result<Vec<Change>, MemoryError> {
        let state = self.state(folder)?;
        let mut live = lock(&state.live);
        let fold = live.current.clone();
        live.teammates
            .join_verses(&fold, book, chapter, first, last)
    }

    /// Adds teammates' changes to `folder`'s fold, as an intake would. An open handle
    /// publishes a new snapshot and sends `Committed` with the cause `Intake`.
    pub fn teammate_edits(
        &self,
        folder: &ProjectRef,
        changes: Vec<Change>,
    ) -> Result<Delta, MemoryError> {
        Ok(self.state(folder)?.intake(changes))
    }

    /// The fold the next open of `folder` gets.
    pub fn fold(&self, folder: &ProjectRef) -> Option<FoldState> {
        let state = self.state(folder).ok()?;
        let live = lock(&state.live);
        Some(FoldState::clone(&live.current))
    }

    /// The project as it was added, with its Door43 remote.
    pub fn project(&self, folder: &ProjectRef) -> Option<MemoryProject> {
        self.state(folder).ok().map(|state| state.seeded.clone())
    }

    /// Every command submitted to `folder`, oldest first, as the store would have received
    /// it: what a test of the UI's saves checks.
    pub fn submitted(&self, folder: &ProjectRef) -> Vec<Command> {
        self.state(folder)
            .map(|state| lock(&state.live).submitted.clone())
            .unwrap_or_default()
    }
}

impl MemoryBackend {
    /// The project now and its version, without opening it.
    fn current(
        &self,
        folder: &ProjectRef,
    ) -> Result<(Arc<ProjectState>, Snapshot, u64), StoreError> {
        let state = self.state(folder).map_err(|e| match e {
            MemoryError::UnknownFolder(_) => StoreError::NotAProject,
            e => StoreError::Other(e.to_string()),
        })?;
        let (snapshot, version) = {
            let live = lock(&state.live);
            let snapshot = Snapshot::new(SignedFold {
                fold: live.current.clone(),
                seq: live.seq,
                signing: state.signing.clone(),
            });
            (snapshot, live.version)
        };
        Ok((state, snapshot, version))
    }
}

/// `metadata.json` as a document, built from the fold's project details as tc-store builds
/// the file (its fields by path, the books' scope), without its derived parts.
fn metadata_document(fold: &FoldState) -> serde_json::Value {
    let source = fold.metadata_source_nfc(
        (
            u64::from(tc_journal::FORMAT_MAJOR),
            u64::from(tc_journal::FORMAT_MINOR),
        ),
        true,
    );
    let mut root = serde_json::Map::new();
    for (path, value) in source.fields {
        set_path(&mut root, &path, value);
    }
    set_path(&mut root, "type.flavorType.currentScope", source.scope);
    serde_json::Value::Object(root)
}

fn set_path(
    root: &mut serde_json::Map<String, serde_json::Value>,
    path: &str,
    value: serde_json::Value,
) {
    let mut at = root;
    let mut segments = path.split('.').peekable();
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            at.insert(segment.to_owned(), value);
            return;
        }
        let next = at
            .entry(segment.to_owned())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if !next.is_object() {
            *next = serde_json::Value::Object(serde_json::Map::new());
        }
        let serde_json::Value::Object(map) = next else {
            return;
        };
        at = map;
    }
}

impl ProjectStore for MemoryBackend {
    fn peek(&self, folder: &ProjectRef) -> Result<super::ProjectPeek, StoreError> {
        let (state, snapshot, version) = self.current(folder)?;
        let text_direction = tc_journal::keys::SettingKey::parse("textDirection")
            .ok()
            .and_then(
                |key| match snapshot.fold().projected(&RecordKey::Setting(key)) {
                    Some(tc_journal::Record::Setting {
                        value: Some(tc_journal::Json::Str(text)),
                        ..
                    }) => Some(text),
                    _ => None,
                },
            );
        Ok(super::ProjectPeek {
            metadata: Ok(metadata_document(snapshot.fold())),
            text_direction,
            remote: state.seeded.remote.clone(),
            // The fold's version, in the store's form: no pending save, no manifest.
            stamp: Some(SummaryStamp::of(
                &[format!("tc4-memory-summary-{version}")],
                &[],
            )),
        })
    }

    fn summary_files(
        &self,
        folder: &ProjectRef,
        books: &[tc_journal::BookId],
        watch: &SummaryWatch,
    ) -> Result<SummaryFiles, StoreError> {
        use tc_journal::fold::DerivedFile;
        let cancelled = || StoreError::Other("cancelled".to_owned());
        let (state, snapshot, _) = self.current(folder)?;
        let reads = CountingSnapshot::new(snapshot, watch.counters.clone());
        let missing = |what: &str| format!("the project holds no {what}");
        let mut files = SummaryFiles::default();
        for book in books {
            watch.cancel.check().map_err(|_| cancelled())?;
            let usfm = reads.book_usfm(*book).ok_or_else(|| missing(book.code()));
            files.books.push((*book, usfm));
        }
        if state.seeded.kind == ProjectKind::Stories {
            let mut stories = Vec::new();
            for n in 1..=tc_api::ids::StoryNo::COUNT {
                watch.cancel.check().map_err(|_| cancelled())?;
                let Some(story) = tc_journal::StoryNo::new(n) else {
                    continue;
                };
                if let Some(bytes) = reads.render(&DerivedFile::Story(story)) {
                    let text = String::from_utf8(bytes)
                        .map_err(|_| "the story is not valid UTF-8".to_owned());
                    stories.push((n, text));
                }
            }
            files.stories = Some(Ok(stories));
            files.resources = Some(
                reads
                    .render(&DerivedFile::Resources)
                    .ok_or_else(|| missing("resources.json")),
            );
        }
        files.conflicts = Ok(reads.render(&DerivedFile::Conflicts));
        Ok(files)
    }

    fn derived_file(
        &self,
        folder: &ProjectRef,
        file: &tc_journal::fold::DerivedFile,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        let (_, snapshot, _) = self.current(folder)?;
        // The read the dialog makes is not one of a screen's counted reads.
        Ok(CountingSnapshot::new(snapshot, Arc::default()).render(file))
    }

    fn list(&self) -> Result<Listing, StoreError> {
        // By folder name: the map is ordered by it. A memory project is never unusable.
        Ok(Listing {
            projects: lock(&self.projects)
                .iter()
                .map(|(folder, state)| ProjectEntry {
                    folder: folder.clone(),
                    id: state.seeded.id,
                    kind: state.seeded.kind,
                })
                .collect(),
            unusable: Vec::new(),
            others: Vec::new(),
        })
    }

    fn open(
        &self,
        folder: &ProjectRef,
        _progress: ProgressSink,
    ) -> Result<Arc<dyn ProjectHandle>, OpenFailure> {
        let state = self.state(folder).map_err(|_| OpenFailure::NotAProject)?;
        let open = {
            let mut live = lock(&state.live);
            if live.open.is_some() {
                // tc-store's O1: this process holds the folder's lock already.
                return Err(OpenFailure::LockHeld {
                    holder: Some(("this computer".to_owned(), std::process::id())),
                });
            }
            live.opens += 1;
            let open = live.opens;
            live.open = Some(open);
            live.seq = 1;
            state.publish(&mut live);
            open
        };
        Ok(Arc::new(MemoryHandle { state, open }))
    }

    /// A memory project has no folder to copy.
    fn copy(&self, _folder: &ProjectRef, _name: &ProjectRef) -> Result<(), CopyFailure> {
        Err(CopyFailure::Failed(
            "the memory backend copies no project".to_owned(),
        ))
    }

    /// A memory project has no lock file.
    fn trust_lock(&self, _folder: &ProjectRef) {}

    fn name_run(&self, _run: tc_api::ids::ServerInstance) {}

    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError> {
        let projects = lock(&self.projects);
        if let Some((folder, _)) = projects
            .iter()
            .find(|(_, state)| lock(&state.live).open.is_some())
        {
            return Err(StoreError::Other(format!(
                "{folder} is open; close every session before a reset"
            )));
        }
        for state in projects.values() {
            let mut live = lock(&state.live);
            let mut fold = state.seeded.fold.clone();
            fold.own_counters();
            live.current = Arc::new(fold);
            live.version += 1;
            live.scripts.clear();
            live.submitted.clear();
            live.mode = SessionMode::Writable;
            // The seed holds no teammate's change, so a run after a reset makes the same
            // changes as the first.
            live.teammates = teammates::Teammates::new();
        }
        Ok(())
    }
}

/// An open memory project.
struct MemoryHandle {
    state: Arc<ProjectState>,
    /// Which open of the folder this is: a handle of an earlier open closes nothing.
    open: u64,
}

impl Drop for MemoryHandle {
    fn drop(&mut self) {
        self.state.close(self.open);
    }
}

impl MemoryHandle {
    /// `outcome`, carried out for a command that showed `shown` and was submitted at
    /// `submitted`: a delay counts from the submit, not from when the reply is first awaited.
    async fn run(
        state: Arc<ProjectState>,
        shown: Vec<(RecordKey, Vec<ChangeId>)>,
        outcome: Scripted,
        submitted: tokio::time::Instant,
    ) -> Result<StoreReply, CommandError> {
        let mut outcome = outcome;
        let mut due = submitted;
        loop {
            match outcome {
                Scripted::Written => return Self::written(&state, &shown, Stored::Durable),
                Scripted::WrittenNotDurable(reason) => {
                    return Self::written(&state, &shown, Stored::WrittenNotDurable(reason));
                }
                Scripted::NotWritten(reason) => return Err(CommandError::NotWritten(reason)),
                Scripted::After(wait, next) => {
                    due += wait;
                    tokio::time::sleep_until(due).await;
                    outcome = *next;
                }
                Scripted::ReadOnly(reason) => {
                    let mode = SessionMode::ReadOnly(reason);
                    let mut live = lock(&state.live);
                    live.mode = mode;
                    ProjectState::send(&mut live, &StoreEvent::Mode(mode));
                    return Err(CommandError::ReadOnly(reason));
                }
                Scripted::TeammateHeads(changes, next) => {
                    state.intake(changes);
                    outcome = *next;
                }
            }
        }
    }

    /// A fresh edit token for each displayed value: what the screen shows after a save that
    /// changed nothing. Each token names exactly the heads the command's token named, signed
    /// again, never the record's heads in the newest snapshot: a teammate's change that
    /// arrived meanwhile (a scripted `TeammateHeads`) would be listed as seen, and the next
    /// save would replace it unseen (I4). For a resolution, those are every head it reviewed.
    /// The reply's `seq` is the newest snapshot's, which holds everything folded so far.
    fn written(
        state: &ProjectState,
        shown: &[(RecordKey, Vec<ChangeId>)],
        stored: Stored,
    ) -> Result<StoreReply, CommandError> {
        let seq = lock(&state.live).published.seq();
        let tokens = shown
            .iter()
            .map(|(key, heads)| {
                ShownToken::mint(&state.signing, key.clone(), heads.iter().copied())
                    .and_then(EditToken::from_journal)
                    .map(|token| (key.clone(), token))
                    .map_err(CommandError::BadToken)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(StoreReply {
            tokens,
            new_conflicts: Vec::new(),
            stored,
            seq,
            changes: Vec::new(),
            skipped: Vec::new(),
        })
    }
}

impl ProjectHandle for MemoryHandle {
    fn facts(&self) -> ProjectFacts {
        ProjectFacts {
            id: self.state.seeded.id,
            kind: self.state.seeded.kind,
            device: DeviceId::local(MEMORY_DEVICE),
            mode: lock(&self.state.live).mode,
        }
    }

    fn snapshot(&self) -> Snapshot {
        lock(&self.state.live).published.clone()
    }

    fn parse_edit(&self, token: &tc_api::ShownToken) -> Result<EditToken, TokenError> {
        EditToken::parse(&self.state.signing, token)
    }

    fn parse_review(&self, token: &tc_api::ReviewToken) -> Result<ReviewToken, TokenError> {
        ReviewToken::parse(&self.state.signing, token)
    }

    fn submit(&self, command: StoreCommand) -> BoxFuture<Result<StoreReply, CommandError>> {
        let state = self.state.clone();
        // A draft has no journal outcome to script. It must not consume the outcome a
        // test queued for the next save, just as tc-store writes it beside the actor.
        if matches!(command.journal(), Command::Draft { .. }) {
            let mut live = lock(&state.live);
            let result = if live.open != Some(self.open) {
                Err(CommandError::Closed)
            } else if let SessionMode::ReadOnly(reason) = live.mode {
                Err(CommandError::ReadOnly(reason))
            } else {
                live.submitted.push(command.journal().clone());
                Ok(StoreReply {
                    tokens: Vec::new(),
                    new_conflicts: Vec::new(),
                    stored: Stored::Unchanged,
                    seq: live.published.seq(),
                    changes: Vec::new(),
                    skipped: Vec::new(),
                })
            };
            return Box::pin(async move { result });
        }
        let next = {
            let mut live = lock(&state.live);
            if live.open != Some(self.open) {
                Err(CommandError::Closed)
            } else {
                live.submitted.push(command.journal().clone());
                match live.mode {
                    // A read-only project refuses before the script is looked at.
                    SessionMode::ReadOnly(reason) => Err(CommandError::ReadOnly(reason)),
                    SessionMode::Writable => {
                        live.scripts.pop_front().ok_or(CommandError::Unavailable)
                    }
                }
            }
        };
        let shown = command.shown().to_vec();
        let submitted = tokio::time::Instant::now();
        Box::pin(async move {
            match next {
                Ok(outcome) => Self::run(state, shown, outcome, submitted).await,
                Err(error) => Err(error),
            }
        })
    }

    fn subscribe(&self) -> UnboundedReceiver<StoreEvent> {
        let (tx, rx) = unbounded_channel();
        let mut live = lock(&self.state.live);
        if live.open == Some(self.open) {
            live.subscribers.push(tx);
        }
        rx
    }

    fn checkpoint(&self, _reason: CheckpointReason) -> BoxFuture<CheckpointOutcome> {
        // A memory project has no git history to commit to.
        Box::pin(async { CheckpointOutcome::Unchanged })
    }

    /// The memory backend touches no file.
    fn counters(&self) -> StoreCounters {
        StoreCounters::default()
    }

    /// Drafts are no-ops here (§4.2), so none is ever offered.
    fn offered_drafts(&self) -> Vec<OfferedDraft> {
        Vec::new()
    }

    fn discard_draft(&self, _id: u64) -> bool {
        false
    }

    fn discard_current_draft(
        &self,
        _field: DraftField,
        _shown: Vec<EditToken>,
        _text: String,
        _at: tc_journal::WallTime,
    ) -> bool {
        true
    }

    fn offer_draft(
        &self,
        _field: DraftField,
        _shown: Vec<DraftToken>,
        _text: String,
        _verified: bool,
        _at: tc_journal::WallTime,
    ) -> Option<u64> {
        None
    }

    /// Nothing runs in the background.
    #[cfg(feature = "test-hooks")]
    fn wait_idle(&self) {}

    fn close(self: Arc<Self>) -> BoxFuture<Result<(), StoreError>> {
        self.state.close(self.open);
        Box::pin(async { Ok(()) })
    }
}

/// Teammates' changes, made by tc-journal's encoder (AD36).
pub mod teammates {
    use tc_journal::record::{DeviceProfileValue, Door43Username, VerseValue};
    use tc_journal::{
        BatchId, BatchRef, Cause, ChangeBody, ChapterNo, DeviceSeq, Record, VerseNo, WallTime,
    };

    use super::*;

    /// 2026-10-01T00:00:00Z: later than any seeded change.
    const TEAMMATE_TIME: i64 = 1_790_812_800_000;
    const MINUTE: i64 = 60_000;
    /// A teammate's next edit is a day after their last, so a record's History lists a
    /// teammate's edits in the order they made them.
    const DAY: i64 = 24 * 60 * MINUTE;

    /// Teammate `n`'s device: a Local device whose bytes sort by `n`, so the screens number
    /// it "Teammate n" (no profile names it).
    pub fn device(n: u8) -> DeviceId {
        let mut random = [0u8; 15];
        random[0] = 0x7E;
        random[1] = n;
        DeviceId::local(random)
    }

    /// The teammates of one project and how far each one's sequence has gone. Each edit
    /// continues its teammate's sequence: a Local device's changes are numbered 1, 2, 3, …,
    /// and two changes that claim one device and number are a `DuplicateDevice` anomaly,
    /// which would put a notice on screens tC4 shows without one. The memory backend keeps
    /// one per project ([`MemoryBackend::teammates_edit_verse`]); a test that builds a fold
    /// by hand keeps its own.
    #[derive(Debug, Clone, Default)]
    pub struct Teammates {
        /// Each teammate's last sequence number; absent before their first edit.
        last: BTreeMap<u8, u64>,
    }

    impl Teammates {
        pub fn new() -> Self {
            Self::default()
        }

        /// One change per text, each by its own teammate (Teammate 1, 2, … in that order),
        /// all with the record's current heads in `fold` as parents: what "two teammates
        /// change Titus 1:1 from the same version" writes. A story frame's record takes the
        /// text as a verse's words do. Each is a batch of one whose id is
        /// derived from its device and number, and each continues its teammate's sequence.
        /// Nothing is counted when an edit cannot be made.
        pub fn edit_verse(
            &mut self,
            fold: &FoldState,
            key: &RecordKey,
            texts: &[&str],
        ) -> Result<Vec<Change>, MemoryError> {
            let not_text = || MemoryError::NotATextVerse(format!("{key:?}"));
            // A frame the story's template still holds has no record yet: the edit makes one.
            let record = match (fold.projected(key), key) {
                (Some(record), _) => record,
                (None, RecordKey::Frame(story, frame)) => Record::Frame {
                    story: *story,
                    frame: *frame,
                    value: String::new(),
                },
                (None, _) => return Err(not_text()),
            };
            let parents = fold.heads(key);
            let mut next = self.clone();
            let changes = texts
                .iter()
                .zip(1u8..)
                .map(|(text, n)| {
                    let mut record = record.clone();
                    match &mut record {
                        Record::Verse {
                            value: VerseValue::Text { words, .. },
                            ..
                        } => *words = (*text).to_owned(),
                        Record::Frame { value, .. } => *value = (*text).to_owned(),
                        _ => return Err(not_text()),
                    }
                    next.change(n, record, parents.clone(), None)
                })
                .collect::<Result<Vec<_>, _>>()?;
            *self = next;
            Ok(changes)
        }

        /// Teammate 1's join of the verses `first` to `last` of `chapter` of `book`, as one
        /// batch over the verses' current heads in `fold` (STORAGE §8.3's "Maria joined
        /// verses 4 and 5"): verse `first` holds every verse's words in order, with the last
        /// verse's layout, and each other verse is covered by it. Every verse must show text
        /// of its own. Nothing is counted when the join cannot be made.
        pub fn join_verses(
            &mut self,
            fold: &FoldState,
            book: BookId,
            chapter: ChapterNo,
            first: VerseNo,
            last: VerseNo,
        ) -> Result<Vec<Change>, MemoryError> {
            let not_text = |v: VerseNo| {
                MemoryError::NotATextVerse(format!("{book:?} {}:{}", chapter.get(), v.get()))
            };
            if last <= first {
                return Err(MemoryError::Change(format!(
                    "{}-{} is not a span",
                    first.get(),
                    last.get()
                )));
            }
            let verses: Vec<VerseNo> = (first.get()..=last.get())
                .filter_map(VerseNo::new)
                .collect();
            let mut words = Vec::with_capacity(verses.len());
            let mut parents = Vec::with_capacity(verses.len());
            let mut shape = None;
            for verse in &verses {
                let key = RecordKey::Verse(book, chapter, *verse);
                let Some(Record::Verse {
                    value:
                        VerseValue::Text {
                            marker,
                            words: text,
                            layout,
                            last: own,
                            ..
                        },
                    ..
                }) = fold.projected(&key)
                else {
                    return Err(not_text(*verse));
                };
                if own != *verse {
                    return Err(not_text(*verse));
                }
                words.push(text);
                parents.push(fold.heads(&key));
                shape = Some((marker, layout));
            }
            let Some((marker, layout)) = shape else {
                return Err(not_text(first));
            };
            let mut next = self.clone();
            let n = u32::try_from(verses.len()).unwrap_or(u32::MAX);
            let seq = next.last.get(&1).map_or(1, |seq| seq + 1);
            let batch = (batch_ref(device(1), seq, n)?, at_of(1, seq));
            let mut changes = Vec::with_capacity(verses.len());
            for (i, verse) in verses.iter().enumerate() {
                let value = if i == 0 {
                    VerseValue::Text {
                        marker: marker.clone(),
                        words: words.join(" "),
                        layout: layout.clone(),
                        last,
                        from: None,
                    }
                } else {
                    VerseValue::Covered { by: first }
                };
                let record = Record::Verse {
                    book,
                    chapter,
                    verse: *verse,
                    value,
                };
                changes.push(next.change(1, record, parents[i].clone(), Some(batch))?);
            }
            *self = next;
            Ok(changes)
        }

        /// Teammate `n` signed in to Door43 as `username`: their device's profile, so the
        /// screens name them by it, and two teammates with one username are one person's
        /// computers (STORAGE §8.5).
        pub fn sign_in(
            &mut self,
            fold: &FoldState,
            n: u8,
            username: &str,
        ) -> Result<Change, MemoryError> {
            let device = device(n);
            let value = DeviceProfileValue {
                display_name: None,
                door43_username: Some(
                    Door43Username::parse(username)
                        .map_err(|e| MemoryError::Change(e.to_string()))?,
                ),
                legacy: None,
            };
            let parents = fold.heads(&RecordKey::DeviceProfile(device));
            let record = Record::DeviceProfile { device, value };
            let mut next = self.clone();
            let change = next.change(n, record, parents, None)?;
            *self = next;
            Ok(change)
        }

        /// The next change of teammate `n`: `record` over `parents`, in `batch` with its
        /// time, or in a batch of its own dated a day after the teammate's last change.
        fn change(
            &mut self,
            n: u8,
            record: Record,
            parents: Vec<ChangeId>,
            batch: Option<(BatchRef, WallTime)>,
        ) -> Result<Change, MemoryError> {
            let seq = self.last.get(&n).map_or(1, |seq| seq + 1);
            let device = device(n);
            let (batch, at) = match batch {
                Some(batch) => batch,
                None => (batch_ref(device, seq, 1)?, at_of(n, seq)),
            };
            let change = Change::new(ChangeBody {
                v: 0,
                record,
                parents,
                device,
                seq: DeviceSeq(seq),
                at,
                batch,
                cause: Cause::Edit,
            })
            .map_err(|e| MemoryError::Change(e.to_string()))?;
            self.last.insert(n, seq);
            Ok(change)
        }
    }

    /// A batch of `n` changes whose id is derived from its device and the number of its
    /// first change.
    fn batch_ref(device: DeviceId, seq: u64, n: u32) -> Result<BatchRef, MemoryError> {
        let digest = ring::digest::digest(
            &ring::digest::SHA256,
            &[
                b"tc4-teammate-batch\0".as_slice(),
                device.as_bytes().as_slice(),
                seq.to_be_bytes().as_slice(),
            ]
            .concat(),
        );
        let mut batch = [0u8; 16];
        batch.copy_from_slice(&digest.as_ref()[..16]);
        BatchRef::new(BatchId::from_bytes(batch), n)
            .ok_or_else(|| MemoryError::Change("an empty batch".to_owned()))
    }

    /// When teammate `n` made their change number `seq`.
    fn at_of(n: u8, seq: u64) -> WallTime {
        let days = i64::try_from(seq - 1).unwrap_or(i64::MAX);
        WallTime(
            TEAMMATE_TIME
                .saturating_add(days.saturating_mul(DAY))
                .saturating_add(i64::from(n) * MINUTE),
        )
    }
}
