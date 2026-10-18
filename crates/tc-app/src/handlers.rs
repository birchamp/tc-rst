//! One handler per product endpoint of phase 0 (APP-DESIGN §3.9). `dispatch` routes to them,
//! and an endpoint row without a handler here does not compile.
//!
//! The account and feedback rows that later units build answer `Unavailable` until then.

use serde_json::json;
use tc_api::account::{
    AccountView, CancelSignIn, HelpLink, LinkView, ProfileLink, Resume, ResumeView, SignIn, SignOut,
};
use tc_api::app::{AppPrefsView, ClientError, Hello, HelloView, Quit};
use tc_api::error::Feature;
use tc_api::feedback::{Compose, FeedbackDraft};
use tc_api::jobs::{Cancel, Get, JobView};
use tc_api::net::{NetView, Set};
use tc_api::prefs::{RecordPlace, SetAlignSuggestions, SetDraftUnit, SetLocale};
use tc_api::refusal::PrefsRefusal;
use tc_api::{API_VERSION, ApiError, NotSaved, Refusal};

use crate::app::{App, lock};
use crate::dispatch::Handle;
use crate::reports::{ReportOp, ReportOutcome, facts};
use crate::sessions::SessionInfo;

/// The longest `app/clientError` message a report keeps. The UI sends a panic's message; a
/// body of several megabytes in each of 50 kept reports would hold hundreds of megabytes.
const MAX_CLIENT_MESSAGE: usize = 8 * 1024;
/// The longest view name a client error keeps.
const MAX_CLIENT_VIEW: usize = 200;

/// `text` cut to `max` bytes at a character boundary, and whether it was cut.
fn cut(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text.get(..end).unwrap_or_default(), true)
}

impl App {
    /// The project behind a scope. A scope this server run does not know is stale: a write
    /// never gets `ApiError::StaleScope`, it gets `NotSaved::StaleScope` (A-19).
    fn session_of(&self, at: &tc_api::Scope) -> Result<SessionInfo, ApiError> {
        self.inner
            .sessions
            .resolve(at)
            .ok_or(ApiError::NotSaved(NotSaved::StaleScope))
    }
}

impl Handle<Hello> for App {
    async fn handle(&self, _req: Hello) -> Result<HelloView, ApiError> {
        let inner = &self.inner;
        let account = lock(&inner.account).view();
        let startup = lock(&inner.startup).clone();
        Ok(HelloView {
            api: API_VERSION,
            build: inner.config.build.clone(),
            instance: inner.instance,
            internet: inner.internet.is_on(),
            account,
            sessions: inner.sessions.open_sessions(),
            prefs: AppPrefsView {
                app_locale: inner.prefs.app_locale(),
            },
            startup,
        })
    }
}

impl Handle<Quit> for App {
    async fn handle(&self, _req: Quit) -> Result<(), ApiError> {
        self.quit().await;
        Ok(())
    }
}

impl Handle<ClientError> for App {
    async fn handle(&self, req: ClientError) -> Result<(), ApiError> {
        let (message, message_cut) = cut(&req.message, MAX_CLIENT_MESSAGE);
        let view = req.view.as_deref().map(|view| cut(view, MAX_CLIENT_VIEW).0);
        let now = self.inner.clock.now();
        let mut record = facts(json!({
            "view": view,
            "project": req.project,
            "at": req.at,
            "request": req.request,
        }));
        if message_cut {
            record.insert("truncated".to_owned(), json!(true));
        }
        self.inner.reports.add(
            ReportOp::ClientError,
            ReportOutcome::Failed {
                reason: message.to_owned(),
            },
            record,
            now,
            now,
        );
        Ok(())
    }
}

impl Handle<SetLocale> for App {
    async fn handle(&self, req: SetLocale) -> Result<(), ApiError> {
        // The writer has already reported a failed write; the dialog stays open and says so.
        self.inner
            .prefs
            .set_locale(req.locale)
            .await
            .map_err(|_| ApiError::Refused(Refusal::Prefs(PrefsRefusal::LocaleNotSaved)))
    }
}

impl Handle<SetDraftUnit> for App {
    async fn handle(&self, req: SetDraftUnit) -> Result<(), ApiError> {
        let session = self.session_of(&req.at)?;
        self.inner.prefs.set_draft_unit(session.id, req.unit);
        Ok(())
    }
}

impl Handle<SetAlignSuggestions> for App {
    async fn handle(&self, req: SetAlignSuggestions) -> Result<(), ApiError> {
        let session = self.session_of(&req.at)?;
        // tC4 keys this switch by the project's folder.
        self.inner
            .prefs
            .set_align_suggestions(session.project, req.on);
        Ok(())
    }
}

impl Handle<RecordPlace> for App {
    async fn handle(&self, req: RecordPlace) -> Result<(), ApiError> {
        let session = self.session_of(&req.at)?;
        self.inner.prefs.record_place(session.id, req.place);
        Ok(())
    }
}

impl Handle<tc_api::net::State> for App {
    async fn handle(&self, _req: tc_api::net::State) -> Result<NetView, ApiError> {
        Ok(NetView {
            on: self.inner.internet.is_on(),
        })
    }
}

impl Handle<Set> for App {
    async fn handle(&self, req: Set) -> Result<NetView, ApiError> {
        self.set_internet(req.on);
        Ok(NetView { on: req.on })
    }
}

impl Handle<Get> for App {
    async fn handle(&self, req: Get) -> Result<Option<JobView>, ApiError> {
        Ok(self.inner.jobs.view(req.job))
    }
}

impl Handle<Cancel> for App {
    async fn handle(&self, req: Cancel) -> Result<(), ApiError> {
        self.inner.jobs.cancel(req.job);
        Ok(())
    }
}

impl Handle<tc_api::account::State> for App {
    /// From memory: it reaches neither Door43 nor the keychain.
    async fn handle(&self, _req: tc_api::account::State) -> Result<AccountView, ApiError> {
        Ok(lock(&self.inner.account).view())
    }
}

impl Handle<HelpLink> for App {
    /// The internet check already ran. The shell opens the link; tC4's help panel hands it to
    /// `openDoor43Page('helpLink', url)`.
    async fn handle(&self, req: HelpLink) -> Result<LinkView, ApiError> {
        Ok(LinkView { url: req.url })
    }
}

impl Handle<SignIn> for App {
    async fn handle(&self, _req: SignIn) -> Result<AccountView, ApiError> {
        Err(ApiError::Unavailable(Feature::Account))
    }
}

impl Handle<CancelSignIn> for App {
    async fn handle(&self, _req: CancelSignIn) -> Result<(), ApiError> {
        Err(ApiError::Unavailable(Feature::Account))
    }
}

impl Handle<Resume> for App {
    async fn handle(&self, _req: Resume) -> Result<ResumeView, ApiError> {
        Err(ApiError::Unavailable(Feature::Account))
    }
}

impl Handle<SignOut> for App {
    async fn handle(&self, _req: SignOut) -> Result<AccountView, ApiError> {
        Err(ApiError::Unavailable(Feature::Account))
    }
}

impl Handle<ProfileLink> for App {
    async fn handle(&self, _req: ProfileLink) -> Result<LinkView, ApiError> {
        Err(ApiError::Unavailable(Feature::Account))
    }
}

impl Handle<Compose> for App {
    /// Unit A5 composes the attachment from the report log.
    async fn handle(&self, _req: Compose) -> Result<FeedbackDraft, ApiError> {
        Err(ApiError::Unavailable(Feature::Feedback))
    }
}
