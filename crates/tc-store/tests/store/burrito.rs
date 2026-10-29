//! INV-VALID-BURRITO: burrito validity everywhere (`docs/STORAGE.md` §16.6, I7): every state the store finishes
//! writing is a valid Scripture Burrito, and no value a person, a teammate or a program puts
//! into a project's details can stop `metadata.json` or a commit.
//!
//! Every test of this binary is also a check: its [`Rig`] has the writer look at the folder
//! after each job that wrote files (outside a write group), and fails the test when one state
//! was not valid. The tests here add the states that name the writes: each command of §2.7
//! on its own, an outside edit, a carried file, a held file, a checkpoint and a close.

use crate::commands::{every_command, open, rich};
use crate::common::*;
use std::path::Path;
use tc_journal::command::Command;
use tc_store::{CheckpointReason, Leave, OpenOptions, ProjectHandle};

/// The folder is a valid burrito once the writer is done, and so is every commit.
fn settled_and_valid(rig: &Rig, handle: &ProjectHandle, folder: &Path, what: &str) {
    handle.leave(Leave::Mode).wait().unwrap();
    handle.wait_idle();
    let problems = burrito_problems(&rig.fs, folder);
    assert!(problems.is_empty(), "after {what}: {problems:#?}");
}

/// A command that runs, whatever it writes, leaves a valid burrito, and so does the
/// checkpoint and the close that follow it.
#[test]
fn every_written_state_is_a_valid_burrito() {
    let (fs, folder) = rich();
    let count = {
        let (_rig, handle) = open(&fs, &folder);
        let count = every_command(&handle).len();
        handle.close().unwrap();
        count
    };
    assert!(count > 20, "{count}");
    for i in 0..count {
        let (fs, folder) = rich();
        let (rig, handle) = open(&fs, &folder);
        settled_and_valid(&rig, &handle, &folder, "the open");
        let (name, command) = every_command(&handle).into_iter().nth(i).unwrap();
        let result = handle.submit(command).wait();
        settled_and_valid(&rig, &handle, &folder, &format!("{name} ({result:?})"));
        let report = handle
            .checkpoint(CheckpointReason::LeavingTranslate)
            .wait()
            .unwrap();
        settled_and_valid(
            &rig,
            &handle,
            &folder,
            &format!("a checkpoint after {name}"),
        );
        assert!(
            report.skipped.is_none() || report.skipped == Some(tc_store::CheckpointSkip::NoChanges),
            "{name}: {report:?}"
        );
        assert_commits_valid(&rig.fs, &folder);
        handle.close().unwrap();
        assert_valid_burrito(&rig.fs, &folder);
    }
}

/// What other programs do to a folder: an edit of a book, a file the project carries, a file
/// damaged while its time stood still, and then the app's own choices about them.
#[test]
fn the_states_after_outside_changes_are_valid_burritos() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    settled_and_valid(&rig, &handle, &folder, "a save");

    // A program edits the book while the app is open.
    let usfm = folder.join("ingredients/TIT.usfm");
    let edited = String::from_utf8(rig.fs.peek(&usfm).unwrap())
        .unwrap()
        .replace("Paul, a servant of God", "Paul, a slave of God");
    rig.fs.put(&usfm, edited.as_bytes(), None);
    handle.focus().wait().unwrap();
    settled_and_valid(&rig, &handle, &folder, "an outside edit");
    // The book waits for the person's answer; *Import* brings the edit in.
    assert!(words(&handle, 1, 1).unwrap().contains("servant"));
    assert_eq!(counts(&import_waiting(&handle)), vec![(1, 0)]);
    settled_and_valid(&rig, &handle, &folder, "an outside edit imported");
    assert!(words(&handle, 1, 1).unwrap().contains("slave"));

    // A file the project carries.
    rig.fs.put_unseen(
        &folder.join("ingredients/audio/song.mp3"),
        b"an hour of singing, in a few bytes",
        None,
    );
    handle.focus().wait().unwrap();
    settled_and_valid(&rig, &handle, &folder, "a carried file");
    rig.fs
        .delete(&folder.join("ingredients/audio/song.mp3"))
        .unwrap();
    handle.focus().wait().unwrap();
    settled_and_valid(&rig, &handle, &folder, "a carried file deleted");

    // A file changed without its size or time changing: held, never written over.
    rig.fs.advance(std::time::Duration::from_secs(5));
    handle
        .checkpoint(CheckpointReason::LeavingTranslate)
        .wait()
        .unwrap();
    settled_and_valid(&rig, &handle, &folder, "a checkpoint");
    assert_commits_valid(&rig.fs, &folder);
    handle.close().unwrap();
    assert_valid_burrito(&rig.fs, &folder);
}

