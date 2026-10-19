//! The share dialog (unit B9b; tC4's `sh`, `tC4:src/views/modals/ShareDialog.jsx`, #362, D84):
//! where the repository goes, then what will be shared. Unit E1b adds the run: its progress
//! line, its refusal on the check step, and the end with the team-access line (#724, Q2).
//! Pure: the page's actions run it.

use tc_api::account::{OrgChoice, ShareTargets};
use tc_api::home::{HomeCard, ProjectKind};
use tc_api::ids::ProjectRef;
use tc_api::shown::Known;
use tc_core::BookId;

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

/// The dialog's steps before the run (`sh.step`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareStep {
    /// Where it goes.
    Target,
    /// Check what will be shared.
    Check,
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
}

/// Whether Door43 accepts `name` as a repository name (tC4's `nameOk`): letters, digits, dots,
/// dashes and underscores, starting and ending with a letter or a digit.
pub fn name_ok(name: &str) -> bool {
    let name = name.trim();
    let bytes = name.as_bytes();
    let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && last.is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

impl ShareForm {
    /// A first share, on its where-it-goes step, reading the organizations (`startShare`).
    pub fn open(project: ShareProject) -> Self {
        let name = project.project.as_str().to_owned();
        Self {
            project,
            step: ShareStep::Target,
            choices: Choices::Loading,
            target: Target::User,
            name,
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
    pub fn pick(&mut self, target: Target) {
        let allowed = match &target {
            Target::User => true,
            Target::Organization(name) => matches!(&self.choices,
                Choices::Ready(orgs) if orgs.iter().any(|o| &o.name == name && o.can_create)),
        };
        if allowed {
            self.target = target;
        }
    }

    /// Next or Back (`shareStep`).
    pub fn go(&mut self, step: ShareStep) {
        if step == ShareStep::Check && !self.can_next() {
            return;
        }
        self.step = step;
    }

    /// Next waits for the organizations, or for the failure to read them.
    pub fn can_next(&self) -> bool {
        !matches!(self.choices, Choices::Loading)
    }

    /// Share needs a name Door43 accepts.
    pub fn can_submit(&self) -> bool {
        name_ok(&self.name)
    }

    /// The name field shows itself invalid once it holds something Door43 would refuse.
    pub fn name_invalid(&self) -> bool {
        !self.name.trim().is_empty() && !name_ok(&self.name)
    }

    pub fn type_name(&mut self, name: String) {
        self.name = name;
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
