//! USB exchange (unit E2; STORAGE §10, APP-DESIGN "E2 USB exchange"): the two dialogs' states,
//! kept pure so the host tests drive them. "Save work for a teammate" writes an exchange file
//! into a folder and says "You can remove the stick" only once the file read back from the
//! device (INV-REMOVE-THE-STICK-ONLY-AFTER-VERIFY). "Receive work from a file" names the
//! file's project, asks to confirm, and shows what the receive brought.

use tc_api::ApiError;
use tc_api::error::Flow;
use tc_api::exchange::{ExchangeFile, ExportedView, IdentifyView, ReceivedView};
use tc_api::ids::{ProjectRef, RequestId};
use tc_api::jobs::ExchangePhase;
use tc_api::refusal::Refusal;
use tc_api::upload::UploadFlow;

use crate::app::jobs::JobEnd;
use crate::app::submit::Submit;
use crate::i18n::{keys, t};

/// A dialog's project, as its Home card names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeProject {
    pub project: ProjectRef,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportStep {
    /// The folder is being chosen; `Some` once chosen.
    Choosing,
    /// The job runs: its phase, once reported.
    Working(Option<ExchangePhase>),
    /// Written and read back from the device: the stick may go.
    Done(ExportedView),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportForm {
    pub project: ExchangeProject,
    pub destination: Option<String>,
    pub submit: Submit,
    pub step: ExportStep,
}

impl ExportForm {
    pub fn open(project: ExchangeProject, request: RequestId) -> Self {
        Self {
            project,
            destination: None,
            submit: Submit::new(request),
            step: ExportStep::Choosing,
        }
    }

    /// The shell's folder picker answered (`None`: cancelled; the folder stays as it was).
    pub fn chose_folder(&mut self, folder: Option<String>, next: RequestId) {
        if let Some(folder) = folder {
            self.destination = Some(folder);
            self.submit.edited(next);
        }
    }

    /// Whether the folder may be chosen: before the first press, or after a failure.
    pub fn choosing(&self) -> bool {
        matches!(self.step, ExportStep::Choosing | ExportStep::Failed(_))
    }

    pub fn can_save(&self) -> bool {
        self.destination.is_some() && self.choosing() && !self.submit.busy()
    }

    /// The job runs: the dialog stays open and its buttons are off.
    pub fn busy(&self) -> bool {
        matches!(self.step, ExportStep::Working(_))
    }

    /// The Save press: the request to send, once per attempt.
    pub fn press(&mut self) -> Option<(RequestId, String)> {
        if !self.can_save() {
            return None;
        }
        let request = self.submit.press()?;
        self.step = ExportStep::Working(None);
        Some((request, self.destination.clone().unwrap_or_default()))
    }

    /// No usable reply to the attempt `request` (the connection failed): the folder step is
    /// back, and the next press sends the same request, which the server's reply cache
    /// answers with the job it started (A-27), so a run that went on is never lost sight of.
    pub fn reply_lost(&mut self, request: RequestId) {
        if self.submit.request() == request && self.busy() {
            self.submit.lost(t(keys::exchange::error::LOST_REPLY, &[]));
            self.step = ExportStep::Choosing;
        }
    }

    /// A report of the attempt `request`; one of an older attempt (a dialog closed and opened
    /// again while its job ran) changes nothing.
    pub fn progress(&mut self, request: RequestId, phase: ExchangePhase) {
        if self.submit.request() == request && matches!(self.step, ExportStep::Working(_)) {
            self.step = ExportStep::Working(Some(phase));
        }
    }

    /// The job of the attempt `request` ended; `next` is the request of the next attempt
    /// after a failure. An older attempt's end never reaches this form
    /// (INV-REMOVE-THE-STICK-ONLY-AFTER-VERIFY: the sentence is this attempt's own).
    pub fn ended(&mut self, request: RequestId, end: JobEnd<ExportedView>, next: RequestId) {
        if self.submit.request() != request || !self.busy() {
            return;
        }
        self.submit.answered();
        self.step = match end {
            JobEnd::Done(view) => ExportStep::Done(view),
            JobEnd::Failed(error) => {
                self.submit.refused(String::new(), next);
                ExportStep::Failed(failure_text(&error))
            }
            JobEnd::Cancelled | JobEnd::Lost | JobEnd::Left => {
                self.submit.refused(String::new(), next);
                ExportStep::Failed(t(keys::exchange::error::UNEXPECTED, &[]))
            }
        };
    }

    /// "You can remove the stick": only after the verify (never while working or failed).
    pub fn stick_sentence(&self) -> Option<String> {
        match &self.step {
            ExportStep::Done(view) => Some(t(keys::exchange::save::DONE, &[("path", &view.path)])),
            _ => None,
        }
    }

    /// The working line of the step the job is in.
    pub fn working_line(&self) -> Option<String> {
        let ExportStep::Working(phase) = &self.step else {
            return None;
        };
        Some(t(
            match phase {
                None | Some(ExchangePhase::Checkpoint) => keys::exchange::save::working::CHECKPOINT,
                Some(ExchangePhase::Write) => keys::exchange::save::working::WRITE,
                Some(ExchangePhase::Verify) => keys::exchange::save::working::VERIFY,
                Some(ExchangePhase::Read | ExchangePhase::TakeIn) => {
                    keys::exchange::save::working::WRITE
                }
            },
            &[],
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiveStep {
    Choosing,
    /// A browser's file is being sent to the server.
    Uploading,
    /// The file is being read for its project.
    Identifying,
    /// The file's project is known; the person confirms.
    Confirm(IdentifyView),
    Working(Option<ExchangePhase>),
    Received(ReceivedView),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveForm {
    pub project: ExchangeProject,
    pub file: Option<ExchangeFile>,
    /// The file's project as identified, kept so a lost reply can return to Confirm.
    pub identified: Option<IdentifyView>,
    pub submit: Submit,
    pub step: ReceiveStep,
}

/// Whether a browser file may be sent: STORAGE's cap for the whole file (A-28).
pub fn fits_upload(size: f64) -> bool {
    size <= UploadFlow::Exchange.cap() as f64
}

impl ReceiveForm {
    pub fn open(project: ExchangeProject, request: RequestId) -> Self {
        Self {
            project,
            file: None,
            identified: None,
            submit: Submit::new(request),
            step: ReceiveStep::Choosing,
        }
    }

    /// A file was chosen (a path from the shell, or an upload): it is read for its project.
    pub fn chose_file(&mut self, file: ExchangeFile, next: RequestId) {
        self.file = Some(file);
        self.identified = None;
        self.submit.edited(next);
        self.step = ReceiveStep::Identifying;
    }

    /// The upload itself failed: nothing was sent, so the file may be chosen again.
    pub fn too_large_or_failed(&mut self, error: ApiError) {
        self.step = ReceiveStep::Failed(failure_text(&error));
    }

    /// A browser file too large to send: refused before anything is sent.
    pub fn too_large(&mut self) {
        self.step = ReceiveStep::Failed(t(keys::exchange::receive::TOO_LARGE, &[]));
    }

    /// `exchange/identify` answered for `file`; an answer for another file (chosen before
    /// this one), or one that comes after the press, changes nothing.
    pub fn identified(&mut self, file: &ExchangeFile, answer: Result<IdentifyView, ApiError>) {
        if self.file.as_ref() != Some(file) || self.step != ReceiveStep::Identifying {
            return;
        }
        self.step = match answer {
            Ok(view) if view.project.as_ref() == Some(&self.project.project) => {
                self.identified = Some(view.clone());
                ReceiveStep::Confirm(view)
            }
            Ok(view) => ReceiveStep::Failed(t(
                keys::exchange::receive::OTHER_PROJECT,
                &[("name", &view.name), ("project", &self.project.name)],
            )),
            Err(error) => ReceiveStep::Failed(failure_text(&error)),
        };
    }

    pub fn can_receive(&self) -> bool {
        matches!(self.step, ReceiveStep::Confirm(_)) && self.file.is_some() && !self.submit.busy()
    }

    /// Whether a file may be chosen: before one is read, or after a failure.
    pub fn choosing(&self) -> bool {
        matches!(self.step, ReceiveStep::Choosing | ReceiveStep::Failed(_))
    }

    /// The file is being sent or read, or the job runs: the dialog stays open.
    pub fn busy(&self) -> bool {
        matches!(
            self.step,
            ReceiveStep::Uploading | ReceiveStep::Identifying | ReceiveStep::Working(_)
        )
    }

    /// A browser file is being sent (up to 4 GB): nothing else may start meanwhile.
    pub fn uploading(&mut self) {
        self.step = ReceiveStep::Uploading;
    }

    /// The Receive press: the request and the file to send, once per attempt.
    pub fn press(&mut self) -> Option<(RequestId, ExchangeFile)> {
        if !self.can_receive() {
            return None;
        }
        let request = self.submit.press()?;
        self.step = ReceiveStep::Working(None);
        Some((request, self.file.clone()?))
    }

    /// As [`ExportForm::reply_lost`]: back to Confirm, the same request next.
    pub fn reply_lost(&mut self, request: RequestId) {
        if self.submit.request() != request || !matches!(self.step, ReceiveStep::Working(_)) {
            return;
        }
        self.submit.lost(t(keys::exchange::error::LOST_REPLY, &[]));
        self.step = match self.identified.clone() {
            Some(view) => ReceiveStep::Confirm(view),
            None => ReceiveStep::Choosing,
        };
    }

    pub fn progress(&mut self, request: RequestId, phase: ExchangePhase) {
        if self.submit.request() == request && matches!(self.step, ReceiveStep::Working(_)) {
            self.step = ReceiveStep::Working(Some(phase));
        }
    }

    /// As [`ExportForm::ended`]: an older attempt's end never reaches this form.
    pub fn ended(&mut self, request: RequestId, end: JobEnd<ReceivedView>, next: RequestId) {
        if self.submit.request() != request || !matches!(self.step, ReceiveStep::Working(_)) {
            return;
        }
        self.submit.answered();
        self.step = match end {
            JobEnd::Done(view) => ReceiveStep::Received(view),
            JobEnd::Failed(error) => {
                self.submit.refused(String::new(), next);
                ReceiveStep::Failed(failure_text(&error))
            }
            // A run the page lost sight of (the server restarted, the dialog left) may have
            // taken some of the file in already: what arrived is kept, never "nothing changed".
            JobEnd::Cancelled | JobEnd::Lost | JobEnd::Left => {
                self.submit.refused(String::new(), next);
                ReceiveStep::Failed(t(keys::exchange::receive::STOPPED, &[]))
            }
        };
    }

    /// What the Received step says, line by line: the count and the senders, the conflicts
    /// to review, and each file edited by hand inside the exchange file.
    pub fn received_lines(&self) -> Vec<String> {
        let ReceiveStep::Received(view) = &self.step else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        let from: Vec<String> = view
            .from
            .iter()
            .map(crate::app::sync::person_label)
            .collect();
        let from = from.join(" · ");
        lines.push(match view.received {
            0 => t(keys::exchange::receive::NONE, &[]),
            1 => t(keys::exchange::receive::RECEIVED_ONE, &[("from", &from)]),
            n => t(
                keys::exchange::receive::RECEIVED,
                &[("n", &n), ("from", &from)],
            ),
        });
        match view.verse_conflicts {
            0 => {}
            1 => lines.push(t(keys::exchange::receive::CONFLICTS_ONE, &[])),
            n => lines.push(t(keys::exchange::receive::CONFLICTS, &[("n", &n)])),
        }
        for file in &view.hand_edited {
            lines.push(t(
                keys::exchange::receive::HAND_EDITED,
                &[("path", &file.path)],
            ));
        }
        lines
    }
}

/// The sentence of a failed export or receive: the refusal's own, a busy flow's, else the
/// unexpected one. Door43's words and paths never reach it (Q36).
pub fn failure_text(error: &ApiError) -> String {
    match error {
        ApiError::Refused(Refusal::Exchange(refusal)) => crate::i18n::refusal_text(refusal),
        ApiError::Busy(Flow::Sync) => t(keys::exchange::error::BUSY_SYNC, &[]),
        ApiError::Busy(Flow::ExchangeExport | Flow::ExchangeReceive) => {
            t(keys::exchange::error::BUSY_EXCHANGE, &[])
        }
        _ => t(keys::exchange::error::UNEXPECTED, &[]),
    }
}
