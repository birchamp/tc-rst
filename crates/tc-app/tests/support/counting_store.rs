//! A `ProjectStore` that counts and holds, around any backend: how many times each folder was
//! opened, which checkpoints ran and how many closes, with an open or a checkpoint held until
//! a test lets it go, or a checkpoint made to fail. It knows nothing of the backend inside, so
//! C0's store instance of the contract suite uses it as the memory instance does.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex};

use tc_api::event::CheckpointOutcome;
use tc_api::ids::ProjectRef;
use tc_app::backend::{
    BoxFuture, CheckpointReason, CommandError, EditToken, Listing, OfferedDraft, OpenFailure,
    ProjectFacts, ProjectHandle, ProjectPeek, ProjectStore, ReviewToken, Snapshot, StoreCommand,
    StoreCounters, StoreError, StoreEvent, StoreReply, SummaryFiles, SummaryWatch,
};
use tc_app::jobs::ProgressSink;
use tc_journal::token::TokenError;
use tokio::sync::watch;

#[derive(Default)]
struct Counts {
    /// Opens started, by folder (a held one included).
    opens: BTreeMap<ProjectRef, usize>,
    /// Folders whose next open waits until released.
    held: BTreeMap<ProjectRef, bool>,
    checkpoints: Vec<(ProjectRef, CheckpointReason)>,
    closes: BTreeMap<ProjectRef, usize>,
    /// The next checkpoint waits until released.
    hold_checkpoint: bool,
    /// The next checkpoint fails with this reason.
    fail_checkpoint: Option<String>,
}

struct Shared {
    counts: Mutex<Counts>,
    /// Wakes a held open.
    opens: Condvar,
    /// `true` lets a held checkpoint go.
    checkpoint: watch::Sender<bool>,
}

pub struct CountingStore<S> {
    inner: S,
    shared: Arc<Shared>,
}

impl<S: ProjectStore> CountingStore<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            shared: Arc::new(Shared {
                counts: Mutex::new(Counts::default()),
                opens: Condvar::new(),
                checkpoint: watch::channel(false).0,
            }),
        }
    }

    pub fn inner(&self) -> &S {
        &self.inner
    }

    fn counts(&self) -> std::sync::MutexGuard<'_, Counts> {
        self.shared.counts.lock().unwrap()
    }

    pub fn opens(&self, folder: &ProjectRef) -> usize {
        self.counts().opens.get(folder).copied().unwrap_or(0)
    }

    pub fn closes(&self, folder: &ProjectRef) -> usize {
        self.counts().closes.get(folder).copied().unwrap_or(0)
    }

    pub fn checkpoints(&self) -> Vec<(ProjectRef, CheckpointReason)> {
        self.counts().checkpoints.clone()
    }

    /// The next open of `folder` waits until [`Self::release_open`].
    pub fn hold_open(&self, folder: &ProjectRef) {
        self.counts().held.insert(folder.clone(), true);
    }

    pub fn release_open(&self, folder: &ProjectRef) {
        self.counts().held.insert(folder.clone(), false);
        self.shared.opens.notify_all();
    }

    /// The next checkpoint waits until [`Self::release_checkpoint`].
    pub fn hold_checkpoint(&self) {
        self.shared.checkpoint.send_replace(false);
        self.counts().hold_checkpoint = true;
    }

    pub fn release_checkpoint(&self) {
        self.shared.checkpoint.send_replace(true);
    }

    /// The next checkpoint fails with `reason`.
    pub fn fail_checkpoint(&self, reason: &str) {
        self.counts().fail_checkpoint = Some(reason.to_owned());
    }
}

impl<S: ProjectStore> ProjectStore for CountingStore<S> {
    fn list(&self) -> Result<Listing, StoreError> {
        self.inner.list()
    }

    fn peek(&self, folder: &ProjectRef) -> Result<ProjectPeek, StoreError> {
        self.inner.peek(folder)
    }

    fn summary_files(
        &self,
        folder: &ProjectRef,
        books: &[tc_journal::BookId],
        watch: &SummaryWatch,
    ) -> Result<SummaryFiles, StoreError> {
        self.inner.summary_files(folder, books, watch)
    }

    fn derived_file(
        &self,
        folder: &ProjectRef,
        file: &tc_journal::fold::DerivedFile,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        self.inner.derived_file(folder, file)
    }

