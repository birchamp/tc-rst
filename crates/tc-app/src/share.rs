//! The share dialog's first step, the repository link and Open from Door43 (unit B9b; tC4
//! #362, #677). The share itself, its push and its end are unit E1b's, and the open's copy
//! from Door43 unit X's: until X lands, [`ScriptedOpenDoor43`] answers for it (AD6).
//!
//! Every call here takes the account and write server, `--door43` (tC4 #120), with the internet
//! check of its endpoint done by dispatch first.

use serde_json::json;
use tc_api::account::{LinkView, OrgChoice, Orgs, RepoLink, ShareTargets};
use tc_api::door43::OpenDoor43;
use tc_api::endpoint::NetKind;
use tc_api::ids::JobId;
use tc_api::jobs::{JobKind, Phase, SyncPhase};
use tc_api::refusal::{OpenDoor43Refusal, ShareRefusal};
use tc_api::{ApiError, Refusal};
use tc_door43::{Door43Error, Session};

use crate::app::App;
use crate::dispatch::Handle;
use crate::jobs::{JobEnd, JobSpec};
use crate::reports::facts;

/// tC4's `recommendOrganization`: of the organizations the person can create in, the one with
/// the most repositories in the project's language. None when no such repository exists, or
/// when two or more share the highest count. `choices` is each organization's
/// `(can_create, repositories_in_language)`, in Door43's order; the answer is an index.
pub fn recommend(choices: &[(bool, u64)]) -> Option<usize> {
    let mut best = None;
    let mut best_count = 0;
    let mut tied = false;
    for (at, &(can_create, count)) in choices.iter().enumerate() {
        if !can_create {
            continue;
        }
        if count > best_count {
            best = Some(at);
            best_count = count;
            tied = false;
        } else if count == best_count && best_count > 0 {
            tied = true;
        }
    }
    if tied { None } else { best }
}

