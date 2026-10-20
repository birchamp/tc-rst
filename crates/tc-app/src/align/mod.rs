//! The Align screen's server side (unit B6; APP-DESIGN AD18, AD19, AD22, A-4, A-14).
//!
//! - **The session.** `align/open` makes one aligner for one verse of the open project: its
//!   text, the pinned original-language verse its numbering names, the stored alignment
//!   ([`open`]). A task owns it ([`actor`]); every edit is one request that names the view it
//!   was made on and returns the whole view, built by a pure function ([`view`]). A verse
//!   whose session is live returns that session. Another verse replaces it, after the links
//!   of the first are saved.
//! - **The save.** The server saves the links, not the page: 2 s after the last change, on
//!   leaving the verse, when the session is replaced or closed, on `align/flush`, and before
//!   the quit's checkpoint. The state machine is [`save`]; its state is in every view and in
//!   the `AlignSave` event, so a gap in the event stream cannot leave "Saving" standing.
//! - **The rail** (`align/index`) and the picker's Align card come from one function
//!   ([`index`]): the rule that says whether an alignment still vouches for its verse.
//!
//! Nothing but the save writes the project. Opening a verse, a click and the rail only read.

mod actor;
pub(crate) mod index;
mod open;
mod save;
mod suggest;
mod view;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tc_align::SourceBook;
use tc_api::ApiError;
use tc_api::align::{
    AlignSave, AlignerView, Close, Edit, Flush, Index, IndexView, KeepReflow, Open, OpenVerse,
    Opened, Suggest,
};
use tc_api::ids::AlignerId;
use tokio::sync::Mutex;

use self::actor::{Cmd, Ctx, Env, Handle};
use self::open::{Prepared, prepare};
use crate::app::App;
use crate::dispatch::Handle as HandleRequest;
use crate::reports::{ReportOp, ReportOutcome, facts};
use crate::session::ProjectSession;

/// Numbers the aligners of the whole server run, so an id is never reused: a restart numbers
/// them again, and the page forgets what it held (§5.4).
static ALIGNERS: AtomicU64 = AtomicU64::new(0);

/// An original-language book cut into verses, with the book and the sha of the pin it was read
/// under.
type CutBook = ((tc_core::BookId, String), Arc<SourceBook>);

/// A session's aligner, and the original-language book it was cut from.
#[derive(Default)]
pub(crate) struct AlignCache {
    slot: Mutex<Option<Handle>>,
    /// What the suggestion engine has learned of each testament.
    hub: Arc<suggest::SuggestHub>,
    /// The original-language book of the verses opened, cut into verses once: the book and
    /// the sha of the pin it was read under.
    sources: Mutex<Option<CutBook>>,
}

impl AlignCache {
    /// Saves the open aligner's links and lets go of it: the person leaves the project, or the
    /// app quits. A save that fails is reported: the links were the only copy.
    pub(crate) async fn finish(
        &self,
        app_reports: &crate::reports::ReportLog,
        now: tc_api::ids::WallMs,
        project: &str,
    ) {
        let Some(open) = self.slot.lock().await.take() else {
            return;
        };
        if let Ok(not_saved @ (AlignSave::Failed(_) | AlignSave::Unsaved | AlignSave::Saving)) =
            open.ask(|reply| Cmd::Close { reply }).await
        {
            app_reports.add(
                ReportOp::Close,
                ReportOutcome::Failed {
                    reason: format!(
                        "the alignment links of {} were not saved: {not_saved:?}",
                        open.place.label()
                    ),
                },
                facts(serde_json::json!({ "project": project })),
                now,
                now,
            );
        }
    }
}

impl AlignCache {
    pub(crate) fn hub(&self) -> &Arc<suggest::SuggestHub> {
        &self.hub
    }

    /// The book of the open aligner, if there is one.
    pub(crate) async fn open_book(&self) -> Option<tc_core::BookId> {
        self.slot.lock().await.as_ref().map(|open| open.book)
    }
}

impl ProjectSession {
    pub(crate) fn align(&self) -> &AlignCache {
        &self.align
    }
}

impl App {
    /// The switch and the engine the views show.
    fn align_ctx(&self, session: &ProjectSession) -> Ctx {
        Ctx {
            suggest_on: self
                .inner
                .prefs
                .align_suggestions(&session.scope().project)
                .unwrap_or(false),
            hub: session.align().hub().clone(),
        }
    }

    fn align_env(&self, session: &ProjectSession) -> Env {
        Env {
            bus: self.inner.bus.clone(),
            scope: session.scope().clone(),
            handle: session.handle().clone(),
        }
    }
}

// ---------------------------------------------------------------------------------------
// align/index
// ---------------------------------------------------------------------------------------

impl HandleRequest<Index> for App {
    async fn handle(&self, req: Index) -> Result<IndexView, ApiError> {
        let session = self.read(&req.at)?;
        Ok(IndexView {
            items: index::items(&session.read(), req.book),
        })
    }
}

// ---------------------------------------------------------------------------------------
// align/open
// ---------------------------------------------------------------------------------------