    fn open(
        &self,
        folder: &ProjectRef,
        progress: ProgressSink,
    ) -> Result<Arc<dyn ProjectHandle>, OpenFailure> {
        {
            let mut counts = self.counts();
            *counts.opens.entry(folder.clone()).or_default() += 1;
            while counts.held.get(folder).copied().unwrap_or(false) {
                counts = self.shared.opens.wait(counts).unwrap();
            }
        }
        let handle = self.inner.open(folder, progress)?;
        Ok(Arc::new(CountingHandle {
            folder: folder.clone(),
            inner: handle,
            shared: self.shared.clone(),
        }))
    }

    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError> {
        self.inner.reset()
    }
}

struct CountingHandle {
    folder: ProjectRef,
    inner: Arc<dyn ProjectHandle>,
    shared: Arc<Shared>,
}

impl ProjectHandle for CountingHandle {
    fn facts(&self) -> ProjectFacts {
        self.inner.facts()
    }

    fn snapshot(&self) -> Snapshot {
        self.inner.snapshot()
    }

    fn parse_edit(&self, token: &tc_api::ShownToken) -> Result<EditToken, TokenError> {
        self.inner.parse_edit(token)
    }

    fn parse_review(&self, token: &tc_api::ReviewToken) -> Result<ReviewToken, TokenError> {
        self.inner.parse_review(token)
    }

    fn submit(&self, command: StoreCommand) -> BoxFuture<Result<StoreReply, CommandError>> {
        self.inner.submit(command)
    }

    fn subscribe(&self) -> tokio::sync::mpsc::UnboundedReceiver<StoreEvent> {
        self.inner.subscribe()
    }

    fn checkpoint(&self, reason: CheckpointReason) -> BoxFuture<CheckpointOutcome> {
        let (hold, fail) = {
            let mut counts = self.shared.counts.lock().unwrap();
            counts
                .checkpoints
                .push((self.folder.clone(), reason.clone()));
            (
                std::mem::take(&mut counts.hold_checkpoint),
                counts.fail_checkpoint.take(),
            )
        };
        let mut released = self.shared.checkpoint.subscribe();
        let inner = self.inner.checkpoint(reason);
        Box::pin(async move {
            if hold {
                let _ = released.wait_for(|go| *go).await;
            }
            match fail {
                Some(reason) => CheckpointOutcome::Failed { reason },
                None => inner.await,
            }
        })
    }

    fn counters(&self) -> StoreCounters {
        self.inner.counters()
    }

    fn offered_drafts(&self) -> Vec<OfferedDraft> {
        self.inner.offered_drafts()
    }

    #[cfg(feature = "test-hooks")]
    fn wait_idle(&self) {
        self.inner.wait_idle();
    }

    fn close(self: Arc<Self>) -> BoxFuture<Result<(), StoreError>> {
        *self
            .shared
            .counts
            .lock()
            .unwrap()
            .closes
            .entry(self.folder.clone())
            .or_default() += 1;
        self.inner.clone().close()
    }
}

/// A store behind an `Arc`, as a contract fixture hands it over, so a [`CountingStore`] can
/// wrap any backend.
pub struct SharedStore(pub Arc<dyn ProjectStore>);

impl ProjectStore for SharedStore {
    fn list(&self) -> Result<Listing, StoreError> {
        self.0.list()
    }

    fn peek(&self, folder: &ProjectRef) -> Result<ProjectPeek, StoreError> {
        self.0.peek(folder)
    }

    fn summary_files(
        &self,
        folder: &ProjectRef,
        books: &[tc_journal::BookId],
        watch: &SummaryWatch,
    ) -> Result<SummaryFiles, StoreError> {
        self.0.summary_files(folder, books, watch)
    }

    fn derived_file(
        &self,
        folder: &ProjectRef,
        file: &tc_journal::fold::DerivedFile,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        self.0.derived_file(folder, file)
    }

    fn open(
        &self,
        folder: &ProjectRef,
        progress: ProgressSink,
    ) -> Result<Arc<dyn ProjectHandle>, OpenFailure> {
        self.0.open(folder, progress)
    }

    #[cfg(feature = "test-hooks")]
    fn reset(&self) -> Result<(), StoreError> {
        self.0.reset()
    }
}