/// §16.6: every commit of a project's history is a valid burrito (I7), whatever happened
/// between two checkpoints: a save, a book added and removed, a carried file, an outside edit,
/// a file held and restored.
#[test]
fn every_commit_tree_is_a_valid_burrito() {
    use tc_journal::command::SeedSource;
    use tc_journal::record::Scope;
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let checkpoint = |handle: &ProjectHandle| {
        handle
            .checkpoint(CheckpointReason::LeavingTranslate)
            .wait()
            .unwrap();
        handle.wait_idle();
    };
    save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
    checkpoint(&handle);
    let book = tc_journal::BookId::Philemon;
    handle
        .submit(Command::AddBook {
            book,
            scope: Scope::Whole,
            seed: SeedSource {
                usfm: "\\id PHM source\n\\usfm 3.0\n\\h PHM\n\\mt PHM\n\\c 1\n\\p\n\\v 1 One.\n"
                    .into(),
            },
        })
        .wait()
        .unwrap();
    checkpoint(&handle);
    rig.fs.put_unseen(
        &folder.join("ingredients/audio/song.mp3"),
        b"an hour of singing, in a few bytes",
        None,
    );
    handle.focus().wait().unwrap();
    checkpoint(&handle);
    let usfm = folder.join("ingredients/TIT.usfm");
    let edited = String::from_utf8(rig.fs.peek(&usfm).unwrap())
        .unwrap()
        .replace("Paul, a servant of God", "Paul, a slave of God");
    rig.fs.put(&usfm, edited.as_bytes(), None);
    handle.focus().wait().unwrap();
    checkpoint(&handle);
    import_waiting(&handle);
    checkpoint(&handle);
    handle
        .submit(Command::RemoveBook {
            book,
            presence_shown: handle
                .mint(&tc_journal::RecordKey::BookPresence(book), None)
                .unwrap(),
        })
        .wait()
        .unwrap();
    checkpoint(&handle);
    handle.close().unwrap();
    let repo = repo_of(&rig.fs, &folder);
    let commits = commits_of(&repo);
    assert!(commits.len() >= 6, "{}", commits.len());
    assert_commits_valid(&rig.fs, &folder);
}

mod arbitrary {
    use super::*;
    use proptest::prelude::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tc_journal::command::{PlannedChange, UpgradePlan};
    use tc_journal::fold::Anomaly;
    use tc_journal::record::{
        BookPresence, PinGroupValue, PresentHow, Scope, ScopeRange, ScopeRanges, SlotValue,
    };
    use tc_journal::{Cause, Json, MetaField, MetaPath, PinGroup, Record, RecordKey, SlotName};

    fn text() -> impl Strategy<Value = String> {
        prop_oneof![
            3 => "[ -~é]{0,10}",
            1 => any::<String>(),
            1 => Just("\u{0}\"\\\n".to_owned()),
        ]
    }

