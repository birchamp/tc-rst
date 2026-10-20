//! The guided fix's screen (M11; tC4 `s.fix`, `openFix`, `patchFix`, `fixFetch`, `fixSideload`,
//! `fixRepin`, `GuidedFix.jsx`). B5a drew it; B5b wires its three ways to C8's endpoints. Pure;
//! the requests are the web glue's.
//!
//! One way runs at a time (tC4 `fix.busy`): a second press while one is on its way does nothing,
//! so a download and a file install never race to install one pin. A way that fails states why
//! and leaves the other two at hand. A completion changes the screen only while the screen is
//! the one it started from (tC4 `fix.id`): an answer that comes after the person closed the
//! dialog and opened it again is not for the dialog now shown.

use tc_api::check::Tool;
use tc_api::resources::FixView;

use crate::i18n::{keys, t};

/// What the dialog shows (tC4 `s.fix` while `s.modal` is `fix`).
#[derive(Debug, Clone, PartialEq)]
pub enum Fix {
    /// `resources/fixState` is on its way.
    Loading(Tool),
    Ready(Box<FixScreen>),
    /// The read failed: its reason.
    Failed(Tool, String),
}

impl Fix {
    pub fn tool(&self) -> Tool {
        match self {
            Self::Loading(tool) | Self::Failed(tool, _) => *tool,
            Self::Ready(screen) => screen.tool,
        }
    }
}

/// The way on its way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    Fetch,
    Sideload,
    Repin,
}

/// The guided fix of one tool, with what one of its ways is doing.
#[derive(Debug, Clone, PartialEq)]
pub struct FixScreen {
    /// This screen's identity: each opening has its own.
    pub id: u64,
    pub tool: Tool,
    pub view: FixView,
    pub busy: Option<Way>,
    /// What the way is doing, in tC4's words (`sources.progress`, `fix.sideload.reading`).
    pub progress: Option<String>,
    pub error: Option<String>,
}

impl FixScreen {
    /// The screen as it opens. The installed copies that could not be read are said at once,
    /// never shown as "none installed" (tC4's `resolutionError`).
    pub fn new(id: u64, tool: Tool, view: FixView) -> Self {
        let error = view.candidates_error.clone();
        Self {
            id,
            tool,
            view,
            busy: None,
            progress: None,
            error,
        }
    }

    /// The pinned repository's name (`es-419_twl`).
    pub fn repo(&self) -> &str {
        let path = &self.view.pin.repo_path;
        path.rsplit('/').next().unwrap_or(path)
    }

    /// "Download it": `false` while another way is on its way.
    pub fn start_fetch(&mut self) -> bool {
        let progress = t(keys::sources::PROGRESS, &[("repo", &self.repo())]);
        self.start(Way::Fetch, Some(progress))
    }

    /// A file was chosen or dropped: `false` while another way is on its way.
    pub fn start_sideload(&mut self, name: &str) -> bool {
        let progress = t(keys::fix::sideload::READING, &[("name", &name)]);
        self.start(Way::Sideload, Some(progress))
    }

    /// An installed version was chosen: `false` while another way is on its way.
    pub fn start_repin(&mut self) -> bool {
        self.start(Way::Repin, None)
    }

    fn start(&mut self, way: Way, progress: Option<String>) -> bool {
        if self.busy.is_some() {
            return false;
        }
        self.busy = Some(way);
        self.progress = progress;
        self.error = None;
        true
    }

    /// The download reports the repository it fetches.
    pub fn fetching(&mut self, repo: &str) {
        if self.busy == Some(Way::Fetch) {
            self.progress = Some(t(keys::sources::PROGRESS, &[("repo", &repo)]));
        }
    }

    /// A download or a file install failed: nothing was installed, and the screen says why.
    pub fn install_failed(&mut self, reason: &str) {
        self.ended(Some(t(keys::fix::FAILED, &[("reason", &reason)])));
    }

    /// The re-pin could not be planned: tC4 says it as a failed upgrade.
    pub fn repin_failed(&mut self, reason: &str) {
        self.ended(Some(t(keys::upgrade::FAILED, &[("reason", &reason)])));
    }

    /// The way stopped with nothing to say: cancelled, or the internet question was declined.
    pub fn stopped(&mut self) {
        self.ended(None);
    }

    fn ended(&mut self, error: Option<String>) {
        self.busy = None;
        self.progress = None;
        self.error = error;
    }
}

/// Patches the screen `id` names and answers what `change` did; an answer for a screen no
/// longer shown changes nothing (`None`).
pub fn patch<R>(
    fix: &mut Option<Fix>,
    id: u64,
    change: impl FnOnce(&mut FixScreen) -> R,
) -> Option<R> {
    match fix {
        Some(Fix::Ready(screen)) if screen.id == id => Some(change(screen)),
        _ => None,
    }
}