impl HandleRequest<Open> for App {
    async fn handle(&self, req: Open) -> Result<Opened, ApiError> {
        let session = self.read(&req.at)?;
        let (chapter, verse) = match req.verse {
            Some(at) => (at.chapter, at.verse),
            None => match resume(&session.read(), req.book) {
                Some(at) => at,
                None => {
                    return Ok(Opened::Blocked {
                        why: tc_api::align::Blocked::Undrafted,
                        at: None,
                    });
                }
            },
        };
        let cache = session.align();
        // Held to the end: a second open waits for this one, and an edit waits for the verse
        // it names.
        let mut slot = cache.slot.lock().await;
        let ctx = self.align_ctx(&session);
        if let Some(open) = slot.as_ref()
            && open.book == req.book
            && (open.place.chapter, open.place.verse) == (chapter, verse)
        {
            return match open.ask(|reply| Cmd::View { ctx, reply }).await {
                Ok(view) => Ok(Opened::Ready(Box::new(view))),
                Err(_) => {
                    *slot = None;
                    Err(ApiError::StaleSession)
                }
            };
        }
        // Another verse replaces the open one once its links are saved. Links that cannot be
        // saved stay where they are: the verse is not left.
        if let Some(open) = slot.as_ref() {
            match open.ask(|reply| Cmd::Close { reply }).await {
                Ok(AlignSave::Saved) | Err(_) => *slot = None,
                Ok(AlignSave::Failed(reason)) => {
                    return Err(ApiError::NotSaved(reason));
                }
                Ok(AlignSave::Unsaved | AlignSave::Saving) => {
                    return Err(ApiError::NotSaved(tc_api::write::NotSaved::StaleScope));
                }
            }
        }
        let id = AlignerId::new(ALIGNERS.fetch_add(1, Ordering::Relaxed) + 1);
        match prepare(self, &session, id, req.book, chapter, verse).await? {
            Prepared::Blocked(why) => Ok(Opened::Blocked {
                why,
                at: Some(OpenVerse { chapter, verse }),
            }),
            Prepared::Ready(ready) => {
                // The engine learns from the project's verses, once a testament is opened with
                // the switch on.
                if ctx.suggest_on {
                    self.train_suggestions(&session, suggest::testament_of(ready.book), false);
                }
                let handle = actor::spawn(self.align_env(&session), *ready);
                let view = handle.ask(|reply| Cmd::View { ctx, reply }).await?;
                *slot = Some(handle);
                Ok(Opened::Ready(Box::new(view)))
            }
        }
    }
}

/// Where the book resumes: the first drafted verse that is not done, else the first drafted
/// one (tC4's `resumeAlignRef` and `firstDraftedRef`).
fn resume(read: &crate::counters::CountingSnapshot, book: tc_core::BookId) -> Option<(u16, u16)> {
    use tc_api::align::RailState;
    let items = index::items(read, book);
    let drafted = || {
        items
            .iter()
            .filter(|i| i.status.state != RailState::Undrafted)
    };
    drafted()
        .find(|i| i.status.state != RailState::Valid)
        .or_else(|| drafted().next())
        .map(|i| (i.place.chapter, i.place.verse))
}

// ---------------------------------------------------------------------------------------
// align/edit, align/keepReflow, align/flush, align/close
// ---------------------------------------------------------------------------------------

impl App {
    /// The task of `aligner`, if it is still the open one.
    async fn aligner_cmd<T>(
        &self,
        session: &ProjectSession,
        aligner: AlignerId,
        make: impl FnOnce(tokio::sync::oneshot::Sender<T>) -> Cmd,
    ) -> Result<T, ApiError> {
        let slot = session.align().slot.lock().await;
        match slot.as_ref().filter(|open| open.id == aligner) {
            Some(open) => open.ask(make).await,
            None => Err(ApiError::StaleSession),
        }
    }
}

impl HandleRequest<Edit> for App {
    async fn handle(&self, req: Edit) -> Result<AlignerView, ApiError> {
        let session = self.read(&req.at)?;
        let ctx = self.align_ctx(&session);
        self.aligner_cmd(&session, req.aligner, |reply| Cmd::Edit {
            ctx,
            seq: req.seq,
            edit: req.edit,
            reply,
        })
        .await?
    }
}

impl HandleRequest<Suggest> for App {
    async fn handle(&self, req: Suggest) -> Result<AlignerView, ApiError> {
        let session = self.read(&req.at)?;
        let ctx = self.align_ctx(&session);
        let (input, testament) = self
            .aligner_cmd(&session, req.aligner, |reply| Cmd::Input { reply })
            .await?;
        // The model answers off the thread that serves the page (PERF:84: 50 ms).
        let model = ctx.suggest_on.then(|| ctx.hub.model(testament)).flatten();
        let raw = match model {
            Some(model) => tokio::task::spawn_blocking(move || model.predict(&input))
                .await
                .ok(),
            None => None,
        };
        self.aligner_cmd(&session, req.aligner, |reply| Cmd::Show { ctx, raw, reply })
            .await
    }
}

impl HandleRequest<KeepReflow> for App {
    async fn handle(&self, req: KeepReflow) -> Result<AlignerView, ApiError> {
        let session = self.read(&req.at)?;
        let ctx = self.align_ctx(&session);
        self.aligner_cmd(&session, req.aligner, |reply| Cmd::KeepReflow {
            ctx,
            seq: req.seq,
            reply,
        })
        .await?
    }
}

impl HandleRequest<Flush> for App {
    async fn handle(&self, req: Flush) -> Result<AlignSave, ApiError> {
        let session = self.read(&req.at)?;
        let slot = session.align().slot.lock().await;
        match slot.as_ref() {
            Some(open) => open.ask(|reply| Cmd::Flush { reply }).await,
            None => Ok(AlignSave::Saved),
        }
    }
}

impl HandleRequest<Close> for App {
    async fn handle(&self, req: Close) -> Result<AlignSave, ApiError> {
        let session = self.read(&req.at)?;
        let mut slot = session.align().slot.lock().await;
        let Some(open) = slot.as_ref().filter(|open| open.id == req.aligner) else {
            return Ok(AlignSave::Saved);
        };
        let saved = open.ask(|reply| Cmd::Close { reply }).await?;
        if saved == AlignSave::Saved {
            *slot = None;
        }
        Ok(saved)
    }
}
