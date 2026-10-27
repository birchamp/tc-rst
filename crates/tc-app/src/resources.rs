//! Get source texts (M8 steps 1 and 2, unit B9a): the gateway-language packages, one
//! organization's package for a book from Door43's catalog (tC4 `readPackage`, `packageRows`),
//! and the `Download` job (tC4 `downloadPackage`).
//!
//! Resources are read from Door43's production server whatever server the account uses: a
//! development build signs in to the QA server, and only account and write calls go there
//! (tC4 #120). The download streams through A11's `install_dcs_watched`, with a
//! [`DownloadWatch`] that hears the job's bytes and answers from its cancel token, so a cancel
//! stops the download at its next chunk and nothing of it stays (§4.6).

use tc_api::endpoint::NetKind;
use tc_api::ids::JobId;
use tc_api::jobs::{JobKind, Phase};
use tc_api::refusal::{Refusal, ResourceRefusal};
use tc_api::resources::{
    Catalog, CatalogView, Download, FromInstalls, GatewayPackage, GatewayRef, PackageRole,
    PackageRow, Seen, SourceGateway, Sources, SourcesView,
};
use tc_api::shown::Direction;
use tc_api::{ApiError, Scope};
use tc_core::BookId;
use tc_door43::{
    CatalogEntry, CatalogQuery, Door43Client, Door43Error, DownloadWatch, RepoRef, ServerUrl,
};
use tc_resources::gateways::{self, Gateway};
use tc_resources::{
    DCS_HOST, InstallError, Packaging, ProjectPins, RepoPath, ResourcePin, ResourceStore, Rung,
    SetSlot, install_dcs_watched, pin_release,
};

use crate::app::{App, lock};
use crate::dispatch::Handle;
use crate::jobs::{CancelToken, JobCtx, JobEnd, JobSpec, ProgressSink};

/// The topic Door43 gives a repository tC4 can use (tC4 `TC_READY_TOPIC`).
const TC_READY: &str = "tc-ready";

/// The server resources are read from: production, unless the account's server is a test
/// server on this computer (the fake), which then serves both. A development build's account
/// server is Door43's QA server, and resources never come from it (tC4 #120).
pub fn resources_server(door43: &ServerUrl) -> ServerUrl {
    if *door43 == ServerUrl::qa() {
        ServerUrl::production()
    } else {
        door43.clone()
    }
}

pub(crate) fn package(gateway: &Gateway) -> GatewayPackage {
    GatewayPackage {
        gateway: GatewayRef {
            language: gateway.language.to_owned(),
            owner: gateway.owner.to_owned(),
        },
        name: gateway.name.to_owned(),
        autonym: gateway.autonym.to_owned(),
        direction: match gateway.direction {
            gateways::Direction::Ltr => Direction::Ltr,
            gateways::Direction::Rtl => Direction::Rtl,
        },
        seen: Seen {
            tn: gateway.seen.tn.to_owned(),
            tw: gateway.seen.tw.to_owned(),
            ta: gateway.seen.ta.to_owned(),
        },
    }
}

pub(crate) fn sets_kind(kind: tc_journal::fold::ProjectKind) -> tc_resources::ProjectKind {
    match kind {
        tc_journal::fold::ProjectKind::Bible => tc_resources::ProjectKind::Bible,
        tc_journal::fold::ProjectKind::Stories => tc_resources::ProjectKind::Obs,
    }
}

/// One repository as the catalog lists it, with what [`package_rows`] reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRepo {
    pub name: String,
    pub flavor: String,
    pub topics: Vec<String>,
    /// The books it holds, as the catalog spells them (lower case).
    pub books: Vec<String>,
    pub description: String,
}

impl CatalogRepo {
    fn of(entry: &CatalogEntry) -> Self {
        Self {
            name: entry.repo.name.to_string(),
            flavor: entry.flavor.clone(),
            topics: entry.topics.clone(),
            books: entry.books.clone(),
            description: entry.title.clone(),
        }
    }
}

