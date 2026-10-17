//! The words of every refusal (APP-DESIGN §3.5): each key resolves in every language with
//! the placeholders its variant fills, every tC4 refusal code has a variant or is listed as
//! unused, no two sync refusals read the same, and the rebuild's own keys never shadow
//! tC4's and are translated before a release.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::samples;
use common::{
    Resolved, Resolver, assert_every_variant, placeholders, rebuild, rebuild_dir, vendored,
};
use tc_api::NetKind;
use tc_api::account::AccountNotice;
use tc_api::feedback::FeedbackCategory;
use tc_api::ids::LOCALE_IDS;
use tc_api::refusal::{FeedbackRefusal, Message, Refusal, SyncBlock, SyncFailure, SyncRefusal};

/// tC4 refusal codes in the vendored catalogs that no rebuild refusal produces, each with
/// the reason (DELTA §3.15.2 and the storage design they follow).
const UNUSED_TC4_CODES: &[(&str, &str)] = &[
    // tC4's journal internals. The rebuild's journal reports these situations as STORAGE
    // notices and open results, never as refusals of an operation (STORAGE §2.6, §6.1).
    (
        "segment.invalid",
        "tC4 journal segments; the rebuild salvages a damaged pack (STORAGE §4.4)",
    ),
    (
        "segment.misnamed",
        "tC4 journal segments; packs are named by content (STORAGE §4.2)",
    ),
    (
        "segment.foreign-actor",
        "tC4 actors; a copied folder rotates its device (STORAGE §6.3)",
    ),
    ("segment.bad-path", "tC4 journal segments"),
    ("segment.differs-from-accepted", "tC4 journal segments"),
    (
        "outbox.invalid",
        "tC4's outbox; the rebuild has pending batches (STORAGE §5.0)",
    ),
    ("outbox.different-action", "tC4's outbox"),
    (
        "actor.record-mismatch",
        "tC4 actors; device identity is STORAGE §6.3",
    ),
    ("actor.name-already-set", "tC4 actors"),
    ("ledger.unreadable", "tC4's ledger; the rebuild has none"),
    (
        "journal.clock-not-ratcheted",
        "tC4's clock ratchet; the rebuild orders by parents",
    ),
    (
        "open.unexplained-divergence",
        "an outside edit becomes changes or a held file (STORAGE §11)",
    ),
    (
        "open.story-divergence",
        "an outside edit becomes changes or a held file (STORAGE §11)",
    ),
    (
        "open.scope-unreadable",
        "the book list is a fold projection, never a separate read",
    ),
    (
        "story.base-missing",
        "a deleted story file is rebuilt from the fold (STORAGE §6.2)",
    ),
    (
        "seed.mismatch",
        "tC4's first-open seeding; the rebuild imports instead (unit I)",
    ),
    (
        "seed.publish-failed",
        "tC4's first-open seeding; the rebuild imports instead (unit I)",
    ),
    (
        "checkpoint.divergence",
        "a changed file is recorded or held before a checkpoint (STORAGE §11)",
    ),
    (
        "checkpoint.scope-mismatch",
        "checkpoints commit the fold's projection (STORAGE §5.13)",
    ),
    (
        "checkpoint.incomplete-inputs",
        "checkpoints commit the fold's projection (STORAGE §5.13)",
    ),
    (
        "checkpoint.metadata-unwritable",
        "metadata.json is written from the fold (STORAGE §5.10)",
    ),
    (
        "checkpoint.unsafe-path",
        "paths are checked when a change is made (STORAGE §3.3)",
    ),
    (
        "checkpoint.too-deep",
        "paths are checked when a change is made (STORAGE §3.3)",
    ),
    // tC4's sync mechanism, which the rebuild does not port (PLAN, DELTA §3.15.2).
    (
        "sync.own-file-differs",
        "own packs are immutable; a damaged copy is repaired (STORAGE §9.2 N3)",
    ),
    (
        "sync.incoming-invalid",
        "a damaged incoming file is salvaged (PLAN Q4)",
    ),
    (
        "sync.metadata-overlay",
        "metadata.json is written from the fold",
    ),
    ("sync.obs-unsupported", "OBS projects sync (PLAN Q2)"),
    ("sync.not-main", "any default branch syncs (PLAN Q10)"),
    ("sync.clone-path-exists", "no scratch clone"),
    ("sync.move-failed", "no scratch clone"),
    ("sync.verify-failed", "no scratch clone"),
    (
        "sync.push-failed",
        "a refused push is RefRejected with Door43's words",
    ),
    (
        "sync.failed",
        "an uncoded failure shows its reason in syncResult.error.other",
    ),
    ("sync.project-replaced", "no scratch clone"),
    ("sync.apply-pending", "no apply marker"),
    ("sync.cleanup-failed", "no scratch clone"),
    ("opslog.not-loaded", "no ops log"),
    (
        "open-door43.pending",
        "no open marker; an unfinished open is removed at start (§6.4 N1)",
    ),
];

