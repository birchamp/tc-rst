//! The share dialog (unit B9b; tC4's `sh`, `tC4:src/views/modals/ShareDialog.jsx`, #362, D84):
//! where the repository goes, then what will be shared. Unit E1b adds the run: its progress
//! line, its refusal on the check step, and the end with the team-access line (#724, Q2).
//! Pure: the page's actions run it.

use tc_api::account::{OrgChoice, ShareTargets};
use tc_api::door43::{RepoName, ShareOwner, SharedRepo};
use tc_api::home::{HomeCard, ProjectKind};
use tc_api::ids::{ProjectRef, RequestId};
use tc_api::jobs::SharePhase;
use tc_api::shown::Known;
use tc_core::BookId;

use crate::app::submit::Submit;
use crate::i18n::keys;

/// What is shared: the whole project, always (D84 point 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedItems {
    /// A Bible's books, in canonical order.
    Books(Vec<BookId>),
    /// "Stories 1 to 50".
    Stories,
}

/// The project a share is for, as its Home card shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareProject {
    pub project: ProjectRef,
    /// The card's name, which the dialog's subtitles carry.
    pub name: String,
    /// The language tag, for "Recommended" (R12); empty when the card does not know it.
    pub language: String,
    pub items: SharedItems,
}

impl ShareProject {
    pub fn of(card: &HomeCard) -> Self {
        let items = match card.kind {
            ProjectKind::Stories => SharedItems::Stories,
            ProjectKind::Bible => SharedItems::Books(match &card.books {
                Known::Known(books) => books.iter().map(|b| b.book).collect(),
                Known::Unknown => Vec::new(),
            }),
        };
        Self {
            project: card.project.clone(),
            name: card.name.clone(),
            language: match &card.tag {
                Known::Known(tag) => tag.clone(),
                Known::Unknown => String::new(),
            },
            items,
        }
    }
}

/// The dialog's steps (`sh.step`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareStep {
    /// Where it goes.
    Target,
    /// Check what will be shared.
    Check,
    /// The run, on its progress line: no footer and no close.
    Running(SharePhase),
    /// Shared: the repository's page and what teammates need.
    Done(SharedRepo),
}

/// Why the last run did not share, on the check step (`sh.error`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareProblem {
    /// The refusal's code (`share.name-exists`), which picks the sentence; `None` for a
    /// failure with only its own words.
    pub code: Option<String>,
    pub text: String,
}

/// The organizations (`sh.choices`, `sh.choicesError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choices {
    /// "Reading your organizations…": Next waits.
    Loading,
    Ready(Vec<OrgChoice>),
    /// "Your organizations could not be read:" and why. The person's own account can still be
    /// chosen.
    Failed(String),
}

/// Where the repository is created (`sh.target`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    User,
    Organization(String),
}

/// tC4's `sh`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareForm {
    pub project: ShareProject,
    pub step: ShareStep,
    pub choices: Choices,
    pub target: Target,
    /// The repository name; the project's folder name until the person types another.
    pub name: String,
    /// Share's attempt: one request per attempt (UI-3).
    pub submit: Submit,
    /// The progress lines shown so far, in order (tC4's `sh.steps`, `data-steps`).
    pub shown: Vec<SharePhase>,
    pub error: Option<ShareProblem>,
    /// Copy link put the address on the clipboard: its label says Copied.
    pub copied: bool,
}

/// Whether Door43 accepts `name` as a repository name (tC4's `nameOk`): letters, digits, dots,
/// dashes and underscores, starting and ending with a letter or a digit.
pub fn name_ok(name: &str) -> bool {
    RepoName::parse(name).is_ok()
}

impl ShareForm {
    /// A first share, on its where-it-goes step, reading the organizations (`startShare`).
    pub fn open(project: ShareProject, request: RequestId) -> Self {
        let name = project.project.as_str().to_owned();
        Self {
            project,
            step: ShareStep::Target,
            choices: Choices::Loading,
            target: Target::User,
            name,
            submit: Submit::new(request),
            shown: Vec::new(),
            error: None,
            copied: false,
        }
    }

    /// The organizations arrived, or could not be read.
    pub fn choices_read(&mut self, read: Result<ShareTargets, String>) {
        self.choices = match read {
            Ok(targets) => Choices::Ready(targets.orgs),
            Err(why) => Choices::Failed(why),
        };
    }

    /// A choice: the person's own account, or an organization they can create in. Another
    /// organization is shown and cannot be chosen (D84 point 3).
    pub fn pick(&mut self, target: Target, next: RequestId) {
        let allowed = match &target {
            Target::User => true,
            Target::Organization(name) => matches!(&self.choices,
                Choices::Ready(orgs) if orgs.iter().any(|o| &o.name == name && o.can_create)),
        };
        if allowed && self.target != target {
            self.target = target;
            self.submit.edited(next);
        }
    }

