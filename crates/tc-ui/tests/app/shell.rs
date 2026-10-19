//! The shell's decisions (tC4's `App.jsx`, unit A5): the project chip's initials, the account
//! button's state, the screen each view draws, and where a reload returns to.

use tc_api::account::{AccountView, KeepState, KeychainState};
use tc_api::app::{AppPrefsView, BuildInfo, HelloView, OpenSession, SessionRole};
use tc_api::event::{SessionMode, StartupProgress, StartupState};
use tc_api::ids::{Epoch, LocaleId, ProjectId, ProjectRef, Scope, ServerInstance};
use tc_api::shown::Direction;
use tc_ui::app::boot::{StartupAction, startup_action};
use tc_ui::app::shell::{
    AccountState, CurrentProject, NAV_KEY, NavSnapshot, ProjectFlavor, Screen, View, account_state,
    boundary_key, in_project, js_trim, project_initials, restore, screen, user_initials,
};

/// Computed once by running tC4's expression in node:
/// `(name || '?').split(/\s+/).map((w) => w[0]).slice(0, 2).join('').toUpperCase()`.
#[test]
fn project_initials_match_tc4() {
    for (name, initials) in [
        ("Equipo Ejemplo — Tito y Jonás", "EE"),
        ("", "?"),
        ("   ", ""),
        (" Equipo Ejemplo", "E"),
        ("Titus", "T"),
        ("ß straße", "SSS"),
        ("a\u{3000}b", "AB"),
        // U+0085 is not JavaScript's \s: one word.
        ("a\u{85}b", "A"),
        ("ñandú árbol", "ÑÁ"),
        // U+FEFF is \s: an empty first word.
        ("\u{FEFF}x y", "X"),
        ("x\u{A0}y", "XY"),
        ("a\u{2028}b", "AB"),
        // U+180E left \s with Unicode 6.3.
        ("a\u{180E}b", "A"),
        ("\u{1680}q r", "Q"),
        ("a\tb\nc", "AB"),
        ("a  b", "AB"),
        ("مرحبا بك", "مب"),
        ("日本語", "日"),
    ] {
        assert_eq!(project_initials(name), initials, "{name:?}");
    }
}

/// A declared difference: tC4's `w[0]` gives half of a surrogate pair ("\ud835D" for this
/// name in node), a broken glyph; the rebuild gives the whole character.
#[test]
fn astral_initial_is_whole() {
    assert_eq!(project_initials("𝔄bc def"), "𝔄D");
}

#[test]
fn js_trim_is_javascripts() {
    assert_eq!(js_trim("\u{FEFF} a \u{3000}"), "a");
    assert_eq!(js_trim("\u{85}a"), "\u{85}a");
}

#[test]
fn account_states_follow_tc4() {
    let view = |user: Option<&str>, kept| AccountView {
        user: user.map(str::to_owned),
        kept,
        notice: None,
        server: "git.door43.org".to_owned(),
        keychain: KeychainState::Works,
    };
    let signed_in = view(Some("jdoe"), KeepState::Kept);
    assert_eq!(account_state(&signed_in), AccountState::In("jdoe"));
    assert_eq!(account_state(&signed_in).data_state(), "in");
    assert_eq!(account_state(&signed_in).label(), "Door43 account: @jdoe");
    let saved = view(None, KeepState::Kept);
    assert_eq!(account_state(&saved), AccountState::Saved);
    assert_eq!(
        account_state(&saved).label(),
        "Door43 account: sign-in saved"
    );
    for kept in [KeepState::NotKept, KeepState::NoKeychain] {
        let out = view(None, kept);
        assert_eq!(account_state(&out), AccountState::Out);
        assert_eq!(account_state(&out).data_state(), "out");
        assert_eq!(account_state(&out).label(), "Door43 account: not signed in");
    }
    assert_eq!(user_initials("jdoe"), "JD");
    assert_eq!(user_initials("x"), "X");
}

fn project(flavor: ProjectFlavor, epoch: u64) -> CurrentProject {
    CurrentProject {
        at: Scope {
            project: ProjectRef::parse("sample_burrito").unwrap(),
            epoch: Epoch::new(epoch),
        },
        id: ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap(),
        name: "Equipo Ejemplo".to_owned(),
        language_tag: "es-419".to_owned(),
        dir: Direction::Ltr,
        flavor,
    }
}

#[test]
fn screen_is_tc4s_main_view() {
    let bible = project(ProjectFlavor::TextTranslation, 1);
    let obs = project(ProjectFlavor::TextStories, 1);
    let rows = [
        (View::Home, None, Screen::Home),
        (View::Read, None, Screen::Nothing),
        (View::Draft, None, Screen::Nothing),
        (View::Check, None, Screen::Nothing),
        (View::Publish, None, Screen::Nothing),
        (View::Home, Some(&bible), Screen::Home),
        (View::Read, Some(&bible), Screen::Understand),
        (View::Draft, Some(&bible), Screen::Draft),
        (View::Check, Some(&bible), Screen::Check),
        (View::Publish, Some(&bible), Screen::CommunityChecking),
        (View::Home, Some(&obs), Screen::Home),
        (View::Read, Some(&obs), Screen::StoryUnderstand),
        (View::Draft, Some(&obs), Screen::StoryDraft),
        (View::Check, Some(&obs), Screen::Check),
        (View::Publish, Some(&obs), Screen::CommunityChecking),
    ];
    for (view, project, expected) in rows {
        assert_eq!(screen(view, project), expected, "{view:?} {project:?}");
    }
    assert!(!in_project(View::Home, Some(&bible)));
    assert!(!in_project(View::Read, None));
    assert!(in_project(View::Check, Some(&obs)));
}

