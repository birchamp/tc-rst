//! Every write path of `docs/STORAGE.md` §5 as a command through the handle: what each one
//! writes (§5.1 to §5.9, §5.15, §5.16), and that a kill at any of its write points, with or
//! without a power cut, leaves its batch whole or absent and a reported save present
//! (§16.2).

use crate::common::*;
use crate::crash::{Outcome, kill_everywhere, outcome_of};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tc_journal::command::{
    AlignmentEdit, Choice, Command, DecisionBasis, DecisionChange, DecisionEdit, Draft, DraftField,
    PlannedChange, RestoreMode, SectionValue, SeedSource, UpgradePlan,
};
use tc_journal::fold::{Anomaly, FoldState};
use tc_journal::keys::{CheckId, CheckKey, CheckTarget, PinGroup, RefPart, SettingKey, ToolId};
use tc_journal::pending::{PendingHeader, Purpose, encode_pending};
use tc_journal::record::{
    AlignmentGroup, BookPresence, CarriedValue, CheckValue, CommentTarget, CommentValue,
    PinGroupValue, PresentHow, ResourceRef, Scope, ScopeRanges, SlotValue, SourceWord, TargetWord,
    UserStatus, VerseValue,
};
use tc_journal::{
    BatchId, BatchRef, BookId, Cause, Change, ChangeBody, ChangeId, ChapterNo, DeviceId, DeviceSeq,
    FORMAT_MINOR, GitOid, ImportSource, Json, JsonMap, Md5, MetaField, ProjectId, Record,
    RecordKey, RepoPath, ShownToken, SlotName, Verified, VerseNo, WallTime,
};
use tc_store::fault::FaultFs;
use tc_store::{CommandError, OpenOptions, ProjectHandle, Refusal, Stored};

const PHM: BookId = BookId::Philemon;
const JUD: BookId = BookId::Jude;
const JN3: BookId = BookId::ThirdJohn;

/// A small literal source for a book's stubs (§5.7): chapters, verses, a span, paragraphs.
fn source(code: &str) -> String {
    format!(
        "\\id {code} source\n\\usfm 3.0\n\\h {code}\n\\mt {code}\n\\c 1\n\\p\n\\v 1 One.\n\\v 2 Two.\n\\v 3-4 Three and four.\n\\p\n\\v 5 Five.\n"
    )
}

fn vkey(book: BookId, c: u16, v: u16) -> RecordKey {
    RecordKey::Verse(book, ChapterNo::new(c).unwrap(), VerseNo::new(v).unwrap())
}

fn tn_key(c: u16, v: u16, occurrence: u32) -> CheckKey {
    CheckKey {
        tool: ToolId::TranslationNotes,
        check_id: CheckId::parse("abcd").unwrap(),
        target: CheckTarget::Verse {
            book: TIT,
            chapter: RefPart::parse(&c.to_string()).unwrap(),
            verse: RefPart::parse(&v.to_string()).unwrap(),
        },
        occurrence,
    }
}

fn oid(n: u8) -> GitOid {
    GitOid::parse(&format!("{:040x}", n)).unwrap()
}

fn tn_resource() -> ResourceRef {
    ResourceRef {
        repo_path: "git.door43.org/unfoldingWord/en_tn".into(),
        sha: oid(7),
    }
}

fn decision_edit(changes: Vec<DecisionChange>) -> DecisionEdit {
    DecisionEdit {
        basis: Some(DecisionBasis {
            context: JsonMap::from([("checkId".to_owned(), Json::from("abcd"))]),
            category: Some("figures".into()),
        }),
        resource: tn_resource(),
        changes,
    }
}

fn submit(handle: &ProjectHandle, command: Command) -> Result<tc_store::StoreReply, CommandError> {
    handle.submit(command).wait()
}

fn token(handle: &ProjectHandle, key: &RecordKey) -> ShownToken {
    handle.mint(key, None).unwrap()
}

fn review(handle: &ProjectHandle, key: &RecordKey) -> ShownToken {
    handle.mint_review(key).unwrap()
}

fn projected(handle: &ProjectHandle, key: &RecordKey) -> Option<Record> {
    handle.snapshot().fold().projected(key)
}

fn project_id(fs: &FaultFs, folder: &Path) -> ProjectId {
    let bytes = fs
        .peek(&folder.join("ingredients/tc/project.json"))
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    ProjectId::parse(json["projectId"].as_str().unwrap()).unwrap()
}

/// A teammate's batch put into `.tc/pending/` as a cloud client or an intake would leave it:
/// `records` written by device `device`, `n` the batch's declared size.
pub(crate) fn teammate_batch(
    fs: &FaultFs,
    folder: &Path,
    device: DeviceId,
    n: u32,
    records: Vec<(Record, Vec<ChangeId>)>,
) -> Vec<ChangeId> {
    let batch = BatchRef::new(BatchId::from_bytes([device.as_bytes()[1]; 16]), n).unwrap();
    let changes: Vec<Change> = records
        .into_iter()
        .enumerate()
        .map(|(i, (record, parents))| {
            Change::new(ChangeBody {
                v: FORMAT_MINOR,
                record,
                parents,
                device,
                seq: DeviceSeq(i as u64 + 1),
                at: WallTime(1_792_195_100_000),
                batch,
                cause: Cause::Edit,
            })
            .unwrap()
        })
        .collect();
    let ids = changes.iter().map(Change::id).collect();
    let header = PendingHeader {
        device,
        project: project_id(fs, folder),
        purpose: Purpose::Save,
        reconciles: None,
    };
    let lines: Vec<Verified> = changes.into_iter().map(Verified::from).collect();
    let file = encode_pending(&header, &lines).unwrap();
    fs.put(
        &folder.join(format!(".tc/pending/{}", file.name)),
        &file.bytes,
        None,
    );
    ids
}

fn alignment_edit(word: &str) -> AlignmentEdit {
    AlignmentEdit {
        alignments: vec![AlignmentGroup {
            top_words: vec![SourceWord {
                word: "Παῦλος".into(),
                strong: None,
                lemma: Some("Παῦλος".into()),
                morph: None,
                occurrence: 1,
                occurrences: 1,
                extra: Default::default(),
            }],
            bottom_words: vec![TargetWord {
                word: word.into(),
                occurrence: 1,
                occurrences: 1,
                extra: Default::default(),
            }],
            extra: Default::default(),
        }],
        word_bank: Vec::new(),
        done: false,
    }
}

fn add_book(handle: &ProjectHandle, book: BookId) -> Result<tc_store::StoreReply, CommandError> {
    submit(
        handle,
        Command::AddBook {
            book,
            scope: Scope::Whole,
            seed: SeedSource {
                usfm: source(book.code()),
            },
        },
    )
}

/// A teammate's device: a Local device other than this folder's.
fn teammate() -> DeviceId {
    DeviceId::local([5u8; 15])
}