/// The other keys the UI shows from this crate's types.
fn other_keys() -> Vec<(&'static str, BTreeSet<String>)> {
    let mut keys: Vec<(&'static str, BTreeSet<String>)> = Vec::new();
    for kind in NetKind::ALL {
        keys.push((kind.ask_key(), BTreeSet::new()));
    }
    for notice in [
        AccountNotice::SignOutFailed,
        AccountNotice::SavedRefused,
        AccountNotice::SavedUnchecked,
    ] {
        keys.push((notice.key(), BTreeSet::new()));
    }
    for category in [
        FeedbackCategory::General,
        FeedbackCategory::Content,
        FeedbackCategory::Bug,
    ] {
        keys.push((category.key(), BTreeSet::new()));
    }
    for failure in samples::sync() {
        if let Some(note) = failure.kept_note() {
            keys.push((note, BTreeSet::new()));
        }
    }
    keys
}

#[test]
fn every_refusal_family_has_a_sample_of_every_variant() {
    assert_every_variant(&samples::open());
    assert_every_variant(&samples::create());
    assert_every_variant(&samples::import());
    assert_every_variant(&samples::export());
    assert_every_variant(&samples::sync_blocks());
    assert_every_variant(&samples::sync_refusals());
    assert_every_variant(&samples::sync());
    assert_every_variant(&samples::share());
    assert_every_variant(&samples::open_door43());
    assert_every_variant(&samples::sign_in());
    assert_every_variant(&samples::resources());
    assert_every_variant(&samples::exchange());
    assert_every_variant(&samples::feedback());
    assert_every_variant(&samples::prefs());
    assert_every_variant(&samples::refusals());
}

#[test]
fn every_refusal_has_a_key_in_every_locale() {
    let resolver = Resolver::load();
    let mut wanted: Vec<(String, &'static str, BTreeSet<String>)> = samples::refusals()
        .iter()
        .map(|refusal| {
            let vars = refusal.vars().into_iter().map(|(name, _)| name.to_owned());
            (format!("{refusal:?}"), refusal.key(), vars.collect())
        })
        .collect();
    wanted.extend(
        other_keys()
            .into_iter()
            .map(|(key, vars)| (key.to_owned(), key, vars)),
    );
    let mut problems = Vec::new();
    for (what, key, vars) in &wanted {
        for locale in LOCALE_IDS {
            let text = match resolver.resolve(locale, key) {
                Resolved::Translated(text) | Resolved::English(text) => text,
                Resolved::Missing => {
                    problems.push(format!("{locale}: {key} ({what}) has no text"));
                    continue;
                }
            };
            let found = placeholders(text);
            if &found != vars {
                problems.push(format!(
                    "{locale}: {key} ({what}) has placeholders {found:?}, the variant fills {vars:?}"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn no_tc4_code_without_a_variant() {
    let produced: BTreeSet<&str> = samples::refusals()
        .iter()
        .filter_map(|r| r.code())
        .collect();
    let unused: BTreeMap<&str, &str> = UNUSED_TC4_CODES.iter().copied().collect();
    assert_eq!(
        unused.len(),
        UNUSED_TC4_CODES.len(),
        "a code is listed twice"
    );
    let catalogs = vendored();
    let mut codes = BTreeSet::new();
    for catalog in catalogs.values() {
        codes.extend(catalog.keys().filter_map(|k| k.strip_prefix("refusal.")));
    }
    assert!(codes.len() >= 50, "the scan found {} codes", codes.len());
    let orphans: Vec<_> = codes
        .iter()
        .filter(|code| !produced.contains(*code) && !unused.contains_key(*code))
        .collect();
    assert!(
        orphans.is_empty(),
        "tC4 refusal codes with no variant and no line in UNUSED_TC4_CODES: {orphans:?}"
    );
    let used_but_listed: Vec<_> = produced
        .iter()
        .filter(|c| unused.contains_key(*c))
        .collect();
    assert!(
        used_but_listed.is_empty(),
        "codes listed as unused that a variant produces: {used_but_listed:?}"
    );
    let stale: Vec<_> = unused.keys().filter(|c| !codes.contains(*c)).collect();
    assert!(
        stale.is_empty(),
        "listed codes tC4's catalogs lack: {stale:?}"
    );
}

/// The keys DELTA §3.15.2 gives to more than one sync refusal.
const SHARED_SYNC_KEYS: &[(&str, &[&str])] = &[
    (
        "syncResult.error.authFailed",
        &["notSignedIn", "authFailed"],
    ),
    (
        "syncResult.error.cloneFailed",
        &["unreachable", "remoteMissing"],
    ),
    (
        "syncResult.error.notSent",
        &["unreachable", "remoteMissing"],
    ),
    ("syncResult.error.other", &["refRejected", "io"]),
];

#[test]
fn no_two_sync_refusals_share_a_message() {
    let resolver = Resolver::load();
    let mut by_key: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for failure in samples::sync() {
        let wire = serde_json::to_value(&failure).unwrap();
        let variant = common::variant_of(&wire["value"]);
        by_key.entry(failure.key()).or_default().insert(variant);
    }
    let allowed: BTreeMap<&str, BTreeSet<String>> = SHARED_SYNC_KEYS
        .iter()
        .map(|(key, variants)| (*key, variants.iter().map(|v| v.to_string()).collect()))
        .collect();
    for (key, variants) in &by_key {
        if variants.len() > 1 {
            let ok = allowed.get(key).is_some_and(|a| variants.is_subset(a));
            assert!(
                ok,
                "{key} is shown for {variants:?}, which DELTA §3.15.2 does not join"
            );
        }
    }
    // Distinct keys read differently, so a message names its refusal.
    let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
    for key in by_key.keys() {
        let text = resolver
            .english(key)
            .unwrap_or_else(|| panic!("{key} has no English text"));
        if let Some(other) = seen.insert(text, key) {
            panic!("{key} and {other} read the same: {text:?}");
        }
    }
}

/// The words that tell the person nothing happened. tC4's sentences and the rebuild's say
/// it in these forms (`syncResult.error.cloneFailed`, `pushFailed`, `needsNewerApp`).
const NOTHING_CHANGED: &[&str] = &["nothing changed", "nothing was changed", "did not change"];

#[test]
fn a_failure_after_intake_never_says_nothing_changed_and_says_the_work_was_kept() {
    let resolver = Resolver::load();
    let kept = resolver.english("syncResult.receivedKept").unwrap();
    let mut after = 0;
    for failure in samples::sync() {
        let SyncFailure::AfterIntake(_) = failure else {
            continue;
        };
        after += 1;
        let sentence = resolver.english(failure.key()).unwrap();
        let note = failure
            .kept_note()
            .map(|note| resolver.english(note).unwrap());
        let banner = format!("{sentence} {}", note.unwrap_or_default()).to_lowercase();
        for words in NOTHING_CHANGED {
            assert!(!banner.contains(words), "{failure:?} reads {banner:?}");
        }
        assert!(
            sentence.contains("received and kept") || note == Some(kept),
            "{failure:?} does not say the received work was kept"
        );
        // One sentence says it, never two.
        assert!(
            !(sentence.contains("received and kept") && note.is_some()),
            "{failure:?} says it twice"
        );
    }
    assert_eq!(after, samples::sync_refusals().len());
    // Before intake there is nothing to tell.
    for failure in samples::sync() {
        if !matches!(failure, SyncFailure::AfterIntake(_)) {
            assert_eq!(failure.kept_note(), None, "{failure:?}");
        }
    }
}

#[test]
fn a_block_is_always_a_failure_before_intake() {
    // A block can only be a failure before intake, so its "Nothing changed" is true.
    let resolver = Resolver::load();
    for block in samples::sync_blocks() {
        let failure = SyncFailure::Blocked(block);
        assert_eq!(failure.kept_note(), None);
        let wire = serde_json::to_value(&failure).unwrap();
        assert_eq!(wire["type"], "blocked", "{wire}");
        assert!(resolver.english(failure.key()).is_some(), "{failure:?}");
    }
}

#[test]
fn sync_refusals_keep_delta_table_keys_and_tc4_codes() {
    let before = SyncFailure::BeforeIntake;
    let not_shared = SyncFailure::Blocked(SyncBlock::NotShared);
    assert_eq!(
        (not_shared.key(), not_shared.code()),
        ("syncResult.error.notShared", Some("sync.not-shared"))
    );
    let cases = [
        (
            SyncRefusal::Offline,
            "syncResult.error.offline",
            Some("sync.offline"),
        ),
        (
            SyncRefusal::AuthFailed,
            "syncResult.error.authFailed",
            Some("sync.auth-failed"),
        ),
        (
            SyncRefusal::NoWriteAccess {
                server_message: "x".into(),
            },
            "syncResult.error.noWriteAccess",
            Some("sync.no-write-access"),
        ),
        (
            SyncRefusal::Busy { attempts: 3 },
            "syncResult.error.nonFastForward",
            Some("sync.non-fast-forward"),
        ),
        (
            SyncRefusal::Unreachable,
            "syncResult.error.cloneFailed",
            Some("sync.clone-failed"),
        ),
        (
            SyncRefusal::RemoteMissing,
            "syncResult.error.cloneFailed",
            Some("sync.clone-failed"),
        ),
        (
            SyncRefusal::RefRejected {
                server_message: "hook declined".into(),
            },
            "syncResult.error.other",
            None,
        ),
    ];
    for (refusal, key, code) in cases {
        let failure = before(refusal);
        assert_eq!((failure.key(), failure.code()), (key, code), "{failure:?}");
    }
    let refused = before(SyncRefusal::RefRejected {
        server_message: "hook declined".into(),
    });
    assert_eq!(refused.vars(), vec![("reason", "hook declined".to_owned())]);
    let no_access = before(SyncRefusal::NoWriteAccess {
        server_message: "user does not have write access".into(),
    });
    assert_eq!(no_access.detail(), Some("user does not have write access"));
}

#[test]
fn a_refusal_delegates_to_its_family() {
    for refusal in samples::refusals() {
        let (code, key, vars) = match &refusal {
            Refusal::Sync(f) => (f.code(), f.key(), f.vars()),
            Refusal::Open(f) => (f.code(), f.key(), f.vars()),
            Refusal::Export(f) => (f.code(), f.key(), f.vars()),
            _ => continue,
        };
        assert_eq!(
            (refusal.code(), refusal.key(), refusal.vars()),
            (code, key, vars)
        );
    }
}

#[test]
fn feedback_reasons_from_the_shell_are_read() {
    for refusal in samples::feedback() {
        let reason = refusal.key().strip_prefix("feedback.reason.").unwrap();
        assert_eq!(FeedbackRefusal::from_shell_reason(reason), Some(refusal));
    }
    assert_eq!(FeedbackRefusal::from_shell_reason("sent"), None);
}

#[test]
fn rebuild_keys_never_shadow_vendored_keys() {
    let tc4: BTreeSet<String> = vendored()
        .into_values()
        .flat_map(|catalog| catalog.into_keys())
        .collect();
    for (locale, catalog) in rebuild() {
        let shadowed: Vec<_> = catalog.keys().filter(|k| tc4.contains(*k)).collect();
        assert!(
            shadowed.is_empty(),
            "{locale}: rebuild keys that tC4 has: {shadowed:?}"
        );
    }
    let pending_keys: BTreeSet<String> = common::pending().into_values().flatten().collect();
    let shadowed: Vec<_> = pending_keys.iter().filter(|k| tc4.contains(*k)).collect();
    assert!(
        shadowed.is_empty(),
        "pending keys that tC4 has: {shadowed:?}"
    );
}

#[test]
fn rebuild_files_cover_every_language_and_nothing_else() {
    let mut expected: BTreeSet<String> = ["README.md".to_owned()].into();
    for id in LOCALE_IDS {
        expected.insert(format!("{id}.json"));
        if id != "en" {
            expected.insert(format!("{id}.pending.json"));
        }
    }
    let on_disk: BTreeSet<String> = std::fs::read_dir(rebuild_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(on_disk, expected);
    let vendored_files: BTreeSet<String> =
        std::fs::read_dir(common::crate_dir().join("../tc-ui/assets/i18n"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
    let ids: BTreeSet<String> = LOCALE_IDS.iter().map(|id| format!("{id}.json")).collect();
    assert_eq!(
        vendored_files, ids,
        "LOCALE_IDS names exactly tC4's catalogs"
    );
}

#[test]
fn pending_lists_hold_exactly_the_untranslated_rebuild_keys() {
    let rebuild = rebuild();
    let english = &rebuild["en"];
    assert!(!english.is_empty(), "the rebuild has keys");
    for (locale, pending) in common::pending() {
        let translated = &rebuild[locale];
        for key in pending.iter() {
            assert!(
                english.contains_key(key),
                "{locale}: pending {key} is not a rebuild key"
            );
            assert!(
                !translated.contains_key(key),
                "{locale}: {key} is translated and pending"
            );
        }
        for (key, text) in translated {
            let source = english
                .get(key)
                .unwrap_or_else(|| panic!("{locale}: {key} has no English source"));
            assert_eq!(
                placeholders(text),
                placeholders(source),
                "{locale}: {key} has other placeholders than English"
            );
        }
        let missing: Vec<_> = english
            .keys()
            .filter(|k| !translated.contains_key(*k) && !pending.contains(*k))
            .collect();
        assert!(
            missing.is_empty(),
            "{locale}: neither translated nor pending: {missing:?}"
        );
    }
}

/// A release waits for every rebuild key to be translated (Q-10, A-32). Debug builds, where
/// development happens, pass and say how many keys are still pending.
#[test]
fn release_has_no_pending_translation() {
    let pending: Vec<(&str, usize)> = common::pending()
        .into_iter()
        .filter(|(_, keys)| !keys.is_empty())
        .map(|(locale, keys)| (locale, keys.len()))
        .collect();
    if cfg!(debug_assertions) {
        if !pending.is_empty() {
            eprintln!("rebuild keys still in English, by language: {pending:?}");
        }
    } else {
        assert!(
            pending.is_empty(),
            "a release needs every rebuild key translated; still in English: {pending:?}"
        );
    }
}
