//! The question about books another program changed and the kept copies (D1): the calls behind
//! *Import*, *Not now* and *Dismiss*, and the look at the folder when the window gets focus.

use leptos::prelude::*;
use tc_api::endpoint::Endpoint;
use tc_api::ids::Scope;
use tc_api::session::{DismissNotice, Focus, ImportKept, NotNow, OutsideEdits};
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::Actions;
use super::js;

fn warn(what: &str, error: &impl std::fmt::Display) {
    web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(&format!(
        "tc4: {what}: {error}"
    )));
}

const FOCUS: &str = "the folder was not looked at";

impl Actions {
    /// The open project's scope and its outside edits as the session lists them.
    pub fn outside_edits(&self) -> Option<(Scope, OutsideEdits)> {
        let at = self
            .s()
            .project
            .with_untracked(|p| p.as_ref().map(|p| p.at.clone()))?;
        let outside = self.s().server.sessions.with_untracked(|sessions| {
            sessions
                .iter()
                .find(|s| s.at == at)
                .map(|s| s.outside.clone())
        })?;
        Some((at, outside))
    }

    /// A refusal shows where the person answered, and the question or the row stays; a focus
    /// look that fails only warns, since no one asked for it.
    async fn outside_call<E: Endpoint>(&self, request: E, what: &str) -> Option<E::Ok> {
        let answer = what != FOCUS;
        match self.runtime().api().call(&request).await {
            Ok(reply) => {
                if answer {
                    self.s().outside_refused.set(None);
                }
                Some(reply)
            }
            Err(error) => {
                warn(what, &error);
                if answer {
                    self.s().outside_refused.set(Some(match &error {
                        // The review's words for a refused write, `NotKept` among them.
                        crate::app::client::CallError::Api(api) => {
                            crate::app::review::failure_text(api)
                        }
                        other => super::sources::said_call(other),
                    }));
                }
                None
            }
        }
    }

    /// The window got focus (STORAGE §11.1): the server looks at the project's files, and a
    /// changed book raises the question.
    pub fn focus_window(&self) {
        let Some((at, _)) = self.outside_edits() else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = Focus {
                request: js::random_request_id(),
                at: at.clone(),
            };
            if let Some(outside) = this.outside_call(request, FOCUS).await {
                this.s().server.session_outside(&at, outside);
            }
        });
    }

    /// The question's *Import*: each waiting book that can be read, its newest copy; then the
    /// reports. A book that cannot be read stays asked, with *Not now* only.
    pub fn import_waiting(&self) {
        let Some((at, outside)) = self.outside_edits() else {
            return;
        };
        let copies = outside
            .waiting
            .iter()
            .filter(|w| w.unreadable.is_none())
            .map(|w| w.copy.clone())
            .collect();
        self.import_copies(at, copies);
    }

    /// *Import* of one kept copy from the conflict review's notices.
    pub fn import_kept(&self, copy: String) {
        let Some((at, _)) = self.outside_edits() else {
            return;
        };
        self.import_copies(at, vec![copy]);
    }

    /// Claims the one answer that may be on its way; `false` while another is.
    fn start_answer(&self) -> bool {
        let busy = &self.s().outside_busy;
        if busy.get_untracked() {
            return false;
        }
        busy.set(true);
        true
    }

    fn import_copies(&self, at: Scope, copies: Vec<String>) {
        if !self.start_answer() {
            return;
        }
        let this = self.clone();
        spawn_local(async move {
            for copy in copies {
                let request = ImportKept {
                    request: js::random_request_id(),
                    at: at.clone(),
                    copy,
                };
                if let Some(report) = this
                    .outside_call(request, "the copy was not imported")
                    .await
                {
                    this.s().server.session_outside(&at, report.outside.clone());
                    this.s()
                        .outside_reports
                        .update(|reports| reports.push(report));
                }
            }
            this.s().outside_busy.set(false);
        });
    }

    /// The question's *Not now*: the project keeps its text for every waiting book.
    pub fn not_now_waiting(&self) {
        let Some((at, outside)) = self.outside_edits() else {
            return;
        };
        if !self.start_answer() {
            return;
        }
        let this = self.clone();
        spawn_local(async move {
            for waiting in outside.waiting {
                let request = NotNow {
                    request: js::random_request_id(),
                    at: at.clone(),
                    file: waiting.file,
                };
                if let Some(outside) = this.outside_call(request, "Not now was not saved").await {
                    this.s().server.session_outside(&at, outside);
                }
            }
            this.s().outside_busy.set(false);
        });
    }

    /// *Dismiss* of a kept copy's notice.
    pub fn dismiss_kept(&self, copy: String) {
        let Some((at, _)) = self.outside_edits() else {
            return;
        };
        let this = self.clone();
        spawn_local(async move {
            let request = DismissNotice {
                request: js::random_request_id(),
                at: at.clone(),
                copy,
            };
            if let Some(outside) = this
                .outside_call(request, "the notice was not dismissed")
                .await
            {
                this.s().server.session_outside(&at, outside);
            }
        });
    }

    pub fn close_outside_reports(&self) {
        self.s().outside_reports.set(Vec::new());
        self.s().outside_refused.set(None);
    }

    /// The report's link: the review of the changes to look at.
    pub fn review_imported(&self) {
        self.close_outside_reports();
        self.open_review(tc_api::conflicts::ReviewTab::Text, None);
    }
}
