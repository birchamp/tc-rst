//! Release upgrades and the gateway change (unit C8; tC4 `checkForUpdates`, `upgradeSet`,
//! `upgradeTextNow`, `previewGatewayChange`, `confirmUpgrade`, `commitGatewayChange`).
//!
//! The pattern is APP-DESIGN §4.8's. The server plans on a snapshot of the project, keeps the
//! plan with the heads it read, and the dialog shows what the plan costs. Confirming sends only
//! the plan's id. The store checks the heads in one `ApplyUpgrade` batch; when some moved (a
//! teammate's work arrived), nothing is written, the plan is made again once from the project as
//! it is, and the dialog is shown again with the new figures for the person to read. The pins
//! move only in that batch, after every resource of the release is installed and verified, so a
//! failed or cancelled download changes nothing, and a failed save keeps the plan for the
//! retry.

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

use tc_api::endpoint::NetKind;
use tc_api::ids::{JobId, PlanId, Scope};
use tc_api::jobs::{JobKind, Phase};
use tc_api::refusal::{Refusal, ResourceRefusal};
use tc_api::resources::{
    ApplyPlan, CheckUpdates, GatewayChange, GatewayRef, ImageChange, OfferRef, PlanApplied,
    PlanGateway, PlanUpgrade, PlanView, PlanWhat, RepoMove, SetOffer as SetOfferView, SetRung,
    TextKind, TextOffer as TextOfferView, UpdatesView,
};
use tc_api::{ApiError, NotSaved};
use tc_check::{Collection, Coverage, ResourceRef, ToolId};
use tc_core::{BookId, Testament};
use tc_door43::{Door43Client, Door43Error, RepoRef};
use tc_helps::WordArticleId;
use tc_helps::metadata::parse_burrito_metadata;
use tc_helps::tsv::{TsvScope, parse_notes, parse_word_links};
use tc_journal::command::{PlannedChange, UpgradePlan};
use tc_journal::keys::PinGroup;
use tc_journal::record::{PinGroupValue, Record};
use tc_journal::{Cause, RecordKey};
use tc_resources::gateway_change::{
    apply_gateway_change, complete_language_sets, merge_optional_pins, source_panes_for_gateway,
};
use tc_resources::gateways::{self, Gateway};
use tc_resources::resolve::backfill_coverage;
use tc_resources::updates::{
    self, LatestReleases, SetOffer, TextKind as ResourceTextKind, TextOffer,
};
use tc_resources::{
    InstallError, LanguageSet, Packaging, ProjectPins, ResourcePin, ResourceStore, Rung,
    install_dcs_watched,
};

use crate::app::{App, lock};
use crate::backend::{CommandError, StoreCommand};
use crate::counters::CountingSnapshot;
use crate::dispatch::Handle;
use crate::jobs::{JobCtx, JobEnd, JobSpec};
use crate::numbering::Numbering;
use crate::plans::{self, ListSource, PlanError, PlanSources, ProjectUnits, UnitSpec};
use crate::resources::{JobWatch, package, sets_kind};
use crate::session::ProjectSession;
use crate::wire;

/// How many plans the server keeps: a person reads one at a time, and a plan is dropped when
/// it is applied or another replaces the oldest.
const KEPT: usize = 8;

/// What to plan: enough to plan the same change again from the project as it is.
#[derive(Debug, Clone)]
pub(crate) enum Recipe {
    /// Move the pins a release offer names.
    Set(SetOffer),
    /// The guided fix: move the pins that name a missing commit to an installed one.
    Repin(SetOffer),
    Text(Box<TextOffer>),
    Gateway(GatewayRef),
}

struct Kept {
    id: PlanId,
    at: Scope,
    recipe: Recipe,
    plan: UpgradePlan,
    /// The books a plan cannot be applied to, named as the refusal names them: a plan with a
    /// blocked unit is shown and never confirmed.
    blocked: String,
}

/// The plans of this run (§4.8).
#[derive(Default)]
pub(crate) struct KeptPlans {
    next: std::sync::atomic::AtomicU64,
    plans: std::sync::Mutex<VecDeque<Kept>>,
}

impl KeptPlans {
    fn fresh(&self) -> PlanId {
        PlanId::new(self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1)
    }