/// The part a repository plays, or `None` when tC4 does not use it (tC4 `roleOf`). The
/// catalog gives `<lang>_tw` and `<lang>_ta` one flavor, so their names decide, and a Bible
/// package offers `<lang>_twl` where a story package reads `<lang>_obs-twl`.
fn role_of(repo: &CatalogRepo, stories: bool) -> Option<PackageRole> {
    let name = repo.name.as_str();
    if stories {
        if name.ends_with("_obs") {
            return Some(PackageRole::ObsText);
        }
        if name.ends_with("_obs-tn") {
            return Some(PackageRole::ObsNotes);
        }
        if name.ends_with("_obs-twl") {
            return Some(PackageRole::ObsWords);
        }
        if repo.flavor == "x-obsimages" {
            return Some(PackageRole::ObsImages);
        }
        let bible_only = ["_tq", "_tn", "_twl", "_ust", "_gst"];
        if bible_only.iter().any(|s| name.ends_with(s)) || repo.flavor == "textTranslation" {
            return None;
        }
    } else if name.contains("_obs-") || name.ends_with("_obs") {
        return None;
    }
    let by_suffix = [
        ("_twl", PackageRole::WordLinks),
        ("_tw", PackageRole::Words),
        ("_ta", PackageRole::Academy),
        ("_tq", PackageRole::Questions),
    ];
    if let Some((_, role)) = by_suffix.iter().find(|(s, _)| name.ends_with(s)) {
        return Some(*role);
    }
    match repo.flavor.as_str() {
        "x-bcvnotes" => Some(PackageRole::Notes),
        "x-bcvquestions" => Some(PackageRole::Questions),
        "textTranslation" => Some(PackageRole::Text),
        _ => None,
    }
}

/// Whether a row of this role is offered only for the books its repository holds.
fn book_scoped(role: PackageRole) -> bool {
    matches!(
        role,
        PackageRole::Text | PackageRole::Notes | PackageRole::Questions | PackageRole::WordLinks
    )
}

/// The rows of one organization's package for one book (tC4 `packageRows`): a repository with
/// the `tc-ready` topic and a role, and, for a book-scoped role, whose books include `book` (a
/// repository that lists no books is offered).
pub fn package_rows(repos: &[CatalogRepo], book: BookId, stories: bool) -> Vec<PackageRow> {
    let code = book.code();
    repos
        .iter()
        .filter(|r| r.topics.iter().any(|t| t == TC_READY))
        .filter_map(|r| {
            let role = role_of(r, stories)?;
            let covers = r.books.is_empty() || r.books.iter().any(|b| b.eq_ignore_ascii_case(code));
            (!book_scoped(role) || covers).then(|| PackageRow {
                role,
                repo: r.name.clone(),
                desc: r.description.clone(),
            })
        })
        .collect()
}

fn gateway_of(gateway: &GatewayRef) -> Result<&'static Gateway, ApiError> {
    gateways::find(&gateway.language, &gateway.owner).ok_or_else(|| ApiError::BadRequest {
        field: "gateway".to_owned(),
        reason: format!("{} is not a gateway language package", gateway.key()),
    })
}

fn named(language: &tc_resources::GatewayLanguage) -> GatewayRef {
    GatewayRef {
        language: language.language.clone(),
        owner: language.owner.as_str().to_owned(),
    }
}

/// The project's pins from its fold, `None` when it records none.
pub(crate) fn project_pins(
    read: &crate::counters::CountingSnapshot,
) -> Result<Option<ProjectPins>, ApiError> {
    crate::understand::project_pins(read).map_err(|reason| ApiError::BadRequest {
        field: "pins".to_owned(),
        reason,
    })
}

