//! The Feedback dialog's rules (M14): tC4's kept-report rules, Send, and what the shell gets.

use tc_api::feedback::FeedbackCategory;
use tc_api::ids::ReportId;
use tc_api::refusal::FeedbackRefusal;
use tc_ui::app::feedback::{
    Attachment, FeedbackForm, FeedbackResult, HelpRequest, Opened, ask_for_help, can_send, dropped,
    keeps_report, not_sent, refusal_key, report_problem, sent, shell_report,
};

fn help(code: &str) -> HelpRequest {
    HelpRequest {
        code: code.to_owned(),
        text: "tC4 could not open the project.".to_owned(),
        report: Some(ReportId::new(3)),
    }
}

fn opened(kept: Option<&FeedbackForm>, help: &HelpRequest) -> FeedbackForm {
    match ask_for_help(kept, help) {
        Opened::New(form) => form,
        Opened::Reopen => panic!("expected a new report"),
    }
}

#[test]
fn ask_for_help_opens_a_bug_report_with_the_refusal() {
    let form = opened(None, &help("open.locked"));
    assert_eq!(form.category, FeedbackCategory::Bug);
    assert_eq!(
        form.message,
        "Refusal code: open.locked\n\ntC4 could not open the project."
    );
    assert_eq!(
        form.refusal.as_deref(),
        Some("open.locked\ntC4 could not open the project.")
    );
    assert_eq!(form.attachment, Attachment::Loading);
    assert_eq!(
        refusal_key(&help("x")),
        "x\ntC4 could not open the project."
    );
}

#[test]
fn an_unsent_report_of_the_same_refusal_reopens() {
    let mut kept = opened(None, &help("open.locked"));
    kept.message.push_str(" More.");
    assert!(keeps_report(Some(&kept), &help("open.locked")));
    assert_eq!(
        ask_for_help(Some(&kept), &help("open.locked")),
        Opened::Reopen
    );
    // Another refusal replaces it.
    assert!(!keeps_report(Some(&kept), &help("sync.offline")));
    // A sent one was cleared.
    sent(&mut kept);
    assert!(!keeps_report(Some(&kept), &help("open.locked")));
    // Not sent: still kept.
    let mut kept = opened(None, &help("open.locked"));
    not_sent(&mut kept, FeedbackRefusal::Offline);
    assert!(keeps_report(Some(&kept), &help("open.locked")));
}

fn general(message: &str) -> FeedbackForm {
    let mut form = report_problem(None);
    form.message = message.to_owned();
    form.attachment = Attachment::Ready("old".to_owned());
    form
}

#[test]
fn report_problem_keeps_typed_text_with_a_new_attachment() {
    let fresh = report_problem(None);
    assert_eq!(fresh.category, FeedbackCategory::General);
    assert!(fresh.refusal.is_none() && fresh.message.is_empty());
    assert_eq!(fresh.attachment, Attachment::Loading);

    let mut kept = general("It crashed");
    kept.category = FeedbackCategory::Content;
    kept.name = "Ana".to_owned();
    let again = report_problem(Some(&kept));
    assert_eq!(again.message, "It crashed");
    assert_eq!(again.name, "Ana");
    assert_eq!(again.category, FeedbackCategory::Content);
    assert_eq!(again.attachment, Attachment::Loading);

    // Only an email typed counts as typed.
    let mut email_only = general("");
    email_only.email = "a@b.c".to_owned();
    assert_eq!(report_problem(Some(&email_only)).email, "a@b.c");

    // Spaces only, a refusal's report, or a sent report: a new General report.
    for kept in [general(" \u{3000} "), opened(None, &help("open.locked")), {
        let mut s = general("x");
        sent(&mut s);
        s
    }] {
        let again = report_problem(Some(&kept));
        assert_eq!(again.category, FeedbackCategory::General);
        assert!(again.message.is_empty() && again.refusal.is_none());
        assert_eq!(again.result, None);
    }
}

#[test]
fn send_needs_a_message_a_shown_attachment_and_no_send_running() {
    let mut form = general("Hello");
    assert!(can_send(&form));
    form.message = " \n\t".to_owned();
    assert!(!can_send(&form));
    form.message = "Hello".to_owned();
    form.attachment = Attachment::Loading;
    assert!(!can_send(&form));
    form.attachment = Attachment::Failed("no server".to_owned());
    assert!(!can_send(&form));
    form.attachment = Attachment::Ready("a".to_owned());
    form.sending = true;
    assert!(!can_send(&form));
}

#[test]
fn sent_clears_the_text_and_keeps_the_category() {
    let mut form = general("Hello");
    form.category = FeedbackCategory::Bug;
    form.name = "Ana".to_owned();
    form.email = "a@b.c".to_owned();
    form.sending = true;
    sent(&mut form);
    assert!(form.message.is_empty() && form.name.is_empty() && form.email.is_empty());
    assert_eq!(form.category, FeedbackCategory::Bug);
    assert_eq!(form.result, Some(FeedbackResult::Sent));
    assert!(!form.sending);
}

#[test]
fn not_sent_keeps_the_message() {
    let mut form = general("Hello");
    form.sending = true;
    not_sent(&mut form, FeedbackRefusal::Timeout);
    assert_eq!(form.message, "Hello");
    assert_eq!(
        form.result,
        Some(FeedbackResult::NotSent(FeedbackRefusal::Timeout))
    );
    assert!(!form.sending);
    let mut form = general("Hello");
    form.sending = true;
    dropped(&mut form);
    assert!(!form.sending);
    assert_eq!(form.result, None);
}

#[test]
fn shell_report_carries_exactly_the_shown_attachment() {
    let mut form = general("Hello\n");
    form.attachment =
        Attachment::Ready("translationCore 1 (x) · Linux x86_64\nno Report\n".to_owned());
    form.name = "  Ana \u{FEFF}".to_owned();
    form.email = " a@b.c ".to_owned();
    let report = shell_report(&form).unwrap();
    assert_eq!(report.category, "General Feedback");
    assert_eq!(report.message, "Hello\n");
    assert_eq!(report.name, "Ana");
    assert_eq!(report.email, "a@b.c");
    assert_eq!(
        report.attachment,
        "translationCore 1 (x) · Linux x86_64\nno Report\n"
    );
    assert!(report.version.starts_with("translationCore "));
    form.attachment = Attachment::Loading;
    assert!(shell_report(&form).is_none());
}
