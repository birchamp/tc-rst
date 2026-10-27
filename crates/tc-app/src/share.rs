//! The share dialog's first step, the repository link and Open from Door43 (unit B9b; tC4
//! #362, #677), and the share itself (E1b). The repository work is unit X's `tc_sync::share`
//! and `join`, behind [`crate::ports::SyncPort`].
//!
//! Every call here takes the account and write server, `--door43` (tC4 #120), with the internet
//! check of its endpoint done by dispatch first.

use serde_json::json;
use tc_api::account::{LinkView, OrgChoice, Orgs, RepoLink, ShareTargets, WebUrl};
use tc_api::door43::{
    OpenDoor43, ProjectName, RepoName, RepoSource, ShareOnDoor43, ShareOwner, SharedRepo,
};
use tc_api::endpoint::NetKind;
use tc_api::error::Feature;
use tc_api::ids::JobId;
use tc_api::ids::ProjectRef;
use tc_api::jobs::{JobKind, Phase, SharePhase, SyncPhase};
use tc_api::refusal::{OpenDoor43Refusal, ShareRefusal, SyncFailure, SyncRefusal};
use tc_api::{ApiError, Refusal};
use tc_door43::{Door43Error, NewRepoOwner, Session};

use crate::app::App;
use crate::dispatch::Handle;
use crate::flows::FlowRequest;
use crate::jobs::{JobEnd, JobSpec, ProgressSink};
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

impl App {
    /// The outcome of the next `door43/share` job on the scripted port.
    #[cfg(feature = "test-hooks")]
    pub fn script_share(&self, outcome: Result<tc_api::door43::SharedRepo, ApiError>) {
        if let Some(scripted) = self.inner.sync.scripted() {
            scripted.script_share(outcome);
        }
    }
}

impl Handle<ShareOnDoor43> for App {
    /// tC4's `shareRun`: a Door43 sign-in first, one share at a time (AD31), then the job,
    /// once the project's saves have settled so the checkpoint holds what the person saw.
    async fn handle(&self, req: ShareOnDoor43) -> Result<JobId, ApiError> {
        self.door43_session().await?;
        let pass = self.inner.flows.enter(FlowRequest::Share)?;
        // Checked under the flow, so two shares of one project cannot both pass it.
        let folder = req.project.clone();
        let connected = self
            .with_store(move |store| store.peek(&folder).map(|peek| peek.remote.is_some()))
            .await?;
        if connected.unwrap_or(false) {
            return Err(ApiError::Refused(Refusal::Share(
                ShareRefusal::AlreadyShared,
            )));
        }
        let spec = JobSpec::new(JobKind::Share, Phase::Share(SharePhase::Preparing))
            .project(req.project.clone())
            .pass(pass);
        let app = self.clone();
        Ok(self
            .inner
            .jobs
            .start_typed::<ShareOnDoor43, _, _>(spec, move |ctx| async move {
                app.inner.sessions.settled(&req.project).await;
                let port = app.inner.sync.clone();
                port.share(app, req.project, req.owner, req.name, ctx.progress)
                    .await
            }))
    }
}

impl App {
    /// Whether `name` is taken by any entry of the projects folder, without letter case
    /// (tC4 `repoPathInUse`): checked before anything is sent or written.
    pub(crate) async fn project_name_in_use(&self, name: &ProjectName) -> Result<bool, ApiError> {
        let name = name.clone();
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
        listed.map_err(|e| {
            self.internal(
                "the projects folder could not be listed",
                facts(json!({ "reason": e.to_string() })),
            )
        })
    }
}