#[test]
fn views_have_tc4s_names() {
    for (view, name) in [
        (View::Home, "home"),
        (View::Read, "read"),
        (View::Draft, "draft"),
        (View::Check, "check"),
        (View::Publish, "publish"),
    ] {
        assert_eq!(view.as_str(), name);
        assert_eq!(View::parse(name), Some(view));
        assert_eq!(serde_json::to_value(view).unwrap(), name);
    }
    assert_eq!(View::parse("understand"), None);
}

fn hello(instance: ServerInstance, sessions: Vec<OpenSession>) -> HelloView {
    HelloView {
        api: 1,
        build: BuildInfo {
            version: "4.0.0".to_owned(),
            commit: "dev".to_owned(),
        },
        instance,
        internet: false,
        account: AccountView {
            user: None,
            kept: KeepState::NotKept,
            notice: None,
            server: "git.door43.org".to_owned(),
            keychain: KeychainState::Works,
        },
        sessions,
        prefs: AppPrefsView {
            app_locale: LocaleId::ENGLISH,
        },
        startup: StartupState::Ready,
    }
}

fn session(p: &CurrentProject, role: SessionRole) -> OpenSession {
    OpenSession {
        at: p.at.clone(),
        id: p.id.clone(),
        mode: SessionMode::Writable,
        role,
    }
}

fn snapshot(instance: ServerInstance, view: View, project: Option<CurrentProject>) -> String {
    serde_json::to_string(&NavSnapshot {
        instance,
        view,
        project,
    })
    .unwrap()
}

#[test]
fn restore_needs_the_same_run_and_an_open_screen_session() {
    let run = ServerInstance::from_bytes([1; 16]);
    let other = ServerInstance::from_bytes([2; 16]);
    let bible = project(ProjectFlavor::TextTranslation, 3);
    let home = (View::Home, None);
    let open = hello(run, vec![session(&bible, SessionRole::Screen)]);

    let good = snapshot(run, View::Check, Some(bible.clone()));
    assert_eq!(
        restore(Some(&good), &open),
        (View::Check, Some(bible.clone()))
    );

    assert_eq!(restore(None, &open), home);
    assert_eq!(restore(Some("{not json"), &open), home);
    assert_eq!(restore(Some("null"), &open), home);
    // Another run: its opens are gone.
    assert_eq!(
        restore(Some(&good), &hello(other, open.sessions.clone())),
        home
    );
    // The project is not open, is open again under another epoch, or only as a side session.
    assert_eq!(restore(Some(&good), &hello(run, vec![])), home);
    let reopened = project(ProjectFlavor::TextTranslation, 4);
    assert_eq!(
        restore(
            Some(&good),
            &hello(run, vec![session(&reopened, SessionRole::Screen)])
        ),
        home
    );
    assert_eq!(
        restore(
            Some(&good),
            &hello(run, vec![session(&bible, SessionRole::Side)])
        ),
        home
    );
    // A view with no project is Home.
    assert_eq!(
        restore(Some(&snapshot(run, View::Draft, None)), &open),
        home
    );
    assert_eq!(restore(Some(&snapshot(run, View::Home, None)), &open), home);
}

#[test]
fn nav_snapshot_round_trips_and_keeps_unknown_fields_out() {
    let run = ServerInstance::from_bytes([9; 16]);
    let snap = NavSnapshot {
        instance: run,
        view: View::Read,
        project: Some(project(ProjectFlavor::TextStories, 2)),
    };
    let text = serde_json::to_string(&snap).unwrap();
    assert_eq!(serde_json::from_str::<NavSnapshot>(&text).unwrap(), snap);
    // A later build's field reads.
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["chapter"] = serde_json::json!(4);
    assert_eq!(serde_json::from_value::<NavSnapshot>(value).unwrap(), snap);
    assert_eq!(NAV_KEY, "tc4.nav.v1");
}

#[test]
fn the_boundary_key_is_the_view_and_the_folder() {
    let a = project(ProjectFlavor::TextTranslation, 1);
    let b = project(ProjectFlavor::TextTranslation, 2);
    assert_eq!(
        boundary_key(View::Read, Some(&a)),
        boundary_key(View::Read, Some(&b))
    );
    assert_ne!(
        boundary_key(View::Read, Some(&a)),
        boundary_key(View::Draft, Some(&a))
    );
    assert_ne!(
        boundary_key(View::Home, None),
        boundary_key(View::Home, Some(&a))
    );
}

#[test]
fn a_page_that_boots_before_the_resources_are_ready_goes_to_prepare() {
    assert_eq!(startup_action(&StartupState::Ready), StartupAction::Proceed);
    assert_eq!(
        startup_action(&StartupState::Preparing(StartupProgress {
            done: 1,
            total: 9
        })),
        StartupAction::Prepare
    );
    assert_eq!(
        startup_action(&StartupState::Failed {
            reason: "x".to_owned()
        }),
        StartupAction::Prepare
    );
}
