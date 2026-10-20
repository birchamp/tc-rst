//! The confirmations of a release upgrade and of a new checking language (unit C8): what the
//! dialogs hold while a plan is made, shown, confirmed again after the project moved, or refused
//! by the disk, and the words they say. tC4's `UpgradeSet.jsx` and `GatewayChange.jsx`.

use tc_api::check::{CheckCollection, Tool};
use tc_api::ids::{PlanId, ProjectRef};
use tc_api::resources::{
    BlockReason, BlockedUnit, Carry, GatewayPackage, GatewayRef, PlanRow, PlanView, PlanWhat,
    RepoMove, Seen, SetOffer, SetRung, TextKind, TextOffer, UpdatesView,
};
use tc_api::shown::Direction;
use tc_core::BookId;
use tc_ui::app::upgrades::{
    GatewayPreview, GatewayState, Installing, UpgradeState, blocked_text, gateway_headline,
    row_text, text_headline,
};

fn package() -> GatewayPackage {
    GatewayPackage {
        gateway: GatewayRef {
            language: "en".into(),
            owner: "unfoldingWord".into(),
        },
        name: "English".into(),
        autonym: "English".into(),
        direction: Direction::Ltr,
        seen: Seen {
            tn: "v89".into(),
            tw: "v89".into(),
            ta: "v89".into(),
        },
    }
}

fn change(repo: &str) -> RepoMove {
    RepoMove {
        repo: repo.into(),
        from: "v18".into(),
        to: "v19".into(),
        published: None,
    }
}

fn row(tool: Tool, book: BookId, decisions: u32, carry: Option<(u32, u32)>) -> PlanRow {
    PlanRow {
        tool,
        unit: CheckCollection::Book(book),
        decisions,
        against: "es-419_twl".into(),
        carry: carry.map(|(carried, check_again)| Carry {
            carried,
            check_again,
        }),
    }
}

fn plan(id: u64, what: PlanWhat, rows: Vec<PlanRow>, blocked: Vec<BlockedUnit>) -> PlanView {
    PlanView {
        plan: PlanId::new(id),
        what,
        decisions_at_risk: rows.iter().map(|r| r.decisions).sum(),
        rows,
        blocked,
        alignments: None,
    }
}

fn set_plan(id: u64, rows: Vec<PlanRow>) -> PlanView {
    plan(
        id,
        PlanWhat::Set {
            rung: SetRung::Primary,
            language: "es-419".into(),
            moves: vec![change("es-419_twl")],
        },
        rows,
        Vec::new(),
    )
}

fn project(name: &str) -> ProjectRef {
    ProjectRef::parse(name).unwrap()
}

#[test]
fn an_answer_for_a_project_the_person_has_left_is_dropped() {
    let mut up = UpgradeState::default();
    up.check_started();
    let view = UpdatesView {
        sets: Vec::new(),
        texts: Vec::new(),
        texts_error: None,
    };
    up.offers_read(&project("alef"), &project("bet"), view.clone());
    assert!(
        up.offers.is_none() && up.checking,
        "still waiting for the right answer"
    );
    up.offers_read(&project("bet"), &project("bet"), view);
    assert!(up.offers.is_some() && !up.checking);
}

#[test]
fn a_text_that_door43_could_not_answer_for_is_said_beside_the_set_offers() {
    let mut up = UpgradeState::default();
    up.check_started();
    let view = UpdatesView {
        sets: vec![SetOffer {
            rung: SetRung::Primary,
            language: "es-419".into(),
            moves: vec![change("es-419_tn")],
        }],
        texts: Vec::new(),
        texts_error: Some("Door43 did not answer".into()),
    };
    up.offers_read(&project("alef"), &project("alef"), view);
    assert_eq!(
        up.error.as_deref(),
        Some("Could not check for updates: Door43 did not answer")
    );
    assert_eq!(
        up.offers.as_ref().unwrap().view.sets.len(),
        1,
        "the set offers stand"
    );
}

#[test]
fn a_download_that_fails_leaves_the_error_and_no_confirmation() {
    let mut up = UpgradeState::default();
    up.install_started(Installing::Set(SetRung::Primary));
    up.install_progress("es-419_tn".into());
    assert_eq!(up.progress.as_deref(), Some("es-419_tn"));
    up.install_failed(Some("the upgrade was not applied: no route".into()));
    assert!(up.installing.is_none() && up.progress.is_none() && up.preview.is_none());
    assert!(up.error.is_some());
    // A cancelled download is no error.
    up.install_started(Installing::Text("el-x-koine_ugnt".into()));
    assert!(up.error.is_none(), "a new try clears the old error");
    up.install_failed(None);
    assert!(up.error.is_none());
}