    /// Next or Back (`shareStep`), between the two steps before the run.
    pub fn go(&mut self, step: ShareStep) {
        let before_run = matches!(self.step, ShareStep::Target | ShareStep::Check);
        match step {
            ShareStep::Target if before_run => self.step = step,
            ShareStep::Check if before_run && self.can_next() => self.step = step,
            _ => {}
        }
    }

    /// Next waits for the organizations, or for the failure to read them.
    pub fn can_next(&self) -> bool {
        !matches!(self.choices, Choices::Loading)
    }

    /// Share needs a name Door43 accepts, and no run in flight.
    pub fn can_submit(&self) -> bool {
        name_ok(&self.name) && !self.submit.busy()
    }

    /// The name field shows itself invalid once it holds something Door43 would refuse.
    pub fn name_invalid(&self) -> bool {
        !self.name.trim().is_empty() && !name_ok(&self.name)
    }

    /// The name changed: the last refusal goes, and the next Share is a new request.
    pub fn type_name(&mut self, name: String, next: RequestId) {
        self.name = name;
        self.error = None;
        self.submit.edited(next);
    }

    /// Share on the check step (`shareRun`): the request to send, with the run on its first
    /// progress line; `None` when Share is off or its request is in flight.
    pub fn run(&mut self) -> Option<(RequestId, ShareOwner, RepoName)> {
        if self.step != ShareStep::Check {
            return None;
        }
        let name = RepoName::parse(&self.name).ok()?;
        let owner = match &self.target {
            Target::User => ShareOwner::Mine,
            Target::Organization(org) => ShareOwner::Organization(RepoName::parse(org).ok()?),
        };
        let request = self.submit.press()?;
        self.step = ShareStep::Running(SharePhase::Preparing);
        self.shown.clear();
        self.error = None;
        Some((request, owner, name))
    }

    /// The run reached `phase`: its line shows, and joins the lines shown.
    pub fn progress(&mut self, phase: SharePhase) {
        if !matches!(self.step, ShareStep::Running(now) if now != phase) {
            return;
        }
        self.step = ShareStep::Running(phase);
        if phase != SharePhase::Preparing {
            self.shown.push(phase);
        }
    }

    /// Door43 holds the project.
    pub fn shared(&mut self, repo: SharedRepo) {
        if matches!(self.step, ShareStep::Running(_)) {
            self.submit.answered();
            self.step = ShareStep::Done(repo);
        }
    }

    /// The run was refused: back to the check step, where the name can change (tC4: "a
    /// refusal returns to the step it started from"); the next Share is a new attempt.
    pub fn refused(&mut self, code: Option<&str>, text: String, next: RequestId) {
        self.submit.refused(text.clone(), next);
        self.back_to_check();
        self.error = Some(ShareProblem {
            code: code.map(str::to_owned),
            text,
        });
    }

    /// The internet question or the sign-in step took over, or the run ended without a word:
    /// the check step again, and the next Share is a new attempt.
    pub fn interrupted(&mut self, next: RequestId) {
        self.submit.next(next);
        self.back_to_check();
    }

    /// Not now at the internet question: nothing was sent, and Share is on again.
    pub fn not_sent(&mut self) {
        self.submit.not_sent();
        self.back_to_check();
    }

    /// No usable reply: the next Share sends the same request, which the server's reply cache
    /// answers if the share started (A-27).
    pub fn lost(&mut self, problem: String) {
        self.submit.lost(problem.clone());
        self.back_to_check();
        self.error = Some(ShareProblem {
            code: None,
            text: problem,
        });
    }

    fn back_to_check(&mut self) {
        if matches!(self.step, ShareStep::Running(_)) {
            self.step = ShareStep::Check;
        }
    }

    /// The done step's sentences, in order. The team-access line is there for every project:
    /// tC4 left it off an Open Bible Stories share because its round 1 synced Bibles only
    /// (#724); stories sync here too (X, #761), so their teammates need the same access.
    pub fn done_lines(&self) -> Vec<crate::i18n::Key> {
        vec![
            keys::share_dialog::DONE_TEXT,
            keys::share_dialog::TEAM_ACCESS,
        ]
    }

    /// The organization the repository goes to, by its account name as tC4's `targetLabel`
    /// names it; `None` for the person's own account, which the caller words with the
    /// username.
    pub fn target_org(&self) -> Option<&str> {
        match &self.target {
            Target::User => None,
            Target::Organization(name) => Some(name),
        }
    }
}