impl App {
    /// Every Door43 install on this computer as a pin, read on the blocking pool.
    pub(crate) async fn installed_pins(&self) -> Result<Vec<ResourcePin>, String> {
        let store = self.inner.resources.clone();
        tokio::task::spawn_blocking(move || store.installed_dcs_pins())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }

    pub(crate) fn resources_client(&self) -> Result<Door43Client, ApiError> {
        self.inner.resources_door43.clone().map_err(|reason| {
            ApiError::Refused(Refusal::Resources(ResourceRefusal::CatalogUnreadable {
                reason,
            }))
        })
    }

    fn project_kind(&self, at: &Scope) -> Result<tc_journal::fold::ProjectKind, ApiError> {
        Ok(self.read(at)?.handle().facts().kind)
    }
}

impl Handle<Sources> for App {
    async fn handle(&self, req: Sources) -> Result<SourcesView, ApiError> {
        let session = self.read(&req.at)?;
        let pins = project_pins(&session.read())?;
        let kind = self.project_kind(&req.at)?;
        let downloaded = lock(&self.inner.downloaded).clone();
        let primary = pins.as_ref().and_then(|p| p.set(Rung::Primary));
        let current = primary.and_then(|set| set.gateway.named()).map(named);
        let installed = self.installed_pins().await;
        let checkable = match &installed {
            Ok(installed) => FromInstalls::Listed(
                gateways::GATEWAYS
                    .iter()
                    .filter(|g| {
                        gateways::language_set_from_installed(installed, g, sets_kind(kind))
                            .is_some()
                    })
                    .map(|g| package(g).gateway)
                    .collect(),
            ),
            Err(reason) => FromInstalls::Failed {
                reason: reason.clone(),
            },
        };
        // An unreadable install list is already stated by `checkable`; the cards then name
        // tC4's releases.
        let pins_here = installed.as_deref().unwrap_or_default();
        Ok(SourcesView {
            gateways: gateways::GATEWAYS
                .iter()
                .map(|g| {
                    let key = g.key().to_string();
                    let installed = downloaded.iter().filter(|(k, _)| *k == key).count();
                    let release = |slot, seen: &str| {
                        gateways::installed_release(pins_here, g, slot, primary)
                            .unwrap_or(seen)
                            .to_owned()
                    };
                    SourceGateway {
                        package: GatewayPackage {
                            seen: Seen {
                                tn: release(SetSlot::TranslationNotes, g.seen.tn),
                                tw: release(SetSlot::TranslationWords, g.seen.tw),
                                ta: release(SetSlot::TranslationAcademy, g.seen.ta),
                            },
                            ..package(g)
                        },
                        installed: u32::try_from(installed).unwrap_or(u32::MAX),
                    }
                })
                .collect(),
            current,
            checkable,
            pinned: pins.as_ref().is_some_and(ProjectPins::has_language_sets),
        })
    }
}

impl Handle<Catalog> for App {
    async fn handle(&self, req: Catalog) -> Result<CatalogView, ApiError> {
        let kind = self.project_kind(&req.at)?;
        let gateway = gateway_of(&req.gateway)?;
        let client = self.resources_client()?;
        let owner =
            gateway
                .owner
                .parse()
                .map_err(|e: tc_door43::ParseError| ApiError::BadRequest {
                    field: "gateway".to_owned(),
                    reason: e.to_string(),
                })?;
        let query = CatalogQuery {
            owners: vec![owner],
            ..CatalogQuery::default()
        };
        let listing = client.catalog_search(&query).await.map_err(catalog_error)?;
        let repos: Vec<CatalogRepo> = listing.items.iter().map(CatalogRepo::of).collect();
        let stories = kind == tc_journal::fold::ProjectKind::Stories;
        Ok(CatalogView {
            rows: package_rows(&repos, req.book, stories),
        })
    }
}

fn catalog_error(error: Door43Error) -> ApiError {
    match error {
        Door43Error::Offline { .. } => ApiError::NeedsInternet(NetKind::Sources),
        other => ApiError::Refused(Refusal::Resources(ResourceRefusal::CatalogUnreadable {
            reason: other.to_string(),
        })),
    }
}