#[test]
fn confirming_sends_one_request_and_a_blocked_plan_is_never_sent() {
    let mut up = UpgradeState::default();
    up.plan_ready(set_plan(1, Vec::new()));
    assert!(up.confirm_started());
    assert!(!up.confirm_started(), "a second press waits for the first");
    up.cancel();
    assert!(
        up.preview.is_some(),
        "the dialog cannot be closed under a save"
    );
    up.apply_failed("The disk is full.".into());
    assert!(up.confirm_started(), "after a failure Confirm tries again");

    let blocked = plan(
        2,
        PlanWhat::Set {
            rung: SetRung::Fallback,
            language: "en".into(),
            moves: Vec::new(),
        },
        Vec::new(),
        vec![BlockedUnit {
            tool: Tool::TranslationWords,
            unit: CheckCollection::Book(BookId::Titus),
            reason: BlockReason::NoResource,
        }],
    );
    let mut up = UpgradeState::default();
    up.plan_ready(blocked);
    assert!(!up.confirm_started());
}

#[test]
fn a_failed_save_holds_the_preview_with_its_reason() {
    let mut up = UpgradeState::default();
    up.plan_ready(set_plan(
        1,
        vec![row(Tool::TranslationWords, BookId::Titus, 2, Some((2, 0)))],
    ));
    assert!(up.confirm_started());
    up.apply_failed("The change was not saved: the disk is full.".into());
    let preview = up.preview.as_ref().unwrap();
    assert!(!preview.applying);
    assert_eq!(
        preview.plan.plan,
        PlanId::new(1),
        "the same plan stays for the retry"
    );
    assert!(up.error.is_some());
    up.cancel();
    assert!(up.preview.is_none() && up.error.is_none());
}

