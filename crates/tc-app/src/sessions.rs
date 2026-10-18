//! The seam between the app and the open projects (APP-DESIGN §4.3).
//!
//! A write that is scoped to a project carries a [`Scope`]: the folder and the open it was
//! made against. The session registry (unit B1b) knows which scopes are open. Until it
//! exists no project opens, so [`NoSessions`] answers that every scope is stale and that no
//! session is open.

use tc_api::app::OpenSession;
use tc_api::ids::{ProjectId, ProjectRef, Scope};

/// The project behind an open scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    /// The folder the scope names.
    pub project: ProjectRef,
    /// The project's id, which keys its preferences (`prefs/<projectId>.json`).
    pub id: ProjectId,
}

pub trait SessionDirectory: Send + Sync + 'static {
    /// The project behind a scope, if that scope's session is open now. A scope of a closed
    /// or replaced open resolves to nothing.
    fn resolve(&self, at: &Scope) -> Option<SessionInfo>;

    /// The sessions open in this server run, so a reload continues without opening the
    /// project again (`hello`).
    fn open_sessions(&self) -> Vec<OpenSession>;
}

/// Phase 0: no project opens before unit B1b, whose `SessionRegistry` replaces this.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSessions;

impl SessionDirectory for NoSessions {
    fn resolve(&self, _at: &Scope) -> Option<SessionInfo> {
        None
    }

    fn open_sessions(&self) -> Vec<OpenSession> {
        Vec::new()
    }
}