    fn keep(&self, kept: Kept) {
        let mut plans = lock(&self.plans);
        plans.retain(|k| k.id != kept.id);
        plans.push_back(kept);
        while plans.len() > KEPT {
            plans.pop_front();
        }
    }

    /// The plan, still kept: a failed save leaves it for the retry.
    fn get(&self, id: PlanId, at: &Scope) -> Option<(Recipe, UpgradePlan, String)> {
        lock(&self.plans)
            .iter()
            .find(|k| k.id == id && k.at == *at)
            .map(|k| (k.recipe.clone(), k.plan.clone(), k.blocked.clone()))
    }

    fn drop_plan(&self, id: PlanId) {
        lock(&self.plans).retain(|k| k.id != id);
    }
}

/// What a test or a parity capture scripted for the resources service (C8): the offers and the
/// plans tC4's gallery draws, whose releases no server holds. Test hooks only.
#[cfg(feature = "test-hooks")]
#[derive(Default)]
pub(crate) struct ScriptedResources {
    updates: std::sync::Mutex<Option<UpdatesView>>,
    plans: std::sync::Mutex<VecDeque<PlanView>>,
}

#[cfg(feature = "test-hooks")]
impl ScriptedResources {
    pub(crate) fn script(&self, next: tc_api::test::Scripted) {
        match next {
            tc_api::test::Scripted::Updates(view) => *lock(&self.updates) = Some(view),
            tc_api::test::Scripted::Plan(view) => lock(&self.plans).push_back(*view),
            tc_api::test::Scripted::OpenDoor43Fails(_) | tc_api::test::Scripted::Sync { .. } => {}
        }
    }

    pub(crate) fn clear(&self) {
        *lock(&self.updates) = None;
        lock(&self.plans).clear();
    }
}

// ---------------------------------------------------------------------------------------
// The installed resources a plan reads
// ---------------------------------------------------------------------------------------

/// [`PlanSources`] over this computer's resource store. A resource's coverage and articles are
/// read once for the plan, however many units ask.
pub(crate) struct StoreSources<'a> {
    pub store: &'a ResourceStore,
    coverage: std::sync::Mutex<std::collections::HashMap<ResourceRef, Option<Coverage>>>,
}

impl<'a> StoreSources<'a> {
    pub(crate) fn new(store: &'a ResourceStore) -> Self {
        Self {
            store,
            coverage: std::sync::Mutex::default(),
        }
    }
}

fn pin_of(resource: &ResourceRef) -> Option<ResourcePin> {
    Some(ResourcePin::new(
        resource.repo_path.parse().ok()?,
        resource.sha,
    ))
}

impl PlanSources for StoreSources<'_> {
    fn coverage(&self, resource: &ResourceRef) -> Option<Coverage> {
        lock(&self.coverage)
            .entry(resource.clone())
            .or_insert_with(|| {
                let pin = pin_of(resource)?;
                tc_resources::resolve::LocalCoverage::local_books(self.store, &pin)
                    .map(|b| b.coverage())
            })
            .clone()
    }

    fn articles(&self, resource: &ResourceRef) -> Option<BTreeSet<WordArticleId>> {
        let installed = self.store.dcs(&pin_of(resource)?).ok()??;
        let text = installed.read_text("metadata.json").ok()?;
        Some(parse_burrito_metadata(&text).ok()?.word_articles())
    }

    fn list(
        &self,
        tool: ToolId,
        collection: Collection,
        source: &ListSource,
        spec: &UnitSpec<'_>,
    ) -> Result<tc_check::derive::CheckList, PlanError> {
        let name = plans::repo_name(&source.list);
        let unreadable = |what: String| PlanError::Unreadable(format!("{name}: {what}"));
        let installed = pin_of(&source.list)
            .and_then(|pin| self.store.dcs(&pin).transpose())
            .ok_or_else(|| unreadable("it is not installed".to_owned()))?
            .map_err(|e| unreadable(e.to_string()))?;
        let table = match collection {
            Collection::Book(book) => installed.book_table(book),
            Collection::Stories => installed.stories_table(),
        };
        // A resource that holds nothing for the book confirms it says nothing about it.
        let Some(table) = table else {
            return Ok(plans::derive_list(tool, collection, &[], &[], None, spec));
        };
        let text = installed
            .read_text(table)
            .map_err(|e| unreadable(e.to_string()))?;
        let scope = match collection {
            Collection::Book(book) => TsvScope::Book(book),
            Collection::Stories => TsvScope::Obs,
        };
        let (notes, links) = match tool {
            ToolId::TranslationNotes => (
                parse_notes(&text, scope)
                    .map_err(|e| unreadable(e.to_string()))?
                    .notes,
                Vec::new(),
            ),
            _ => (
                Vec::new(),
                parse_word_links(&text, scope).map_err(|e| unreadable(e.to_string()))?,
            ),
        };
        let articles = source.articles.as_ref().and_then(|r| self.articles(r));
        Ok(plans::derive_list(
            tool,
            collection,
            &notes,
            &links,
            articles.as_ref(),
            spec,
        ))
    }
}

