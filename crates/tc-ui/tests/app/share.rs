//! The share dialog's first two steps and Open a project from Door43 (unit B9b): the rules of
//! tC4's `sh` and `od`, and the port of `test/openFromDoor43Dialog.test.tsx` (its clicks and
//! drawing are the journeys' and the parity captures').

use tc_api::account::{OrgChoice, ShareTargets, WebUrl};
use tc_api::door43::{RepoName, RepoSource, ShareOwner, SharedRepo};
use tc_api::endpoint::NetKind;
use tc_api::home::{BookTileView, Door43Card, HomeCard, ProjectKind};
use tc_api::ids::ProjectRef;
use tc_api::ids::RequestId;
use tc_api::jobs::SharePhase;
use tc_api::shown::{Direction, Known};
use tc_core::BookId;
use tc_ui::app::internet::{InternetFlow, NetStep, Pending};
use tc_ui::app::open_door43::{OpenForm, RepoError, refusal_key};
use tc_ui::app::share::{
    Choices, ShareForm, ShareProject, ShareStep, SharedItems, Target, name_ok,
};
use tc_ui::i18n::keys;

const HOST: &str = "git.door43.org";

fn od(repo: &str) -> OpenForm {
    OpenForm {
        repo: repo.into(),
        ..OpenForm::new(tc_api::ids::RequestId::from_bytes([1; 16]))
    }
}

/// Ruling H14: the open asks for no name; the repository alone makes the request.
#[test]
fn open_project_sends_the_repository_alone_and_one_on_another_server_cannot_be_opened() {
    let source = od("anna_k/titus-team-draft").ready(HOST).unwrap();
    assert_eq!(source.to_string(), "anna_k/titus-team-draft");
    let other = od("https://example.org/anna_k/titus-team-draft");
    assert_eq!(other.ready(HOST), None);
    assert_eq!(other.repo_error(HOST), Some(RepoError::Bad));
    assert_eq!(od("").repo_error(HOST), None, "an empty box shows no error");
}

#[test]
fn while_the_open_runs_open_project_is_off() {
    let mut form = od("anna_k/titus-team-draft");
    form.submit.press();
    assert_eq!(form.ready(HOST), None);
}

#[test]
fn a_change_takes_the_last_refusal_away() {
    let mut form = od("anna_k/titus-team-draft");
    form.refused(
        Some("open-door43.no-journal"),
        "no journal".into(),
        tc_api::ids::RequestId::from_bytes([2; 16]),
    );
    form.type_repo("anna_k/titus-team-draft2".into());
    assert_eq!(form.error, None);
}

/// Ruling H14: a project already here is named, Open project can be pressed again, and a new
/// repository takes the offer away.
#[test]
fn a_project_already_here_is_offered_until_the_repository_changes() {
    let mut form = od("anna_k/titus-team-draft");
    form.submit.press();
    let here = ProjectRef::parse("titus-team-draft").unwrap();
    form.already_here(here.clone(), tc_api::ids::RequestId::from_bytes([2; 16]));
    assert_eq!(form.yours, Some(here));
    assert_eq!(form.refusal(), None, "it is no failure");
    assert!(!form.busy());
    form.type_repo("anna_k/other".into());
    assert_eq!(form.yours, None);
}