/// What a download tells its job, and what it asks it.
pub(crate) struct JobWatch {
    pub(crate) progress: ProgressSink,
    pub(crate) cancel: CancelToken,
    pub(crate) phase: Phase,
}

impl DownloadWatch for JobWatch {
    fn received(&self, received: u64, announced: Option<u64>) {
        self.progress
            .forward(self.phase.clone(), received, announced);
    }

    fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

/// The pin of `repo` the project names and this computer lacks (tC4
/// `unsatisfiedProjectPinFor`): a download fetches that version, never a newer one.
fn wanted_pin(
    pins: Option<&ProjectPins>,
    repo: &RepoRef,
    installed: &[ResourcePin],
) -> Option<ResourcePin> {
    pins?
        .all_pins()
        .into_iter()
        .find(|p| p.is_repo(repo) && !installed.iter().any(|i| i.same_identity(p)))
        .cloned()
}

/// How one repository of a download ended.
enum Fetched {
    Installed,
    /// This computer already holds it.
    Present,
}

async fn fetch_one(
    store: &ResourceStore,
    client: &Door43Client,
    path: &RepoPath,
    wanted: Option<ResourcePin>,
    present: bool,
    watch: &JobWatch,
) -> Result<Fetched, InstallError> {
    let pin = match wanted {
        Some(pin) => pin,
        None if present => return Ok(Fetched::Present),
        None => {
            let release = client.latest_release(path.repo()).await?;
            pin_release(client, path, &release.tag).await?
        }
    };
    let packaging = Packaging::for_pin(&pin);
    install_dcs_watched(store, client, &pin, packaging, watch).await?;
    Ok(Fetched::Installed)
}

impl Handle<Download> for App {
    async fn handle(&self, req: Download) -> Result<JobId, ApiError> {
        let session = self.read(&req.at)?;
        let gateway = gateway_of(&req.gateway)?;
        let client = self.resources_client()?;
        let pins = project_pins(&session.read())?;
        let mut paths = Vec::new();
        for name in &req.repos {
            let path: RepoPath = format!("{DCS_HOST}/{}/{name}", gateway.owner)
                .parse()
                .map_err(|e: tc_resources::ParseError| ApiError::BadRequest {
                    field: "repos".to_owned(),
                    reason: e.to_string(),
                })?;
            paths.push(path);
        }
        let first = paths.first().map(|p| p.repo().name.to_string());
        let spec = JobSpec::new(
            JobKind::Download,
            Phase::Install {
                repo: first.unwrap_or_default(),
            },
        )
        .project(session.scope().project.clone());
        let app = self.clone();
        let key = req.gateway.key();
        let book = req.book;
        let runtime = tokio::runtime::Handle::current();
        Ok(self
            .inner
            .jobs
            .start_typed_blocking::<Download, _>(spec, move |ctx: JobCtx| {
                let installed = app.inner.resources.installed_dcs_pins().unwrap_or_default();
                let mut failed = Vec::new();
                let mut fetched_any = false;
                let mut stopped = None;
                for path in &paths {
                    if ctx.cancel.check().is_err() {
                        stopped = Some(JobEnd::Cancelled);
                        break;
                    }
                    let name = path.repo().name.to_string();
                    let phase = Phase::Install { repo: name.clone() };
                    ctx.progress.phase(phase.clone(), None);
                    let watch = JobWatch {
                        progress: ctx.progress.clone(),
                        cancel: ctx.cancel.clone(),
                        phase,
                    };
                    let wanted = wanted_pin(pins.as_ref(), path.repo(), &installed);
                    let present = installed.iter().any(|p| p.is_repo(path.repo()));
                    let store = &app.inner.resources;
                    let outcome =
                        runtime.block_on(fetch_one(store, &client, path, wanted, present, &watch));
                    match outcome {
                        Ok(Fetched::Installed) => fetched_any = true,
                        Ok(Fetched::Present) => {}
                        Err(InstallError::Door43(Door43Error::Cancelled { .. })) => {
                            stopped = Some(JobEnd::Cancelled);
                            break;
                        }
                        Err(InstallError::Door43(Door43Error::Offline { .. })) => {
                            stopped =
                                Some(JobEnd::Failed(ApiError::NeedsInternet(NetKind::Sources)));
                            break;
                        }
                        Err(e) => failed.push(format!("{name}: {e}")),
                    }
                }
                // Each repository installs whole, so one finished before a cancel or a failure stays.
                if fetched_any {
                    // The open book's panes and helps read again: what was missing may be here now
                    // (tC4 bumps `installEpoch`, which Understand's loader watches).
                    runtime.block_on(session.books().forget());
                }
                // tC4 counts a package for step 1 once any of its repositories is here.
                if fetched_any || (stopped.is_none() && failed.len() < paths.len()) {
                    lock(&app.inner.downloaded).insert((key, book));
                }
                if let Some(end) = stopped {
                    return Err(end);
                }
                // What the download brought joins the project's sets (tC4 `adoptDownloadedPins`).
                if (fetched_any || failed.len() < paths.len())
                    && let Err(error) =
                        runtime.block_on(app.adopt_downloaded_pins(&session, gateway))
                {
                    return Err(JobEnd::Failed(ApiError::Refused(Refusal::Resources(
                        ResourceRefusal::AdoptFailed { error },
                    ))));
                }
                if failed.is_empty() {
                    Ok(if fetched_any {
                        tc_api::resources::Downloaded::Fetched
                    } else {
                        tc_api::resources::Downloaded::AlreadyHere
                    })
                } else {
                    Err(JobEnd::Failed(ApiError::Refused(Refusal::Resources(
                        ResourceRefusal::SomeFailed {
                            list: failed.join(" · "),
                        },
                    ))))
                }
            }))
    }
}

#[cfg(test)]
mod tests {
    //! tC4's `test/packageRows.test.ts`, ported: the real catalog shape of `unfoldingWord`,
    //! trimmed to what the rows read, with its two traps (`en_tw` and `en_ta` share a flavor,
    //! and `en_twl` carries `x-bcvarticles`).

