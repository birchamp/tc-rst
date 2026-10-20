//! The guided fix's screen (M11; B5b wires its three ways): one way at a time, what each says
//! while it runs and when it fails, and answers that come for a screen no longer shown. tC4's
//! `openFix`, `patchFix`, `fixFetch`, `fixSideload` and `fixRepin`.

use tc_api::check::Tool;
use tc_api::resources::{FixView, PinLabel, SetRung};
use tc_ui::app::fix::{self, Fix, FixScreen, Way};
use tc_ui::i18n::{keys, t};

fn view(candidates_error: Option<&str>) -> FixView {
    FixView {
        pin: PinLabel {
            repo_path: "git.door43.org/es-419_gl/es-419_twl".into(),
            version: Some("v18".into()),
            sha: "c".repeat(40),
        },
        rung: SetRung::Primary,
        candidates: vec![PinLabel {
            repo_path: "git.door43.org/es-419_gl/es-419_twl".into(),
            version: Some("v19".into()),
            sha: "e".repeat(40),
        }],
        candidates_error: candidates_error.map(str::to_owned),
    }
}

fn screen() -> FixScreen {
    FixScreen::new(7, Tool::TranslationWords, view(None))
}

/// tC4 states an unreadable install list at once, never "no other version is installed".
#[test]
fn installed_copies_that_could_not_be_read_are_said_when_the_screen_opens() {
    let opened = FixScreen::new(1, Tool::TranslationWords, view(Some("the disk is busy")));
    assert_eq!(opened.error.as_deref(), Some("the disk is busy"));
    assert_eq!(opened.busy, None);
    assert_eq!(screen().error, None);
}

/// One way at a time (tC4 `fix.busy`): a second press while one runs does nothing, so a download
/// and a file install never race to install one pin.
#[test]
fn a_way_does_not_start_while_another_is_on_its_way() {
    let mut s = screen();
    assert!(s.start_fetch());
    assert_eq!(s.busy, Some(Way::Fetch));
    assert!(!s.start_sideload("es-419_twl.zip"));
    assert!(!s.start_repin());
    assert!(!s.start_fetch());
    assert_eq!(s.busy, Some(Way::Fetch));
    s.stopped();
    assert!(s.start_repin());
    assert_eq!(s.busy, Some(Way::Repin));
}

/// The download names the repository it fetches, in tC4's `sources.progress`; a file install
/// names the file (`fix.sideload.reading`).
#[test]
fn a_running_way_says_what_it_is_doing() {
    let mut s = screen();
    s.start_fetch();
    assert_eq!(
        s.progress,
        Some(t(keys::sources::PROGRESS, &[("repo", &"es-419_twl")]))
    );
    s.fetching("es-419_tw");
    assert_eq!(
        s.progress,
        Some(t(keys::sources::PROGRESS, &[("repo", &"es-419_tw")]))
    );
    s.stopped();
    // A late progress report of a download that ended changes nothing.
    s.fetching("es-419_tw");
    assert_eq!(s.progress, None);
    s.start_sideload("twl.zip");
    assert_eq!(
        s.progress,
        Some(t(keys::fix::sideload::READING, &[("name", &"twl.zip")]))
    );
}

/// A failed download or file install states that nothing was installed and why, and frees the
/// other ways; a re-pin that cannot be planned says so as a failed upgrade (tC4 `applyOffer`).
#[test]
fn a_failure_is_said_and_leaves_every_way_at_hand() {
    let mut s = screen();
    s.start_sideload("twl.zip");
    s.install_failed("the file declares another commit");
    assert_eq!(
        s.error,
        Some(t(
            keys::fix::FAILED,
            &[("reason", &"the file declares another commit")]
        ))
    );
    assert_eq!((s.busy, s.progress.as_deref()), (None, None));
    // Starting again clears the failure.
    assert!(s.start_repin());
    assert_eq!(s.error, None);
    s.repin_failed("that version is not installed here");
    assert_eq!(
        s.error,
        Some(t(
            keys::upgrade::FAILED,
            &[("reason", &"that version is not installed here")]
        ))
    );
    assert!(s.start_fetch());
}

/// An answer for a screen that was closed, or closed and opened again, changes nothing (tC4's
/// `fix.id`).
#[test]
fn an_answer_changes_only_the_screen_it_started_from() {
    let mut shown = Some(Fix::Ready(Box::new(screen())));
    assert_eq!(
        fix::patch(&mut shown, 7, FixScreen::start_fetch),
        Some(true)
    );
    // The person closed it and opened it again: a new screen.
    let reopened = FixScreen::new(8, Tool::TranslationWords, view(None));
    shown = Some(Fix::Ready(Box::new(reopened.clone())));
    assert_eq!(
        fix::patch(&mut shown, 7, |s| s.install_failed("offline")),
        None
    );
    assert_eq!(shown, Some(Fix::Ready(Box::new(reopened))));
    let mut closed: Option<Fix> = None;
    assert_eq!(fix::patch(&mut closed, 8, FixScreen::stopped), None);
    let mut loading = Some(Fix::Loading(Tool::TranslationNotes));
    assert_eq!(fix::patch(&mut loading, 8, FixScreen::stopped), None);
    assert_eq!(
        loading.as_ref().map(Fix::tool),
        Some(Tool::TranslationNotes)
    );
}
