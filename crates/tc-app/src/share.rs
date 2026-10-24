//! The share dialog's first step, the repository link and Open from Door43 (unit B9b; tC4
//! #362, #677). The share itself, its push and its end are unit E1b's, and the open's copy
//! from Door43 unit X's: until X lands, [`ScriptedOpenDoor43`] answers for it (AD6).
//!
//! Every call here takes the account and write server, `--door43` (tC4 #120), with the internet
//! check of its endpoint done by dispatch first.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde_json::json;
use tc_api::account::{LinkView, OrgChoice, Orgs, RepoLink, ShareTargets};
use tc_api::door43::OpenDoor43;
use tc_api::endpoint::NetKind;
use tc_api::error::Feature;
use tc_api::ids::{JobId, ProjectRef};
use tc_api::jobs::{JobKind, Phase, SyncPhase};
use tc_api::refusal::{OpenDoor43Refusal, ShareRefusal};
use tc_api::{ApiError, Refusal};
use tc_door43::{Door43Error, Session};

use crate::app::{App, lock};
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

/// The open from Door43 before unit X (AD6): the outcomes a test scripted, in order, and
/// `Unavailable(Sync)` when none is left. It sends nothing and writes nothing.
#[derive(Default)]
pub(crate) struct ScriptedOpenDoor43 {
    scripted: Mutex<VecDeque<Result<ProjectRef, ApiError>>>,
}

impl ScriptedOpenDoor43 {
    fn next(&self) -> Result<ProjectRef, ApiError> {
        lock(&self.scripted)
            .pop_front()
            .unwrap_or(Err(ApiError::Unavailable(Feature::Sync)))
    }

    #[cfg(feature = "test-hooks")]
    pub(crate) fn clear(&self) {
        lock(&self.scripted).clear();
    }
}

impl App {
    /// The outcome of the next `door43/open` job: the project it added, or its refusal.
    #[cfg(feature = "test-hooks")]
    pub fn script_open_door43(&self, outcome: Result<ProjectRef, ApiError>) {
        lock(&self.inner.open_door43.scripted).push_back(outcome);
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
                app.inner.open_door43.next().map_err(JobEnd::Failed)
            }))
    }
}