// ---------------------------------------------------------------------------------------
// The next pins
// ---------------------------------------------------------------------------------------

fn rung_of(rung: SetRung) -> Rung {
    match rung {
        SetRung::Primary => Rung::Primary,
        SetRung::Fallback => Rung::Fallback,
    }
}

pub(crate) fn api_rung(rung: Rung) -> SetRung {
    match rung {
        Rung::Primary => SetRung::Primary,
        Rung::Fallback => SetRung::Fallback,
    }
}

fn stale() -> ApiError {
    refused_upgrade(t_stale())
}

/// tC4's `upgrade.stale`, which `upgrade.failed` wraps.
fn t_stale() -> String {
    "this offer is out of date — the project's pins changed. Check for updates again.".to_owned()
}

fn refused_upgrade(reason: String) -> ApiError {
    ApiError::Refused(Refusal::Resources(ResourceRefusal::UpgradeFailed {
        reason,
    }))
}

fn refused_gateway(reason: String) -> ApiError {
    ApiError::Refused(Refusal::Resources(ResourceRefusal::GatewayFailed {
        reason,
    }))
}

/// A label for a pin, as tC4 writes a release: its tag, else the first 12 digits of its commit.
fn label(pin: &ResourcePin) -> String {
    pin.version.as_ref().map_or_else(
        || pin.sha.to_hex().chars().take(12).collect(),
        ToString::to_string,
    )
}

fn repo_move(from: &ResourcePin, to: &ResourcePin, published: Option<String>) -> RepoMove {
    RepoMove {
        repo: from.repo.repo().name.to_string(),
        from: label(from),
        to: label(to),
        published,
    }
}

fn set_language(set: &LanguageSet) -> String {
    set.gateway
        .named()
        .map(|g| g.language.clone())
        .unwrap_or_default()
}

/// What a recipe changes the pins into, with what the dialog says of the change.
struct Next {
    pins: ProjectPins,
    what: PlanWhat,
}

fn local_pins(store: &ResourceStore) -> Result<Vec<ResourcePin>, ApiError> {
    store
        .installed_dcs_pins()
        .map_err(|e| refused_gateway(e.to_string()))
}

