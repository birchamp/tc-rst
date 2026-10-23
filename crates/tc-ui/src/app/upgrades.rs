//! Release upgrades and the gateway change (unit C8; tC4 `ResourceUpdates.jsx`,
//! `UpgradeSet.jsx`, `GatewayChange.jsx`): tC4's `upgrade` slice and its `gatewayPreview`,
//! `gatewayBusy` and `gatewayError`. Pure; the requests are the server's, and what the dialogs
//! say is decided here so a host test can hold it.
//!
//! The server plans and keeps the plan; the person reads it; confirming sends its id. A
//! confirm that finds the project moved comes back with the plan made again, which replaces the
//! one shown, so the person reads the new figures before anything is written. A failed save
//! leaves the plan on screen with the reason, and Confirm tries again (tC4 #580).

use tc_api::check::{CheckCollection, Tool};
use tc_api::ids::ProjectRef;
use tc_api::resources::{
    BlockReason, BlockedUnit, GatewayPackage, PlanRow, PlanView, PlanWhat, RepoMove, SetRung,
    TextKind, UpdatesView,
};

/// What the download of an upgrade is for (tC4 `upgrade.installing`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installing {
    Set(SetRung),
    /// A scripture text, by its repository's name.
    Text(String),
}

/// What Check for updates found, for the project it was asked for (tC4 `offersFor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offers {
    pub project: ProjectRef,
    pub view: UpdatesView,
}

/// The confirmation the person is reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub plan: PlanView,
    /// Confirm is on its way: both buttons wait.
    pub applying: bool,
}

/// tC4's `s.upgrade`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpgradeState {
    pub checking: bool,
    pub installing: Option<Installing>,
    /// The repository being downloaded (`sources.progress`).
    pub progress: Option<String>,
    /// A failed check, install or confirm, in tC4's words; one place, as tC4's is.
    pub error: Option<String>,
    pub offers: Option<Offers>,
    pub preview: Option<Preview>,
}

impl UpgradeState {
    /// "Check for updates" was pressed for `project`.
    pub fn check_started(&mut self) {
        self.checking = true;
        self.error = None;
        self.offers = None;
    }

    /// Door43 answered. An answer for a project the person has left is dropped (tC4 binds the
    /// answer to the project it was made for).
    pub fn offers_read(&mut self, project: &ProjectRef, current: &ProjectRef, view: UpdatesView) {
        if project != current {
            return;
        }
        self.checking = false;
        self.error = view.texts_error.map(check_failed);
        self.offers = Some(Offers {
            project: project.clone(),
            view,
        });
    }

    /// The check failed; `reason` is already tC4's sentence.
    pub fn check_failed(&mut self, reason: String) {
        self.checking = false;
        self.error = Some(reason);
    }

    /// An offer's Upgrade was pressed: nothing else can start while it downloads.
    pub fn install_started(&mut self, what: Installing) {
        self.installing = Some(what);
        self.error = None;
        self.progress = None;
    }

    pub fn install_progress(&mut self, repo: String) {
        if self.installing.is_some() {
            self.progress = Some(repo);
        }
    }

    /// The release is installed and its plan is ready: the confirmation opens.
    pub fn plan_ready(&mut self, plan: PlanView) {
        self.installing = None;
        self.progress = None;
        self.preview = Some(Preview {
            plan,
            applying: false,
        });
    }

    /// The download or the plan failed or was cancelled; the pins did not move.
    pub fn install_failed(&mut self, reason: Option<String>) {
        self.installing = None;
        self.progress = None;
        if reason.is_some() {
            self.error = reason;
        }
    }

    /// "Keep the current release" or the ✕: nothing changes.
    pub fn cancel(&mut self) {
        if self.preview.as_ref().is_some_and(|p| p.applying) {
            return;
        }
        self.preview = None;
        self.error = None;
    }

    /// Confirm was pressed. `false` when it must not go: the plan is blocked, or one is on its way.
    pub fn confirm_started(&mut self) -> bool {
        let Some(preview) = &mut self.preview else {
            return false;
        };
        if preview.applying || !preview.plan.blocked.is_empty() {
            return false;
        }
        preview.applying = true;
        self.error = None;
        true
    }

    /// The plan was written: the offer it answered is spent.
    pub fn applied(&mut self) {
        if let Some(preview) = self.preview.take()
            && let (Some(offers), Some(spent)) = (&mut self.offers, spent_by(&preview.plan))
        {
            match spent {
                Spent::Set(rung) => {
                    if let Some(set) = offers.view.sets.iter_mut().find(|s| s.rung == rung) {
                        set.moves.clear();
                    }
                }
                Spent::Text(repo) => offers.view.texts.retain(|t| t.change.repo != repo),
            }
        }
        self.error = None;
    }