impl Handle<OpenDoor43> for App {
    /// tC4's `openFromDoor43Asking` without its question (ruling H14): a project of that
    /// repository already here is the answer, since a project has one copy on a computer;
    /// else the folder is the first free one of the projects folder, without letter case.
    /// Both before anything is sent.
    async fn handle(&self, req: OpenDoor43) -> Result<JobId, ApiError> {
        let repo = req.repo.clone();
        // A remote names its server as written in the project's URL: the host with its port.
        let server = self.inner.config.door43.as_url();
        let host = server.host_str().map(|h| match server.port() {
            Some(port) => format!("{h}:{port}"),
            None => h.to_owned(),
        });
        let listed = self
            .with_store(move |store| {
                store.list().map(|listing| {
                    let ours = listing.projects.iter().find(|entry| {
                        store.peek(&entry.folder).is_ok_and(|peek| {
                            peek.remote.is_some_and(|remote| {
                                host.as_deref()
                                    .is_some_and(|h| h.eq_ignore_ascii_case(&remote.host))
                                    && remote.owner.eq_ignore_ascii_case(repo.owner())
                                    && remote.repo.eq_ignore_ascii_case(repo.name())
                            })
                        })
                    });
                    match ours {
                        Some(entry) => Err(entry.folder.clone()),
                        None => Ok(tc_api::door43::open_folder(
                            &repo,
                            listing
                                .projects
                                .iter()
                                .map(|p| p.folder.as_str())
                                .chain(listing.unusable.iter().map(|u| u.name.as_str()))
                                .chain(listing.others.iter().map(String::as_str)),
                        )),
                    }
                })
            })
            .await?;
        let name = listed
            .map_err(|e| {
                self.internal(
                    "the projects folder could not be listed",
                    facts(json!({ "reason": e.to_string() })),
                )
            })?
            .map_err(|project| {
                ApiError::Refused(Refusal::OpenDoor43(OpenDoor43Refusal::AlreadyHere {
                    project,
                }))
            })?;
        let app = self.clone();
        let spec = JobSpec::new(JobKind::Join, Phase::Sync(SyncPhase::Fetch));
        Ok(self
            .inner
            .jobs
            .start_typed::<OpenDoor43, _, _>(spec, move |_ctx| async move {
                let port = app.inner.sync.clone();
                port.open(app, req.repo, name).await.map_err(JobEnd::Failed)
            }))
    }
}

