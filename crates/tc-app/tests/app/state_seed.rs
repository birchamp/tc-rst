//! STORAGE §13.2's state seed of tC4 projects (unit B1a, APP-DESIGN AD7): the parity
//! scenarios' projects come back from the fold as they are, the changes are the importer's,
//! and every way a folder can be wrong refuses with nothing made.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use serde_json::{Value, json};
use tc_app::backend::seed::{
    BatchLabel, CheckRule, DeclaredDifference, IgnoredWhy, Mismatch, SeedRefusal, StateSeed,
    Tc4Folder, fixture_project_id, tc4_state_seed,
};
use tc_journal::decision::verse_text_md5_v1;
use tc_journal::fold::DerivedFile;
use tc_journal::record::{CheckValue, DecisionRecord, UserStatus};
use tc_journal::{
    BatchId, BookId, Cause, DeviceId, DeviceKind, ImportSource, Record, RecordKey, ToolId,
};

const BIBLE: &str = "sample_burrito";
const OBS: &str = "sample_burrito_obs";
const SCENARIOS: [&str; 3] = ["sample", "en-pins", "shared"];
const TN: &str = "ingredients/checking/translationNotes/TIT.json";
const TW: &str = "ingredients/checking/translationWords/TIT.json";
const ALIGNMENTS: &str = "ingredients/checking/alignments/TIT.json";
const RESOURCES: &str = "ingredients/checking/resources.json";

type Files = Vec<(String, Vec<u8>)>;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sample_dir(project: &str) -> PathBuf {
    match project {
        BIBLE => root().join("fixtures/tc4/sample-burrito"),
        OBS => root().join("fixtures/tc4/sample-burrito-obs"),
        other => panic!("no sample {other}"),
    }
}