/// A language tag Door43's search may be asked for; anything else counts nothing.
fn countable(language: &str) -> bool {
    !language.is_empty()
        && language.len() <= 64
        && language
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl App {
    /// What a Door43 failure of a share step answers: the question again when the switch went
    /// off, the sign-in step when Door43 refused the token (which is then forgotten), "Door43
    /// cannot be reached" when it did not answer, and a report for anything else.
    async fn share_failure(&self, session: &Session, error: Door43Error, step: &str) -> ApiError {
        match error {
            Door43Error::Offline { .. } => ApiError::NeedsInternet(NetKind::Share),
            Door43Error::AuthFailed { .. } => self.door43_refused(session).await,
            Door43Error::Unreachable { .. }
            | Door43Error::Server { .. }
            | Door43Error::RateLimited { .. } => {
                ApiError::Refused(Refusal::Share(ShareRefusal::Offline))
            }
            other => self.internal(
                "a share step failed",
                facts(json!({ "step": step, "reason": other.to_string() })),
            ),
        }
    }
}

impl Handle<Orgs> for App {
    /// tC4's `organizationChoices`: the organizations, each one's permission and its count of
    /// repositories in the language, one Door43 call after another, in Door43's order.
    async fn handle(&self, req: Orgs) -> Result<ShareTargets, ApiError> {
        let session = self.door43_session().await?;
        let client = self.door43()?;
        let orgs = match client.list_orgs(&session).await {
            Ok(orgs) => orgs,
            Err(error) => return Err(self.share_failure(&session, error, "orgs").await),
        };
        let mut counts = Vec::with_capacity(orgs.len());
        for org in &orgs {
            let count = if countable(&req.language) {
                match client
                    .count_repositories(Some(&session), &org.name, &req.language)
                    .await
                {
                    Ok(count) => count,
                    Err(error) => return Err(self.share_failure(&session, error, "count").await),
                }
            } else {
                0
            };
            counts.push((org.can_create_repository, count));
        }
        let recommended = recommend(&counts);
        Ok(ShareTargets {
            user: session.username.to_string(),
            orgs: orgs
                .into_iter()
                .enumerate()
                .map(|(at, org)| OrgChoice {
                    name: org.name.to_string(),
                    full_name: org.full_name,
                    can_create: org.can_create_repository,
                    recommended: recommended == Some(at),
                })
                .collect(),
        })
    }
}

impl Handle<RepoLink> for App {
    /// The address as given, when it is on the app's Door43 server; the shell opens it.
    async fn handle(&self, req: RepoLink) -> Result<LinkView, ApiError> {
        let server = self.inner.config.door43.as_url().as_str();
        let base = server.trim_end_matches('/');
        let on_server = req
            .url
            .as_str()
            .strip_prefix(base)
            .is_some_and(|rest| rest.starts_with('/') && rest.len() > 1);
        if !on_server {
            return Err(ApiError::BadRequest {
                field: "url".to_owned(),
                reason: "not on the app's Door43 server".to_owned(),
            });
        }
        Ok(LinkView { url: req.url })
    }
}

impl App {
    /// The outcome of the next `door43/open` job: the project it added, or its refusal.
    #[cfg(feature = "test-hooks")]
    pub fn script_open_door43(&self, outcome: Result<tc_api::ids::ProjectRef, ApiError>) {
        if let Some(scripted) = self.inner.sync.scripted() {
            scripted.script_open(outcome);
        }
    }
}

impl Handle<OpenDoor43> for App {
    /// tC4's `openFromDoor43Asking` after its question: the name is checked against every
    /// entry of the projects folder, without letter case, before anything is sent; then the
    /// job.
    async fn handle(&self, req: OpenDoor43) -> Result<JobId, ApiError> {
        let name = req.name.clone();
        let listed = self
            .with_store(move |store| {
                store.list().map(|listing| {
                    name.in_use(
                        listing
                            .projects
                            .iter()
                            .map(|p| p.folder.as_str())
                            .chain(listing.unusable.iter().map(|u| u.name.as_str()))
                            .chain(listing.others.iter().map(String::as_str)),
                    )
                })
            })
            .await?;
        let in_use = listed.map_err(|e| {
            self.internal(
                "the projects folder could not be listed",
                facts(json!({ "reason": e.to_string() })),
            )
        })?;
        if in_use {
            return Err(ApiError::Refused(Refusal::OpenDoor43(
                OpenDoor43Refusal::NameExists,
            )));
        }
        let app = self.clone();
        let spec = JobSpec::new(JobKind::Join, Phase::Sync(SyncPhase::Fetch));
        Ok(self
            .inner
            .jobs
            .start_typed::<OpenDoor43, _, _>(spec, move |_ctx| async move {
                let port = app.inner.sync.clone();
                port.open(app, req.repo, req.name)
                    .await
                    .map_err(JobEnd::Failed)
            }))
    }
}

impl App {
    /// The open from Door43 (E1a, unit X's `tc_sync::join`): `repo` fetched and placed as the
    /// project `name`. A refusal before anything was placed changes nothing.
    pub(crate) async fn door43_open(
        &self,
        repo: tc_api::door43::RepoSource,
        name: tc_api::door43::ProjectName,
    ) -> Result<tc_api::ids::ProjectRef, ApiError> {
        let client = self.door43()?.clone();
        let session = self.door43_session().await?;
        let repo: tc_door43::RepoRef = format!("{}/{}", repo.owner(), repo.name())
            .parse()
            .map_err(|_| ApiError::BadRequest {
                field: "repo".to_owned(),
                reason: "not owner/repository".to_owned(),
            })?;
        let store = self.inner.sessions.store().clone();
        let joined = store
            .door43_join(client, Some(session), repo, name.as_str().to_owned())
            .await
            .ok_or(ApiError::Unavailable(tc_api::error::Feature::Sync))?;
        let folder = joined.map_err(|e| match e {
            tc_sync::JoinError::Blocked(block) => {
                ApiError::Refused(Refusal::Sync(tc_api::refusal::SyncFailure::Blocked(block)))
            }
            tc_sync::JoinError::Refused(refusal) => ApiError::Refused(Refusal::Sync(
                tc_api::refusal::SyncFailure::BeforeIntake(refusal),
            )),
            other => self.internal(
                "the project from Door43 could not be placed",
                facts(json!({ "reason": other.to_string() })),
            ),
        })?;
        let placed = folder
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| tc_api::ids::ProjectRef::parse(n).ok());
        placed.ok_or_else(|| {
            self.internal(
                "the project from Door43 was placed under a name the app cannot read",
                facts(json!({ "folder": folder.display().to_string() })),
            )
        })
    }
}