#[test]
fn a_refusal_shows_its_code_and_each_code_its_own_sentence() {
    let mut form = od("anna_k/titus-team-draft");
    form.refused(
        Some("sync.incoming-invalid"),
        "1 journal file fails".into(),
        tc_api::ids::RequestId::from_bytes([2; 16]),
    );
    let refusal = form.refusal().unwrap();
    assert_eq!(refusal.code.as_deref(), Some("sync.incoming-invalid"));
    assert_eq!(refusal.reason, "1 journal file fails");

    let own = [
        ("sync.incoming-invalid", keys::open_door43::refused::INVALID),
        (
            "open-door43.no-journal",
            keys::open_door43::refused::NO_JOURNAL,
        ),
        (
            "sync.clone-path-exists",
            keys::open_door43::refused::CLONE_PATH_EXISTS,
        ),
        (
            "opslog.not-loaded",
            keys::open_door43::refused::OPS_NOT_LOADED,
        ),
        ("sync.offline", keys::open_door43::refused::OFFLINE),
        ("sync.move-failed", keys::open_door43::refused::MOVE_FAILED),
    ];
    for (code, key) in own {
        assert_eq!(refusal_key(Some(code)), key, "{code}");
    }
    // Every other code, and none, read Q-21's sentence, not tC4's "make sure that the
    // repository is public" (Q3); a story project is no refusal here (Q2).
    for code in [
        Some("sync.clone-failed"),
        Some("sync.obs-unsupported"),
        None,
    ] {
        assert_eq!(
            refusal_key(code),
            keys::open_door43::refused::NOT_OPENED,
            "{code:?}"
        );
    }
    let mut distinct: Vec<_> = own.iter().map(|(_, key)| *key).collect();
    distinct.push(keys::open_door43::refused::NOT_OPENED);
    distinct.sort_by_key(|key| key.as_str());
    distinct.dedup();
    assert_eq!(distinct.len(), own.len() + 1);
}

#[test]
fn open_asks_the_internet_question_of_an_open() {
    let mut flow = InternetFlow::Idle;
    assert_eq!(flow.ask(false, Pending::OpenDoor43), NetStep::Nothing);
    assert_eq!(flow.asking(), Some(NetKind::Open));
    assert_eq!(flow.not_now(), NetStep::Dropped(Pending::OpenDoor43));
}

// ---- the share dialog ------------------------------------------------------------------

fn card(kind: ProjectKind, tag: Known<String>) -> HomeCard {
    let tile = |book: BookId| BookTileView {
        book,
        drafted: Known::Unknown,
        conflicts: 0,
    };
    HomeCard {
        project: ProjectRef::parse("sample_burrito").unwrap(),
        name: "Muestra".into(),
        tag,
        dir: Direction::Ltr,
        kind,
        door43: Door43Card::Local,
        books: match kind {
            ProjectKind::Bible => Known::Known(vec![
                tile("JON".parse().unwrap()),
                tile("TIT".parse().unwrap()),
            ]),
            ProjectKind::Stories => Known::Known(vec![]),
        },
        stories: None,
        conflicts: Known::Unknown,
    }
}

fn org(name: &str, can_create: bool, recommended: bool) -> OrgChoice {
    OrgChoice {
        name: name.into(),
        full_name: name.into(),
        can_create,
        recommended,
    }
}