/// Every file under `dir`, by `/`-separated path, sorted.
fn files_under(dir: &Path) -> Files {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for entry in std::fs::read_dir(dir.join(&rel)).unwrap() {
            let entry = entry.unwrap();
            let sub = rel.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                stack.push(sub);
            } else {
                let path = sub.to_str().unwrap().replace('\\', "/");
                out.push((path, std::fs::read(dir.join(&sub)).unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// A sample project's files, as `fixtures/tc4` holds them.
fn sample_files(project: &str) -> Files {
    files_under(&sample_dir(project))
}

/// A scenario's project: the sample with `fixtures/parity-state/<scenario>/<project>/` laid
/// over it, exactly as `build-state.mjs` makes it.
fn scenario_files(scenario: &str, project: &str) -> Files {
    let mut files: BTreeMap<String, Vec<u8>> = sample_files(project).into_iter().collect();
    let overlay = root()
        .join("fixtures/parity-state")
        .join(scenario)
        .join(project);
    if overlay.is_dir() {
        files.extend(files_under(&overlay));
    }
    files.into_iter().collect()
}

fn get(files: &Files, path: &str) -> Vec<u8> {
    files
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, b)| b.clone())
        .unwrap_or_else(|| panic!("no {path}"))
}

fn put(files: &mut Files, path: &str, bytes: Vec<u8>) {
    match files.iter_mut().find(|(p, _)| p == path) {
        Some(entry) => entry.1 = bytes,
        None => files.push((path.to_owned(), bytes)),
    }
}

fn remove(files: &mut Files, path: &str) {
    files.retain(|(p, _)| p != path);
}

/// `files` with the JSON file at `path` edited.
fn with_json(mut files: Files, path: &str, edit: impl FnOnce(&mut Value)) -> Files {
    let mut doc: Value = serde_json::from_slice(&get(&files, path)).unwrap();
    edit(&mut doc);
    put(&mut files, path, serde_json::to_vec_pretty(&doc).unwrap());
    files
}

/// `files` with the text file at `path` edited.
fn with_text(mut files: Files, path: &str, edit: impl FnOnce(&str) -> String) -> Files {
    let text = String::from_utf8(get(&files, path)).unwrap();
    put(&mut files, path, edit(&text).into_bytes());
    files
}

fn seed(name: &str, files: Files) -> Result<StateSeed, SeedRefusal> {
    let folder = Tc4Folder::from_files(name, files)?;
    tc4_state_seed(&folder, fixture_project_id(name))
}

fn mismatches(result: Result<StateSeed, SeedRefusal>) -> Vec<Mismatch> {
    match result {
        Err(SeedRefusal::Mismatches(m)) => m,
        Err(other) => panic!("refused for another reason: {other}"),
        Ok(_) => panic!("the seed passed"),
    }
}

/// A mismatch as the tuple the write-up lists: (path, record, field, rule).
pub(crate) fn tuple(m: &Mismatch) -> (String, Option<String>, Option<String>, CheckRule) {
    (m.path.clone(), m.record.clone(), m.field.clone(), m.rule)
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn decision<'a>(seed: &'a StateSeed, check_id: &str) -> (&'a DecisionRecord, i64) {
    seed.batches
        .iter()
        .flat_map(|b| &b.changes)
        .find_map(|c| match &c.body().record {
            Record::Check {
                key,
                value: CheckValue::Decision(d),
            } if key.check_id.as_str() == check_id => Some((d, c.body().at.0)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no decision {check_id}"))
}

fn projected_json(seed: &StateSeed, file: &DerivedFile) -> Value {
    serde_json::from_slice(&seed.fold.render(file).unwrap()).unwrap()
}

fn change_ids(seed: &StateSeed) -> Vec<tc_journal::ChangeId> {
    seed.batches
        .iter()
        .flat_map(|b| b.changes.iter().map(|c| c.id()))
        .collect()
}

/// The decisions' records of a projected decision file, by check id.
fn projected_decisions(seed: &StateSeed, tool: ToolId) -> BTreeMap<String, Value> {
    projected_json(seed, &DerivedFile::Decisions(tool, BookId::Titus))["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["contextId"]["checkId"].as_str().unwrap().to_owned(),
                d.clone(),
            )
        })
        .collect()
}

// ---- The row's "done when" ----------------------------------------------------------------

#[test]
fn scenario_projects_are_the_recorded_state_bytes() {
    let recorded: Value = serde_json::from_slice(
        &std::fs::read(root().join("parity/app/state.sha256.json")).unwrap(),
    )
    .unwrap();
    for scenario in SCENARIOS {
        for project in [BIBLE, OBS] {
            let prefix = format!("repos/_local_/_local_/{project}/");
            let want: BTreeMap<String, String> = recorded["scenarios"][scenario]
                .as_object()
                .unwrap()
                .iter()
                .filter_map(|(path, sha)| {
                    Some((
                        path.strip_prefix(&prefix)?.to_owned(),
                        sha.as_str()?.to_owned(),
                    ))
                })
                .collect();
            let have: BTreeMap<String, String> = scenario_files(scenario, project)
                .into_iter()
                .map(|(path, bytes)| (path, sha256_hex(&bytes)))
                .collect();
            assert!(!want.is_empty(), "{scenario} {project}");
            assert_eq!(have, want, "{scenario} {project}");
        }
    }
}

/// What the Bible sample declares in every scenario: its two decision files' summary
/// caches, and gr8c's verseEdits, which the fold computes from the current verse.
pub(crate) fn bible_declared() -> Vec<DeclaredDifference> {
    vec![
        DeclaredDifference::SummaryDropped { path: TN.into() },
        DeclaredDifference::VerseEditsRecomputed {
            path: TN.into(),
            check: "translationNotes TIT 1:4 gr8c #1".into(),
            source: true,
            projected: false,
        },
        DeclaredDifference::SummaryDropped { path: TW.into() },
    ]
}

#[test]
fn bible_project_reproduces_every_file_in_every_scenario() {
    for scenario in SCENARIOS {
        let files = scenario_files(scenario, BIBLE);
        let seed = seed(BIBLE, files.clone()).unwrap_or_else(|e| panic!("{scenario}: {e}"));
        for book in [BookId::Titus, BookId::Jonah] {
            let path = format!("ingredients/{}.usfm", book.code());
            assert_eq!(
                seed.fold.book_usfm(book).unwrap().into_bytes(),
                get(&files, &path),
                "{scenario} {path}"
            );
        }
        assert_eq!(
            seed.fold.render(&DerivedFile::Versification).unwrap(),
            get(&files, "ingredients/vrs.json"),
            "{scenario}"
        );
        let source = |path: &str| -> Value { serde_json::from_slice(&get(&files, path)).unwrap() };
        assert_eq!(
            projected_json(&seed, &DerivedFile::Resources),
            source(RESOURCES),
            "{scenario}"
        );
        assert_eq!(
            projected_json(&seed, &DerivedFile::Settings),
            source("ingredients/checking/settings.json"),
            "{scenario}"
        );
        assert_eq!(seed.report.declared, bible_declared(), "{scenario}");
        let mut ignored = vec![(".gitignore".to_owned(), IgnoredWhy::AppOwned)];
        if scenario == "shared" {
            ignored.push((".remotes.json".into(), IgnoredWhy::MockServerRemotes));
        }
        let got: Vec<(String, IgnoredWhy)> = seed
            .report
            .ignored
            .iter()
            .map(|i| (i.path.clone(), i.why))
            .collect();
        assert_eq!(got, ignored, "{scenario}");
        let batches: Vec<(BatchLabel, usize)> = seed
            .batches
            .iter()
            .map(|b| (b.label, b.changes.len()))
            .collect();
        assert_eq!(
            batches,
            [
                (BatchLabel::Project, 17),
                (BatchLabel::Book(BookId::Jonah), 54),
                (BatchLabel::Book(BookId::Titus), 56),
            ],
            "{scenario}: 9 fields, vrs, 5 pin groups, 2 settings; JON's presence, header, 4 \
             fronts and 48 verses; TIT's presence, header, 3 fronts, 46 verses, 1 alignment \
             and 4 decisions"
        );
        assert_eq!(seed.report.counts.changes, 127);
        assert!(seed.report.added_files.is_empty(), "{scenario}");
        assert!(seed.report.notes.is_empty(), "{scenario}");
    }
}

/// The OBS sample's mismatches, written up in docs/evidence/state-seed-check-2026-10-18.md
/// and handed to the storage team: the same in every scenario.
pub(crate) fn obs_written_up() -> Vec<(String, Option<String>, Option<String>, CheckRule)> {
    let mut out = vec![
        (
            RESOURCES.to_owned(),
            Some("pins Primary".to_owned()),
            Some("relationships".to_owned()),
            CheckRule::FoldAnomaly,
        ),
        (
            RESOURCES.to_owned(),
            Some("pins Fallback".to_owned()),
            Some("relationships".to_owned()),
            CheckRule::FoldAnomaly,
        ),
        (
            "ingredients/checking/settings.json".to_owned(),
            None,
            None,
            CheckRule::FileNotDerived,
        ),
    ];
    for story in 1..=50 {
        out.push((
            format!("ingredients/content/{story:02}.md"),
            Some(format!("story {story} frame 0")),
            None,
            CheckRule::FrameText,
        ));
    }
    out
}

#[test]
fn obs_project_outcome_is_exactly_the_written_up_mismatches() {
    for scenario in SCENARIOS {
        let found = mismatches(seed(OBS, scenario_files(scenario, OBS)));
        let tuples: Vec<_> = found.iter().map(tuple).collect();
        assert_eq!(tuples, obs_written_up(), "{scenario}");
        // The title: tC4 reads it without the story's number, the fold with it.
        let story_one = found
            .iter()
            .find(|m| m.record.as_deref() == Some("story 1 frame 0"))
            .unwrap();
        assert_eq!(story_one.source, "\"La Creación\"");
        assert_eq!(story_one.projected, "\"1. La Creación\"");
        let story_two = found
            .iter()
            .find(|m| m.record.as_deref() == Some("story 2 frame 0"))
            .unwrap();
        assert_eq!(
            (story_two.source.as_str(), story_two.projected.as_str()),
            ("\"\"", "\"2.\"")
        );
        let anomaly = &found[0];
        assert!(
            anomaly.projected.contains("\"gloss\""),
            "{scenario}: {}",
            anomaly.projected
        );
    }
}

#[test]
fn shared_scenario_seeds_exactly_like_sample() {
    let sample = seed(BIBLE, scenario_files("sample", BIBLE)).unwrap();
    let shared = seed(BIBLE, scenario_files("shared", BIBLE)).unwrap();
    assert_eq!(change_ids(&shared), change_ids(&sample));
    assert_eq!(shared.fold.frontier(), sample.fold.frontier());
    assert!(
        shared
            .report
            .ignored
            .iter()
            .any(|i| i.path == ".remotes.json" && i.why == IgnoredWhy::MockServerRemotes)
    );
}

#[test]
fn en_pins_decision_files_resolve_to_the_english_pins() {
    let files = scenario_files("en-pins", BIBLE);
    let seed = seed(BIBLE, files.clone()).unwrap();
    for (tool, path, repo, sha) in [
        (
            ToolId::TranslationNotes,
            TN,
            "git.door43.org/unfoldingWord/en_tn",
            "e586762e330f482a60c52aedd1c7b3a2f155df8a",
        ),
        (
            ToolId::TranslationWords,
            TW,
            "git.door43.org/unfoldingWord/en_twl",
            "7cbb3419f79780ce84b92ad7d42bd476cc5d8a64",
        ),
    ] {
        let source: Value = serde_json::from_slice(&get(&files, path)).unwrap();
        let projected = projected_json(&seed, &DerivedFile::Decisions(tool, BookId::Titus));
        assert_eq!(projected["resource"], source["resource"]);
        assert_eq!(projected["resource"]["repoPath"], repo);
        assert_eq!(projected["resource"]["sha"], sha);
        assert_eq!(projected["resource"]["languageSet"], "primary");
        for decision in projected["decisions"].as_array().unwrap() {
            let id = decision["contextId"]["checkId"].as_str().unwrap();
            assert_eq!(decision["invalidated"], id == "gr8c", "{id}");
        }
    }
}

// ---- The seed -----------------------------------------------------------------------------

#[test]
fn seed_is_deterministic() {
    let a = seed(BIBLE, sample_files(BIBLE)).unwrap();
    let b = seed(BIBLE, sample_files(BIBLE)).unwrap();
    assert_eq!(change_ids(&a), change_ids(&b));
    assert_eq!(a.fold.frontier(), b.fold.frontier());
    let raws = |s: &StateSeed| -> Vec<Vec<u8>> {
        s.batches
            .iter()
            .flat_map(|b| b.changes.iter().map(|c| c.raw().to_vec()))
            .collect()
    };
    assert_eq!(raws(&a), raws(&b));
}

#[test]
fn seed_does_not_depend_on_record_order() {
    // A second alignment record at 1:2 (stale: its hash is 1:1's), in both orders.
    let with_two = |first_two: bool| {
        let files = with_json(sample_files(BIBLE), ALIGNMENTS, |doc| {
            let verses = doc["chapters"]["1"].as_object_mut().unwrap();
            let one = verses["1"].clone();
            let mut two = one.clone();
            two["invalid"] = json!(true);
            verses.clear();
            if first_two {
                verses.insert("2".into(), two);
                verses.insert("1".into(), one);
            } else {
                verses.insert("1".into(), one);
                verses.insert("2".into(), two);
            }
        });
        let files = with_json(files, TN, |doc| {
            doc["decisions"].as_array_mut().unwrap().reverse();
        });
        with_json(files, "ingredients/checking/settings.json", |doc| {
            let map = doc.as_object_mut().unwrap();
            let entries: Vec<(String, Value)> = map
                .iter()
                .rev()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            map.clear();
            map.extend(entries);
        })
    };
    let reordered = seed(BIBLE, with_two(true)).unwrap();
    let ordered = seed(BIBLE, with_two(false)).unwrap();
    assert_eq!(change_ids(&reordered), change_ids(&ordered));
    // Reversing the decisions alone changes no change either.
    let reversed = with_json(sample_files(BIBLE), TN, |doc| {
        doc["decisions"].as_array_mut().unwrap().reverse();
    });
    assert_eq!(
        change_ids(&seed(BIBLE, reversed).unwrap()),
        change_ids(&seed(BIBLE, sample_files(BIBLE)).unwrap())
    );
}

#[test]
fn seed_changes_are_the_importer_s_one_batch_per_book_plus_the_project() {
    let seed = seed(BIBLE, sample_files(BIBLE)).unwrap();
    let project = fixture_project_id(BIBLE);
    let mut input = b"import\0tc4-seed".to_vec();
    input.extend_from_slice(project.as_bytes());
    let device = DeviceId::derived(DeviceKind::Importer, &input);
    assert_eq!(seed.device, device);
    assert_eq!(device.kind(), DeviceKind::Importer);
    for batch in &seed.batches {
        let label = match batch.label {
            BatchLabel::Project => "project".to_owned(),
            BatchLabel::Book(book) => book.code().to_owned(),
            BatchLabel::Story(story) => format!("story-{:02}", story.get()),
        };
        let mut hashed = b"tc4-seed-batch\0".to_vec();
        hashed.extend_from_slice(project.as_bytes());
        hashed.extend_from_slice(label.as_bytes());
        let digest = ring::digest::digest(&ring::digest::SHA256, &hashed);
        let id = BatchId::from_bytes(digest.as_ref()[..16].try_into().unwrap());
        let mut keys = Vec::new();
        for (i, change) in batch.changes.iter().enumerate() {
            let body = change.body();
            assert_eq!(body.device, device);
            assert_eq!(
                body.cause,
                Cause::Import(ImportSource::Burrito {
                    file_name: BIBLE.into()
                })
            );
            assert!(body.parents.is_empty());
            assert_eq!(body.seq.0, i as u64);
            assert_eq!(body.batch.id, id, "{label}");
            assert_eq!(body.batch.n() as usize, batch.changes.len());
            keys.push(body.record.key());
            if let Some(book) = body.record.key().book() {
                assert_eq!(batch.label, BatchLabel::Book(book));
            } else if !matches!(body.record, Record::Check { .. }) {
                assert_eq!(batch.label, BatchLabel::Project);
            }
        }
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "{label}: changes in record key order");
    }
}

#[test]
fn every_seeded_record_is_the_value_the_fold_shows() {
    for (project, files) in [
        (BIBLE, sample_files(BIBLE)),
        (BIBLE, scenario_files("en-pins", BIBLE)),
    ] {
        let seed = seed(project, files).unwrap();
        assert_eq!(seed.fold.conflicts().count(), 0);
        assert_eq!(seed.fold.anomalies().count(), 0);
        for change in seed.batches.iter().flat_map(|b| &b.changes) {
            let key = change.body().record.key();
            assert_eq!(
                seed.fold.projected(&key).as_ref(),
                Some(&change.body().record)
            );
            assert_eq!(seed.fold.heads(&key), [change.id()]);
        }
    }
}

#[test]
fn decision_hash_is_the_current_verse_and_tc4_flags_carry_over() {
    let files = sample_files(BIBLE);
    let seed = seed(BIBLE, files.clone()).unwrap();
    let usfm = String::from_utf8(get(&files, "ingredients/TIT.usfm")).unwrap();
    let verse = |n: &str| -> String {
        let line = usfm
            .lines()
            .find(|l| l.starts_with(&format!("\\v {n} ")))
            .unwrap();
        line[format!("\\v {n} ").len()..].to_owned()
    };
    let stamp = |text: &str| {
        let ms = |s: &str| -> i64 {
            // 2026-07-02 is day 20,636 after 1970-01-01.
            let (h, rest) = s.split_at(2);
            let (m, sec) = (&rest[1..3], &rest[4..6]);
            20_636 * 86_400_000
                + h.parse::<i64>().unwrap() * 3_600_000
                + m.parse::<i64>().unwrap() * 60_000
                + sec.parse::<i64>().unwrap() * 1000
        };
        ms(text)
    };
    let (gr8c, gr8c_at) = decision(&seed, "gr8c");
    assert_eq!(
        gr8c.judgement.verse_text_md5,
        verse_text_md5_v1(&verse("4"))
    );
    assert_eq!(
        verse_text_md5_v1(&verse("4")),
        verse_text_md5_v1(&format!("{}\n", verse("4")))
    );
    assert!(gr8c.imported_invalid);
    assert_eq!(gr8c.judgement.status, None);
    assert_eq!(gr8c_at, stamp("15:40:56"));
    let (swi9, swi9_at) = decision(&seed, "swi9");
    assert_eq!(
        swi9.judgement.verse_text_md5,
        verse_text_md5_v1(&verse("1"))
    );
    assert!(!swi9.imported_invalid);
    assert_eq!(
        swi9.comment.as_deref(),
        Some("Verificar si «fe» comunica bien el sustantivo abstracto en este contexto.")
    );
    assert_eq!(swi9_at, stamp("15:02:11"));
    let (a9p2, a9p2_at) = decision(&seed, "a9p2");
    assert!(a9p2.bookmarked);
    assert_eq!(a9p2.judgement.status, None);
    assert_eq!(a9p2_at, stamp("14:24:39"));
    let (t1g7, t1g7_at) = decision(&seed, "t1g7");
    assert_eq!(t1g7.judgement.status, Some(UserStatus::Valid));
    assert_eq!(t1g7_at, stamp("14:21:07"));
    // The projection writes each decision's own time back.
    let tn = projected_decisions(&seed, ToolId::TranslationNotes);
    assert_eq!(tn["gr8c"]["modifiedTimestamp"], "2026-07-02T15:40:56.000Z");
    assert_eq!(tn["gr8c"]["invalidated"], true);
    assert_eq!(tn["gr8c"]["status"], "invalid");
    assert_eq!(tn["swi9"]["invalidated"], false);
}

#[test]
fn tc4_status_survives_on_invalidated_decisions() {
    // tC4 keeps a person's To do on an invalidated decision, and an Invalid that
    // `userInvalid` says is their own; the decoder alone would drop both.
    let files = with_json(sample_files(BIBLE), TN, |doc| {
        for d in doc["decisions"].as_array_mut().unwrap() {
            if d["contextId"]["checkId"] == "gr8c" {
                d["status"] = json!("todo");
            }
            if d["contextId"]["checkId"] == "swi9" {
                d["invalidated"] = json!(true);
                d["status"] = json!("invalid");
                d["userInvalid"] = json!(true);
            }
        }
    });
    let seed = seed(BIBLE, files).unwrap();
    assert_eq!(
        decision(&seed, "gr8c").0.judgement.status,
        Some(UserStatus::Todo)
    );
    assert_eq!(
        decision(&seed, "swi9").0.judgement.status,
        Some(UserStatus::Invalid)
    );
    let tn = projected_decisions(&seed, ToolId::TranslationNotes);
    assert_eq!(tn["gr8c"]["status"], "todo");
    assert_eq!(tn["gr8c"]["invalidated"], true);
    assert_eq!(tn["swi9"]["status"], "invalid");
    assert_eq!(tn["swi9"]["userInvalid"], true);
}

#[test]
fn tc4_invalid_alignment_is_marked_invalid() {
    let files = with_json(sample_files(BIBLE), ALIGNMENTS, |doc| {
        doc["chapters"]["1"]["1"]["invalid"] = json!(true);
    });
    let seed = seed(BIBLE, files).unwrap();
    let alignment = seed
        .batches
        .iter()
        .flat_map(|b| &b.changes)
        .find_map(|c| match &c.body().record {
            Record::Alignment { value, .. } => Some(value.clone()),
            _ => None,
        })
        .unwrap();
    assert!(alignment.marked_invalid);
    let projected = projected_json(&seed, &DerivedFile::Alignments(BookId::Titus));
    assert_eq!(projected["chapters"]["1"]["1"]["invalid"], true);
    // The sample's own record is valid, and stays so.
    let valid = self::seed(BIBLE, sample_files(BIBLE)).unwrap();
    let projected = projected_json(&valid, &DerivedFile::Alignments(BookId::Titus));
    assert_eq!(projected["chapters"]["1"]["1"]["invalid"], false);
}

#[test]
fn an_unknown_field_in_a_record_is_kept() {
    let files = with_json(sample_files(BIBLE), TW, |doc| {
        doc["decisions"][0]["x-note"] = json!({"kept": [1, 2]});
    });
    let files = with_json(files, ALIGNMENTS, |doc| {
        doc["chapters"]["1"]["1"]["x-note"] = json!("kept");
    });
    let seed = seed(BIBLE, files).unwrap();
    assert_eq!(
        projected_decisions(&seed, ToolId::TranslationWords)["a9p2"]["x-note"],
        json!({"kept": [1, 2]})
    );
    let projected = projected_json(&seed, &DerivedFile::Alignments(BookId::Titus));
    assert_eq!(projected["chapters"]["1"]["1"]["x-note"], "kept");
    let (a9p2, _) = decision(&seed, "a9p2");
    assert!(a9p2.extra.contains_key("x-note"));
    let alignment = seed
        .batches
        .iter()
        .flat_map(|b| &b.changes)
        .find_map(|c| match &c.body().record {
            Record::Alignment { value, .. } => Some(value.clone()),
            _ => None,
        })
        .unwrap();
    assert!(alignment.extra.contains_key("x-note"));
}

#[test]
fn a_decision_without_a_timestamp_takes_the_project_s_time_and_is_declared() {
    let files = with_json(sample_files(BIBLE), TW, |doc| {
        for d in doc["decisions"].as_array_mut().unwrap() {
            d.as_object_mut().unwrap().remove("modifiedTimestamp");
        }
    });
    let meta: Value = serde_json::from_slice(&get(&files, "metadata.json")).unwrap();
    let created = meta["meta"]["dateCreated"].as_str().unwrap().to_owned();
    assert_eq!(created, "2026-07-02T12:00:00.000Z");
    let seed = seed(BIBLE, files).unwrap();
    let tw = projected_decisions(&seed, ToolId::TranslationWords);
    for check in ["a9p2", "t1g7"] {
        assert_eq!(tw[check]["modifiedTimestamp"], created.as_str(), "{check}");
        assert_eq!(decision(&seed, check).1, 1_782_993_600_000, "{check}");
    }
    let mut declared = bible_declared();
    for check in ["a9p2", "t1g7"] {
        declared.push(DeclaredDifference::TimestampAdded {
            path: TW.into(),
            check: format!("translationWords TIT 1:1 {check} #1"),
            projected: format!("\"{created}\""),
        });
    }
    assert_eq!(seed.report.declared, declared);
}

#[test]
fn backups_dot_files_and_git_history_are_listed_not_imported() {
    let mut files = sample_files(BIBLE);
    files.push(("ingredients/TIT.usfm.bak".into(), b"\\id TIT old".to_vec()));
    files.push((".remotes.json".into(), b"[]".to_vec()));
    files.push((".git/HEAD".into(), b"ref: refs/heads/main\n".to_vec()));
    files.push((".git/objects/ab/cdef".into(), vec![0, 1, 2]));
    files.push(("README.md".into(), b"# notes".to_vec()));
    let plain = seed(BIBLE, sample_files(BIBLE)).unwrap();
    let seed = seed(BIBLE, files).unwrap();
    let ignored: Vec<(&str, IgnoredWhy)> = seed
        .report
        .ignored
        .iter()
        .map(|i| (i.path.as_str(), i.why))
        .collect();
    assert_eq!(
        ignored,
        [
            (".git/", IgnoredWhy::GitHistory),
            (".gitignore", IgnoredWhy::AppOwned),
            (".remotes.json", IgnoredWhy::MockServerRemotes),
            ("README.md", IgnoredWhy::ForeignRootEntry),
            ("ingredients/TIT.usfm.bak", IgnoredWhy::Tc4Backup),
        ]
    );
    assert_eq!(change_ids(&seed), change_ids(&plain));
}

#[test]
fn reading_a_project_folder_equals_building_it_from_its_files() {
    let dir = sample_dir(BIBLE);
    let read = Tc4Folder::read(&dir).unwrap();
    assert_eq!(read.name(), "sample-burrito");
    let built = Tc4Folder::from_files("sample-burrito", sample_files(BIBLE)).unwrap();
    assert_eq!(read, built);
    // A copy with git history and a link: the history is skipped whole, a link refuses.
    let temp = tempfile::tempdir().unwrap();
    let copy = temp.path().join("copied");
    for (path, bytes) in sample_files(BIBLE) {
        let target = copy.join(&path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, bytes).unwrap();
    }
    std::fs::create_dir_all(copy.join(".git/objects")).unwrap();
    std::fs::write(copy.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    let read = Tc4Folder::read(&copy).unwrap();
    assert_eq!(read.files(), built.files());
    assert!(
        read.ignored()
            .iter()
            .any(|i| i.why == IgnoredWhy::GitHistory)
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(copy.join("metadata.json"), copy.join("ingredients/link.md"))
            .unwrap();
        assert!(matches!(
            Tc4Folder::read(&copy),
            Err(SeedRefusal::Io { path, .. }) if path == "ingredients/link.md"
        ));
    }
}

#[test]
fn metadata_fields_survive_field_for_field() {
    let added = json!({
        "localizedNames": {"book-tit": {"short": {"en": "Titus"}, "abbr": {"en": "Tit"}, "long": {"en": "The Letter of Paul to Titus"}}},
        "agencies": [{"id": "dcs::es-419_gl", "roles": ["rightsHolder"], "name": {"en": "Equipo Ejemplo"}}],
        "upstream": {"dcs": [{"es-419_gl/es-419_ulb": {"revision": "v1", "timestamp": "2026-07-02T12:00:00.000Z"}}]}
    });
    let files = with_json(sample_files(BIBLE), "metadata.json", |doc| {
        doc["localizedNames"] = added["localizedNames"].clone();
        doc["agencies"] = added["agencies"].clone();
        doc["identification"]["upstream"] = added["upstream"].clone();
    });
    let seed = seed(BIBLE, files).unwrap_or_else(|e| panic!("{e}"));
    let fields = seed
        .fold
        .metadata_source((
            tc_journal::FORMAT_MAJOR.into(),
            tc_journal::FORMAT_MINOR.into(),
        ))
        .fields;
    assert_eq!(fields["localizedNames"], added["localizedNames"]);
    assert_eq!(fields["agencies"], added["agencies"]);
    assert_eq!(fields["identification.upstream"], added["upstream"]);
    assert_eq!(
        fields["identification.name"],
        json!({"en": "Equipo Ejemplo — Tito y Jonás", "es-419": "Equipo Ejemplo — Tito y Jonás"})
    );
    assert_eq!(fields["meta.dateCreated"], "2026-07-02T12:00:00.000Z");
}

// ---- Refusals -----------------------------------------------------------------------------

#[test]
fn an_unknown_top_level_field_refuses() {
    let files = with_json(sample_files(BIBLE), TN, |doc| {
        doc["foo"] = json!(1);
    });
    let found = mismatches(seed(BIBLE, files));
    let tuples: Vec<_> = found.iter().map(tuple).collect();
    assert_eq!(
        tuples,
        [(
            TN.into(),
            None,
            Some("foo".into()),
            CheckRule::DocumentField
        )]
    );
    assert_eq!(found[0].source, "1");
    assert_eq!(found[0].projected, "absent");
}

#[test]
fn a_duplicate_json_key_refuses() {
    let files = with_text(sample_files(BIBLE), ALIGNMENTS, |text| {
        // The verse key "1" twice in chapter 1: serde_json alone would keep the second.
        text.replacen(
            "\"chapters\": {\n    \"1\": {\n",
            "\"chapters\": {\n    \"1\": {\n      \"1\": {},\n",
            1,
        )
    });
    match seed(BIBLE, files) {
        Err(SeedRefusal::DuplicateKey { path, at, key }) => {
            assert_eq!(
                (path.as_str(), at.as_str(), key.as_str()),
                (ALIGNMENTS, "chapters.1", "1")
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn two_decisions_on_one_check_refuse() {
    let files = with_json(sample_files(BIBLE), TN, |doc| {
        let decisions = doc["decisions"].as_array_mut().unwrap();
        let mut again = decisions[0].clone();
        again["contextId"]["quoteString"] = json!("χάρις");
        decisions.push(again);
    });
    match seed(BIBLE, files) {
        Err(SeedRefusal::DuplicateRecord { path, key }) => {
            assert_eq!(path, TN);
            assert_eq!(key, "translationNotes TIT 1:4 gr8c #1");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_decision_against_another_pin_refuses_as_a_mismatch() {
    // Every decision then records a resource the pins do not resolve to, so the fold
    // invalidates swi9, which tC4 showed as valid: a check the translator saw as valid never
    // comes back invalidated without a refusal.
    let files = with_json(sample_files(BIBLE), TN, |doc| {
        doc["resource"]["sha"] = json!("0123456789abcdef0123456789abcdef01234567");
    });
    let found = mismatches(seed(BIBLE, files));
    let tuples: Vec<_> = found.iter().map(tuple).collect();
    let swi9 = Some("translationNotes TIT 1:1 swi9 #1".to_owned());
    assert_eq!(
        tuples,
        [
            (
                TN.into(),
                None,
                Some("resource".into()),
                CheckRule::DocumentField
            ),
            (
                TN.into(),
                swi9.clone(),
                Some("invalidated".into()),
                CheckRule::RecordField
            ),
            (
                TN.into(),
                swi9,
                Some("status".into()),
                CheckRule::RecordField
            ),
        ]
    );
}

/// The Bible sample with no decision files and a `schemaVersion: 1` `resources.json` made of
/// its fallback set, as BURRITO-SPEC §5.3 describes one.
fn with_v1_resources(links: Option<Value>) -> (Files, Value) {
    let mut files = sample_files(BIBLE);
    remove(&mut files, TN);
    remove(&mut files, TW);
    let v2: Value = serde_json::from_slice(&get(&files, RESOURCES)).unwrap();
    let fallback = v2["languageSets"]["fallback"].as_object().unwrap().clone();
    let mut resources = serde_json::Map::new();
    for (slot, pin) in &fallback {
        if slot != "gatewayLanguage" && slot != "translationWordsLinks" {
            resources.insert(slot.clone(), pin.clone());
        }
    }
    if let Some(links) = &links {
        resources.insert("translationWordsLinks".into(), links.clone());
    }
    resources.insert(
        "originalLanguage".into(),
        v2["resources"]["originalLanguage"].clone(),
    );
    resources.insert("lexicon".into(), v2["resources"]["lexicon"].clone());
    let v1 = json!({
        "schemaVersion": 1,
        "gatewayLanguage": fallback["gatewayLanguage"],
        "resources": resources,
        "extraScripture": v2["extraScripture"],
    });
    put(
        &mut files,
        RESOURCES,
        serde_json::to_vec_pretty(&v1).unwrap(),
    );
    (files, v2)
}

#[test]
fn resources_schema_1_migrates_to_two_language_sets() {
    let (files, v2) = with_v1_resources(None);
    let seed = seed(BIBLE, files).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        seed.report
            .declared
            .contains(&DeclaredDifference::ResourcesMigrated { from: 1 })
    );
    let projected = projected_json(&seed, &DerivedFile::Resources);
    assert_eq!(projected["schemaVersion"], 2);
    assert_eq!(
        projected["languageSets"]["primary"],
        projected["languageSets"]["fallback"]
    );
    let links = &projected["languageSets"]["primary"]["translationWordsLinks"];
    assert_eq!(links["repoPath"], "git.door43.org/unfoldingWord/en_twl");
    assert_eq!(links["version"], "v91");
    assert_eq!(links["sha"], "7cbb3419f79780ce84b92ad7d42bd476cc5d8a64");
    assert_eq!(links["flavor"], "parascriptural/x-bcvarticles");
    // Everything else of the set, and the other groups, as the file had them.
    for (slot, pin) in v2["languageSets"]["fallback"].as_object().unwrap() {
        if slot != "translationWordsLinks" {
            assert_eq!(&projected["languageSets"]["fallback"][slot], pin, "{slot}");
        }
    }
    assert_eq!(projected["resources"], v2["resources"]);
    assert_eq!(projected["extraScripture"], v2["extraScripture"]);
}

#[test]
fn resources_schema_1_keeps_a_links_pin_it_has() {
    let links = json!({
        "repoPath": "git.door43.org/unfoldingWord/en_twl",
        "version": "v90",
        "sha": "0123456789abcdef0123456789abcdef01234567",
        "flavor": "parascriptural/x-bcvarticles"
    });
    let (files, _) = with_v1_resources(Some(links.clone()));
    let seed = seed(BIBLE, files).unwrap_or_else(|e| panic!("{e}"));
    let projected = projected_json(&seed, &DerivedFile::Resources);
    assert_eq!(
        projected["languageSets"]["primary"]["translationWordsLinks"],
        links
    );
    assert_eq!(
        projected["languageSets"]["fallback"]["translationWordsLinks"],
        links
    );
}

#[test]
fn an_unknown_resources_schema_refuses() {
    for version in [Some(json!(3)), None, Some(json!("2"))] {
        let files = with_json(sample_files(BIBLE), RESOURCES, |doc| {
            let map = doc.as_object_mut().unwrap();
            match &version {
                Some(v) => {
                    map.insert("schemaVersion".into(), v.clone());
                }
                None => {
                    map.remove("schemaVersion");
                }
            }
        });
        match seed(BIBLE, files) {
            Err(SeedRefusal::Schema { path, .. }) => assert_eq!(path, RESOURCES),
            other => panic!("{version:?}: {other:?}"),
        }
    }
}

#[test]
fn a_project_with_a_journal_is_refused_for_the_tc4_importer() {
    let mut files = sample_files(BIBLE);
    files.push((
        "ingredients/checking/journal/abc/2026-07-02.ndjson".into(),
        b"{}\n".to_vec(),
    ));
    let refusal = seed(BIBLE, files).unwrap_err();
    assert_eq!(refusal, SeedRefusal::HasJournal);
    assert!(refusal.to_string().contains("importer"));
}

#[test]
fn a_folder_of_the_new_format_is_refused() {
    let mut files = sample_files(BIBLE);
    files.push(("ingredients/tc/project.json".into(), b"{}".to_vec()));
    assert_eq!(seed(BIBLE, files).unwrap_err(), SeedRefusal::NewFormat);
    let mut no_metadata = sample_files(BIBLE);
    remove(&mut no_metadata, "metadata.json");
    assert_eq!(
        seed(BIBLE, no_metadata).unwrap_err(),
        SeedRefusal::NoMetadata
    );
}

#[test]
fn unreadable_usfm_refuses_with_its_line() {
    let files = with_text(sample_files(BIBLE), "ingredients/TIT.usfm", |text| {
        text.replacen("\\v 4 a Tito", "\\v 3 a Tito", 1)
    });
    match seed(BIBLE, files) {
        Err(SeedRefusal::Unreadable { path, problem }) => {
            assert_eq!(path, "ingredients/TIT.usfm");
            assert!(problem.starts_with("line 13:"), "{problem}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_book_in_scope_without_its_file_refuses() {
    let files = with_json(sample_files(BIBLE), "metadata.json", |doc| {
        doc["type"]["flavorType"]["currentScope"]["RUT"] = json!([]);
    });
    assert_eq!(
        seed(BIBLE, files).unwrap_err(),
        SeedRefusal::ScopeWithoutFile { book: "RUT".into() }
    );
}

#[test]
fn a_book_file_named_in_lower_case_refuses() {
    let mut files = sample_files(BIBLE);
    files.push((
        "ingredients/tit.usfm".into(),
        get(&files, "ingredients/TIT.usfm"),
    ));
    match seed(BIBLE, files) {
        Err(SeedRefusal::UnexpectedFile { path, .. }) => assert_eq!(path, "ingredients/tit.usfm"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unknown_sidecar_refuses() {
    for path in [
        "ingredients/checking/notes/TIT.json",
        "ingredients/checking/translationNotes/OBS.json",
        "ingredients/content/01.md",
    ] {
        let mut files = sample_files(BIBLE);
        files.push((path.into(), b"{}".to_vec()));
        match seed(BIBLE, files) {
            Err(SeedRefusal::UnexpectedFile { path: refused, .. }) => assert_eq!(refused, path),
            other => panic!("{path}: {other:?}"),
        }
    }
}

#[test]
fn an_invalid_metadata_value_refuses_as_a_mismatch() {
    let files = with_json(sample_files(BIBLE), "metadata.json", |doc| {
        doc["identification"]["name"]["en"] = json!("Equipo Ejemplo ");
    });
    let found = mismatches(seed(BIBLE, files));
    let tuples: BTreeSet<_> = found.iter().map(tuple).collect();
    assert!(
        tuples.contains(&(
            "metadata.json".into(),
            None,
            Some("identification.name".into()),
            CheckRule::MetadataField
        )),
        "{found:?}"
    );
    assert!(
        tuples.contains(&(
            "metadata.json".into(),
            None,
            Some("identification.name".into()),
            CheckRule::FoldAnomaly
        )),
        "{found:?}"
    );
}

// ---- Property ------------------------------------------------------------------------------

fn verse_text() -> impl Strategy<Value = String> {
    "[a-zA-ZáéíóúñÁÉÍÓÚÑüç«»¿¡,.;:!?()'-]{1,12}( [a-zA-ZáéíóúñÁÉÍÓÚÑüç«»¿¡,.;:!?()'-]{1,12}){0,8}"
}

#[derive(Debug, Clone)]
struct Judged {
    comments: Option<String>,
    reminders: bool,
    nothing_to_select: bool,
    status: Option<&'static str>,
}

fn judged() -> impl Strategy<Value = Judged> {
    (
        prop::option::of("[\\p{L} ,.!?]{1,30}"),
        any::<bool>(),
        any::<bool>(),
        prop::option::of(prop::sample::select(vec!["valid", "todo"])),
    )
        .prop_map(|(comments, reminders, nothing_to_select, status)| Judged {
            comments,
            reminders,
            nothing_to_select,
            status,
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_texts_and_decisions_round_trip(
        texts in prop::collection::vec(verse_text(), 5),
        decisions in prop::collection::vec(judged(), 3),
    ) {
        let files = with_text(sample_files(BIBLE), "ingredients/TIT.usfm", |usfm| {
            usfm.lines()
                .map(|line| {
                    for (i, text) in texts.iter().enumerate() {
                        let marker = format!("\\v {} ", i + 1);
                        if line.starts_with(&marker) {
                            return format!("{marker}{text}\n");
                        }
                    }
                    format!("{line}\n")
                })
                .collect()
        });
        let apply = |d: &mut Value, j: &Judged| {
            d["comments"] = j.comments.clone().map_or(json!(false), Value::from);
            d["reminders"] = json!(j.reminders);
            d["nothingToSelect"] = json!(j.nothing_to_select);
            let map = d.as_object_mut().unwrap();
            match j.status {
                Some(s) => {
                    map.insert("status".into(), json!(s));
                }
                None => {
                    map.remove("status");
                }
            }
        };
        let files = with_json(files, TN, |doc| {
            for d in doc["decisions"].as_array_mut().unwrap() {
                if d["contextId"]["checkId"] == "swi9" {
                    apply(d, &decisions[0]);
                }
            }
        });
        let files = with_json(files, TW, |doc| {
            for (d, j) in doc["decisions"].as_array_mut().unwrap().iter_mut().zip(&decisions[1..]) {
                apply(d, j);
            }
        });
        let seeded = seed(BIBLE, files.clone());
        let seeded = match seeded {
            Ok(s) => s,
            Err(e) => return Err(TestCaseError::fail(e.to_string())),
        };
        prop_assert_eq!(
            seeded.fold.book_usfm(BookId::Titus).unwrap().into_bytes(),
            get(&files, "ingredients/TIT.usfm")
        );
        let tn = projected_decisions(&seeded, ToolId::TranslationNotes);
        let swi9 = &tn["swi9"];
        prop_assert_eq!(&swi9["comments"], &decisions[0].comments.clone().map_or(json!(false), Value::from));
        prop_assert_eq!(&swi9["reminders"], &json!(decisions[0].reminders));
        prop_assert_eq!(&swi9["nothingToSelect"], &json!(decisions[0].nothing_to_select));
        prop_assert_eq!(swi9.get("status").and_then(Value::as_str), decisions[0].status);
        prop_assert_eq!(&swi9["invalidated"], &json!(false));
        let key = RecordKey::Verse(
            BookId::Titus,
            tc_journal::ChapterNo::new(1).unwrap(),
            tc_journal::VerseNo::new(1).unwrap(),
        );
        prop_assert!(seeded.fold.projected(&key).is_some());
    }
}