    use super::*;

    fn repo(name: &str, flavor: &str, topics: &[&str], books: &[&str]) -> CatalogRepo {
        CatalogRepo {
            name: name.into(),
            flavor: flavor.into(),
            topics: topics.iter().map(|t| (*t).into()).collect(),
            books: books.iter().map(|b| (*b).into()).collect(),
            description: format!("{name} description"),
        }
    }

    fn catalog() -> Vec<CatalogRepo> {
        let ready = &["tc-ready"][..];
        let obs = &["tc-ready", "obs"][..];
        vec![
            repo("en_tn", "x-bcvnotes", ready, &["tit", "jon", "heb"]),
            repo("en_tq", "x-bcvquestions", ready, &["tit", "jon"]),
            repo("en_tw", "x-peripheralArticles", ready, &["bible"]),
            repo(
                "en_ta",
                "x-peripheralArticles",
                ready,
                &["intro", "process"],
            ),
            repo("en_twl", "x-bcvarticles", ready, &["tit", "jon"]),
            repo("en_ult", "textTranslation", ready, &["tit", "jon"]),
            repo("en_ust", "textTranslation", ready, &["tit", "jon"]),
            repo("en_t4t", "textTranslation", ready, &["gen", "jon"]),
            repo("en_obs", "textStories", obs, &["obs"]),
            repo("en_obs-tn", "x-notes", obs, &["obs"]),
            repo("en_obs-twl", "x-links", obs, &["obs"]),
            repo("ContentTechs", "", &[], &[]),
        ]
    }