/// The project every test below starts from, closed: Titus with an alignment on 1:1, a
/// decision on 1:2, a comment on 1:2, a setting, a carried file, Philemon added, Jude added
/// and removed, a conflict of two of this device's versions of 1:3, a conflict of this
/// device's and a teammate's versions of 1:4, and a teammate's batch of two of which only
/// one change arrived.
pub(crate) fn rich() -> (FaultFs, PathBuf) {
    static BASE: std::sync::OnceLock<(FaultFs, PathBuf)> = std::sync::OnceLock::new();
    let (fs, folder) = BASE.get_or_init(|| {
        let (rig, folder) = titus();
        let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
        save_verse(&handle, 1, 1, "Paul, a servant of God").unwrap();
        submit(
            &handle,
            Command::SaveAlignment {
                shown: token(
                    &handle,
                    &RecordKey::Alignment(
                        TIT,
                        ChapterNo::new(1).unwrap(),
                        VerseNo::new(1).unwrap(),
                    ),
                ),
                verse_shown: token(&handle, &verse(1, 1)),
                value: alignment_edit("Paul"),
            },
        )
        .unwrap();
        save_verse(&handle, 1, 2, "in hope of eternal life").unwrap();
        submit(
            &handle,
            Command::SaveDecision {
                shown: token(&handle, &RecordKey::Check(tn_key(1, 2, 1))),
                verse_shown: token(&handle, &verse(1, 2)),
                value: decision_edit(vec![DecisionChange::Status(Some(UserStatus::Valid))]),
            },
        )
        .unwrap();
        submit(
            &handle,
            Command::AddComment {
                target: CommentTarget::Verse {
                    book: TIT,
                    chapter: ChapterNo::new(1).unwrap(),
                    verse: VerseNo::new(2).unwrap(),
                },
                text: "Check eternal".into(),
            },
        )
        .unwrap();
        let setting = RecordKey::Setting(SettingKey::parse("textDirection").unwrap());
        submit(
            &handle,
            Command::SetSetting {
                shown: token(&handle, &setting),
                value: Some(Json::from("ltr")),
            },
        )
        .unwrap();
        submit(
            &handle,
            Command::ApplyUpgrade {
                plan: UpgradePlan {
                    changes: vec![PlannedChange {
                        record: Record::CarriedFile {
                            path: RepoPath::parse("ingredients/audio/tit-1.mp3").unwrap(),
                            value: CarriedValue::Present {
                                oid: oid(9),
                                size: 3,
                                md5: Md5::of(b"mp3"),
                            },
                        },
                        parents: Vec::new(),
                        cause: Cause::Seed,
                    }],
                },
            },
        )
        .unwrap();
        add_book(&handle, PHM).unwrap();
        add_book(&handle, JUD).unwrap();
        submit(
            &handle,
            Command::RemoveBook {
                book: JUD,
                presence_shown: token(&handle, &RecordKey::BookPresence(JUD)),
            },
        )
        .unwrap();
        // Two of this device's versions of 1:3, both from one displayed token.
        let shown = token(&handle, &verse(1, 3));
        for words in ["One version", "Another version"] {
            submit(
                &handle,
                Command::SaveVerse {
                    shown: shown.clone(),
                    words: words.into(),
                },
            )
            .unwrap();
        }
        let before_teammate = handle.snapshot().fold().heads(&verse(1, 4));
        save_verse(&handle, 1, 4, "My version of four").unwrap();
        handle.wait_idle();
        handle.close().unwrap();
        // A teammate's version of 1:4 from the same base, and one change of a batch of two.
        let Some(Record::Verse { value, .. }) = ({
            let rig = rig_as(rig.fs.copy(), APP, "host-1");
            let h = rig.store.open(&folder, &OpenOptions::default()).unwrap();
            let v = h.snapshot().fold().projected(&verse(1, 4));
            h.close().unwrap();
            v
        }) else {
            panic!("1:4 has a value");
        };
        let VerseValue::Text {
            marker,
            layout,
            last,
            from,
            ..
        } = value
        else {
            panic!("1:4 is text");
        };
        let theirs = Record::Verse {
            book: TIT,
            chapter: ChapterNo::new(1).unwrap(),
            verse: VerseNo::new(4).unwrap(),
            value: VerseValue::Text {
                marker,
                words: "Their version of four".into(),
                layout,
                last,
                from,
            },
        };
        teammate_batch(
            &rig.fs,
            &folder,
            teammate(),
            1,
            vec![(theirs, before_teammate)],
        );
        let half = Record::Comment {
            id: tc_journal::CommentId::from_bytes([4u8; 16]),
            value: CommentValue {
                target: CommentTarget::Verse {
                    book: TIT,
                    chapter: ChapterNo::new(1).unwrap(),
                    verse: VerseNo::new(5).unwrap(),
                },
                text: "Half of a batch".into(),
            },
        };
        teammate_batch(
            &rig.fs,
            &folder,
            DeviceId::local([6u8; 15]),
            2,
            vec![(half, Vec::new())],
        );
        // One open folds and seals them as any open would.
        let reopened = rig.store.open(&folder, &OpenOptions::default()).unwrap();
        reopened.wait_idle();
        reopened.close().unwrap();
        (rig.fs.crash(false), folder)
    });
    (fs.copy(), folder.clone())
}

