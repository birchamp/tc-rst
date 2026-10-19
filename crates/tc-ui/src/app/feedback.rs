//! The Feedback dialog's rules (M14, tC4 #378, `tC4:src/state.jsx:4089-4167`,
//! `tC4:src/data/feedback.ts`): which report "Ask for help" and "Report a problem" open, when
//! Send is on, and what the shell receives.
//!
//! The report lives in memory until the page goes, as tC4's lives until the app quits: nothing
//! about it is stored (A5 D11). A name and an email are personal data nobody asked the app to
//! keep, and the message is not project work.

use tc_api::app::{BUILD_COMMIT, BUILD_VERSION};
use tc_api::feedback::FeedbackCategory;
use tc_api::ids::ReportId;
use tc_api::refusal::FeedbackRefusal;

use crate::app::shell::js_trim;

/// "Ask for help" on a refusal: its code, the banner's diagnosis and the report of the refused
/// operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpRequest {
    pub code: String,
    pub text: String,
    pub report: Option<ReportId>,
}

/// The attachment the server composes (`feedback/compose`). tC4 composes it on the spot, so
/// its Send never waits; here Send waits for it (see [`can_send`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attachment {
    Loading,
    Ready(String),
    /// The compose failed, for this reason (the call's error, in English). The dialog says so in
    /// the shown language and Send stays off; the next opening of the report composes again,
    /// with the typed text kept.
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackResult {
    Sent,
    NotSent(FeedbackRefusal),
}

/// tC4's `s.fb`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackForm {
    /// The refusal "Ask for help" opened it for (`refusalKey`), or none for "Report a problem".
    pub refusal: Option<String>,
    pub category: FeedbackCategory,
    pub message: String,
    pub name: String,
    pub email: String,
    pub attachment: Attachment,
    pub sending: bool,
    pub result: Option<FeedbackResult>,
}

/// tC4's `refusalKey`: which refusal a report was opened for.
pub fn refusal_key(help: &HelpRequest) -> String {
    format!("{}\n{}", help.code, help.text)
}

/// tC4's `keepsReport`: an unsent report of the same refusal is reopened as it is (Cancel keeps
/// the message); another refusal replaces it; a sent report was cleared.
pub fn keeps_report(kept: Option<&FeedbackForm>, help: &HelpRequest) -> bool {
    kept.is_some_and(|fb| {
        fb.result != Some(FeedbackResult::Sent)
            && fb.refusal.as_deref() == Some(refusal_key(help).as_str())
    })
}

/// What "Ask for help" opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// The kept report, as it is.
    Reopen,
    /// A report whose attachment the server composes now: a new one, or the kept one when its
    /// compose failed.
    New(FeedbackForm),
}

/// A kept report whose attachment failed reopens with its text and a new compose, so the person
/// gets a report they can send by opening it again.
pub fn ask_for_help(kept: Option<&FeedbackForm>, help: &HelpRequest) -> Opened {
    if let Some(fb) = kept.filter(|_| keeps_report(kept, help)) {
        return match fb.attachment {
            Attachment::Failed(_) => Opened::New(FeedbackForm {
                attachment: Attachment::Loading,
                ..fb.clone()
            }),
            Attachment::Loading | Attachment::Ready(_) => Opened::Reopen,
        };
    }
    Opened::New(FeedbackForm {
        refusal: Some(refusal_key(help)),
        category: FeedbackCategory::Bug,
        message: format!("Refusal code: {}\n\n{}", help.code, help.text),
        name: String::new(),
        email: String::new(),
        attachment: Attachment::Loading,
        sending: false,
        result: None,
    })
}

/// "Report a problem" (tC4 #521): an unsent report the menu started that holds a message, a
/// name or an email reopens with that text and a new attachment; anything else opens a new
/// General Feedback report, which replaces a refusal's report.
pub fn report_problem(kept: Option<&FeedbackForm>) -> FeedbackForm {
    let typed = kept.filter(|fb| {
        fb.result != Some(FeedbackResult::Sent)
            && fb.refusal.is_none()
            && !js_trim(&format!("{}{}{}", fb.message, fb.name, fb.email)).is_empty()
    });
    match typed {
        Some(fb) => FeedbackForm {
            attachment: Attachment::Loading,
            ..fb.clone()
        },
        None => FeedbackForm {
            refusal: None,
            category: FeedbackCategory::General,
            message: String::new(),
            name: String::new(),
            email: String::new(),
            attachment: Attachment::Loading,
            sending: false,
            result: None,
        },
    }
}

/// Whether Send is on: not while a send runs, not with an empty message (tC4), and not until
/// the attachment is shown, so nothing is sent that the dialog did not show (a rebuild rule:
/// tC4 composes the attachment at once).
pub fn can_send(form: &FeedbackForm) -> bool {
    !form.sending
        && !js_trim(&form.message).is_empty()
        && matches!(form.attachment, Attachment::Ready(_))
}

/// The help desk received it: the text goes, the category stays (tC4's `finish`).
pub fn sent(form: &mut FeedbackForm) {
    form.sending = false;
    form.message.clear();
    form.name.clear();
    form.email.clear();
    form.result = Some(FeedbackResult::Sent);
}

/// It was not sent: everything stays, with the reason.
pub fn not_sent(form: &mut FeedbackForm, why: FeedbackRefusal) {
    form.sending = false;
    form.result = Some(FeedbackResult::NotSent(why));
}

/// The internet question's "Not now": nothing was sent and nothing is said.
pub fn dropped(form: &mut FeedbackForm) {
    form.sending = false;
}

/// What the shell's `feedbackSend` receives (APP-DESIGN 5.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellReport {
    /// The help desk's English name of the category.
    pub category: &'static str,
    pub message: String,
    pub name: String,
    pub email: String,
    /// `translationCore {version} ({commit})`, as tC4's `APP_LINE`.
    pub version: String,
    /// Exactly the text the dialog shows.
    pub attachment: String,
}

/// The report as the shell sends it, or none while the attachment is not shown.
pub fn shell_report(form: &FeedbackForm) -> Option<ShellReport> {
    let Attachment::Ready(attachment) = &form.attachment else {
        return None;
    };
    Some(ShellReport {
        category: form.category.subject_name(),
        message: form.message.clone(),
        name: js_trim(&form.name).to_owned(),
        email: js_trim(&form.email).to_owned(),
        version: format!("translationCore {BUILD_VERSION} ({BUILD_COMMIT})"),
        attachment: attachment.clone(),
    })
}
