//! The internet question (M15, APP-DESIGN 5.6, 6.7, AD25): tC4's `allowInternet`
//! (`tC4:src/state.jsx:4243-4310`) as a value with transitions. A step that needs the internet
//! asks while the switch is off; "Not now" sends nothing; "Turn on internet" turns the switch on
//! and then runs the step; a switch that could not be turned on says so and runs nothing. One
//! question is open at a time: a step that asks meanwhile is dropped, as tC4's `pendingAsk`
//! drops it.

use tc_api::account::WebUrl;
use tc_api::endpoint::NetKind;
use tc_api::ids::ProjectRef;

use crate::app::share::ShareProject;

/// A link whose address the UI holds: the help pages' (A5) and a shared repository's, the share
/// dialog's "Open on Door43" (B9b). The profile is [`Pending::Profile`], whose address the
/// server gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Help,
    Repo,
}

/// The step to run once the question is answered, or once the sign-in that a step needed is
/// done (6.7). The flow units add theirs (`Share`, `Sync`, `OpenDoor43`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    /// Feedback's Send: the shell sends the report.
    Feedback,
    /// A page of another site, opened in the system browser.
    Link(LinkKind, WebUrl),
    /// A Home card's "Share on Door43" (B2 asks): the sign-in if needed, then the dialog on
    /// its where-it-goes step, reading the organizations (B9b).
    Share(Box<ShareProject>),
    /// A Home card's "Sync with Door43" (B2 asks; B10 runs the sync).
    Sync(ProjectRef),
    /// The sign-in step's Sign in (M12), sent once the internet is on: tC4 asks when the
    /// password is sent, not when the step opens. `flow` names the step it was pressed in.
    SignIn { flow: u64 },
    /// The account menu's "Sign-in saved on this computer": Door43 checks the kept token.
    CheckSignIn,
    /// The account menu's "Open my page on Door43".
    Profile,
    /// "Open project" in Open a project from Door43 (B9b): the open of the form as it stands.
    OpenDoor43,
    /// Get source texts reads a package's rows from Door43's catalog (B9a, tC4 `loadPackage`).
    Catalog,
    /// Get source texts' Download (B9a, tC4 `downloadPackage`).
    Download,
    /// "Check for updates" (C8, tC4 `checkForUpdates`).
    Updates,
    /// A language set's Upgrade (C8, tC4 `upgradeSet`).
    UpgradeSet(tc_api::resources::SetRung),
    /// A scripture text's Upgrade (C8, tC4 `upgradeText`), by its repository's name.
    UpgradeText(String),
    /// The guided fix's "Download it" (B5b, tC4 `fixFetch`), for the screen the id names.
    Fix(u64),
    /// Import's "Look up on Door43" (F1, tC4 `importGoOnline`).
    ImportVersions,
}

impl Pending {
    /// The question it asks (`net.ask.<kind>`).
    pub fn kind(&self) -> NetKind {
        match self {
            Self::Feedback => NetKind::Feedback,
            Self::Link(LinkKind::Help, _) => NetKind::HelpLink,
            Self::Link(LinkKind::Repo, _) => NetKind::RepoLink,
            Self::Share(_) => NetKind::Share,
            Self::Sync(_) => NetKind::Sync,
            Self::SignIn { .. } => NetKind::SignIn,
            Self::CheckSignIn => NetKind::CheckSignIn,
            Self::Profile => NetKind::Profile,
            Self::OpenDoor43 => NetKind::Open,
            Self::Fix(_) => NetKind::Fix,
            Self::ImportVersions => NetKind::ImportVersions,
            Self::Catalog
            | Self::Download
            | Self::Updates
            | Self::UpgradeSet(_)
            | Self::UpgradeText(_) => NetKind::Sources,
        }
    }
}

/// Where the question is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InternetFlow {
    #[default]
    Idle,
    /// The question is open.
    Asking { kind: NetKind, then: Pending },
    /// "Turn on internet" was chosen; `net/set` is on its way. The question has closed, as
    /// tC4's `answerAsk` closes it at once, and the step waits for the switch.
    TurningOn { kind: NetKind, then: Pending },
    /// "tC4 could not use the internet".
    Failed,
}

/// What the caller does after a transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetStep {
    /// Run the step now.
    Run(Pending),
    /// The step will not run: nothing is sent, and its own state goes back (Feedback's
    /// `sending` ends).
    Dropped(Pending),
    /// Ask for the switch on ([`SwitchRequests::ask`]), then call [`InternetFlow::turned_on`]
    /// when the requests settle.
    TurnOn,
    /// Nothing to do.
    Nothing,
}