pub(crate) fn open(fs: &FaultFs, folder: &Path) -> (Rig, ProjectHandle) {
    let rig = rig_as(fs.clone(), APP, "host-1");
    let handle = rig.store.open(folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    (rig, handle)
}

fn conflict_of(fold: &FoldState, key: &RecordKey) -> tc_journal::fold::Conflict {
    fold.conflicts()
        .find(|c| c.key == *key || c.also_keys.contains(key))
        .cloned()
        .unwrap_or_else(|| panic!("no conflict on {key:?}"))
}

/// One command of each kind of §2.7, built from what `handle` shows now. `None` when the
/// command does not apply to the project.
pub(crate) fn every_command(handle: &ProjectHandle) -> Vec<(&'static str, Command)> {
    let snapshot = handle.snapshot();
    let fold = snapshot.fold();
    let mut out: Vec<(&'static str, Command)> = Vec::new();
    out.push((
        "SaveVerse",
        Command::SaveVerse {
            shown: token(handle, &verse(1, 6)),
            words: "if anyone is blameless".into(),
        },
    ));
    // A layout another verse of the chapter has.
    let layout_of = |v: u16| match fold.projected(&verse(1, v)) {
        Some(Record::Verse {
            value: VerseValue::Text { layout, .. },
            ..
        }) => Some(layout),
        _ => None,
    };
    let own = layout_of(7).unwrap();
    if let Some(other) = (1..=16).filter_map(layout_of).find(|l| *l != own) {
        out.push((
            "SaveLayout",
            Command::SaveLayout {
                shown: token(handle, &verse(1, 7)),
                layout: other,
            },
        ));
    }
    // A join of 1:8 and 1:9.
    let (Some(Record::Verse { value: eight, .. }), Some(Record::Verse { .. })) =
        (fold.projected(&verse(1, 8)), fold.projected(&verse(1, 9)))
    else {
        panic!("1:8 and 1:9 exist");
    };
    let VerseValue::Text { marker, layout, .. } = eight else {
        panic!("1:8 is text");
    };
    out.push((
        "SaveSection",
        Command::SaveSection {
            book: TIT,
            shown: vec![
                (
                    token(handle, &verse(1, 8)),
                    Some(SectionValue::Verse(VerseValue::Text {
                        marker,
                        words: "eight and nine".into(),
                        layout,
                        last: VerseNo::new(9).unwrap(),
                        from: None,
                    })),
                ),
                (
                    token(handle, &verse(1, 9)),
                    Some(SectionValue::Verse(VerseValue::Covered {
                        by: VerseNo::new(8).unwrap(),
                    })),
                ),
            ],
            layout: None,
        },
    ));
    let a11 = RecordKey::Alignment(TIT, ChapterNo::new(1).unwrap(), VerseNo::new(1).unwrap());
    out.push((
        "SaveAlignment",
        Command::SaveAlignment {
            shown: token(handle, &a11),
            verse_shown: token(handle, &verse(1, 1)),
            value: alignment_edit("servant"),
        },
    ));
    out.push((
        "SaveDecision",
        Command::SaveDecision {
            shown: token(handle, &RecordKey::Check(tn_key(1, 2, 1))),
            verse_shown: token(handle, &verse(1, 2)),
            value: decision_edit(vec![DecisionChange::Bookmarked(true)]),
        },
    ));
    out.push((
        "AddComment",
        Command::AddComment {
            target: CommentTarget::Verse {
                book: TIT,
                chapter: ChapterNo::new(1).unwrap(),
                verse: VerseNo::new(6).unwrap(),
            },
            text: "Another comment".into(),
        },
    ));
    let comment = fold
        .record_keys()
        .into_iter()
        .find(|k| {
            matches!(k, RecordKey::Comment(_))
                && matches!(fold.projected(k), Some(Record::Comment { value, .. }) if value.text == "Check eternal")
        })
        .unwrap();
    out.push((
        "ReviseComment",
        Command::ReviseComment {
            shown: token(handle, &comment),
            text: "Check the word eternal".into(),
        },
    ));
    let setting = RecordKey::Setting(SettingKey::parse("textDirection").unwrap());
    out.push((
        "SetSetting",
        Command::SetSetting {
            shown: token(handle, &setting),
            value: Some(Json::from("rtl")),
        },
    ));
    out.push((
        "SetMeta",
        Command::SetMeta {
            shown: token(handle, &RecordKey::ProjectMeta(MetaField::Name)),
            value: Some(Json::Obj(JsonMap::from([(
                "en".to_owned(),
                Json::from("Titus, revised"),
            )]))),
        },
    ));
    let pins = RecordKey::Pins(PinGroup::Lexicon);
    out.push((
        "SetPins",
        Command::SetPins {
            shown: token(handle, &pins),
            value: PinGroupValue(BTreeMap::from([(
                SlotName::parse("ugl").unwrap(),
                SlotValue(Json::Obj(JsonMap::from([
                    (
                        "repoPath".to_owned(),
                        Json::from("git.door43.org/unfoldingWord/en_ugl"),
                    ),
                    ("sha".to_owned(), Json::from(oid(3).to_hex())),
                    ("version".to_owned(), Json::from("v1")),
                ]))),
            )])),
        },
    ));
    out.push((
        "AddBook",
        Command::AddBook {
            book: JN3,
            scope: Scope::Whole,
            seed: SeedSource {
                usfm: source("3JN"),
            },
        },
    ));
    out.push((
        "AddBookText",
        Command::AddBookText {
            book: BookId::SecondJohn,
            scope: Scope::Whole,
            usfm: ult("TIT"),
            file_name: "2JN.usfm".into(),
        },
    ));
    out.push((
        "RemoveBook",
        Command::RemoveBook {
            book: PHM,
            presence_shown: token(handle, &RecordKey::BookPresence(PHM)),
        },
    ));
    for (name, mode) in [
        ("RestoreBook KeepText", RestoreMode::KeepText),
        ("RestoreBook StartEmpty", RestoreMode::StartEmpty),
    ] {
        out.push((
            name,
            Command::RestoreBook {
                book: JUD,
                presence_shown: token(handle, &RecordKey::BookPresence(JUD)),
                mode,
            },
        ));
    }
    out.push((
        "SetBookScope",
        Command::SetBookScope {
            presence_shown: token(handle, &RecordKey::BookPresence(PHM)),
            scope: Scope::Ranges(ScopeRanges::new(vec!["1:1-2".parse().unwrap()]).unwrap()),
        },
    ));
    out.push((
        "ApplyUpgrade",
        Command::ApplyUpgrade {
            plan: UpgradePlan {
                changes: vec![PlannedChange {
                    record: Record::Check {
                        key: tn_key(1, 2, 1),
                        value: match fold.projected(&RecordKey::Check(tn_key(1, 2, 1))) {
                            Some(Record::Check {
                                value: CheckValue::Decision(mut d),
                                ..
                            }) => {
                                d.resource = ResourceRef {
                                    repo_path: d.resource.repo_path.clone(),
                                    sha: oid(8),
                                };
                                CheckValue::Decision(d)
                            }
                            other => panic!("{other:?}"),
                        },
                    },
                    parents: fold.heads(&RecordKey::Check(tn_key(1, 2, 1))),
                    cause: Cause::CarryOver {
                        group: PinGroup::Primary,
                    },
                }],
            },
        },
    ));
    let mine = conflict_of(fold, &verse(1, 3));
    out.push((
        "Resolve",
        Command::Resolve {
            conflict: mine.id,
            shown: vec![review(handle, &verse(1, 3))],
            choice: Choice::Candidate {
                head: mine.candidates[0].head,
            },
        },
    ));
    let shared = conflict_of(fold, &verse(1, 4));
    out.push((
        "ResolveBulk",
        Command::ResolveBulk {
            devices: BTreeSet::from([handle.facts().device]),
            conflicts: vec![(shared.id, vec![review(handle, &verse(1, 4))])],
        },
    ));
    out.push((
        "MoveCheck",
        Command::MoveCheck {
            shown: token(handle, &RecordKey::Check(tn_key(1, 2, 1))),
            to: tn_key(1, 2, 2),
            to_shown: token(handle, &RecordKey::Check(tn_key(1, 2, 2))),
        },
    ));
    let first = fold.history(&verse(1, 1))[0];
    out.push((
        "Restore",
        Command::Restore {
            shown: token(handle, &verse(1, 1)),
            from: first,
        },
    ));
    out.push((
        "RemoveCarriedFile",
        Command::RemoveCarriedFile {
            shown: token(
                handle,
                &RecordKey::CarriedFile(RepoPath::parse("ingredients/audio/tit-1.mp3").unwrap()),
            ),
        },
    ));
    let held = fold
        .anomalies()
        .find_map(|a| match a {
            Anomaly::IncompleteBatch { batch, .. } => Some(*batch),
            _ => None,
        })
        .expect("the half batch is held");
    out.push(("ReleaseBatch", Command::ReleaseBatch { batch: held }));
    out.push((
        "Dismiss",
        Command::Dismiss {
            shown: token(handle, &a11),
        },
    ));
    out.push((
        "KeepDraft",
        Command::KeepDraft {
            draft: Draft {
                field: DraftField::Words,
                shown: vec![token(handle, &verse(1, 10))],
                text: "a draft kept".into(),
            },
            section: Vec::new(),
        },
    ));
    out
}

/// Every change the fold holds: those it applies, and those of batches it holds back.
fn all_ids(fold: &FoldState) -> BTreeSet<ChangeId> {
    let held: Vec<BatchId> = fold
        .anomalies()
        .filter_map(|a| match a {
            Anomaly::IncompleteBatch { batch, .. } => Some(*batch),
            _ => None,
        })
        .collect();
    fold.record_keys()
        .iter()
        .flat_map(|k| fold.history(k))
        .chain(held.into_iter().flat_map(|b| fold.batch_changes(b)))
        .collect()
}

/// The kind of a record, for comparing what two runs of one command wrote.
fn kind(key: &RecordKey) -> String {
    let text = format!("{key:?}");
    text.split(['(', ' ', '{'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// What one command wrote over `before`: the kinds of its records, sorted. Panics unless
/// the new changes are whole batches with every change present.
fn written(name: &str, before: &BTreeSet<ChangeId>, fold: &FoldState) -> Vec<String> {
    let new: Vec<ChangeId> = all_ids(fold).difference(before).copied().collect();
    let mut batches: BTreeMap<BatchRef, u32> = BTreeMap::new();
    let mut kinds = Vec::new();
    for id in &new {
        let body = fold.body(id).unwrap();
        *batches.entry(body.batch).or_default() += 1;
        kinds.push(kind(&body.record.key()));
    }
    assert!(
        batches.len() <= 1,
        "{name} wrote {} batches: {kinds:?}",
        batches.len()
    );
    for (batch, have) in &batches {
        assert_eq!(*have, batch.n(), "a half batch: {have} of {}", batch.n());
    }
    assert!(
        !fold
            .anomalies()
            .any(|a| matches!(a, Anomaly::IncompleteBatch { batch, .. } if batches.keys().any(|b| b.id == *batch))),
        "the batch is listed as incomplete"
    );
    kinds.sort();
    kinds
}

/// The names of every command of [`every_command`] on the `rich` project, and every change it
/// holds before any of them runs.
fn every_command_name(base: &FaultFs, folder: &Path) -> (Vec<&'static str>, BTreeSet<ChangeId>) {
    let (rig, handle) = open(base, folder);
    let names: Vec<&'static str> = every_command(&handle).into_iter().map(|(n, _)| n).collect();
    let before = all_ids(handle.snapshot().fold());
    handle.close().unwrap();
    drop(rig);
    assert!(names.len() >= 24, "{names:?}");
    (names, before)
}

/// What `name` writes on a copy of `base` when nothing stops it: one whole batch, durable.
fn run_unstopped(
    base: &FaultFs,
    folder: &Path,
    name: &str,
    before: &BTreeSet<ChangeId>,
) -> Vec<String> {
    let (rig, handle) = open(&base.copy(), folder);
    let command = every_command(&handle)
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap()
        .1;
    let reply = submit(&handle, command).unwrap_or_else(|e| panic!("{name}: {e:?}"));
    assert_eq!(reply.stored, Stored::Durable, "{name}");
    let after = written(name, before, handle.snapshot().fold());
    assert!(!after.is_empty(), "{name} wrote nothing");
    handle.close().unwrap();
    drop(rig);
    after
}

/// Every command of §2.7 writes one whole batch, durable before its reply (§16.2). The kill
/// at each write point is `crash_every_command_at_every_write_point_nightly`: every command
/// writes through the one pending-file write, whose kill sweeps run in the gate as
/// `crash::crash_save_verse_at_every_write_point`, `crash::crash_section_save_is_all_or_nothing`
/// and `adding_a_book_from_usfm_is_one_batch_whole_or_absent`.
#[test]
fn every_command_writes_one_whole_durable_batch() {
    let (base, folder) = rich();
    let (names, before) = every_command_name(&base, &folder);
    for name in names {
        run_unstopped(&base, &folder, name, &before);
    }
}

/// Every command, killed before each of its write points with and without power loss: the
/// reopened project holds the command's whole batch or none of it, and a command that
/// replied "Saved" holds it after a power cut too (§16.2, rows W1 to W4).
#[test]
#[ignore = "nightly: 26 commands share one write path; every_command_writes_one_whole_durable_batch and crash::crash_save_verse_at_every_write_point catch its bugs in the gate"]
fn crash_every_command_at_every_write_point_nightly() {
    let (base, folder) = rich();
    let (names, before) = every_command_name(&base, &folder);
    for name in names {
        let after = run_unstopped(&base, &folder, name, &before);
        let points = kill_everywhere(
            &base,
            &folder,
            &|handle| {
                let command = every_command(handle)
                    .into_iter()
                    .find(|(n, _)| *n == name)
                    .unwrap()
                    .1;
                outcome_of(submit(handle, command))
            },
            &|handle, outcome| {
                let got = written(name, &before, handle.snapshot().fold());
                match outcome {
                    Outcome::Saved => assert_eq!(got, after, "{name}: a saved batch is gone"),
                    Outcome::NotSaved | Outcome::SavedWithDiskError => assert!(
                        got.is_empty() || got == after,
                        "{name}: neither before nor after: {got:?}"
                    ),
                }
            },
        );
        assert!(points >= 4, "{name}: {points} kill points");
    }
}

// ---------------------------------------------------------------------------------------
// What each command writes
// ---------------------------------------------------------------------------------------

#[test]
fn a_layout_save_changes_only_the_layout() {
    let (fs, folder) = rich();
    let (_rig, handle) = open(&fs, &folder);
    let before = projected(&handle, &verse(1, 7));
    let (_, command) = every_command(&handle)
        .into_iter()
        .find(|(n, _)| *n == "SaveLayout")
        .unwrap();
    let Command::SaveLayout { layout, .. } = &command else {
        panic!("unexpected")
    };
    let layout = layout.clone();
    submit(&handle, command).unwrap();
    let (
        Some(Record::Verse {
            value:
                VerseValue::Text {
                    words: w0,
                    marker: m0,
                    last: l0,
                    ..
                },
            ..
        }),
        Some(Record::Verse {
            value:
                VerseValue::Text {
                    words: w1,
                    marker: m1,
                    last: l1,
                    layout: new,
                    ..
                },
            ..
        }),
    ) = (before, projected(&handle, &verse(1, 7)))
    else {
        panic!("1:7 is text");
    };
    assert_eq!((w0, m0, l0), (w1, m1, l1));
    assert_eq!(new, layout);
    handle.close().unwrap();
}

#[test]
fn a_judgement_records_the_text_it_was_made_against_and_to_do_stamps_nothing() {
    let (fs, folder) = rich();
    let (_rig, handle) = open(&fs, &folder);
    let key = RecordKey::Check(tn_key(1, 2, 1));
    let decision = |handle: &ProjectHandle| match projected(handle, &key) {
        Some(Record::Check {
            value: CheckValue::Decision(d),
            ..
        }) => d,
        other => panic!("{other:?}"),
    };
    let judged = decision(&handle);
    assert_eq!(judged.judgement.status, Some(UserStatus::Valid));
    assert_eq!(
        judged.judgement.verse_text_md5,
        tc_journal::decision::verse_text_md5_v1("in hope of eternal life")
    );
    assert_eq!(judged.resource, tn_resource());
    // The verse changes; To do changes only the status (§5.4).
    save_verse(&handle, 1, 2, "in the hope of life eternal").unwrap();
    submit(
        &handle,
        Command::SaveDecision {
            shown: token(&handle, &key),
            verse_shown: token(&handle, &verse(1, 2)),
            value: decision_edit(vec![DecisionChange::Status(Some(UserStatus::Todo))]),
        },
    )
    .unwrap();
    let todo = decision(&handle);
    assert_eq!(todo.judgement.status, Some(UserStatus::Todo));
    assert_eq!(
        todo.judgement.verse_text_md5,
        judged.judgement.verse_text_md5
    );
    handle.close().unwrap();
}

#[test]
fn a_project_detail_that_would_break_the_burrito_is_refused_in_words() {
    let (fs, folder) = rich();
    let (_rig, handle) = open(&fs, &folder);
    let result = submit(
        &handle,
        Command::SetMeta {
            shown: token(&handle, &RecordKey::ProjectMeta(MetaField::Name)),
            value: Some(Json::from("a name with a trailing space ")),
        },
    );
    assert!(
        matches!(
            result,
            Err(CommandError::Refused(Refusal::NotValidInBurrito(_)))
        ),
        "{result:?}"
    );
    handle.close().unwrap();
}

#[test]
fn a_book_is_added_with_stubs_removed_with_what_was_seen_and_restored_either_way() {
    let (fs, folder) = rich();
    let (rig, handle) = open(&fs, &folder);
    // Added: presence, header, fronts and a stub per verse; the span stays a span.
    assert!(handle.snapshot().fold().book(PHM).projects());
    match projected(&handle, &vkey(PHM, 1, 3)) {
        Some(Record::Verse {
            value: VerseValue::Text { words, last, .. },
            ..
        }) => {
            assert_eq!(words, "___");
            assert_eq!(last.get(), 4);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        projected(&handle, &vkey(PHM, 1, 4)),
        Some(Record::Verse {
            value: VerseValue::Covered { .. },
            ..
        })
    ));
    // Adding it again is refused.
    assert!(matches!(
        add_book(&handle, PHM),
        Err(CommandError::Refused(Refusal::Book(_)))
    ));
    // Removed: the change lists every head of the book it saw; the records stay.
    let heads_of_book: BTreeSet<ChangeId> = handle
        .snapshot()
        .fold()
        .record_keys()
        .into_iter()
        .filter(|k| k.book() == Some(PHM) && !matches!(k, RecordKey::BookPresence(_)))
        .flat_map(|k| handle.snapshot().fold().heads(&k))
        .collect();
    let reply = submit(
        &handle,
        Command::RemoveBook {
            book: PHM,
            presence_shown: token(&handle, &RecordKey::BookPresence(PHM)),
        },
    )
    .unwrap();
    let body = handle.snapshot().fold().body(&reply.changes[0]).unwrap();
    let Record::BookPresence {
        value: BookPresence::Removed { seen },
        ..
    } = body.record
    else {
        panic!("{:?}", body.record);
    };
    assert_eq!(seen.len(), heads_of_book.len());
    assert!(!handle.snapshot().fold().book(PHM).projects());
    handle.leave(tc_store::Leave::Mode).wait().unwrap();
    assert!(rig.fs.peek(&folder.join("ingredients/PHM.usfm")).is_none());
    assert_valid_burrito(&rig.fs, &folder);
    // Restored with its text.
    submit(
        &handle,
        Command::RestoreBook {
            book: PHM,
            presence_shown: token(&handle, &RecordKey::BookPresence(PHM)),
            mode: RestoreMode::KeepText,
        },
    )
    .unwrap();
    assert!(matches!(
        projected(&handle, &RecordKey::BookPresence(PHM)),
        Some(Record::BookPresence {
            value: BookPresence::Present {
                how: PresentHow::RestoredText,
                ..
            },
            ..
        })
    ));
    // Jude, removed in the base, restored empty: every verse a StartOver change whose
    // parents keep the old text in History.
    let jude_heads = handle.snapshot().fold().heads(&vkey(JUD, 1, 1));
    let reply = submit(
        &handle,
        Command::RestoreBook {
            book: JUD,
            presence_shown: token(&handle, &RecordKey::BookPresence(JUD)),
            mode: RestoreMode::StartEmpty,
        },
    )
    .unwrap();
    let fold = handle.snapshot().fold().clone();
    let restarted = reply
        .changes
        .iter()
        .filter_map(|id| fold.body(id))
        .find(|b| b.record.key() == vkey(JUD, 1, 1))
        .unwrap();
    assert!(matches!(restarted.cause, Cause::StartOver));
    assert_eq!(restarted.parents, jude_heads);
    handle.close().unwrap();
}

fn add_text(book: BookId, usfm: String) -> Command {
    Command::AddBookText {
        book,
        scope: Scope::Whole,
        usfm,
        file_name: format!("{}.usfm", book.code()),
    }
}

/// The keys of every record of `book` the fold holds.
fn book_keys(handle: &ProjectHandle, book: BookId) -> Vec<RecordKey> {
    handle
        .snapshot()
        .fold()
        .record_keys()
        .into_iter()
        .filter(|k| k.book() == Some(book))
        .collect()
}

/// tC4 #767 (`addUsfmBooks`): the presence, the verses and the alignments are one batch. A
/// kill at any write point leaves no book, or the whole book with its alignments; never the
/// stub book tC4 left, which every retry refused as "already in the project".
#[test]
fn adding_a_book_from_usfm_is_one_batch_whole_or_absent() {
    let (base, folder) = rich();
    let book = BookId::SecondJohn;
    let whole = {
        let (_rig, handle) = open(&base.copy(), &folder);
        submit(&handle, add_text(book, ult("TIT"))).unwrap();
        book_keys(&handle, book)
    };
    assert!(whole.len() > 46 + 46, "{}", whole.len());
    let points = kill_everywhere(
        &base,
        &folder,
        &|handle| outcome_of(submit(handle, add_text(book, ult("TIT")))),
        &|handle, outcome| {
            let got = book_keys(handle, book);
            if outcome == Outcome::Saved {
                assert_eq!(got, whole, "a saved book is gone");
            }
            assert!(got.is_empty() || got == whole, "half a book: {}", got.len());
            if got.is_empty() {
                let fold = handle.snapshot().fold().clone();
                assert!(
                    fold.heads(&RecordKey::BookPresence(book)).is_empty(),
                    "no presence, so a retry is not refused"
                );
            }
        },
    );
    assert!(points >= 4, "{points} kill points");
}

/// tC4 #553: an aligned file keeps its alignments. Every verse of en_ult Titus splits into its
/// plain text and an alignment record, and no verse keeps inline `\zaln` markup.
#[test]
fn aligned_titus_added_as_a_book_has_no_inline_zaln_and_46_records() {
    let (base, folder) = rich();
    let (_rig, handle) = open(&base, &folder);
    let book = BookId::SecondJohn;
    let reply = submit(&handle, add_text(book, ult("TIT"))).unwrap();
    let fold = handle.snapshot().fold().clone();
    let (mut alignments, mut verses) = (0, 0);
    for key in book_keys(&handle, book) {
        match fold.projected(&key) {
            Some(Record::Alignment {
                value,
                chapter,
                verse,
                ..
            }) => {
                alignments += 1;
                let Some(Record::Verse {
                    value: VerseValue::Text { words, layout, .. },
                    ..
                }) = fold.projected(&vkey(book, chapter.get(), verse.get()))
                else {
                    continue;
                };
                assert_eq!(
                    value.target_verse_md5,
                    tc_journal::decision::verse_text_md5_v1(&tc_usfm::codec::join_verse(
                        &words, &layout
                    ))
                );
            }
            Some(Record::Verse {
                value: VerseValue::Text { words, layout, .. },
                ..
            }) => {
                verses += 1;
                assert!(
                    !words.contains("\\zaln") && !layout.contains("\\zaln"),
                    "{words}"
                );
            }
            _ => {}
        }
    }
    assert_eq!((alignments, verses), (46, 46));
    for id in &reply.changes {
        let body = fold.body(id).unwrap();
        assert!(
            matches!(&body.cause, Cause::Import(ImportSource::Usfm { file_name, unknown_base: false }) if &**file_name == "2JN.usfm"),
            "{:?}",
            body.cause
        );
    }
    // The text and the alignment are the file's: nothing the file said is dropped.
    let (words, layout) = match fold.projected(&vkey(book, 1, 1)) {
        Some(Record::Verse {
            value: VerseValue::Text { words, layout, .. },
            ..
        }) => (words, layout),
        other => panic!("{other:?}"),
    };
    assert!(tc_usfm::codec::join_verse(&words, &layout).starts_with("Paul, a servant"));
}

/// tC4 #767 (DELTA C37): a poetry book adds whole, every verse present with its text, and its
/// alignments split out; the Selah inside `\qs` stays in the verse.
#[test]
fn a_psalm_adds_whole_with_every_verse_and_its_selah() {
    let (base, folder) = rich();
    let (_rig, handle) = open(&base, &folder);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/tc4/test-fixtures/en_ult/PSA-3.usfm");
    let usfm = std::fs::read_to_string(path).unwrap();
    submit(&handle, add_text(BookId::Psalms, usfm)).unwrap();
    let fold = handle.snapshot().fold().clone();
    let mut words = Vec::new();
    for v in 1..=2 {
        match fold.projected(&vkey(BookId::Psalms, 3, v)) {
            Some(Record::Verse {
                value: VerseValue::Text {
                    words: w, layout, ..
                },
                ..
            }) => {
                assert!(
                    !w.contains("\\zaln") && !layout.contains("\\zaln"),
                    "verse {v}"
                );
                words.push(w);
            }
            other => panic!("verse {v}: {other:?}"),
        }
    }
    assert!(words.iter().any(|w| w.contains("Selah")), "{words:?}");
    let alignments = book_keys(&handle, BookId::Psalms)
        .iter()
        .filter(|k| matches!(k, RecordKey::Alignment(..)))
        .count();
    assert_eq!(alignments, 2);
}

/// §13.5: a verse whose markup does not read apart keeps its text as written, and the rest
/// of the book still adds.
#[test]
fn a_verse_whose_alignment_does_not_read_apart_keeps_its_text_as_written() {
    let (base, folder) = rich();
    let (_rig, handle) = open(&base, &folder);
    let odd = "\\zaln-s |x-strong=\"G1\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"a\"\\*\\w Paul|x-occurrence=\"1\" x-occurrences=\"1\" x-lemma=\"p\"\\w*\\zaln-e\\*";
    let usfm = format!("\\id 2JN\n\\c 1\n\\p\n\\v 1 {odd}\n\\v 2 plain words\n");
    submit(&handle, add_text(BookId::SecondJohn, usfm)).unwrap();
    let fold = handle.snapshot().fold().clone();
    match fold.projected(&vkey(BookId::SecondJohn, 1, 1)) {
        Some(Record::Verse {
            value: VerseValue::Text { words, layout, .. },
            ..
        }) => assert!(
            tc_usfm::codec::join_verse(&words, &layout).contains("\\zaln-s"),
            "{words}"
        ),
        other => panic!("{other:?}"),
    }
    assert!(
        book_keys(&handle, BookId::SecondJohn)
            .iter()
            .all(|k| !matches!(k, RecordKey::Alignment(..)))
    );
    assert!(matches!(
        fold.projected(&vkey(BookId::SecondJohn, 1, 2)),
        Some(Record::Verse { .. })
    ));
}

/// tC4 #484: a book the project holds, or removed, is refused with nothing written; a file
/// that does not cut into chapters and verses is refused before anything is written.
#[test]
fn a_book_from_a_file_is_refused_when_present_or_when_the_file_does_not_read() {
    let (base, folder) = rich();
    let (_rig, handle) = open(&base, &folder);
    let before = handle.snapshot().fold().record_keys().len();
    for (book, usfm) in [
        (PHM, source("PHM")),
        (JUD, source("JUD")),
        (BookId::SecondJohn, "not a USFM file at all".to_owned()),
    ] {
        let result = submit(&handle, add_text(book, usfm));
        assert!(
            matches!(result, Err(CommandError::Refused(Refusal::Book(_)))),
            "{book:?}: {result:?}"
        );
    }
    assert_eq!(handle.snapshot().fold().record_keys().len(), before);
}

/// A scope the burrito would refuse cannot be built (`ScopeRange` parses only what it holds),
/// so the refusal a person can reach is a scope for a book the project no longer holds.
#[test]
fn a_book_scope_is_set_and_one_for_a_removed_book_is_refused() {
    let (fs, folder) = rich();
    let (rig, handle) = open(&fs, &folder);
    let (_, command) = every_command(&handle)
        .into_iter()
        .find(|(n, _)| *n == "SetBookScope")
        .unwrap();
    submit(&handle, command).unwrap();
    assert!(matches!(
        projected(&handle, &RecordKey::BookPresence(PHM)),
        Some(Record::BookPresence {
            value: BookPresence::Present {
                scope: Scope::Ranges(_),
                how: PresentHow::Added,
            },
            ..
        })
    ));
    let before = all_ids(handle.snapshot().fold());
    let removed = submit(
        &handle,
        Command::SetBookScope {
            presence_shown: token(&handle, &RecordKey::BookPresence(JUD)),
            scope: Scope::Ranges(ScopeRanges::new(vec!["1:1-2".parse().unwrap()]).unwrap()),
        },
    );
    assert!(
        matches!(removed, Err(CommandError::Refused(Refusal::Book(_)))),
        "{removed:?}"
    );
    assert_eq!(
        all_ids(handle.snapshot().fold()),
        before,
        "nothing is written"
    );
    handle.leave(tc_store::Leave::Mode).wait().unwrap();
    let metadata: serde_json::Value =
        serde_json::from_slice(&rig.fs.peek(&folder.join("metadata.json")).unwrap()).unwrap();
    assert_eq!(
        metadata["type"]["flavorType"]["currentScope"]["PHM"],
        serde_json::json!(["1:1-2"])
    );
    handle.close().unwrap();
}

#[test]
fn an_upgrade_plan_made_from_an_older_snapshot_is_made_again() {
    let (fs, folder) = rich();
    let (_rig, handle) = open(&fs, &folder);
    let (_, command) = every_command(&handle)
        .into_iter()
        .find(|(n, _)| *n == "ApplyUpgrade")
        .unwrap();
    // A decision of the plan changes before the plan is applied.
    submit(
        &handle,
        Command::SaveDecision {
            shown: token(&handle, &RecordKey::Check(tn_key(1, 2, 1))),
            verse_shown: token(&handle, &verse(1, 2)),
            value: decision_edit(vec![DecisionChange::Comment(Some("later".into()))]),
        },
    )
    .unwrap();
    let before = all_ids(handle.snapshot().fold());
    assert!(matches!(
        submit(&handle, command),
        Err(CommandError::PlanStale)
    ));
    assert_eq!(
        all_ids(handle.snapshot().fold()),
        before,
        "nothing is written"
    );
    handle.close().unwrap();
}

#[test]
fn a_resolution_keeps_the_chosen_version_and_names_every_head() {
    let (fs, folder) = rich();
    let (_rig, handle) = open(&fs, &folder);
    let fold = handle.snapshot().fold().clone();
    let conflict = conflict_of(&fold, &verse(1, 3));
    let chosen = conflict.candidates[1].clone();
    // An edit token is no resolution, and a stale review token is "look again".
    assert!(matches!(
        submit(
            &handle,
            Command::Resolve {
                conflict: conflict.id,
                shown: vec![token(&handle, &verse(1, 3))],
                choice: Choice::Candidate { head: chosen.head },
            }
        ),
        Err(CommandError::Refused(Refusal::WrongTokenKind))
    ));
    let review_token = review(&handle, &verse(1, 3));
    let reply = submit(
        &handle,
        Command::Resolve {
            conflict: conflict.id,
            shown: vec![review_token.clone()],
            choice: Choice::Candidate { head: chosen.head },
        },
    )
    .unwrap();
    let body = handle.snapshot().fold().body(&reply.changes[0]).unwrap();
    assert!(matches!(body.cause, Cause::Resolve));
    let mut heads = fold.heads(&verse(1, 3));
    heads.sort();
    assert_eq!(body.parents, heads);
    assert_eq!(projected(&handle, &verse(1, 3)), Some(chosen.value));
    assert!(
        handle
            .snapshot()
            .fold()
            .conflicts()
            .all(|c| c.id != conflict.id)
    );
    // The same choice again, from the screen that showed the conflict: look again.
    assert!(matches!(
        submit(
            &handle,
            Command::Resolve {
                conflict: conflict.id,
                shown: vec![review_token],
                choice: Choice::Candidate { head: chosen.head },
            }
        ),
        Err(CommandError::LookAgain)
    ));
    handle.close().unwrap();
}

#[test]
fn a_bulk_choice_by_this_computer_keeps_its_versions_and_skips_conflicts_inside_it() {
    let (fs, folder) = rich();
    let (_rig, handle) = open(&fs, &folder);
    let fold = handle.snapshot().fold().clone();
    let shared = conflict_of(&fold, &verse(1, 4));
    let own = conflict_of(&fold, &verse(1, 3));
    let reply = submit(
        &handle,
        Command::ResolveBulk {
            devices: BTreeSet::from([handle.facts().device]),
            conflicts: vec![
                (shared.id, vec![review(&handle, &verse(1, 4))]),
                (own.id, vec![review(&handle, &verse(1, 3))]),
            ],
        },
    )
    .unwrap();
    // 1:3's two versions are both this computer's: left for a choice of its own (L78).
    assert_eq!(reply.skipped, vec![own.id]);
    assert_eq!(words(&handle, 1, 4).as_deref(), Some("My version of four"));
    handle.close().unwrap();
}

/// Ruling H5: the newest version wins, and an older one comes back from History. A decision
/// made before its check moved is listed in the History of the check it moved to, and
/// restoring it writes it there.
#[test]
fn a_moved_checks_older_decision_is_restored() {
    let (fs, folder) = rich();
    let (_rig, handle) = open(&fs, &folder);
    let old = RecordKey::Check(tn_key(1, 2, 1));
    let new = RecordKey::Check(tn_key(1, 2, 2));
    let older = handle.snapshot().fold().projected_change(&old).unwrap();
    let Some(Record::Check {
        value: older_value, ..
    }) = projected(&handle, &old)
    else {
        panic!("a decision");
    };
    submit(
        &handle,
        Command::SaveDecision {
            shown: token(&handle, &old),
            verse_shown: token(&handle, &verse(1, 2)),
            value: decision_edit(vec![DecisionChange::Status(Some(UserStatus::Todo))]),
        },
    )
    .unwrap();
    let Some(Record::Check {
        value: CheckValue::Decision(was),
        ..
    }) = projected(&handle, &old)
    else {
        panic!("a decision");
    };
    submit(
        &handle,
        Command::MoveCheck {
            shown: token(&handle, &old),
            to: tn_key(1, 2, 2),
            to_shown: token(&handle, &new),
        },
    )
    .unwrap();
    // The decision moves whole under the new key.
    assert_eq!(
        projected(&handle, &new),
        Some(Record::Check {
            key: tn_key(1, 2, 2),
            value: CheckValue::Decision(was),
        })
    );
    let fold = handle.snapshot().fold().clone();
    assert_eq!(fold.moved_into(&tn_key(1, 2, 2)), vec![tn_key(1, 2, 1)]);
    assert!(fold.history(&new).contains(&older));
    submit(
        &handle,
        Command::Restore {
            shown: token(&handle, &new),
            from: older,
        },
    )
    .unwrap();
    assert_eq!(
        projected(&handle, &new),
        Some(Record::Check {
            key: tn_key(1, 2, 2),
            value: older_value,
        })
    );
    // The check it moved from still says where it went.
    assert!(matches!(
        projected(&handle, &old),
        Some(Record::Check {
            value: CheckValue::MovedTo { .. },
            ..
        })
    ));
    handle.close().unwrap();
}

#[test]
fn a_restored_version_continues_what_was_shown() {
    let (fs, folder) = rich();
    let (_rig, handle) = open(&fs, &folder);
    let fold = handle.snapshot().fold().clone();
    let first = fold.history(&verse(1, 1))[0];
    let first_value = fold.body(&first).unwrap().record;
    let reply = submit(
        &handle,
        Command::Restore {
            shown: token(&handle, &verse(1, 1)),
            from: first,
        },
    )
    .unwrap();
    let body = handle.snapshot().fold().body(&reply.changes[0]).unwrap();
    assert_eq!(body.cause, Cause::Restore { from: first });
    assert_eq!(body.parents, fold.heads(&verse(1, 1)));
    assert_eq!(projected(&handle, &verse(1, 1)), Some(first_value));
    // A version of another record is refused.
    assert!(
        submit(
            &handle,
            Command::Restore {
                shown: token(&handle, &verse(1, 2)),
                from: first,
            }
        )
        .is_err()
    );
    handle.close().unwrap();
}

#[test]
fn a_carried_file_is_removed_only_by_its_command() {
    let (fs, folder) = rich();
    let (_rig, handle) = open(&fs, &folder);
    let key = RecordKey::CarriedFile(RepoPath::parse("ingredients/audio/tit-1.mp3").unwrap());
    let heads = handle.snapshot().fold().heads(&key);
    let reply = submit(
        &handle,
        Command::RemoveCarriedFile {
            shown: token(&handle, &key),
        },
    )
    .unwrap();
    let body = handle.snapshot().fold().body(&reply.changes[0]).unwrap();
    assert_eq!(
        body.record,
        Record::CarriedFile {
            path: RepoPath::parse("ingredients/audio/tit-1.mp3").unwrap(),
            value: CarriedValue::Removed,
        }
    );
    assert_eq!(body.parents, heads);
    handle.close().unwrap();
}

#[test]
fn what_was_recovered_of_a_batch_is_applied_by_a_release() {
    let (fs, folder) = rich();
    let (_rig, handle) = open(&fs, &folder);
    let fold = handle.snapshot().fold().clone();
    let held = fold
        .anomalies()
        .find_map(|a| match a {
            Anomaly::IncompleteBatch { batch, .. } => Some(*batch),
            _ => None,
        })
        .unwrap();
    let comment = |handle: &ProjectHandle| {
        handle.snapshot().fold().record_keys().into_iter().any(|k| {
            matches!(
                handle.snapshot().fold().projected(&k),
                Some(Record::Comment { value, .. }) if value.text == "Half of a batch"
            )
        })
    };
    assert!(!comment(&handle), "a held batch applies nothing");
    let reply = submit(&handle, Command::ReleaseBatch { batch: held }).unwrap();
    assert_eq!(reply.changes.len(), 1);
    let body = handle.snapshot().fold().body(&reply.changes[0]).unwrap();
    assert_eq!(
        body.record,
        Record::BatchRelease {
            batch: held,
            value: tc_journal::record::BatchReleaseValue {
                changes: handle.snapshot().fold().batch_changes(held),
            },
        }
    );
    assert!(body.parents.is_empty(), "a release is grow-only (V6)");
    assert!(comment(&handle), "the recovered change applies");
    // A batch that is not held has nothing to release.
    assert!(matches!(
        submit(
            &handle,
            Command::ReleaseBatch {
                batch: BatchId::from_bytes([1u8; 16])
            }
        ),
        Err(CommandError::Refused(Refusal::NothingToRelease))
    ));
    handle.close().unwrap();
}

/// §7.4: a conflict marked as incomplete history is a warning, never a reason to hide a
/// choice. Juan's batch of two lost one of its changes on the way; Ana continued the change
/// that arrived, and this device edited the verse from the same base: the conflict is marked,
/// and *Keep this version* works beside *Apply what was recovered*.
#[test]
fn release_batch_is_accepted_beside_the_other_choices() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let base = handle.snapshot().fold().heads(&verse(1, 1));
    let shown = token(&handle, &verse(1, 1));
    submit(
        &handle,
        Command::SaveVerse {
            shown,
            words: "My version of one".into(),
        },
    )
    .unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    let words = |text: &str| Record::Verse {
        book: TIT,
        chapter: ChapterNo::new(1).unwrap(),
        verse: VerseNo::new(1).unwrap(),
        value: VerseValue::Text {
            marker: tc_journal::record::MarkerWs::canonical(),
            words: text.into(),
            layout: "\n".into(),
            last: VerseNo::new(1).unwrap(),
            from: None,
        },
    };
    let lost = teammate_batch(
        &rig.fs,
        &folder,
        DeviceId::local([2u8; 15]),
        2,
        vec![(words("Juan's first"), base)],
    );
    teammate_batch(
        &rig.fs,
        &folder,
        DeviceId::local([3u8; 15]),
        1,
        vec![(words("Ana continues"), vec![lost[0]])],
    );
    let (_rig, handle) = open(&rig.fs, &folder);
    let fold = handle.snapshot().fold().clone();
    let conflict = conflict_of(&fold, &verse(1, 1));
    assert!(conflict.incomplete_history, "{conflict:?}");
    let held = fold
        .anomalies()
        .find_map(|a| match a {
            Anomaly::IncompleteBatch { batch, .. } => Some(*batch),
            _ => None,
        })
        .expect("Juan's batch is held");
    // Every version can be kept, whatever the history lacks.
    for candidate in &conflict.candidates {
        assert!(
            tc_journal::fold::FoldState::resolution(
                &fold,
                &conflict.id,
                &Choice::Candidate {
                    head: candidate.head
                }
            )
            .is_ok(),
            "{candidate:?} cannot be kept"
        );
    }
    let kept = conflict.candidates[0].clone();
    submit(
        &handle,
        Command::Resolve {
            conflict: conflict.id,
            shown: vec![review(&handle, &verse(1, 1))],
            choice: Choice::Candidate { head: kept.head },
        },
    )
    .unwrap();
    assert_eq!(projected(&handle, &verse(1, 1)), Some(kept.value));
    // And what was recovered of the batch can still be applied.
    let reply = submit(&handle, Command::ReleaseBatch { batch: held }).unwrap();
    assert_eq!(reply.changes.len(), 1);
    handle.close().unwrap();
}

#[test]
fn a_dismissed_alignment_is_kept_with_its_flag() {
    let (fs, folder) = rich();
    let (_rig, handle) = open(&fs, &folder);
    let key = RecordKey::Alignment(TIT, ChapterNo::new(1).unwrap(), VerseNo::new(1).unwrap());
    let before = projected(&handle, &key);
    submit(
        &handle,
        Command::Dismiss {
            shown: token(&handle, &key),
        },
    )
    .unwrap();
    let (Some(Record::Alignment { value: was, .. }), Some(Record::Alignment { value: now, .. })) =
        (before, projected(&handle, &key))
    else {
        panic!("alignments");
    };
    assert!(now.dismissed && !was.dismissed);
    assert_eq!(now.alignments, was.alignments);
    // A verse is not something to dismiss.
    assert!(matches!(
        submit(
            &handle,
            Command::Dismiss {
                shown: token(&handle, &verse(1, 1)),
            }
        ),
        Err(CommandError::Refused(Refusal::NotDismissable))
    ));
    handle.close().unwrap();
}

/// A read-only project refuses every command of §2.7 and writes nothing (APP-DESIGN §4.2
/// `read_only_refuses_every_command_and_writes_nothing`).
#[test]
fn read_only_refuses_every_command_and_writes_nothing() {
    let (fs, folder) = rich();
    let (rig, writer) = open(&fs, &folder);
    writer.close().unwrap();
    drop(rig);
    // The projects folder is found to be cloud-synced: the next open is read-only.
    let cloud = cloud_synced_rig(fs.clone());
    let reader = cloud.store.open(&folder, &OpenOptions::default()).unwrap();
    assert!(matches!(
        reader.facts().mode,
        tc_store::Mode::ReadOnly(tc_store::ReadOnlyReason::UnsafeFolder(_))
    ));
    let before = files(&fs, &folder);
    for (name, command) in every_command(&reader) {
        let result = submit(&reader, command);
        assert!(
            matches!(result, Err(CommandError::ReadOnly(_))),
            "{name}: {result:?}"
        );
    }
    let draft = submit(
        &reader,
        Command::Draft {
            draft: Draft {
                field: DraftField::Words,
                shown: vec![token(&reader, &verse(1, 1))],
                text: "unsaved".into(),
            },
            at: tc_journal::WallTime(0),
        },
    );
    assert!(matches!(draft, Err(CommandError::ReadOnly(_))));
    reader.wait_idle();
    reader.close().unwrap();
    let after = files(&fs, &folder);
    let changed: Vec<&String> = after
        .keys()
        .chain(before.keys())
        .filter(|k| *k != ".tc/lock" && before.get(*k) != after.get(*k))
        .collect();
    assert!(changed.is_empty(), "a read-only open wrote {changed:?}");
}

/// An Open Bible Stories project made as a new one is (§3.2): its details and each story's
/// template base, seeded from the shipped template.
pub(crate) fn stories() -> (FaultFs, PathBuf) {
    let fs = FaultFs::new();
    let rig = rig(fs.clone());
    let mut records: Vec<Record> = first_records()
        .into_iter()
        .filter(|r| !matches!(r, Record::Versification { .. }))
        .collect();
    for story in tc_obs::ids::StoryNo::all() {
        records.push(Record::StoryBase {
            story: tc_journal::StoryNo::new(story.get()).unwrap(),
            value: tc_obs::story::seed_story(tc_obs::template::template_story(story)).unwrap(),
        });
    }
    let folder = rig
        .store
        .create(&tc_store::NewProject {
            name: "Stories".into(),
            kind: tc_journal::fold::ProjectKind::Stories,
            records,
            books: Vec::new(),
        })
        .unwrap();
    (fs.crash(false), folder)
}

fn frame(story: u8, frame: u16) -> RecordKey {
    RecordKey::Frame(
        tc_journal::StoryNo::new(story).unwrap(),
        tc_journal::FrameNo::new(frame),
    )
}

/// §5.15: a frame, a title and a reference line are saved by tC4's text rules and written to
/// the story's file.
#[test]
fn an_obs_frame_is_saved_by_tc4s_text_rules() {
    let (fs, folder) = stories();
    let (rig, handle) = open(&fs, &folder);
    assert_eq!(handle.facts().kind, tc_journal::fold::ProjectKind::Stories);
    submit(
        &handle,
        Command::SaveFrame {
            shown: token(&handle, &frame(1, 1)),
            text: "This is how God made everything.".into(),
        },
    )
    .unwrap();
    submit(
        &handle,
        Command::SaveFrame {
            shown: token(&handle, &frame(1, 0)),
            text: "The Creation".into(),
        },
    )
    .unwrap();
    // Two paragraphs, a leading newline or a reference line where a frame goes: refused.
    for bad in [
        "one\n\ntwo",
        "\nleading",
        "_A Bible story from: Genesis 1-2_",
    ] {
        let refused = submit(
            &handle,
            Command::SaveFrame {
                shown: token(&handle, &frame(1, 2)),
                text: bad.into(),
            },
        );
        assert!(
            matches!(refused, Err(CommandError::Refused(Refusal::Text(_)))),
            "{bad:?}: {refused:?}"
        );
    }
    // A verse save is no command of a stories project.
    assert!(matches!(
        save_verse(&handle, 1, 1, "x"),
        Err(CommandError::Refused(_)) | Err(CommandError::BadToken(_))
    ));
    handle.leave(tc_store::Leave::Mode).wait().unwrap();
    let story = String::from_utf8(
        rig.fs
            .peek(&folder.join("ingredients/content/01.md"))
            .unwrap(),
    )
    .unwrap();
    assert!(
        story.contains("This is how God made everything."),
        "{story}"
    );
    // The title is saved without its number and written as tC4's `writeTitle` writes it.
    assert!(story.starts_with("# 1. The Creation\n"), "{story}");
    assert_valid_burrito(&rig.fs, &folder);
    handle.close().unwrap();
}

/// §5.15: a kill at any write point of a frame save leaves it whole or absent.
#[test]
#[ignore = "nightly: a frame save is one pending-file write as every command is; crash::crash_save_verse_at_every_write_point catches its bugs in the gate"]
fn crash_an_obs_frame_save_at_every_write_point_nightly() {
    let (base, folder) = stories();
    let before = {
        let (_rig, handle) = open(&base, &folder);
        let ids = all_ids(handle.snapshot().fold());
        handle.close().unwrap();
        ids
    };
    let points = kill_everywhere(
        &base,
        &folder,
        &|handle| {
            outcome_of(submit(
                handle,
                Command::SaveFrame {
                    shown: token(handle, &frame(2, 1)),
                    text: "God put the man in the garden.".into(),
                },
            ))
        },
        &|handle, outcome| {
            let got = written("SaveFrame", &before, handle.snapshot().fold());
            match outcome {
                Outcome::Saved => assert_eq!(got, vec!["Frame".to_owned()]),
                Outcome::NotSaved | Outcome::SavedWithDiskError => {
                    assert!(got.is_empty() || got == vec!["Frame".to_owned()])
                }
            }
        },
    );
    assert!(points >= 4);
}

// ---------------------------------------------------------------------------------------
// A pins choice with its carry-over (§8.3, APP-DESIGN R7)
// ---------------------------------------------------------------------------------------

fn pin(sha: u8) -> PinGroupValue {
    PinGroupValue(BTreeMap::from([(
        SlotName::parse("ugl").unwrap(),
        SlotValue(Json::Obj(JsonMap::from([
            (
                "repoPath".to_owned(),
                Json::from("git.door43.org/unfoldingWord/en_ugl"),
            ),
            ("sha".to_owned(), Json::from(oid(sha).to_hex())),
            ("version".to_owned(), Json::from("v1")),
        ]))),
    )]))
}

/// Ruling H5: two computers pinned the lexicon differently. The newest pin wins with no
/// conflict listed, and the other stays in History, where Restore brings it back.
#[test]
fn of_two_pins_the_newest_wins_and_the_other_is_restored() {
    let (fs, folder) = rich();
    let group = RecordKey::Pins(PinGroup::Lexicon);
    let (rig, handle) = open(&fs, &folder);
    let before_heads = handle.snapshot().fold().heads(&group);
    submit(
        &handle,
        Command::SetPins {
            shown: token(&handle, &group),
            value: pin(1),
        },
    )
    .unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    teammate_batch(
        &rig.fs,
        &folder,
        DeviceId::local([8u8; 15]),
        1,
        vec![(
            Record::Pins {
                group: PinGroup::Lexicon,
                value: pin(2),
            },
            before_heads,
        )],
    );
    let (_rig, handle) = open(&rig.fs, &folder);
    let fold = handle.snapshot().fold().clone();
    assert!(fold.conflicts().all(|c| c.key != group), "no pins conflict");
    let heads = fold.heads(&group);
    assert_eq!(heads.len(), 2);
    let newest = heads
        .iter()
        .copied()
        .max_by_key(|h| (fold.body(h).unwrap().at, *h))
        .unwrap();
    let older = heads.iter().copied().find(|h| *h != newest).unwrap();
    assert_eq!(
        fold.key_state(&group).map(|k| k.state),
        Some(tc_journal::fold::Resolved::SettledByRule {
            head: newest,
            rule: tc_journal::fold::AutoRule::Newest,
        })
    );
    assert!(fold.history(&group).contains(&older));
    let older_value = fold.body(&older).unwrap().record;
    submit(
        &handle,
        Command::Restore {
            shown: token(&handle, &group),
            from: older,
        },
    )
    .unwrap();
    assert_eq!(projected(&handle, &group), Some(older_value));
    assert_eq!(handle.snapshot().fold().heads(&group).len(), 1);
    handle.close().unwrap();
}

/// A small USFM book of generated chapters, paragraphs, verses, spans and words.
fn generated_book() -> impl proptest::strategy::Strategy<Value = String> {
    use proptest::prelude::*;
    let word = "[a-zA-Z\u{e9}\u{4e2d}\u{5d0}]{1,7}";
    let verse = (
        prop::option::of(prop::sample::select(vec!["\\p", "\\q1", "\\m"])),
        prop::collection::vec(word, 1..6),
    );
    let chapter = prop::collection::vec(verse, 1..6);
    prop::collection::vec(chapter, 1..4).prop_map(|chapters| {
        let mut out = String::from("\\id 2JN source\n\\h 2 John\n\\mt 2 John\n");
        for (c, verses) in chapters.iter().enumerate() {
            out.push_str(&format!("\\c {}\n", c + 1));
            for (v, (marker, words)) in verses.iter().enumerate() {
                if let Some(marker) = marker {
                    out.push_str(&format!("{marker}\n"));
                }
                out.push_str(&format!("\\v {} {}\n", v + 1, words.join(" ")));
            }
        }
        out
    })
}

/// A book added from a file reads back as that file, byte for byte, and writes no alignment for
/// text that carries none: nothing of the translator's file is dropped or rewritten.
#[test]
fn adding_a_book_from_a_file_loses_no_byte_of_it() {
    use proptest::prelude::*;
    let (base, folder) = rich();
    let mut runner = proptest::test_runner::TestRunner::new(ProptestConfig {
        cases: 48,
        failure_persistence: None,
        ..ProptestConfig::default()
    });
    runner
        .run(&generated_book(), |usfm| {
            let (_rig, handle) = open(&base.copy(), &folder);
            submit(&handle, add_text(BookId::SecondJohn, usfm.clone())).unwrap();
            let fold = handle.snapshot().fold().clone();
            prop_assert_eq!(fold.book_usfm(BookId::SecondJohn), Some(usfm));
            prop_assert!(
                book_keys(&handle, BookId::SecondJohn)
                    .iter()
                    .all(|k| !matches!(k, RecordKey::Alignment(..)))
            );
            Ok(())
        })
        .unwrap();
}

/// The ULT v91 book `code` as the app ships it.
fn bundled_ult(code: &str) -> String {
    use std::io::Read;
    let file = std::fs::File::open(repo_root().join("bundle/en/en_ult_v91.sb.zip")).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut entry = archive
        .by_name(&format!("en_ult/ingredients/{code}.usfm"))
        .unwrap();
    let mut text = String::new();
    entry.read_to_string(&mut text).unwrap();
    text
}

/// tC4 #812: a verse with a `\p`, `\ts\*`, `\b`, `\q3` or `\s` line inside shows every
/// word, and a save of it keeps every word, on the screen and in the book file.
#[test]
fn verses_with_a_layout_line_inside_save_whole() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let books: BTreeMap<&str, String> = ["1SA", "REV", "MAT"]
        .into_iter()
        .map(|code| (code, bundled_ult(code)))
        .collect();
    for (code, usfm) in &books {
        let book: BookId = code.parse().unwrap();
        handle
            .submit(Command::AddBookText {
                book,
                scope: Scope::Whole,
                usfm: usfm.clone(),
                file_name: format!("{code}.usfm"),
            })
            .wait()
            .unwrap();
    }
    let verses = [("1SA", 2, 2), ("1SA", 4, 1), ("REV", 7, 12), ("MAT", 27, 9)];
    let words_of = |text: &str, c: u16, v: u16| -> Vec<String> {
        let parts = tc_usfm::codec::decompose(text).unwrap();
        let Some(tc_usfm::codec::VersePart::Text(verse)) = parts.chapters[&c].verses.get(&v) else {
            panic!("{c}:{v}");
        };
        tc_usfm::target_words(&format!("{}{}", verse.words, verse.layout))
            .into_iter()
            .map(|w| w.text)
            .collect()
    };
    let mut want = BTreeMap::new();
    for (code, c, v) in verses {
        let book: BookId = code.parse().unwrap();
        let all = words_of(&books[code], c, v);
        let key = RecordKey::Verse(book, ChapterNo::new(c).unwrap(), VerseNo::new(v).unwrap());
        let shown = |handle: &ProjectHandle| match handle.snapshot().fold().projected(&key) {
            Some(Record::Verse {
                value: VerseValue::Text { words, .. },
                ..
            }) => words,
            other => panic!("{code} {c}:{v}: {other:?}"),
        };
        let before = shown(&handle);
        let shown_words: Vec<String> = tc_usfm::target_words(&before)
            .into_iter()
            .map(|w| w.text)
            .collect();
        assert_eq!(shown_words, all, "{code} {c}:{v} shows every word");
        let edited = format!("{before} Amén");
        let token = handle.mint(&key, None).unwrap();
        handle
            .submit(Command::SaveVerse {
                shown: token,
                words: edited.clone(),
            })
            .wait()
            .unwrap();
        assert_eq!(shown(&handle), edited);
        let mut all = all;
        all.push("Amén".into());
        want.insert((code, c, v), all);
    }
    handle.close().unwrap();
    for ((code, c, v), all) in want {
        let file = rig
            .fs
            .peek(&folder.join(format!("ingredients/{code}.usfm")))
            .unwrap();
        assert_eq!(
            words_of(&String::from_utf8(file).unwrap(), c, v),
            all,
            "{code} {c}:{v} in the file"
        );
    }
    assert_valid_burrito(&rig.fs, &folder);
}

/// A save from a screen that showed a merged verse is stored even when a sync moved the
/// verse's heads since (no-loss; C7a's finding): the save continues the heads the screen
/// showed, so it stands beside the newer text as a conflict, and both texts are kept.
#[test]
fn a_save_of_a_merged_verse_whose_heads_moved_is_stored_as_a_conflict() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle.wait_idle();
    let base = handle.snapshot().fold().heads(&verse(1, 1));
    let Some(Record::Verse { value, .. }) = projected(&handle, &verse(1, 1)) else {
        panic!("1:1");
    };
    handle.close().unwrap();
    let with = |words: Option<&str>, layout: Option<&str>| {
        let mut value = value.clone();
        if let VerseValue::Text {
            words: w,
            layout: l,
            ..
        } = &mut value
        {
            if let Some(words) = words {
                *w = words.into();
            }
            if let Some(layout) = layout {
                *l = layout.into();
            }
        }
        Record::Verse {
            book: TIT,
            chapter: ChapterNo::new(1).unwrap(),
            verse: VerseNo::new(1).unwrap(),
            value,
        }
    };
    // Two teammates change different parts of 1:1: the fold merges them.
    let a = teammate_batch(
        &rig.fs,
        &folder,
        DeviceId::local([8u8; 15]),
        1,
        vec![(with(Some("Paul, a servant of God"), None), base.clone())],
    );
    teammate_batch(
        &rig.fs,
        &folder,
        DeviceId::local([9u8; 15]),
        1,
        vec![(with(None, Some("\n\\p\n")), base)],
    );
    let (rig, handle) = open(&rig.fs, &folder);
    let shown = token(&handle, &verse(1, 1));
    assert_eq!(shown.heads().len(), 2, "a merged verse");
    handle.close().unwrap();
    // A third teammate continues the first: the heads move.
    let moved = teammate_batch(
        &rig.fs,
        &folder,
        DeviceId::local([7u8; 15]),
        1,
        vec![(with(Some("Paul, a slave of God"), None), a)],
    );
    let (_rig, handle) = open(&rig.fs, &folder);
    assert_ne!(handle.snapshot().fold().heads(&verse(1, 1)), shown.heads());
    let reply = submit(
        &handle,
        Command::SaveVerse {
            shown: shown.clone(),
            words: "Paul, a servant of God and an apostle".into(),
        },
    )
    .expect("stored, not refused");
    let mine = handle.snapshot().fold().body(&reply.changes[0]).unwrap();
    assert_eq!(mine.parents, shown.heads());
    let mut heads = handle.snapshot().fold().heads(&verse(1, 1));
    heads.sort();
    let mut want = vec![reply.changes[0], moved[0]];
    want.sort();
    assert_eq!(heads, want, "both texts are kept");
    let conflict = conflict_of(handle.snapshot().fold(), &verse(1, 1));
    assert_eq!(conflict.candidates.len(), 2);
    handle.close().unwrap();
}