    fn rows(book: BookId) -> Vec<PackageRow> {
        package_rows(&catalog(), book, false)
    }

    fn role(rows: &[PackageRow], repo: &str) -> Option<PackageRole> {
        rows.iter().find(|r| r.repo == repo).map(|r| r.role)
    }

    fn repos(rows: &[PackageRow]) -> Vec<&str> {
        let mut names: Vec<&str> = rows.iter().map(|r| r.repo.as_str()).collect();
        names.sort_unstable();
        names
    }

    #[test]
    fn words_and_academy_are_told_apart_by_name_not_by_flavor() {
        let tit = rows(BookId::Titus);
        assert_eq!(role(&tit, "en_tw"), Some(PackageRole::Words));
        assert_eq!(role(&tit, "en_ta"), Some(PackageRole::Academy));
    }

    #[test]
    fn word_links_are_their_own_book_scoped_row() {
        let tit = rows(BookId::Titus);
        assert_eq!(role(&tit, "en_twl"), Some(PackageRole::WordLinks));
        assert!(!PackageRole::WordLinks.fixed());
        assert_eq!(role(&rows(BookId::Hebrews), "en_twl"), None);
    }

    #[test]
    fn notes_questions_and_texts_have_their_roles() {
        let tit = rows(BookId::Titus);
        assert_eq!(role(&tit, "en_tn"), Some(PackageRole::Notes));
        assert_eq!(role(&tit, "en_tq"), Some(PackageRole::Questions));
        assert_eq!(role(&tit, "en_ult"), Some(PackageRole::Text));
        assert!(PackageRole::Text.fixed() && !PackageRole::Notes.fixed());
    }

    #[test]
    fn repositories_without_the_topic_and_story_resources_are_left_out_of_a_bible() {
        let tit = rows(BookId::Titus);
        assert_eq!(role(&tit, "ContentTechs"), None);
        assert!(tit.iter().all(|r| !r.repo.starts_with("en_obs")));
    }

    #[test]
    fn a_story_project_gets_story_text_story_helps_and_shared_articles() {
        let obs = package_rows(&catalog(), BookId::Titus, true);
        assert_eq!(
            repos(&obs),
            ["en_obs", "en_obs-tn", "en_obs-twl", "en_ta", "en_tw"]
        );
        assert_eq!(role(&obs, "en_obs"), Some(PackageRole::ObsText));
        assert!(PackageRole::ObsText.fixed());
    }

    #[test]
    fn a_book_scoped_repository_that_lacks_the_book_is_not_offered() {
        let heb = rows(BookId::Hebrews);
        assert_eq!(role(&heb, "en_tq"), None);
        assert_eq!(role(&heb, "en_tn"), Some(PackageRole::Notes));
        assert_eq!(role(&heb, "en_t4t"), None);
    }

    #[test]
    fn articles_are_offered_for_every_book() {
        for book in [BookId::Titus, BookId::Hebrews, BookId::Revelation] {
            let rows = rows(book);
            assert!(role(&rows, "en_tw").is_some() && role(&rows, "en_ta").is_some());
        }
        assert_eq!(repos(&rows(BookId::Revelation)), ["en_ta", "en_tw"]);
    }

    #[test]
    fn row_keys_are_tc4s() {
        let tit = rows(BookId::Titus);
        let notes = tit.iter().find(|r| r.repo == "en_tn").unwrap();
        assert_eq!(notes.key(), "notes:en_tn");
    }

    #[test]
    fn a_development_build_downloads_resources_from_production() {
        // tC4 #120: the account server of a development build is QA; resources are not.
        assert_eq!(resources_server(&ServerUrl::qa()), ServerUrl::production());
        assert_eq!(
            resources_server(&ServerUrl::production()),
            ServerUrl::production()
        );
        let fake: ServerUrl = "http://127.0.0.1:3000".parse().unwrap();
        assert_eq!(resources_server(&fake), fake);
    }
}