impl InternetFlow {
    /// Whether a question is under way: on screen, waiting for the switch, or failed. A step
    /// that asks meanwhile is dropped.
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    /// The question on screen, if any.
    pub fn asking(&self) -> Option<NetKind> {
        match self {
            Self::Asking { kind, .. } => Some(*kind),
            Self::Idle | Self::TurningOn { .. } | Self::Failed => None,
        }
    }

    /// A step that needs the internet: run it while the switch is on; else ask, unless a
    /// question is open already, which drops it.
    pub fn ask(&mut self, internet_on: bool, then: Pending) -> NetStep {
        if internet_on {
            return NetStep::Run(then);
        }
        if !matches!(self, Self::Idle) {
            return NetStep::Dropped(then);
        }
        *self = Self::Asking {
            kind: then.kind(),
            then,
        };
        NetStep::Nothing
    }

    /// "Not now", Escape, the ✕ or the scrim: nothing is sent.
    pub fn not_now(&mut self) -> NetStep {
        match std::mem::take(self) {
            Self::Asking { then, .. } => NetStep::Dropped(then),
            other => {
                *self = other;
                NetStep::Nothing
            }
        }
    }

    /// "Turn on internet".
    pub fn turn_on(&mut self) -> NetStep {
        match std::mem::take(self) {
            Self::Asking { kind, then } => {
                *self = Self::TurningOn { kind, then };
                NetStep::TurnOn
            }
            other => {
                *self = other;
                NetStep::Nothing
            }
        }
    }

    /// The answer of `net/set`: the step runs, or the failure shows and the step is dropped.
    pub fn turned_on(&mut self, ok: bool) -> NetStep {
        match std::mem::take(self) {
            Self::TurningOn { then, .. } if ok => NetStep::Run(then),
            Self::TurningOn { then, .. } => {
                *self = Self::Failed;
                NetStep::Dropped(then)
            }
            other => {
                *self = other;
                NetStep::Nothing
            }
        }
    }

    /// The failure's Close.
    pub fn close_failed(&mut self) {
        if matches!(self, Self::Failed) {
            *self = Self::Idle;
        }
    }

    /// The server refused with `NeedsInternet`: the switch went off after the question (AD25).
    /// Ask once more; a second refusal for the same step drops it.
    pub fn needs_internet(&mut self, then: Pending, asked_once: bool) -> NetStep {
        if asked_once {
            return NetStep::Dropped(then);
        }
        self.ask(false, then)
    }
}

/// The `net/set` requests of the menu's switch and of "Turn on internet", one at a time. tC4's
/// `setInternet` changes the switch before anything else, so a double-click on the menu's row
/// turns it on and off again; here the server holds the switch, so a press turns away from the
/// state last asked for, not the one the server has confirmed. A request waits for the one on
/// its way, and only the last state asked for meanwhile goes after it, so the server ends at
/// the person's last choice whatever order two requests would reach it in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SwitchRequests {
    /// The state the request on its way asks for.
    sent: Option<bool>,
    /// The state last asked for while it was on its way.
    next: Option<bool>,
}

/// What the caller does after a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchStep {
    /// Send `net/set` with this state.
    Send(bool),
    /// No request is on its way: the server's last answer (`None` when the last request
    /// failed). "Turn on internet" goes on from here ([`InternetFlow::turned_on`]).
    Settled(Option<bool>),
}

impl SwitchRequests {
    /// The state a press of the switch turns away from.
    pub fn base(&self, confirmed: bool) -> bool {
        self.next.or(self.sent).unwrap_or(confirmed)
    }

    /// The person asks for `on`: the request to send now, unless one is on its way.
    pub fn ask(&mut self, on: bool) -> Option<bool> {
        if self.sent.is_some() {
            self.next = Some(on);
            return None;
        }
        self.sent = Some(on);
        Some(on)
    }

    /// The reply of the request on its way: the server's state, or `None` when it failed.
    pub fn answered(&mut self, state: Option<bool>) -> SwitchStep {
        self.sent = None;
        match self.next.take() {
            Some(on) if state != Some(on) => {
                self.sent = Some(on);
                SwitchStep::Send(on)
            }
            _ => SwitchStep::Settled(state),
        }
    }
}