    fn json() -> impl Strategy<Value = Json> {
        let leaf = prop_oneof![
            Just(Json::Null),
            any::<bool>().prop_map(Json::Bool),
            any::<i32>().prop_map(|n| Json::from(i64::from(n))),
            text().prop_map(Json::Str),
        ];
        leaf.prop_recursive(3, 16, 3, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..3).prop_map(Json::Arr),
                prop::collection::btree_map(text(), inner, 0..3)
                    .prop_map(|map| Json::Obj(map.into_iter().collect())),
            ]
        })
    }

    fn field() -> impl Strategy<Value = MetaField> {
        prop_oneof![
            Just(MetaField::Name),
            Just(MetaField::Abbreviation),
            Just(MetaField::Description),
            Just(MetaField::Languages),
            Just(MetaField::Copyright),
            "[a-zA-Z]{1,8}(\\.[a-zA-Z]{1,8}){0,2}"
                .prop_filter_map("a free path", |s| MetaPath::parse(&s).ok())
                .prop_map(MetaField::Extra),
        ]
    }

    fn scope() -> impl Strategy<Value = Scope> {
        prop_oneof![
            Just(Scope::Whole),
            prop::collection::vec(
                prop::sample::select(vec!["1", "1-2", "3:1", "3:1-9", "3:1-4:2"]),
                1..4
            )
            .prop_map(|texts| Scope::Ranges(
                ScopeRanges::new(
                    texts
                        .iter()
                        .map(|t| t.parse::<ScopeRange>().unwrap())
                        .collect()
                )
                .unwrap()
            )),
        ]
    }

    fn slot() -> impl Strategy<Value = SlotName> {
        "[a-zA-Z][a-zA-Z0-9_]{0,12}"
            .prop_filter("not a prototype name", |s| {
                s != "constructor" && s != "prototype"
            })
            .prop_map(|s| SlotName::parse(&s).unwrap())
    }

    #[derive(Default)]
    struct Census {
        used: AtomicU32,
        invalid: AtomicU32,
        outside: AtomicU32,
        /// Outside edits whose name is a record of the project afterwards.
        outside_recorded: AtomicU32,
        refused: AtomicU32,
    }

    fn path_of(field: &MetaField) -> String {
        match field {
            MetaField::Name => "identification.name".to_owned(),
            MetaField::Abbreviation => "identification.abbreviation".to_owned(),
            MetaField::Description => "identification.description".to_owned(),
            MetaField::Languages => "languages".to_owned(),
            MetaField::Copyright => "copyright".to_owned(),
            MetaField::Extra(path) => path.as_str().to_owned(),
        }
    }

    type Case = (Vec<(MetaField, Json)>, Vec<(SlotName, Json)>, Scope, Json);

    fn run_case((details, slots, scope, outside): &Case, census: &Census) {
        let (rig, folder) = titus();
        let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
        handle.wait_idle();
        let fold = handle.snapshot().fold().clone();
        // Values `SetMeta` would refuse arrive as a teammate's or Door43's change would:
        // through a plan whose cause is a seed.
        let mut changes: Vec<PlannedChange> = Vec::new();
        let mut keys = std::collections::BTreeSet::new();
        let mut plan_of = |record: Record| {
            let key = record.key();
            if keys.insert(key.clone()) {
                changes.push(PlannedChange {
                    parents: fold.heads(&key),
                    record,
                    cause: Cause::Seed,
                });
            }
        };
        for (field, value) in details {
            plan_of(Record::ProjectMeta {
                field: field.clone(),
                value: Some(value.clone()),
            });
        }
        let mut group = PinGroupValue::default();
        for (slot, value) in slots {
            group.0.insert(slot.clone(), SlotValue(value.clone()));
        }
        plan_of(Record::Pins {
            group: PinGroup::Primary,
            value: group,
        });
        plan_of(Record::BookPresence {
            book: TIT,
            value: BookPresence::Present {
                scope: scope.clone(),
                how: PresentHow::Added,
            },
        });
        if handle
            .submit(Command::ApplyUpgrade {
                plan: UpgradePlan { changes },
            })
            .wait()
            .is_err()
        {
            census.refused.fetch_add(1, Ordering::Relaxed);
        }
        handle.wait_idle();
        handle.leave(Leave::Mode).wait().unwrap();
        handle.close().unwrap();

        // And a program edits the details in `metadata.json` itself.
        let metadata = folder.join("metadata.json");
        let mut document: serde_json::Value =
            serde_json::from_slice(&rig.fs.peek(&metadata).unwrap()).unwrap();
        document["identification"]["name"] = serde_json::Value::from(outside);
        rig.fs
            .put_unseen(&metadata, document.to_string().as_bytes(), None);
        census.outside.fetch_add(1, Ordering::Relaxed);

        let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
        handle.wait_idle();
        let report = handle
            .checkpoint(CheckpointReason::LeavingTranslate)
            .wait()
            .unwrap();
        handle.wait_idle();
        // A commit is made, whatever the values are, and it is a valid burrito.
        assert!(
            report.commit.is_some() || report.skipped == Some(tc_store::CheckpointSkip::NoChanges),
            "{report:?} after {details:?}"
        );
        assert_commits_valid(&rig.fs, &folder);
        assert_valid_burrito(&rig.fs, &folder);

        // The name the program wrote is a record of the project, whatever it is: the document
        // uses it, or the project lists it as one it cannot use (§11.1 X2).
        let snapshot = handle.snapshot();
        let fold = snapshot.fold();
        let name_key = RecordKey::ProjectMeta(MetaField::Name);
        let wanted = serde_json::Value::from(outside);
        let was_recorded = fold.history(&name_key).iter().any(|id| {
            matches!(
                fold.body(id).map(|b| b.record),
                Some(Record::ProjectMeta { value: Some(v), .. })
                    if serde_json::Value::from(&v) == wanted
            )
        });
        assert!(
            was_recorded,
            "the name {wanted} written in metadata.json is no record of the project: {:?}",
            handle.notices()
        );
        census.outside_recorded.fetch_add(1, Ordering::Relaxed);
        let shown = match fold.projected(&name_key) {
            Some(Record::ProjectMeta {
                value: Some(shown), ..
            }) => Some(serde_json::Value::from(&shown)),
            _ => None,
        };
        if shown.as_ref() == Some(&wanted) {
            let source = fold.metadata_source((1, 0));
            assert!(
                source.fields.get("identification.name") == Some(&wanted)
                    || fold.anomalies().any(|a| matches!(
                        a,
                        Anomaly::InvalidForBurrito { key, .. } if *key == name_key
                    )),
                "the name {wanted} is neither used nor listed"
            );
        }

        // Every value the document does not use is listed.
        let source = fold.metadata_source((1, 0));
        for key in fold.record_keys() {
            let RecordKey::ProjectMeta(field) = &key else {
                continue;
            };
            let Some(Record::ProjectMeta {
                value: Some(value), ..
            }) = fold.projected(&key)
            else {
                continue;
            };
            if source.fields.get(&path_of(field)) == Some(&serde_json::Value::from(&value)) {
                census.used.fetch_add(1, Ordering::Relaxed);
            } else {
                census.invalid.fetch_add(1, Ordering::Relaxed);
                assert!(
                    fold.anomalies().any(
                        |a| matches!(a, Anomaly::InvalidForBurrito { key: k, .. } if *k == key)
                    ),
                    "{key:?} is not used and not listed"
                );
            }
        }
        handle.close().unwrap();
    }

    /// `cases` projects whose details, pins and scope hold arbitrary JSON, from a plan and
    /// from an edit of `metadata.json`: `metadata.json` builds, a commit is made and is a
    /// valid burrito, and every value that could not be used is listed. The census asserts
    /// need a few cases of each kind, so a short run takes a fixed seed rather than flake.
    pub fn property(cases: u32, seed: proptest::test_runner::RngSeed) {
        let config = ProptestConfig {
            cases,
            rng_seed: seed,
            max_shrink_iters: 0,
            failure_persistence: None,
            ..ProptestConfig::default()
        };
        let strategy = (
            prop::collection::vec((field(), json()), 0..5),
            prop::collection::vec((slot(), json()), 0..3),
            scope(),
            json(),
        );
        let census = Census::default();
        let mut runner = proptest::test_runner::TestRunner::new(config);
        if let Err(e) = runner.run(&strategy, |case| {
            run_case(&case, &census);
            Ok(())
        }) {
            panic!("{e}");
        }
        let used = census.used.load(Ordering::Relaxed);
        let invalid = census.invalid.load(Ordering::Relaxed);
        assert!(used > 0, "no value was ever used: {invalid} invalid");
        assert!(invalid > 0, "no value was ever invalid: {used} used");
        assert!(
            census.outside_recorded.load(Ordering::Relaxed) > 0,
            "no edit of metadata.json was ever recorded"
        );
        assert!(
            census.refused.load(Ordering::Relaxed) < cases * 5 / 8,
            "most plans were refused: the claim is not tested"
        );
    }
}

#[test]
fn arbitrary_meta_never_blocks_commit() {
    arbitrary::property(12, proptest::test_runner::RngSeed::Fixed(7));
}

#[test]
#[ignore = "nightly: 256 cases; the gate runs arbitrary_meta_never_blocks_commit at 12"]
fn arbitrary_meta_never_blocks_commit_nightly() {
    arbitrary::property(256, proptest::test_runner::RngSeed::Random);
}