fn next_pins(
    recipe: &Recipe,
    current: &ProjectPins,
    store: &ResourceStore,
    kind: tc_resources::ProjectKind,
) -> Result<Next, ApiError> {
    match recipe {
        Recipe::Set(offer) | Recipe::Repin(offer) => {
            let Some(set) = current.set(offer.rung) else {
                return Err(stale());
            };
            if updates::offer_is_stale(offer, Some(set)) {
                return Err(stale());
            }
            // Coverage is recorded for the new pins only (D41): the other set's pins and the
            // groups beside it stay as the project holds them.
            let upgraded = updates::apply_upgrade(current, offer);
            let (filled, _) = backfill_coverage(&upgraded, store);
            let mut pins = current.clone();
            *pins.set_mut(offer.rung) = filled.set(offer.rung).cloned();
            let moves = offer
                .upgrades
                .iter()
                .map(|u| repo_move(&u.from, &u.to, u.published.clone()))
                .collect();
            let (rung, language) = (api_rung(offer.rung), set_language(set));
            let what = if matches!(recipe, Recipe::Repin(_)) {
                PlanWhat::Repin {
                    rung,
                    language,
                    moves,
                }
            } else {
                PlanWhat::Set {
                    rung,
                    language,
                    moves,
                }
            };
            Ok(Next { pins, what })
        }
        Recipe::Text(offer) => {
            if updates::text_offer_is_stale(offer, current) {
                return Err(stale());
            }
            let applied = updates::apply_text_upgrade(current, offer);
            let (filled, _) = backfill_coverage(&applied, store);
            // The new pin as the filled pins hold it, which records the installed copy's books.
            let to = updates::text_repos_of(&filled)
                .into_iter()
                .find(|r| {
                    r.slots
                        .iter()
                        .any(|(s, _)| offer.slots.iter().any(|(o, _)| o == s))
                })
                .and_then(|r| r.slots.into_iter().next())
                .map_or_else(|| offer.to.clone(), |(_, pin)| pin);
            let filled_offer = TextOffer {
                to: to.clone(),
                ..TextOffer::clone(offer)
            };
            let pins = updates::apply_text_upgrade(current, &filled_offer);
            let what = PlanWhat::Text {
                kind: match offer.kind {
                    ResourceTextKind::Original => TextKind::Original,
                    ResourceTextKind::Gateway => TextKind::Gateway,
                },
                change: repo_move(&offer.from, &offer.to, offer.published.clone()),
            };
            Ok(Next { pins, what })
        }
        Recipe::Gateway(gateway) => {
            let package_found =
                gateways::find(&gateway.language, &gateway.owner).ok_or_else(|| {
                    ApiError::BadRequest {
                        field: "gateway".to_owned(),
                        reason: format!("{} is not a gateway language package", gateway.key()),
                    }
                })?;
            let installed = local_pins(store)?;
            let Some(primary) =
                gateways::language_set_from_installed(&installed, package_found, kind)
            else {
                return Err(refused_gateway(format!(
                    "{} does not have a whole suite installed here",
                    package_found.autonym
                )));
            };
            // A project with an empty pin file still gets both language sets (tC4 #485).
            let complete = complete_language_sets(current);
            let panes = (kind == tc_resources::ProjectKind::Bible).then(|| {
                source_panes_for_gateway(
                    &complete.extra_scripture,
                    &gateways::gateway_bibles_from_installed(&installed, package_found),
                )
            });
            let before = complete.set(Rung::Primary).cloned();
            let (pins, _) = backfill_coverage(
                &apply_gateway_change(&complete, primary.clone(), panes),
                store,
            );
            let images = (kind == tc_resources::ProjectKind::Obs).then(|| ImageChange {
                from: before
                    .as_ref()
                    .and_then(|s| s.pin(tc_resources::SetSlot::ObsImages))
                    .map(|p| p.repo.repo().name.to_string()),
                to: primary
                    .pin(tc_resources::SetSlot::ObsImages)
                    .map(|p| p.repo.repo().name.to_string()),
            });
            let from = before
                .as_ref()
                .and_then(gateways::of_set)
                .map(|g: &Gateway| package(g));
            let what = PlanWhat::Gateway(Box::new(GatewayChange {
                from,
                to: package(package_found),
                images,
            }));
            Ok(Next { pins, what })
        }
    }
}

/// The verses whose alignments an original-language text upgrade marks for review: every aligned
/// verse of the project's books in the testaments it covers. Every alignment is kept.
fn aligned_verses(read: &CountingSnapshot, recipe: &Recipe) -> Option<u32> {
    let Recipe::Text(offer) = recipe else {
        return None;
    };
    let testaments: Vec<Testament> = updates::invalidated_testaments(offer);
    if offer.kind != ResourceTextKind::Original {
        return None;
    }
    let verses: usize = read
        .projected_books()
        .into_iter()
        .filter(|book| testaments.contains(&book.testament()))
        .map(|book| read.alignments_of(book).len())
        .sum();
    Some(u32::try_from(verses).unwrap_or(u32::MAX))
}

// ---------------------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------------------

/// tC4's `gateway.blockedError` names the books by code.
fn blocked_books(blocked: &[tc_api::resources::BlockedUnit]) -> String {
    let mut books: Vec<String> = blocked
        .iter()
        .map(|b| match b.unit {
            tc_api::check::CheckCollection::Book(book) => book.code().to_owned(),
            tc_api::check::CheckCollection::Stories => "OBS".to_owned(),
        })
        .collect();
    books.dedup();
    books.join(", ")
}

