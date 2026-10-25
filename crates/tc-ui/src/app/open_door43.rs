//! "Open a project from Door43" (M6, unit B9b; tC4's `od`, #677,
//! `tC4:src/views/modals/OpenFromDoor43.jsx`): the repository, the project name that follows
//! it until the person types one, the inline name errors, and the refusal with its code. Pure;
//! the rules of a repository and a name are `tc_api::door43`'s, which the server applies too.

use tc_api::door43::{ProjectName, RepoSource, default_project_name};
use tc_api::ids::RequestId;

use crate::app::submit::Submit;
use crate::i18n::{Key, keys};

/// A refusal of the open: its code and the sentence the code has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRefusal {
    pub code: Option<String>,
    /// What "Ask for help" adds after the sentence: the refusal as the server gave it.
    pub reason: String,
}

/// What the last run left on the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenProblem {
    /// `open-door43.name-exists`: shown under the name, as an error of the field.
    NameInUse,
    Refused(OpenRefusal),
}

/// The refusal code that is shown under the name field rather than in the callout.
pub const NAME_IN_USE: &str = "open-door43.name-exists";

/// tC4's `od`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenForm {
    pub repo: String,
    pub name: String,
    /// The person typed a name: it stops following the repository.
    pub named: bool,
    /// Open project's attempt: one request per attempt (UI-3).
    pub submit: Submit,
    pub error: Option<OpenProblem>,
}

/// The error under the repository field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoError {
    /// `openDoor43.repoBad`.
    Bad,
}

/// The error under the project name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    /// `openDoor43.nameBad`.
    Bad,
    /// `openDoor43.nameInUse`.
    InUse,
}

impl OpenForm {
    /// The dialog as it opens, with the request of its first attempt.
    pub fn new(request: RequestId) -> Self {
        Self {
            repo: String::new(),
            name: String::new(),
            named: false,
            submit: Submit::new(request),
            error: None,
        }
    }

    /// An open runs: the dialog's controls are off.
    pub fn busy(&self) -> bool {
        self.submit.busy()
    }

    /// The repository typed (tC4's `onRepo`): the name follows it until the person types one,
    /// and the last refusal goes.
    pub fn type_repo(&mut self, repo: String, host: &str) {
        if !self.named {
            self.name = RepoSource::from_input(&repo, host)
                .map(|source| default_project_name(source.name()))
                .unwrap_or_default();
        }
        self.repo = repo;
        self.error = None;
    }

    /// A name typed: it is the person's own from now on.
    pub fn type_name(&mut self, name: String) {
        self.name = name;
        self.named = true;
        self.error = None;
    }

    pub fn source(&self, host: &str) -> Option<RepoSource> {
        RepoSource::from_input(&self.repo, host)
    }

    pub fn repo_error(&self, host: &str) -> Option<RepoError> {
        (!self.repo.trim().is_empty() && self.source(host).is_none()).then_some(RepoError::Bad)
    }

    /// tC4's `nameErrorOf`: not a folder name, or the name of a folder Home lists (without
    /// letter case), or the server's own refusal of the name.
    pub fn name_error<'a>(&self, listing: impl IntoIterator<Item = &'a str>) -> Option<NameError> {
        if self.name.trim().is_empty() {
            return None;
        }
        let Ok(name) = ProjectName::parse(&self.name) else {
            return Some(NameError::Bad);
        };
        let in_use = matches!(self.error, Some(OpenProblem::NameInUse)) || name.in_use(listing);
        in_use.then_some(NameError::InUse)
    }

    /// The refusal the callout shows; a name in use shows under the name instead.
    pub fn refusal(&self) -> Option<&OpenRefusal> {
        match &self.error {
            Some(OpenProblem::Refused(refusal)) => Some(refusal),
            Some(OpenProblem::NameInUse) | None => None,
        }
    }

    /// What Open project sends, or `None` while it is off: nothing parses, the name is empty
    /// or has an error, or an open runs.
    pub fn ready<'a>(
        &self,
        host: &str,
        listing: impl IntoIterator<Item = &'a str>,
    ) -> Option<(RepoSource, ProjectName)> {
        if self.busy() || self.name_error(listing).is_some() {
            return None;
        }
        Some((self.source(host)?, ProjectName::parse(&self.name).ok()?))
    }

    /// No usable answer from the app's server: the reason shows, and the next press sends the
    /// same request again.
    pub fn lost(&mut self, reason: String) {
        self.submit.lost(reason.clone());
        self.error = Some(OpenProblem::Refused(OpenRefusal { code: None, reason }));
    }

    /// The run ended in a refusal: the form takes it and can be changed again, and the next
    /// press is a new attempt with `next`.
    pub fn refused(&mut self, code: Option<&str>, reason: String, next: RequestId) {
        self.submit.refused(reason.clone(), next);
        self.error = Some(if code == Some(NAME_IN_USE) {
            OpenProblem::NameInUse
        } else {
            OpenProblem::Refused(OpenRefusal {
                code: code.map(str::to_owned),
                reason,
            })
        });
    }
}

/// The sentence of a refusal code (tC4's `REFUSAL_TEXT`): one per code the open meets, and the
/// general one for any other or none. The general one is Q-21's, without tC4's "make sure that
/// the repository is public": the rebuild opens private repositories after sign-in (Q3). tC4's
/// `sync.obs-unsupported` has none: story projects open and sync like Bibles here (Q2).
pub fn refusal_key(code: Option<&str>) -> Key {
    match code {
        Some("sync.incoming-invalid") => keys::open_door43::refused::INVALID,
        Some("open-door43.no-journal") => keys::open_door43::refused::NO_JOURNAL,
        Some("sync.clone-path-exists") => keys::open_door43::refused::CLONE_PATH_EXISTS,
        Some("opslog.not-loaded") => keys::open_door43::refused::OPS_NOT_LOADED,
        Some("sync.offline") => keys::open_door43::refused::OFFLINE,
        Some("sync.move-failed") => keys::open_door43::refused::MOVE_FAILED,
        _ => keys::open_door43::refused::NOT_OPENED,
    }
}