    /// The project moved under the plan: the plan made again is on screen to read.
    pub fn replanned(&mut self, plan: PlanView) {
        self.error = None;
        self.preview = Some(Preview {
            plan,
            applying: false,
        });
    }

    /// The save failed: the plan stays, with the reason, for another try.
    pub fn apply_failed(&mut self, reason: String) {
        if let Some(preview) = &mut self.preview {
            preview.applying = false;
        }
        self.error = Some(reason);
    }
}

enum Spent {
    Set(SetRung),
    Text(String),
}

fn spent_by(plan: &PlanView) -> Option<Spent> {
    match &plan.what {
        PlanWhat::Set { rung, .. } => Some(Spent::Set(*rung)),
        PlanWhat::Text { change, .. } => Some(Spent::Text(change.repo.clone())),
        PlanWhat::Repin { .. } | PlanWhat::Gateway(_) => None,
    }
}

/// One plain sentence per cause (tC4 #833.1), in place of tC4's `upgrade.checkFailed` with the
/// client's own text as its reason.
fn check_failed(why: tc_api::refusal::UpdateCheckFailure) -> String {
    crate::i18n::t_dyn(why.key(), &[], None)
}

/// What the gateway dialog shows while a plan is made or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayPreview {
    /// The plan could not be made: the error, and Close, never a Change that could mistake a
    /// failed read for success.
    Failed { package: Box<GatewayPackage> },
    Plan {
        package: Box<GatewayPackage>,
        plan: Box<PlanView>,
    },
}

/// tC4's `gatewayBusy`, `gatewayPreview` and `gatewayError`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GatewayState {
    /// A plan is being made or a change is being written: no second choice starts, and
    /// neither button answers (tC4 #580).
    pub busy: bool,
    pub preview: Option<GatewayPreview>,
    pub error: Option<String>,
}

impl GatewayState {
    /// A package was chosen. `false` when a change is already previewed or under way: a second
    /// choice does nothing (tC4 #580, interruption 3).
    pub fn asking(&mut self) -> bool {
        if self.busy || self.preview.is_some() {
            return false;
        }
        self.busy = true;
        self.error = None;
        true
    }

    pub fn planned(&mut self, package: GatewayPackage, plan: PlanView) {
        self.busy = false;
        self.preview = Some(GatewayPreview::Plan {
            package: Box::new(package),
            plan: Box::new(plan),
        });
    }

    /// The plan could not be made, or the work before it could not finish.
    pub fn plan_failed(&mut self, package: GatewayPackage, reason: String) {
        self.busy = false;
        self.preview = Some(GatewayPreview::Failed {
            package: Box::new(package),
        });
        self.error = Some(reason);
    }

    /// "Keep …" or the ✕. After Change nothing cancels: the buttons are off until it ends.
    pub fn cancel(&mut self) {
        if self.busy {
            return;
        }
        self.preview = None;
        self.error = None;
    }

    /// Change was pressed. `false` when it must not go: the dialog holds no plan, a unit is
    /// blocked, or a change is already on its way.
    pub fn confirm_started(&mut self) -> bool {
        if self.busy {
            return false;
        }
        let Some(GatewayPreview::Plan { plan, .. }) = &self.preview else {
            return false;
        };
        if !plan.blocked.is_empty() {
            return false;
        }
        self.busy = true;
        self.error = None;
        true
    }

    pub fn applied(&mut self) {
        self.busy = false;
        self.preview = None;
        self.error = None;
    }

    /// The project moved under the plan: the plan made again replaces the one shown.
    pub fn replanned(&mut self, plan: PlanView) {
        self.busy = false;
        self.error = None;
        if let Some(GatewayPreview::Plan { plan: shown, .. }) = &mut self.preview {
            **shown = plan;
        }
    }

    /// The save failed: the dialog stays on its plan, states why, and Change tries again.
    pub fn apply_failed(&mut self, reason: String) {
        self.busy = false;
        self.error = Some(reason);
    }
}

// ---- what the dialogs say ----------------------------------------------------------------