/// Whether an open from Door43 carried a sign-in: only a try without one may ask for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenTry {
    Anonymous,
    SignedIn,
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
        // A public repository opens without a sign-in; a kept one is resumed if there is one.
        let session = match self.door43_session().await {
            Ok(session) => Some(session),
            Err(ApiError::NeedsSignIn(_)) => None,
            Err(e) => return Err(e),
        };
        let attempt = match session {
            Some(_) => OpenTry::SignedIn,
            None => OpenTry::Anonymous,
        };
        let repo: tc_door43::RepoRef = format!("{}/{}", repo.owner(), repo.name())
            .parse()
            .map_err(|_| ApiError::BadRequest {
                field: "repo".to_owned(),
                reason: "not owner/repository".to_owned(),
            })?;
        let store = self.inner.sessions.store().clone();
        let joined = store
            .door43_join(client, session.clone(), repo, name.as_str().to_owned())
            .await
            .ok_or(ApiError::Unavailable(tc_api::error::Feature::Sync))?;
        // INV-PRIVATE-OPEN-ASKS-SIGN-IN-ONCE: Door43 answers a private repository with 401 (git)
        // or 404 (API) to a request without a token. Only an anonymous try asks the sign-in; the
        // try after it carries the token, and its 401 or 404 is the refusal.
        if let Err(tc_sync::JoinError::Refused(
            refusal @ (SyncRefusal::AuthFailed | SyncRefusal::RemoteMissing),
        )) = &joined
        {
            match (attempt, &session) {
                (OpenTry::Anonymous, _) => {
                    return Err(ApiError::NeedsSignIn(
                        tc_api::error::SignInReason::NotSignedIn,
                    ));
                }
                (OpenTry::SignedIn, Some(session)) if *refusal == SyncRefusal::AuthFailed => {
                    return Err(self.door43_refused(session).await);
                }
                (OpenTry::SignedIn, _) => {}
            }
        }
        let folder = joined.map_err(|e| match e {
            tc_sync::JoinError::Blocked(block) => {
                ApiError::Refused(Refusal::Sync(tc_api::refusal::SyncFailure::Blocked(block)))
            }
            tc_sync::JoinError::Refused(refusal) => ApiError::Refused(Refusal::Sync(
                tc_api::refusal::SyncFailure::BeforeIntake(refusal),
            )),
            // Taken since the handler's check: the same answer the check gives, nothing touched.
            tc_sync::JoinError::Create(tc_store::CreateError::NameExists(_)) => {
                ApiError::Refused(Refusal::OpenDoor43(OpenDoor43Refusal::NameExists))
            }
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

impl App {
    /// The first share of `folder` (E1b, unit X's `tc_sync::share`): the repository `name` under
    /// `owner`, refused before any push when that name exists, then the sync that checkpoints
    /// and pushes the project's `main` without force. A push that fails after the repository
    /// was made ends with the sync's refusal; the project stays connected, so its card offers
    /// Sync, which sends the work, and the dialog offers no second Share.
    pub(crate) async fn door43_share(
        &self,
        folder: ProjectRef,
        owner: ShareOwner,
        name: RepoName,
        progress: ProgressSink,
    ) -> Result<SharedRepo, JobEnd> {
        let client = self.door43().map_err(JobEnd::Failed)?.clone();
        let session = self.door43_session().await.map_err(JobEnd::Failed)?;
        let bad_name = |field: &str| {
            JobEnd::Failed(ApiError::BadRequest {
                field: field.to_owned(),
                reason: "not a name Door43 accepts".to_owned(),
            })
        };
        let new_owner = match &owner {
            ShareOwner::Mine => NewRepoOwner::SignedInUser,
            ShareOwner::Organization(org) => {
                NewRepoOwner::Organization(org.as_str().parse().map_err(|_| bad_name("owner"))?)
            }
        };
        let new_name: tc_door43::NewRepoName =
            name.as_str().parse().map_err(|_| bad_name("name"))?;
        let lease = self
            .inner
            .sessions
            .open_side(&folder, progress.clone())
            .await
            .map_err(|e| {
                JobEnd::Failed(self.internal(
                    "the project to share did not open",
                    facts(json!({ "reason": format!("{e:?}") })),
                ))
            })?;
        let handle = lease.session().handle().clone();
        progress.phase(Phase::Share(SharePhase::Create), None);
        let pushing = {
            let progress = progress.clone();
            Box::new(move || progress.phase(Phase::Share(SharePhase::Push), None))
        };
        let shared = handle
            .door43_share(client, session.clone(), new_owner, new_name, pushing)
            .await;
        drop(lease);
        self.inner.sessions.settled(&folder).await;
        self.inner
            .bus
            .publish(tc_api::event::Event::HomeCard { project: folder });
        let refused = |r: ShareRefusal| JobEnd::Failed(ApiError::Refused(Refusal::Share(r)));
        let (created, report) = match shared {
            None => return Err(JobEnd::Failed(ApiError::Unavailable(Feature::Sync))),
            Some(Ok(done)) => done,
            Some(Err(tc_sync::ShareError::NameExists)) => {
                return Err(refused(ShareRefusal::NameExists));
            }
            Some(Err(tc_sync::ShareError::Refused(refusal))) => {
                return Err(match refusal {
                    SyncRefusal::AuthFailed => JobEnd::Failed(self.door43_refused(&session).await),
                    SyncRefusal::Offline | SyncRefusal::Unreachable => {
                        refused(ShareRefusal::Offline)
                    }
                    SyncRefusal::NoWriteAccess { server_message }
                    | SyncRefusal::RefRejected { server_message } => {
                        refused(ShareRefusal::CreateRejected { server_message })
                    }
                    other => refused(ShareRefusal::CreateRejected {
                        server_message: format!("{other:?}"),
                    }),
                });
            }
            Some(Err(tc_sync::ShareError::Connect(e))) => {
                return Err(JobEnd::Failed(self.internal(
                    "the shared repository could not be connected",
                    facts(json!({ "reason": e.to_string() })),
                )));
            }
        };
        match report.ended {
            tc_sync::machine::Ended::Synced { .. } => {}
            tc_sync::machine::Ended::Failed(
                SyncFailure::BeforeIntake(SyncRefusal::AuthFailed)
                | SyncFailure::AfterIntake(SyncRefusal::AuthFailed),
            ) => return Err(JobEnd::Failed(self.door43_refused(&session).await)),
            tc_sync::machine::Ended::Failed(failure) => {
                return Err(JobEnd::Failed(ApiError::Refused(Refusal::Sync(failure))));
            }
        }
        let repo = RepoSource::new(created.repo.owner.as_str(), created.repo.name.as_str())
            .map_err(|_| {
                JobEnd::Failed(self.internal(
                    "Door43 named the shared repository in a form the app cannot read",
                    facts(json!({ "repo": created.repo.to_string() })),
                ))
            })?;
        let url = WebUrl::parse(created.html_url.as_str()).map_err(|_| {
            JobEnd::Failed(self.internal(
                "Door43 gave the shared repository an address the app cannot open",
                facts(json!({ "url": created.html_url.as_str() })),
            ))
        })?;
        Ok(SharedRepo { repo, url })
    }
}