#[test]
fn a_stale_confirm_replaces_the_plan_shown_with_the_one_made_again() {
    // tC4 #595: the person reads the new figures before anything is written.
    let mut up = UpgradeState::default();
    up.plan_ready(set_plan(
        1,
        vec![row(Tool::TranslationWords, BookId::Titus, 2, Some((2, 0)))],
    ));
    assert!(up.confirm_started());
    up.replanned(set_plan(
        2,
        vec![row(Tool::TranslationWords, BookId::Titus, 2, Some((1, 1)))],
    ));
    let preview = up.preview.as_ref().unwrap();
    assert_eq!(preview.plan.plan, PlanId::new(2));
    assert!(!preview.applying, "it waits for the person's own confirm");

    let mut gw = GatewayState::default();
    assert!(gw.asking());
    gw.planned(package(), set_plan(1, Vec::new()));
    assert!(gw.confirm_started());
    gw.replanned(set_plan(2, Vec::new()));
    assert!(!gw.busy);
    match &gw.preview {
        Some(GatewayPreview::Plan { plan, .. }) => assert_eq!(plan.plan, PlanId::new(2)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_set_that_was_upgraded_is_current_and_a_text_that_was_upgraded_is_not_offered_again() {
    let mut up = UpgradeState::default();
    let view = UpdatesView {
        sets: vec![SetOffer {
            rung: SetRung::Primary,
            language: "es-419".into(),
            moves: vec![change("es-419_twl")],
        }],
        texts: vec![TextOffer {
            kind: TextKind::Original,
            change: change("el-x-koine_ugnt"),
        }],
        texts_error: None,
    };
    up.offers_read(&project("alef"), &project("alef"), view);
    up.plan_ready(set_plan(1, Vec::new()));
    up.applied();
    let offers = up.offers.as_ref().unwrap();
    assert!(
        offers.view.sets[0].moves.is_empty(),
        "the set is up to date"
    );
    assert_eq!(offers.view.texts.len(), 1, "the other offers stand");
    up.plan_ready(plan(
        3,
        PlanWhat::Text {
            kind: TextKind::Original,
            change: change("el-x-koine_ugnt"),
        },
        Vec::new(),
        Vec::new(),
    ));
    up.applied();
    assert!(up.offers.as_ref().unwrap().view.texts.is_empty());
    assert!(up.preview.is_none());
}

#[test]
fn a_second_choice_of_a_language_does_nothing_while_one_is_previewed_or_under_way() {
    // tC4 #580, interruptions 1 to 3.
    let mut gw = GatewayState::default();
    assert!(gw.asking());
    assert!(!gw.asking(), "under way");
    gw.planned(package(), set_plan(1, Vec::new()));
    assert!(!gw.asking(), "previewed");
    assert!(gw.confirm_started());
    gw.cancel();
    assert!(
        gw.preview.is_some(),
        "a change that was confirmed cannot be cancelled"
    );
    gw.apply_failed("The disk is full.".into());
    assert_eq!(gw.error.as_deref(), Some("The disk is full."));
    assert!(
        gw.preview.is_some(),
        "the dialog holds the preview after a failed save"
    );
    gw.cancel();
    assert!(gw.preview.is_none() && gw.error.is_none());
    assert!(gw.asking());
}

#[test]
fn a_plan_that_could_not_be_made_shows_its_error_and_never_a_change_button() {
    let mut gw = GatewayState::default();
    assert!(gw.asking());
    gw.plan_failed(package(), "es-419_tn: it is not installed".into());
    assert!(matches!(gw.preview, Some(GatewayPreview::Failed { .. })));
    assert!(
        !gw.confirm_started(),
        "a failed read is never mistaken for success"
    );
    assert!(!gw.busy);
}

#[test]
fn the_gateway_headline_names_the_books_and_counts_as_tc4_does() {
    let titus = row(Tool::TranslationWords, BookId::Titus, 2, Some((2, 0)));
    let notes = row(Tool::TranslationNotes, BookId::Titus, 1, Some((0, 1)));
    let (headline, detail) = gateway_headline(&set_plan(1, vec![titus.clone(), notes]));
    assert_eq!(
        headline,
        "3 decisions in Titus were made against the notes you are leaving."
    );
    assert!(
        detail
            .unwrap()
            .starts_with("Decisions that match a check in the new notes carry over.")
    );
    let (one, _) = gateway_headline(&set_plan(
        1,
        vec![row(Tool::TranslationNotes, BookId::Jonah, 1, None)],
    ));
    assert_eq!(
        one,
        "1 decision in Jonah were made against the notes you are leaving."
    );
    let many: Vec<PlanRow> = [
        BookId::Genesis,
        BookId::Exodus,
        BookId::Leviticus,
        BookId::Numbers,
        BookId::Ruth,
    ]
    .into_iter()
    .map(|b| row(Tool::TranslationNotes, b, 1, None))
    .collect();
    assert_eq!(
        gateway_headline(&set_plan(1, many)).0,
        "5 decisions in Genesis, Exodus, Leviticus and 2 more were made against the notes you are leaving."
    );
    let (harmless, none) = gateway_headline(&set_plan(1, Vec::new()));
    assert_eq!(
        harmless,
        "Nothing you have already checked will be affected."
    );
    assert!(none.is_none());
}

#[test]
fn a_row_says_what_carried_over_or_how_many_decisions_it_was_checked_against() {
    assert_eq!(
        row_text(&row(Tool::TranslationWords, BookId::Titus, 2, Some((2, 0)))),
        "Titus · Translation Words: 2 carried over, 0 to check again"
    );
    assert_eq!(
        row_text(&row(Tool::TranslationNotes, BookId::Titus, 4, None)),
        "Titus · Translation Notes — 4 decision(s) checked against es-419_twl"
    );
}

#[test]
fn the_blocked_callout_gives_the_remedy_of_the_first_reason() {
    let unit = |reason| BlockedUnit {
        tool: Tool::TranslationNotes,
        unit: CheckCollection::Book(BookId::Titus),
        reason,
    };
    assert_eq!(
        blocked_text(&[unit(BlockReason::NoResource)]),
        "This change cannot be applied: Titus (Translation Notes) already carry checking \
         decisions, and the new language does not cover them. Install a suite that covers \
         these books first. Nothing was changed."
    );
    let text = blocked_text(&[
        unit(BlockReason::NoResource),
        unit(BlockReason::VersificationUnknown),
    ]);
    assert!(
        text.contains("versification scheme could not be identified"),
        "{text}"
    );
}

#[test]
fn a_text_upgrade_says_how_many_verses_it_marks_or_that_nothing_changes() {
    assert_eq!(
        text_headline(TextKind::Original, Some(12)),
        (
            "The alignments of 12 verse(s) will be marked invalid.".to_owned(),
            true
        )
    );
    assert_eq!(
        text_headline(TextKind::Original, Some(0)),
        ("No alignment will change.".to_owned(), false)
    );
    assert_eq!(
        text_headline(TextKind::Gateway, None).0,
        "No alignment and no check decision will change. The source pane will show the new text."
    );
}