impl App {
    /// The optional pins a download brought, written into the project's sets that name its
    /// package (tC4 `adoptDownloadedPins`): one batch, over the heads read, again from the
    /// project as it is when a teammate's change landed meanwhile, as tC4's retries. A pin a
    /// set holds never moves. `Err` says why the pins were not written.
    pub(crate) async fn adopt_downloaded_pins(
        &self,
        session: &Arc<ProjectSession>,
        gateway: &'static Gateway,
    ) -> Result<(), String> {
        for _ in 0..4 {
            let read = session.read();
            let Some(current) = crate::resources::project_pins(&read).map_err(|e| e.to_string())?
            else {
                return Ok(());
            };
            let installed = self.installed_pins().await?;
            let Some(merged) = merge_optional_pins(&current, gateway, &installed) else {
                return Ok(());
            };
            let store = self.inner.resources.clone();
            let changes = tokio::task::spawn_blocking(move || {
                let (filled, _) = backfill_coverage(&merged, &store);
                pin_changes(&read, &current, &filled).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| e.to_string())??;
            let plan = UpgradePlan { changes };
            match session
                .handle()
                .submit(StoreCommand::apply_upgrade(plan))
                .await
            {
                Ok(_) => return Ok(()),
                Err(CommandError::PlanStale) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("the project kept changing".to_owned())
    }

    /// The next scripted plan, when a test or a capture scripted one.
    #[cfg(feature = "test-hooks")]
    pub(crate) fn scripted_plan(&self) -> Option<PlanView> {
        lock(&self.inner.scripted_resources.plans).pop_front()
    }

    pub(crate) fn project_stories(&self, session: &ProjectSession) -> bool {
        session.handle().facts().kind == tc_journal::fold::ProjectKind::Stories
    }

    /// Plans `recipe` from the project as it is now, keeps the plan and answers what the
    /// dialog shows. Reads and derives on the blocking pool: a plan costs the books it affects.
    pub(crate) async fn plan_recipe(
        &self,
        session: &Arc<ProjectSession>,
        recipe: Recipe,
        job: Option<&JobCtx>,
    ) -> Result<Option<PlanView>, ApiError> {
        let read = session.read();
        let current = crate::resources::project_pins(&read)?.unwrap_or_default();
        let stories = self.project_stories(session);
        let kind = sets_kind(session.handle().facts().kind);
        let store = self.inner.resources.clone();
        let numbering = Numbering::of(crate::understand::versification(&read).as_ref());
        let cancel = job.map(|ctx| ctx.cancel.clone());
        let progress = job.map(|ctx| ctx.progress.clone());
        let work = {
            let recipe = recipe.clone();
            tokio::task::spawn_blocking(move || -> Result<Option<Planned>, ApiError> {
                let next = next_pins(&recipe, &current, &store, kind)?;
                let current = if matches!(recipe, Recipe::Gateway(_)) {
                    complete_language_sets(&current)
                } else {
                    current
                };
                let project = ProjectUnits {
                    collections: plans::collections(&read, stories),
                    numbering,
                };
                let sources = StoreSources::new(&store);
                let cancelled = || cancel.as_ref().is_some_and(|c| c.is_cancelled());
                let on_unit = |collection: Collection| {
                    if let Some(progress) = &progress {
                        let book = match collection {
                            Collection::Book(book) => Some(book),
                            Collection::Stories => None,
                        };
                        progress.phase(Phase::Plan { book }, None);
                    }
                };
                let decisions = plans::plan_decisions(
                    &read, &sources, &current, &next.pins, &project, &cancelled, &on_unit,
                )
                .map_err(|PlanError::Unreadable(reason)| refused_gateway(reason))?;
                if cancelled() {
                    return Ok(None);
                }
                let mut changes = pin_changes(&read, &current, &next.pins)?;
                changes.extend(decisions.changes);
                Ok(Some(Planned {
                    what: next.what,
                    alignments: aligned_verses(&read, &recipe),
                    changes,
                    rows: decisions.rows,
                    blocked: decisions.blocked,
                    at_risk: decisions.at_risk,
                }))
            })
        }
        .await
        .map_err(|stopped| {
            self.internal(
                "a plan could not be made",
                crate::reports::facts(
                    serde_json::json!({"stopped": crate::jobs::stop_reason(stopped)}),
                ),
            )
        })??;
        let Some(work) = work else {
            return Ok(None);
        };
        let id = self.inner.plans.fresh();
        let view = PlanView {
            plan: id,
            what: work.what,
            decisions_at_risk: u32::try_from(work.at_risk).unwrap_or(u32::MAX),
            rows: work.rows,
            blocked: work.blocked.clone(),
            alignments: work.alignments,
        };
        self.inner.plans.keep(Kept {
            id,
            at: session.scope().clone(),
            recipe,
            plan: UpgradePlan {
                changes: work.changes,
            },
            blocked: blocked_books(&work.blocked),
        });
        Ok(Some(view))
    }
}

struct Planned {
    what: PlanWhat,
    alignments: Option<u32>,
    changes: Vec<PlannedChange>,
    rows: Vec<tc_api::resources::PlanRow>,
    blocked: Vec<tc_api::resources::BlockedUnit>,
    at_risk: usize,
}

/// One change for each pin group the change moves, over the heads the group had in the snapshot;
/// a group in conflict is refused.
fn pin_changes(
    read: &CountingSnapshot,
    current: &ProjectPins,
    next: &ProjectPins,
) -> Result<Vec<PlannedChange>, ApiError> {
    let unwritable = |e: tc_resources::GroupError| refused_gateway(e.to_string());
    let mut out = Vec::new();
    for group in PinGroup::ALL {
        let value: PinGroupValue = next.group_value(group).map_err(unwritable)?;
        if current.group_value(group).map_err(unwritable)? == value {
            continue;
        }
        let key = RecordKey::Pins(group);
        // A change over both heads would end the conflict and hide the other computer's pins.
        if read.in_conflict(&key) {
            return Err(refused_gateway(
                "the project's resources have a conflict to resolve first".to_owned(),
            ));
        }
        out.push(PlannedChange {
            parents: read.heads(&key),
            record: Record::Pins { group, value },
            cause: Cause::Edit,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------
// resources/checkUpdates
// ---------------------------------------------------------------------------------------

fn net_error(error: &Door43Error) -> ApiError {
    match error {
        Door43Error::Offline { .. } => ApiError::NeedsInternet(NetKind::Sources),
        other => ApiError::Refused(Refusal::Resources(ResourceRefusal::UpdateCheckFailed {
            reason: other.to_string(),
        })),
    }
}

impl Handle<CheckUpdates> for App {
    async fn handle(&self, req: CheckUpdates) -> Result<UpdatesView, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
        #[cfg(feature = "test-hooks")]
        if let Some(view) = lock(&self.inner.scripted_resources.updates).take() {
            return Ok(view);
        }
        let client = self.resources_client()?;
        let Some(pins) = crate::resources::project_pins(&read)? else {
            return Ok(UpdatesView {
                sets: Vec::new(),
                texts: Vec::new(),
                texts_error: None,
            });
        };
        let mut set_repos: Vec<RepoRef> = Vec::new();
        for rung in Rung::LADDER {
            if let Some(set) = pins.set(rung) {
                set_repos.extend(
                    set.help_repos()
                        .into_iter()
                        .map(|(p, _)| p.repo.repo().clone()),
                );
            }
        }
        let latest = updates::latest_releases(&client, &set_repos)
            .await
            .map_err(|e| net_error(&e))?;
        let sets = Rung::LADDER
            .into_iter()
            .filter_map(|rung| {
                let set = pins.set(rung)?;
                let offer = updates::offer_for_set(rung, set, &latest);
                Some(SetOfferView {
                    rung: api_rung(rung),
                    language: set_language(set),
                    moves: offer
                        .upgrades
                        .iter()
                        .map(|u| repo_move(&u.from, &u.to, u.published.clone()))
                        .collect(),
                })
            })
            .collect();
        // A text Door43 cannot answer for is said, and does not hide the set offers (#258).
        let testaments: Vec<Testament> = read
            .projected_books()
            .into_iter()
            .map(BookId::testament)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let text_repos: Vec<RepoRef> = updates::text_repos_of(&pins)
            .iter()
            .filter_map(|r| r.slots.first().map(|(_, p)| p.repo.repo().clone()))
            .collect();
        let (texts, texts_error) = match updates::latest_releases(&client, &text_repos).await {
            Ok(latest) => (updates::text_offers(&pins, &testaments, &latest).0, None),
            Err(error) => (Vec::new(), Some(error.to_string())),
        };
        Ok(UpdatesView {
            sets,
            texts: texts
                .iter()
                .map(|o| TextOfferView {
                    kind: match o.kind {
                        ResourceTextKind::Original => TextKind::Original,
                        ResourceTextKind::Gateway => TextKind::Gateway,
                    },
                    change: repo_move(&o.from, &o.to, o.published.clone()),
                })
                .collect(),
            texts_error,
        })
    }
}

// ---------------------------------------------------------------------------------------
// resources/planUpgrade, resources/planGateway
// ---------------------------------------------------------------------------------------

/// The offer a request names, found again against Door43's newest releases.
async fn find_offer(
    client: &Door43Client,
    pins: &ProjectPins,
    testaments: &[Testament],
    offer: &OfferRef,
) -> Result<Recipe, ApiError> {
    let repos: Vec<RepoRef> = match offer {
        OfferRef::Set { rung } => pins
            .set(rung_of(*rung))
            .map(|set| {
                set.help_repos()
                    .into_iter()
                    .map(|(p, _)| p.repo.repo().clone())
                    .collect()
            })
            .unwrap_or_default(),
        OfferRef::Text { .. } => updates::text_repos_of(pins)
            .iter()
            .filter_map(|r| r.slots.first().map(|(_, p)| p.repo.repo().clone()))
            .collect(),
    };
    let latest: LatestReleases = updates::latest_releases(client, &repos)
        .await
        .map_err(|e| net_error(&e))?;
    match offer {
        OfferRef::Set { rung } => {
            let rung = rung_of(*rung);
            let set = pins.set(rung).ok_or_else(stale)?;
            let found = updates::offer_for_set(rung, set, &latest);
            if found.is_empty() {
                return Err(stale());
            }
            Ok(Recipe::Set(found))
        }
        OfferRef::Text { repo } => updates::text_offers(pins, testaments, &latest)
            .0
            .into_iter()
            .find(|o| o.to.repo.repo().name.to_string().eq_ignore_ascii_case(repo))
            .map(|o| Recipe::Text(Box::new(o)))
            .ok_or_else(stale),
    }
}

fn pins_to_install(recipe: &Recipe) -> Vec<ResourcePin> {
    match recipe {
        Recipe::Set(offer) | Recipe::Repin(offer) => {
            offer.upgrades.iter().map(|u| u.to.clone()).collect()
        }
        Recipe::Text(offer) => vec![offer.to.clone()],
        Recipe::Gateway(_) => Vec::new(),
    }
}

impl Handle<PlanUpgrade> for App {
    async fn handle(&self, req: PlanUpgrade) -> Result<JobId, ApiError> {
        let session = self.read(&req.at)?;
        #[cfg(feature = "test-hooks")]
        if let Some(view) = self.scripted_plan() {
            let spec = JobSpec::new(JobKind::UpgradePlan, Phase::Plan { book: None })
                .project(session.scope().project.clone());
            return Ok(self.inner.jobs.start_typed::<PlanUpgrade, _, _>(
                spec,
                move |_ctx| async move { Ok(Box::new(view)) },
            ));
        }
        let client = self.resources_client()?;
        let spec = JobSpec::new(
            JobKind::UpgradePlan,
            Phase::Install {
                repo: String::new(),
            },
        )
        .project(session.scope().project.clone());
        let app = self.clone();
        Ok(self
            .inner
            .jobs
            .start_typed::<PlanUpgrade, _, _>(spec, move |ctx| async move {
                let read = session.read();
                let pins = crate::resources::project_pins(&read)?.ok_or_else(stale)?;
                let testaments: Vec<Testament> = read
                    .projected_books()
                    .into_iter()
                    .map(BookId::testament)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                let recipe = find_offer(&client, &pins, &testaments, &req.offer).await?;
                // Every resource of the release is installed and verified before the plan is made;
                // a failure or a cancel leaves the pins as they are.
                let store = &app.inner.resources;
                for pin in pins_to_install(&recipe) {
                    ctx.cancel.check()?;
                    let phase = Phase::Install {
                        repo: pin.repo.repo().name.to_string(),
                    };
                    ctx.progress.phase(phase.clone(), None);
                    let watch = JobWatch {
                        progress: ctx.progress.clone(),
                        cancel: ctx.cancel.clone(),
                        phase,
                    };
                    match install_dcs_watched(
                        store,
                        &client,
                        &pin,
                        Packaging::for_pin(&pin),
                        &watch,
                    )
                    .await
                    {
                        Ok(_) => {}
                        Err(InstallError::Door43(Door43Error::Cancelled { .. })) => {
                            return Err(JobEnd::Cancelled);
                        }
                        Err(InstallError::Door43(Door43Error::Offline { .. })) => {
                            return Err(JobEnd::Failed(ApiError::NeedsInternet(NetKind::Sources)));
                        }
                        Err(e) => return Err(JobEnd::Failed(refused_upgrade(e.to_string()))),
                    }
                }
                ctx.progress.phase(Phase::Plan { book: None }, None);
                let view = app.plan_recipe(&session, recipe, Some(&ctx)).await;
                // What was missing may be here now (tC4 bumps `installEpoch`).
                session.books().forget().await;
                let view = view?.ok_or(JobEnd::Cancelled)?;
                Ok(Box::new(view))
            }))
    }
}

impl Handle<PlanGateway> for App {
    async fn handle(&self, req: PlanGateway) -> Result<JobId, ApiError> {
        let session = self.read(&req.at)?;
        let spec = JobSpec::new(JobKind::GatewayPlan, Phase::Plan { book: None })
            .project(session.scope().project.clone());
        #[cfg(feature = "test-hooks")]
        if let Some(view) = self.scripted_plan() {
            return Ok(self.inner.jobs.start_typed::<PlanGateway, _, _>(
                spec,
                move |_ctx| async move { Ok(Box::new(view)) },
            ));
        }
        let app = self.clone();
        Ok(self
            .inner
            .jobs
            .start_typed::<PlanGateway, _, _>(spec, move |ctx| async move {
                let view = app
                    .plan_recipe(&session, Recipe::Gateway(req.gateway), Some(&ctx))
                    .await?
                    .ok_or(JobEnd::Cancelled)?;
                Ok(Box::new(view))
            }))
    }
}

// ---------------------------------------------------------------------------------------
// resources/applyPlan
// ---------------------------------------------------------------------------------------

impl Handle<ApplyPlan> for App {
    async fn handle(&self, req: ApplyPlan) -> Result<PlanApplied, ApiError> {
        let session = self.read(&req.at)?;
        // A plan the server no longer holds (it restarted, or the project opened again) is a
        // plan over a project that may have moved: the dialog says so and the person asks again.
        let Some((recipe, plan, blocked)) = self.inner.plans.get(req.plan, &req.at) else {
            return Err(ApiError::NotSaved(NotSaved::PlanStale));
        };
        if !blocked.is_empty() {
            return Err(ApiError::Refused(Refusal::Resources(
                ResourceRefusal::GatewayBlocked { books: blocked },
            )));
        }
        let command = StoreCommand::apply_upgrade(plan).requested_by(req.request);
        match session.handle().submit(command).await {
            Ok(_) => {
                self.inner.plans.drop_plan(req.plan);
                session.books().forget().await;
                Ok(PlanApplied::Applied)
            }
            // Heads moved since the plan was read: make it again, once, and show it.
            Err(CommandError::PlanStale) => {
                self.inner.plans.drop_plan(req.plan);
                let view = self
                    .plan_recipe(&session, recipe, None)
                    .await?
                    .ok_or_else(stale)?;
                Ok(PlanApplied::Replanned(Box::new(view)))
            }
            Err(error) => Err(match wire::error::command_error(error) {
                Ok(api) => api,
                Err(unworded) => self.internal(
                    "the store refused to apply a plan",
                    crate::reports::facts(serde_json::json!({"reason": unworded.0})),
                ),
            }),
        }
    }
}
