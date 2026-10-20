//! The endpoint list (APP-DESIGN §3.3, §3.9): one row per endpoint, the phase-0 set and the
//! rows of the units that have landed since.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use tc_api::{ALL, Endpoint, Kind, NetKind};

/// The endpoints of phase 0 (APP-DESIGN §3.9 and §10): those of the app, prefs, net, jobs,
/// account and feedback services that units A2 to A9 serve, and the test hooks they use; then
/// the rows of later units: C1b's draft offer and copy.
const PHASE_0: &[&str] = &[
    "app/hello",
    "app/quit",
    "app/clientError",
    "prefs/setLocale",
    "prefs/setDraftUnit",
    "prefs/setAlignSuggestions",
    "prefs/recordPlace",
    "net/state",
    "net/set",
    "jobs/get",
    "jobs/cancel",
    "account/state",
    "account/signIn",
    "account/cancelSignIn",
    "account/resume",
    "account/signOut",
    "account/profileLink",
    "account/helpLink",
    "feedback/compose",
    "session/drafts",
    "session/keepDraft",
    "session/discardDraft",
    "session/copy",
    "session/otherComputerClosed",
    // B2: Home and its dialogs.
    "home/list",
    "home/card",
    "home/versifications",
    "home/languages",
    // B3: open and Understand.
    "session/open",
    "session/close",
    "editor/book",
    "editor/sources",
    "helps/passage",
    "helps/article",
    // B9b: the share dialog's first step, the repository link, and Open from Door43.
    "account/repoLink",
    "account/orgs",
    "door43/open",
    // B5a: the Check tool picker, a tool's rail and the guided fix.
    "check/picker",
    "check/open",
    "check/rows",
    // B7: the stories of an Open Bible Stories project, their helps and their checks.
    "story/list",
    "story/open",
    "helps/story",
    "helps/storyArticle",
    "check/storyItem",
    "resources/fixState",
    // B10: the sync of a shared project, and the scripted outcomes of the open and the sync.
    "door43/sync",
    // B9a: Project settings from Home, and Get source texts with its downloads.
    "settings/viewFromHome",
    "resources/sources",
    "resources/catalog",
    "resources/download",
    "test/quiet",
    "test/echo",
    "test/crash",
    "test/hold",
    "test/release",
    "test/fieldStore",
    "test/saveVerse",
    "test/fieldLog",
    "test/fieldDraft",
    "test/keychain",
    "test/reset",
    "test/lastEdit",
    "test/script",
];

#[test]
fn the_list_is_the_phase_0_endpoints_each_once() {
    let paths: Vec<&str> = ALL.iter().map(|e| e.path).collect();
    let unique: BTreeSet<&str> = paths.iter().copied().collect();
    assert_eq!(unique.len(), paths.len(), "a path is listed twice");
    assert_eq!(unique, PHASE_0.iter().copied().collect());
}

#[test]
fn every_path_is_service_slash_name() {
    for endpoint in ALL {
        let (service, name) = endpoint.path.split_once('/').unwrap();
        assert!(
            !service.is_empty()
                && service
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
            "{}",
            endpoint.path
        );
        assert!(
            name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && name.chars().all(|c| c.is_ascii_alphanumeric()),
            "{}",
            endpoint.path
        );
    }
}

#[test]
fn the_list_and_the_impls_agree() {
    use tc_api::account::{HelpLink, Orgs, ProfileLink, RepoLink, Resume, SignIn};
    use tc_api::door43::{OpenDoor43, SyncProject};
    assert_eq!(<SignIn as Endpoint>::NET, Some(NetKind::SignIn));
    assert_eq!(<Resume as Endpoint>::NET, Some(NetKind::CheckSignIn));
    assert_eq!(<ProfileLink as Endpoint>::NET, Some(NetKind::Profile));
    assert_eq!(<HelpLink as Endpoint>::NET, Some(NetKind::HelpLink));
    assert_eq!(<RepoLink as Endpoint>::NET, Some(NetKind::RepoLink));
    assert_eq!(<Orgs as Endpoint>::NET, Some(NetKind::Share));
    assert_eq!(<OpenDoor43 as Endpoint>::NET, Some(NetKind::Open));
    assert_eq!(<OpenDoor43 as Endpoint>::KIND, Kind::StartJob);
    assert_eq!(<SyncProject as Endpoint>::NET, Some(NetKind::Sync));
    assert_eq!(<SyncProject as Endpoint>::KIND, Kind::StartJob);
    assert_eq!(
        <tc_api::resources::Download as Endpoint>::NET,
        Some(NetKind::Sources)
    );
    assert_eq!(
        <tc_api::resources::Download as Endpoint>::KIND,
        Kind::StartJob
    );
    assert_eq!(<tc_api::app::Hello as Endpoint>::KIND, Kind::Query);
    for endpoint in ALL {
        let asks = endpoint.net.is_some();
        let expected = matches!(
            endpoint.path,
            "account/signIn"
                | "account/resume"
                | "account/profileLink"
                | "account/helpLink"
                | "account/repoLink"
                | "account/orgs"
                | "door43/open"
                | "door43/sync"
                | "resources/catalog"
                | "resources/download"
        );
        assert_eq!(
            asks, expected,
            "{} asks the internet question",
            endpoint.path
        );
    }
}

#[test]
fn no_endpoint_asks_the_upload_question() {
    // The owner's ruling Q2 removed the Upload changes dialog; tC4's key stays.
    assert!(ALL.iter().all(|e| e.net != Some(NetKind::Upload)));
}