fn rid(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

fn opened() -> ShareForm {
    ShareForm::open(
        ShareProject::of(&card(ProjectKind::Bible, Known::Known("es-419".into()))),
        rid(1),
    )
}

#[test]
fn a_share_is_the_whole_project_its_books_or_its_stories() {
    let bible = ShareProject::of(&card(ProjectKind::Bible, Known::Known("es-419".into())));
    assert_eq!(
        bible.items,
        SharedItems::Books(vec!["JON".parse().unwrap(), "TIT".parse().unwrap()])
    );
    assert_eq!(bible.language, "es-419");
    let stories = ShareProject::of(&card(ProjectKind::Stories, Known::Unknown));
    assert_eq!(stories.items, SharedItems::Stories);
    assert_eq!(stories.language, "", "an unknown tag recommends nothing");
}

#[test]
fn the_dialog_opens_on_where_it_goes_reading_the_organizations() {
    let form = opened();
    assert_eq!(form.step, ShareStep::Target);
    assert_eq!(form.choices, Choices::Loading);
    assert_eq!(form.target, Target::User);
    assert_eq!(
        form.name, "sample_burrito",
        "the folder's name until another is typed"
    );
}

#[test]
fn next_waits_for_the_organizations_or_for_the_failure_to_read_them() {
    let mut form = opened();
    assert!(!form.can_next());
    form.go(ShareStep::Check);
    assert_eq!(form.step, ShareStep::Target);
    form.choices_read(Err("Door43 cannot be reached.".into()));
    assert!(
        form.can_next(),
        "the person's own account can still be chosen"
    );
    form.choices_read(Ok(ShareTargets {
        user: "facilitator-zq".into(),
        orgs: vec![org("Equipo-Ejemplo", true, true)],
    }));
    assert!(form.can_next());
    form.go(ShareStep::Check);
    assert_eq!(form.step, ShareStep::Check);
    form.go(ShareStep::Target);
    assert_eq!(form.step, ShareStep::Target);
}

#[test]
fn an_organization_the_person_cannot_create_in_cannot_be_chosen() {
    let mut form = opened();
    form.choices_read(Ok(ShareTargets {
        user: "facilitator-zq".into(),
        orgs: vec![
            org("Equipo-Ejemplo", true, true),
            org("es-419_gl", false, false),
        ],
    }));
    form.pick(Target::Organization("es-419_gl".into()), rid(2));
    assert_eq!(form.target, Target::User);
    form.pick(Target::Organization("Equipo-Ejemplo".into()), rid(2));
    assert_eq!(form.target_org(), Some("Equipo-Ejemplo"));
    form.pick(Target::Organization("nobody".into()), rid(2));
    assert_eq!(form.target_org(), Some("Equipo-Ejemplo"));
    form.pick(Target::User, rid(2));
    assert_eq!(form.target_org(), None);
}

#[test]
fn share_needs_a_name_door43_accepts() {
    for good in ["sample_burrito", "a", "en_ult.v2", "A-1", "  titus  "] {
        assert!(name_ok(good), "{good:?}");
    }
    for bad in ["", " ", "-a", "a-", ".a", "a.", "a b", "a/b", "ñ"] {
        assert!(!name_ok(bad), "{bad:?}");
    }
    let mut form = opened();
    assert!(form.can_submit());
    form.type_name("my repo".into(), rid(2));
    assert!(!form.can_submit() && form.name_invalid());
    form.type_name(String::new(), rid(3));
    assert!(
        !form.can_submit() && !form.name_invalid(),
        "empty is not shown as invalid"
    );
}

#[test]
fn share_asks_the_share_question() {
    let project = ShareProject::of(&card(ProjectKind::Bible, Known::Unknown));
    assert_eq!(Pending::Share(Box::new(project)).kind(), NetKind::Share);
}

fn on_check() -> ShareForm {
    let mut form = opened();
    form.choices_read(Ok(ShareTargets {
        user: "facilitator-zq".into(),
        orgs: vec![org("Equipo-Ejemplo", true, true)],
    }));
    form.go(ShareStep::Check);
    form
}

fn shared_repo() -> SharedRepo {
    SharedRepo {
        repo: RepoSource::new("facilitator-zq", "sample_burrito").unwrap(),
        url: WebUrl::parse("https://git.door43.org/facilitator-zq/sample_burrito").unwrap(),
    }
}

#[test]
fn share_sends_one_request_per_attempt_and_shows_each_progress_line() {
    let mut form = on_check();
    form.pick(Target::Organization("Equipo-Ejemplo".into()), rid(2));
    let (request, owner, name) = form.run().expect("Share runs");
    assert_eq!(request, rid(2));
    assert_eq!(
        owner,
        ShareOwner::Organization(RepoName::parse("Equipo-Ejemplo").unwrap())
    );
    assert_eq!(name.as_str(), "sample_burrito");
    assert_eq!(form.step, ShareStep::Running(SharePhase::Preparing));
    assert_eq!(
        form.run(),
        None,
        "a second press while it runs sends nothing"
    );
    assert!(!form.can_submit());
    form.progress(SharePhase::Create);
    form.progress(SharePhase::Create);
    form.progress(SharePhase::Push);
    assert_eq!(form.step, ShareStep::Running(SharePhase::Push));
    assert_eq!(form.shown, vec![SharePhase::Create, SharePhase::Push]);
    form.shared(shared_repo());
    assert_eq!(form.step, ShareStep::Done(shared_repo()));
    assert_eq!(
        form.shown,
        vec![SharePhase::Create, SharePhase::Push],
        "the done step keeps the lines"
    );
    let mut quick = on_check();
    quick.run().unwrap();
    quick.progress(SharePhase::Push);
    assert_eq!(
        quick.shown,
        vec![SharePhase::Create, SharePhase::Push],
        "a create no report showed is a line too"
    );
    let mut unseen = on_check();
    unseen.run().unwrap();
    unseen.shared(shared_repo());
    assert_eq!(
        unseen.shown,
        vec![SharePhase::Create, SharePhase::Push],
        "a share that ended before any report showed went through both steps"
    );
    form.go(ShareStep::Check);
    assert_eq!(
        form.step,
        ShareStep::Done(shared_repo()),
        "Back is not on the done step"
    );
}

#[test]
fn a_refusal_returns_to_the_check_step_with_its_sentence_and_the_next_share_is_new() {
    let mut form = on_check();
    let (first, ..) = form.run().unwrap();
    form.progress(SharePhase::Create);
    form.refused(
        Some("share.name-exists"),
        "That name exists on this account or organization; pick another.".into(),
        rid(9),
    );
    assert_eq!(form.step, ShareStep::Check);
    assert_eq!(
        form.error.as_ref().and_then(|e| e.code.as_deref()),
        Some("share.name-exists")
    );
    form.type_name("sample_burrito_2".into(), rid(10));
    assert_eq!(form.error, None, "typing takes the refusal away");
    let (second, _, name) = form.run().unwrap();
    assert_ne!(second, first);
    assert_eq!(name.as_str(), "sample_burrito_2");
}

#[test]
fn a_lost_reply_sends_the_same_request_again_and_not_now_sends_nothing() {
    let mut form = on_check();
    let (first, ..) = form.run().unwrap();
    form.lost("the connection to the app was lost".into());
    assert_eq!(form.step, ShareStep::Check);
    assert!(form.error.is_some());
    assert_eq!(
        form.run().map(|(r, ..)| r),
        Some(first),
        "the reply cache answers it (A-27)"
    );
    form.not_sent();
    assert_eq!(form.step, ShareStep::Check);
    assert_eq!(form.run().map(|(r, ..)| r), Some(first));
    form.interrupted(rid(7));
    assert_eq!(
        form.run().map(|(r, ..)| r),
        Some(rid(7)),
        "after the sign-in step, a new attempt"
    );
}

#[test]
fn the_done_step_names_team_access_for_every_project() {
    for kind in [ProjectKind::Bible, ProjectKind::Stories] {
        let form = ShareForm::open(ShareProject::of(&card(kind, Known::Unknown)), rid(1));
        assert_eq!(
            form.done_lines(),
            vec![
                keys::share_dialog::DONE_TEXT,
                keys::share_dialog::TEAM_ACCESS
            ],
            "{kind:?}: teammates need write access to sync, stories too (#724)"
        );
    }
}

#[test]
fn the_run_asks_the_share_question() {
    assert_eq!(Pending::ShareRun.kind(), NetKind::Share);
}

#[test]
fn a_push_refused_after_the_repository_was_made_never_offers_a_second_share() {
    let mut form = on_check();
    form.run().unwrap();
    form.progress(SharePhase::Push);
    form.push_refused(
        Some("sync.unreachable"),
        "Door43 cannot be reached.".into(),
        rid(8),
    );
    assert_eq!(form.step, ShareStep::Check);
    assert!(form.error.is_some(), "the sync's sentence shows");
    form.type_name("another_name".into(), rid(9));
    assert!(
        !form.can_submit(),
        "another name would move the project to a second repository"
    );
    assert_eq!(form.run(), None);
}