/// A (tool, book) in the sentences of the dialogs (`check.tool.*`).
pub fn tool_key(tool: Tool) -> crate::i18n::Key {
    match tool {
        Tool::TranslationNotes => crate::i18n::keys::check::tool::TRANSLATION_NOTES,
        Tool::TranslationWords => crate::i18n::keys::check::tool::TRANSLATION_WORDS,
    }
}

/// The unit's name: a book's English name, or the stories'.
pub fn unit_name(unit: CheckCollection) -> String {
    match unit {
        CheckCollection::Book(book) => book.english_name().to_owned(),
        CheckCollection::Stories => "Open Bible Stories".to_owned(),
    }
}

/// One row of the per-book list: tC4's `gateway.carryOver` when the carry-over was counted,
/// else `gateway.affectedRow`.
pub fn row_text(row: &PlanRow) -> String {
    use crate::i18n::{keys::gateway, t};
    let (book, tool) = (unit_name(row.unit), t(tool_key(row.tool), &[]));
    match row.carry {
        Some(carry) => t(
            gateway::CARRY_OVER,
            &[
                ("book", &book),
                ("tool", &tool),
                ("carried", &carry.carried),
                ("invalidated", &carry.check_again),
            ],
        ),
        None => t(
            gateway::AFFECTED_ROW,
            &[
                ("book", &book),
                ("tool", &tool),
                ("n", &row.decisions),
                ("resource", &row.against),
            ],
        ),
    }
}

/// The callout of a plan that cannot be confirmed (tC4 `gateway.blocked*`): the remedy depends on
/// why, so the first unit with a reason of its own decides the sentence.
pub fn blocked_text(blocked: &[BlockedUnit]) -> String {
    use crate::i18n::{keys::gateway, t};
    let books = blocked
        .iter()
        .map(|b| format!("{} ({})", unit_name(b.unit), t(tool_key(b.tool), &[])))
        .collect::<Vec<_>>()
        .join(", ");
    let key = match blocked
        .iter()
        .map(|b| b.reason)
        .find(|r| *r != BlockReason::NoResource)
    {
        Some(BlockReason::VersificationUnknown) => gateway::BLOCKED_VERSIFICATION_UNKNOWN,
        Some(BlockReason::NoResource) | None => gateway::BLOCKED,
    };
    t(key, &[("books", &books)])
}

/// The headline and detail of the gateway change (tC4 `describeConsequences`). tC4 writes them
/// in English whatever the language of the app, which is kept: a translated sentence needs
/// the catalogs' words first.
pub fn gateway_headline(plan: &PlanView) -> (String, Option<String>) {
    if plan.harmless() || plan.decisions_at_risk == 0 {
        return (
            "Nothing you have already checked will be affected.".to_owned(),
            None,
        );
    }
    let mut books: Vec<String> = Vec::new();
    for row in &plan.rows {
        let name = unit_name(row.unit);
        if !books.contains(&name) {
            books.push(name);
        }
    }
    let named: Vec<&String> = books.iter().take(3).collect();
    let more = books.len() - named.len();
    let list = if more > 0 {
        format!(
            "{} and {more} more",
            named
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        named
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" and ")
    };
    let n = plan.decisions_at_risk;
    (
        format!(
            "{n} decision{} in {list} were made against the notes you are leaving.",
            if n == 1 { "" } else { "s" }
        ),
        Some(
            "Decisions that match a check in the new notes carry over. The rest stop applying, so \
             those checks come back as work — a book that was finished will not be finished any \
             more. Nothing is deleted."
                .to_owned(),
        ),
    )
}

/// The moves a confirmation lists, as `upgrade.move` writes them.
pub fn move_text(change: &RepoMove) -> String {
    crate::i18n::t(
        crate::i18n::keys::upgrade::MOVE,
        &[
            ("repo", &change.repo),
            ("from", &change.from),
            ("to", &change.to),
        ],
    )
}

/// What a confirmation of a text upgrade says first: no alignment changes for a gateway Bible,
/// or the verses whose alignments an original-language text marks (tC4 `TextUpgrade`).
pub fn text_headline(kind: TextKind, alignments: Option<u32>) -> (String, bool) {
    use crate::i18n::{keys::upgrade, t};
    match (kind, alignments) {
        (TextKind::Original, Some(n)) if n > 0 => {
            (t(upgrade::TEXT_INVALIDATES, &[("n", &n)]), true)
        }
        (TextKind::Original, _) => (t(upgrade::TEXT_NO_ALIGNMENTS, &[]), false),
        (TextKind::Gateway, _) => (t(upgrade::TEXT_GATEWAY, &[]), false),
    }
}
