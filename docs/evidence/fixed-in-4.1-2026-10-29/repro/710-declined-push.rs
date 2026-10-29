//! #710 repro: Door43's recorded reply to a push without write access (HTTP 200, the
//! pre-receive hook's `error: User permission denied for writing.` on side band 2 and
//! `ng <ref> pre-receive hook declined`; docs/evidence/door43-live-2026-10-17.md step 6) reaches
//! the sync as `sync.no-write-access`. The repository's permissions say the account may push
//! (as when write access is taken away after the check), so the refusal comes only from the
//! push: real `git receive-pack` in tc-door43-fake runs a hook that prints Gitea's exact lines.
//!
//! Goes at crates/tc-sync/tests/repro_710.rs. Run:
//!   tc-heavy cargo test -p tc-sync --test repro_710 repro_710 -- --nocapture

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code, unused_imports)]

#[path = "sync/door43.rs"]
mod door43;

use door43::*;
use tc_door43_fake::seeded;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repro_710_door43s_declined_push_is_no_write_access() {
    let name = "titus-revoked";
    let door43 = door43(name).await;
    let dir = tempfile::tempdir().unwrap();
    let ana = computer(dir.path(), "ana");
    let folder = titus(&ana);
    let a = open(&ana, &folder);
    connect(&a, &door43, name);
    synced(&sync(&a, &door43).await);
    let bare = door43.fake.git_dir(seeded::USER, name);
    let tip = git2::Repository::open(&bare)
        .unwrap()
        .refname_to_id("refs/heads/main")
        .unwrap();
    // Gitea's `fail()`: an empty line on stdout, then three `error:` lines on stderr.
    let hook = bare.join("hooks/pre-receive");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(
        &hook,
        "#!/bin/sh\ncat >/dev/null\necho\nprintf 'error:\\nerror: User permission denied for writing.\\nerror:\\n' >&2\nexit 1\n",
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    save(&a, 1, 1, "Paul, a servant of God");
    let report = sync(&a, &door43).await;
    println!("#710 sync ended: {:?}", report.ended);
    let failure = match &report.ended {
        tc_sync::machine::Ended::Failed(f) => f.clone(),
        other => panic!("not refused: {other:?}"),
    };
    let refusal = match &failure {
        tc_api::refusal::SyncFailure::BeforeIntake(r) | tc_api::refusal::SyncFailure::AfterIntake(r) => r.clone(),
        other => panic!("{other:?}"),
    };
    let code = tc_api::refusal::Message::code(&failure);
    println!("#710 refusal code: {code:?}");
    assert!(
        matches!(
            &refusal,
            tc_api::refusal::SyncRefusal::NoWriteAccess { server_message }
                if server_message == "User permission denied for writing."
        ),
        "{refusal:?}"
    );
    let after = git2::Repository::open(&bare)
        .unwrap()
        .refname_to_id("refs/heads/main")
        .unwrap();
    assert_eq!(after, tip, "Door43's main did not move");
    assert_eq!(words(&a, 1, 1).as_deref(), Some("Paul, a servant of God"), "the save is kept");
    a.close().unwrap();
}
