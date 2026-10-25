//! Save work for a teammate and Receive work from a file (E2): the dialogs' rules, driven on
//! the host. The stick sentence needs a verified export; a browser file over 4 GiB is refused
//! before it is sent; one request per attempt.

use tc_api::ApiError;
use tc_api::error::Flow;
use tc_api::exchange::{
    ExchangeFile, ExportedView, HandEditOutcome, HandEdited, IdentifyView, ReceivedView,
};
use tc_api::ids::{ProjectRef, RequestId};
use tc_api::jobs::ExchangePhase;
use tc_api::refusal::{ExchangeRefusal, Refusal};
use tc_api::shown::{PersonLabel, Who};
use tc_ui::app::exchange::{
    ExchangeProject, ExportForm, ExportStep, ReceiveForm, ReceiveStep, failure_text, fits_upload,
};
use tc_ui::app::jobs::JobEnd;
use tc_ui::i18n::{keys, t};

fn rid(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

fn titus() -> ExchangeProject {
    ExchangeProject {
        project: ProjectRef::parse("titus").unwrap(),
        name: "Tito".into(),
    }
}

fn exported() -> ExportedView {
    ExportedView {
        path: "/media/stick/tit-2026-10-25-ana.zip".into(),
    }
}

/// INV-REMOVE-THE-STICK-ONLY-AFTER-VERIFY: the sentence shows once the export ended verified,
/// never while it works or after it failed.
#[test]
fn the_stick_sentence_needs_a_verified_export() {
    let mut ex = ExportForm::open(titus(), rid(1));
    assert!(!ex.can_save(), "no folder yet");
    assert_eq!(ex.stick_sentence(), None);
    ex.chose_folder(Some("/media/stick".into()), rid(2));
    assert!(ex.can_save());
    let (request, destination) = ex.press().unwrap();
    assert_eq!((request, destination.as_str()), (rid(2), "/media/stick"));
    assert_eq!(ex.press(), None, "one request per attempt");
    ex.progress(rid(2), ExchangePhase::Verify);
    assert_eq!(ex.step, ExportStep::Working(Some(ExchangePhase::Verify)));
    assert_eq!(
        ex.stick_sentence(),
        None,
        "INV-REMOVE-THE-STICK-ONLY-AFTER-VERIFY"
    );
    assert!(ex.working_line().is_some());
    ex.ended(rid(2), JobEnd::Done(exported()), rid(3));
    let sentence = ex.stick_sentence().unwrap();
    assert!(sentence.contains("remove the stick"), "{sentence}");
    assert!(sentence.contains("tit-2026-10-25-ana.zip"), "{sentence}");
}

#[test]
fn a_failed_export_says_why_and_the_next_press_is_a_new_attempt() {
    let mut ex = ExportForm::open(titus(), rid(1));
    ex.chose_folder(Some("/media/stick".into()), rid(2));
    ex.press().unwrap();
    ex.ended(
        rid(2),
        JobEnd::Failed(ApiError::Refused(Refusal::Exchange(
            ExchangeRefusal::VerifyFailed,
        ))),
        rid(3),
    );
    let ExportStep::Failed(text) = &ex.step else {
        panic!("{:?}", ex.step);
    };
    assert_eq!(text, &t(keys::exchange::error::VERIFY_FAILED, &[]));
    assert_eq!(ex.stick_sentence(), None);
    assert_eq!(ex.submit.request(), rid(3), "a new attempt");
    assert!(ex.can_save(), "Save again, or another folder");
    assert_eq!(ex.press().map(|(r, _)| r), Some(rid(3)));
}

/// INV-REMOVE-THE-STICK-ONLY-AFTER-VERIFY: a job of an earlier attempt (a dialog closed and
/// opened again while it ran) never ends on the new form.
#[test]
fn an_older_attempts_end_never_reaches_a_new_form() {
    let mut ex = ExportForm::open(titus(), rid(1));
    ex.chose_folder(Some("/media/stick".into()), rid(2));
    ex.press().unwrap();
    let mut again = ExportForm::open(titus(), rid(5));
    again.chose_folder(Some("/media/other".into()), rid(6));
    again.ended(rid(2), JobEnd::Done(exported()), rid(7));
    assert_eq!(
        again.stick_sentence(),
        None,
        "INV-REMOVE-THE-STICK-ONLY-AFTER-VERIFY"
    );
    assert_eq!(again.step, ExportStep::Choosing);
    again.progress(rid(2), ExchangePhase::Write);
    assert_eq!(again.step, ExportStep::Choosing);
}

/// A-27: a lost reply leaves the next press to send the same request, which the reply cache
/// answers with the job that ran; a refusal makes the next press a new attempt.
#[test]
fn a_lost_reply_resends_the_same_request() {
    let mut ex = ExportForm::open(titus(), rid(1));
    ex.chose_folder(Some("/media/stick".into()), rid(2));
    assert_eq!(ex.press().map(|(r, _)| r), Some(rid(2)));
    ex.reply_lost(rid(2));
    assert_eq!(ex.step, ExportStep::Choosing);
    assert_eq!(ex.press().map(|(r, _)| r), Some(rid(2)), "the same request");
    let mut rx = ReceiveForm::open(titus(), rid(1));
    rx.chose_file(ExchangeFile::Path("/media/stick/tit.zip".into()), rid(2));
    let view = IdentifyView {
        name: "Tito".into(),
        project: Some(ProjectRef::parse("titus").unwrap()),
    };
    rx.identified(
        &ExchangeFile::Path("/media/stick/tit.zip".into()),
        Ok(view.clone()),
    );
    assert_eq!(rx.press().map(|(r, _)| r), Some(rid(2)));
    rx.reply_lost(rid(2));
    assert_eq!(rx.step, ReceiveStep::Confirm(view));
    assert_eq!(rx.press().map(|(r, _)| r), Some(rid(2)), "the same request");
}

/// An identify answer for a file chosen earlier, or one that comes after the press, changes
/// nothing.
#[test]
fn an_identify_answer_for_another_file_is_ignored() {
    let mut rx = ReceiveForm::open(titus(), rid(1));
    let first = ExchangeFile::Path("/media/stick/first.zip".into());
    let second = ExchangeFile::Path("/media/stick/second.zip".into());
    rx.chose_file(first.clone(), rid(2));
    rx.chose_file(second.clone(), rid(3));
    rx.identified(
        &first,
        Ok(IdentifyView {
            name: "Rut".into(),
            project: None,
        }),
    );
    assert_eq!(
        rx.step,
        ReceiveStep::Identifying,
        "the first file's answer is stale"
    );
    rx.identified(
        &second,
        Ok(IdentifyView {
            name: "Tito".into(),
            project: Some(ProjectRef::parse("titus").unwrap()),
        }),
    );
    assert!(rx.can_receive());
}

/// A-28: STORAGE's cap for the whole file, checked before anything is sent.
#[test]
fn an_exchange_file_over_4_gib_is_refused_before_it_is_sent() {
    let gib = 1024f64 * 1024.0 * 1024.0;
    assert!(fits_upload(4.0 * gib));
    assert!(!fits_upload(4.0 * gib + 1.0));
    let mut rx = ReceiveForm::open(titus(), rid(1));
    rx.too_large();
    assert_eq!(
        rx.step,
        ReceiveStep::Failed(t(keys::exchange::receive::TOO_LARGE, &[]))
    );
    assert!(!rx.can_receive());
}

#[test]
fn a_receive_confirms_the_files_project_then_shows_what_it_brought() {
    let mut rx = ReceiveForm::open(titus(), rid(1));
    assert_eq!(rx.press(), None);
    let file = ExchangeFile::Path("/media/stick/tit.zip".into());
    rx.chose_file(file.clone(), rid(2));
    assert_eq!(rx.step, ReceiveStep::Identifying);
    rx.identified(
        &file,
        Ok(IdentifyView {
            name: "Tito".into(),
            project: Some(ProjectRef::parse("titus").unwrap()),
        }),
    );
    assert!(rx.can_receive());
    let (request, file) = rx.press().unwrap();
    assert_eq!(request, rid(2));
    assert_eq!(file, ExchangeFile::Path("/media/stick/tit.zip".into()));
    rx.progress(rid(2), ExchangePhase::TakeIn);
    rx.ended(
        rid(2),
        JobEnd::Done(ReceivedView {
            received: 3,
            from: vec![PersonLabel {
                who: Who::Username("juan".into()),
                computer: None,
            }],
            kept_newest: 0,
            verse_conflicts: 1,
            alignments: 0,
            hand_edited: vec![HandEdited {
                path: "ingredients/TIT.usfm".into(),
                outcome: HandEditOutcome::NotTakenIn,
            }],
        }),
        rid(3),
    );
    let lines = rx.received_lines();
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(
        lines[0].contains('3') && lines[0].contains("juan"),
        "{lines:?}"
    );
    assert!(lines[1].contains('1'), "{lines:?}");
    assert!(lines[2].contains("ingredients/TIT.usfm"), "{lines:?}");
}

#[test]
fn a_file_of_another_project_is_refused_before_the_receive() {
    let mut rx = ReceiveForm::open(titus(), rid(1));
    let file = ExchangeFile::Path("/media/stick/rut.zip".into());
    rx.chose_file(file.clone(), rid(2));
    rx.identified(
        &file,
        Ok(IdentifyView {
            name: "Rut".into(),
            project: None,
        }),
    );
    assert_eq!(
        rx.step,
        ReceiveStep::Failed(t(
            keys::exchange::receive::OTHER_PROJECT,
            &[("name", &"Rut"), ("project", &"Tito")]
        ))
    );
    assert!(!rx.can_receive());
    assert!(rx.choosing(), "another file may be chosen");
}

#[test]
fn a_busy_sync_or_exchange_has_its_own_sentence() {
    assert_eq!(
        failure_text(&ApiError::Busy(Flow::Sync)),
        t(keys::exchange::error::BUSY_SYNC, &[])
    );
    assert_eq!(
        failure_text(&ApiError::Busy(Flow::ExchangeReceive)),
        t(keys::exchange::error::BUSY_EXCHANGE, &[])
    );
    assert_eq!(
        failure_text(&ApiError::Refused(Refusal::Exchange(
            ExchangeRefusal::Damaged
        ))),
        t(keys::exchange::error::DAMAGED, &[])
    );
}
