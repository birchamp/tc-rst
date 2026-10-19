//! Three simulated years (`docs/PERFORMANCE.md` rule 1: an interactive action costs what it
//! touches, never the size of the project or its history).
//!
//! A fourth computer receives, every working day, the packs the team of three seals and
//! merges on the real schedule (`tc_corpus::Corpus::timeline`: four checkpoints a day, a
//! completed month into one pack, a current month's small packs past 32), opens the project
//! on a ManualClock that follows the days, and at the end of each simulated month measures
//! what a person feels: opening, saving a verse, recording a decision, opening a book, the
//! fold's step, the close. It edits only Genesis 50 and Psalm 119, which the team seeded and
//! never touches (`Spec::reserved`), so its own edits never conflict with theirs and the
//! counts stay comparable from month to month.
//!
//! `years-prepare` (a child process: it holds the whole generated corpus) writes each day's
//! packs under `DIR/years/days/`; `years` plays them into `DIR/years/projects/bible` with the
//! store, one open and close a working day by default.

use crate::common::{ms, store_with};
use crate::machine::private_mib;
use crate::table::{Measured, median, p95};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tc_core::BookId;
use tc_corpus::timeline::Schedule;
use tc_corpus::{Corpus, Spec};
use tc_journal::command::{Command, DecisionBasis, DecisionChange, DecisionEdit};
use tc_journal::fold::{FoldInput, FoldState};
use tc_journal::format::FormatDoc;
use tc_journal::json::{Json, JsonMap};
use tc_journal::keys::{CheckId, CheckKey, CheckTarget, RefPart, ToolId};
use tc_journal::record::{CheckValue, ResourceRef, UserStatus, VerseValue};
use tc_journal::{
    BatchId, BatchRef, Cause, Change, ChangeBody, ChapterNo, DeviceId, DeviceSeq, Record,
    RecordKey, VerseNo, WallTime,
};
use tc_store::layout::{GITATTRIBUTES_BYTES, GITIGNORE_BYTES};
use tc_store::{Leave, ManualClock, OpenOptions, Progress, ProgressSink, StoreCounters};

fn years_dir(dir: &Path) -> PathBuf {
    dir.join("years")
}

pub fn years_spec(days: u32) -> Spec {
    let share = f64::from(days) / 750.0;
    Spec::whole_bible(1)
        .with_history(days, share)
        .with_reserved(vec![(BookId::Genesis, 50), (BookId::Psalms, 119)])
}

fn tool_name(tool: ToolId) -> &'static str {
    match tool {
        ToolId::TranslationNotes => "tn",
        _ => "tw",
    }
}

/// Writes the days of the team's three years under `DIR/years/days/` and `prepared.json`.
pub fn prepare(dir: &Path, days: u32) -> Vec<Measured> {
    crate::common::header("years-prepare");
    let root = years_dir(dir);
    let spec = years_spec(days);
    let _ = std::fs::remove_dir_all(root.join("days"));
    std::fs::create_dir_all(root.join("days")).expect("a folder");
    // The checks of Genesis 50, which the fourth computer decides.
    let corpus = Corpus::generate(&spec).expect("the corpus builds");
    let genesis = corpus.domain.book(BookId::Genesis).expect("Genesis");
    let checks: Vec<serde_json::Value> = genesis
        .checks
        .iter()
        .filter(|row| row.chapter == "50")
        .filter_map(|row| {
            let key = tc_corpus::check_key(row).ok()?;
            let context = Json::Obj(tc_corpus::check_context(row)).to_canonical();
            Some(serde_json::json!({
                "tool": tool_name(key.tool),
                "id": row.id,
                "chapter": row.chapter,
                "verse": row.verse,
                "occurrence": row.occurrence,
                "category": row.category,
                "context": String::from_utf8_lossy(&context),
            }))
        })
        .collect();
    let psalms = corpus.domain.book(BookId::Psalms).expect("Psalms");
    let prepared = serde_json::json!({
        "project": corpus.journal.project.to_string(),
        "days": days,
        "genesis50": { "verses": genesis.chapters.get(49).copied().unwrap_or(0), "checks": checks },
        "psalms119": { "verses": psalms.chapters.get(118).copied().unwrap_or(0) },
    });
    std::fs::write(root.join("prepared.json"), prepared.to_string()).expect("prepared.json");
    drop(corpus);
    let mut days_written = 0usize;
    let mut bytes = 0u64;
    let started = Instant::now();
    Corpus::timeline(&spec, &Schedule::default(), &mut |day| {
        let folder = root.join("days").join(format!("{:04}", day.day));
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        for pack in &day.add {
            let target = folder.join("add").join(&pack.path);
            std::fs::create_dir_all(target.parent().ok_or("a path")?).map_err(|e| e.to_string())?;
            std::fs::write(&target, &pack.bytes).map_err(|e| e.to_string())?;
            bytes += pack.bytes.len() as u64;
        }
        std::fs::write(folder.join("remove.txt"), day.remove.join("\n"))
            .map_err(|e| e.to_string())?;
        let meta = serde_json::json!({
            "day": day.day,
            "evening": day.evening.0,
            "monthEnds": day.month_ends,
            "month": day.month.to_string(),
            "changes": day.changes,
        });
        std::fs::write(folder.join("day.json"), meta.to_string()).map_err(|e| e.to_string())?;
        days_written += 1;
        Ok(())
    })
    .expect("the timeline runs");
    vec![Measured::new(
        "years.prepare",
        vec![ms(started)],
        format!("{days_written} working days, {} MB of packs", bytes >> 20),
    )]
}

struct Prepared {
    project: String,
    genesis50_verses: u16,
    checks: Vec<CheckSpec>,
}

struct CheckSpec {
    key: CheckKey,
    context: JsonMap,
    category: String,
}

fn read_prepared(dir: &Path) -> Prepared {
    let text = std::fs::read_to_string(years_dir(dir).join("prepared.json"))
        .expect("run years-prepare first");
    let v: serde_json::Value = serde_json::from_str(&text).expect("prepared.json");
    let checks = v["genesis50"]["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .map(|c| {
            let tool = if c["tool"] == "tn" {
                ToolId::TranslationNotes
            } else {
                ToolId::TranslationWords
            };
            let context =
                match tc_journal::json::parse(c["context"].as_str().unwrap_or("{}").as_bytes()) {
                    Ok(Json::Obj(map)) => map,
                    _ => JsonMap::new(),
                };
            CheckSpec {
                key: CheckKey {
                    tool,
                    check_id: CheckId::parse(c["id"].as_str().unwrap_or("")).expect("a check id"),
                    target: CheckTarget::Verse {
                        book: BookId::Genesis,
                        chapter: RefPart::parse(c["chapter"].as_str().unwrap_or("50"))
                            .expect("chapter"),
                        verse: RefPart::parse(c["verse"].as_str().unwrap_or("1")).expect("verse"),
                    },
                    occurrence: c["occurrence"].as_u64().unwrap_or(1) as u32,
                },
                context,
                category: c["category"].as_str().unwrap_or("").to_owned(),
            }
        })
        .collect();
    Prepared {
        project: v["project"].as_str().unwrap_or("").to_owned(),
        genesis50_verses: v["genesis50"]["verses"].as_u64().unwrap_or(26) as u16,
        checks,
    }
}

fn put(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("a folder");
    std::fs::write(path, bytes).expect("a file");
}

/// What one month's measurement found.
#[derive(Debug, Default, Clone)]
struct Month {
    month: String,
    changes: usize,
    packs: usize,
    pending: usize,
    git_mib: f64,
    tree_mib: f64,
    open_ms: f64,
    first_report_ms: f64,
    save_p50: f64,
    save_p95: f64,
    save_opened: f64,
    save_created: f64,
    save_fsyncs: f64,
    save_bytes: f64,
    decision_p50: f64,
    decision_p95: f64,
    book_p50: f64,
    book_p95: f64,
    psalms_p95: f64,
    book_bytes: f64,
    chapter_p95: f64,
    step_p50: f64,
    step_p95: f64,
    step_counts: String,
    counts_ms: f64,
    close_ms: f64,
    private_mib: f64,
}

const TSV_HEADER: &str = "month\tchanges\tpacks\tpending\tgit_mib\ttree_mib\topen_ms\tfirst_report_ms\tsave_p50\tsave_p95\tsave_opened\tsave_created\tsave_fsyncs\tsave_bytes\tdecision_p50\tdecision_p95\tbook_p50\tbook_p95\tpsalms_p95\tbook_bytes\tchapter_p95\tstep_p50\tstep_p95\tcounts_ms\tclose_ms\tprivate_mib\tstep_counts";

impl Month {
    fn tsv(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{:.0}\t{:.0}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.0}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{:.0}\t{:.3}\t{:.3}\t{:.3}\t{:.5}\t{:.0}\t{:.0}\t{}",
            self.month,
            self.changes,
            self.packs,
            self.pending,
            self.git_mib,
            self.tree_mib,
            self.open_ms,
            self.first_report_ms,
            self.save_p50,
            self.save_p95,
            self.save_opened,
            self.save_created,
            self.save_fsyncs,
            self.save_bytes,
            self.decision_p50,
            self.decision_p95,
            self.book_p50,
            self.book_p95,
            self.psalms_p95,
            self.book_bytes,
            self.chapter_p95,
            self.step_p50,
            self.step_p95,
            self.counts_ms,
            self.close_ms,
            self.private_mib,
            self.step_counts
        )
    }
}

fn count_files(dir: &Path) -> usize {
    let mut n = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => n += 1,
                _ => {}
            }
        }
    }
    n
}

fn tree_size(root: &Path) -> u64 {
    let mut total = 0;
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let name = entry.file_name();
        if name == ".git" || name == ".tc" {
            continue;
        }
        let path = entry.path();
        total += if path.is_dir() {
            crate::common::dir_size(&path)
        } else {
            entry.metadata().map_or(0, |m| m.len())
        };
    }
    total
}

/// Plays the prepared days into a project and measures at each month's end.
pub fn run(dir: &Path, open_every: u32, only_months: Option<usize>) -> Vec<Measured> {
    crate::common::header("years");
    let root = years_dir(dir);
    let prepared = read_prepared(dir);
    let project_dir = root.join("projects").join("bible");
    let _ = std::fs::remove_dir_all(root.join("projects"));
    let _ = std::fs::remove_dir_all(root.join("app"));
    put(
        &project_dir.join("ingredients/tc/project.json"),
        serde_json::json!({
            "schemaVersion": 1,
            "projectId": prepared.project,
            "kind": "bible",
            "created": "2023-01-02T00:00:00.000Z",
        })
        .to_string()
        .as_bytes(),
    );
    put(
        &project_dir.join("ingredients/tc/format.json"),
        &FormatDoc::current().to_bytes(),
    );
    put(&project_dir.join(".gitignore"), GITIGNORE_BYTES);
    put(&project_dir.join(".gitattributes"), GITATTRIBUTES_BYTES);
    let mut day_dirs: Vec<PathBuf> = std::fs::read_dir(root.join("days"))
        .expect("prepared days")
        .flatten()
        .map(|e| e.path())
        .collect();
    day_dirs.sort();
    let first_evening = read_evening(&day_dirs[0]);
    let clock = Arc::new(ManualClock::at(first_evening));
    let mut now = first_evening;
    let store = store_with(&root, clock.clone());
    store.wait_cleanup();
    let mut months: Vec<Month> = Vec::new();
    let mut tsv = String::from(TSV_HEADER);
    tsv.push('\n');
    let started = Instant::now();
    let mut opens = 0usize;
    let mut open_total = 0.0;
    for (i, day) in day_dirs.iter().enumerate() {
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(day.join("day.json")).expect("day.json"))
                .expect("day.json");
        let evening = meta["evening"].as_i64().unwrap_or(now);
        // The team's packs reach the folder while the project is closed.
        for rel in std::fs::read_to_string(day.join("remove.txt"))
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.is_empty())
        {
            let _ = std::fs::remove_file(project_dir.join(rel));
        }
        copy_tree(&day.join("add"), &project_dir);
        clock.advance(evening - now);
        now = evening;
        let month_ends = meta["monthEnds"].as_bool().unwrap_or(false);
        let open_today = (i as u32).is_multiple_of(open_every.max(1)) || month_ends;
        if !open_today {
            continue;
        }
        let first = Arc::new(Mutex::new(None::<f64>));
        let sink: ProgressSink = {
            let first = first.clone();
            Arc::new(move |p: Progress| {
                let mut slot = first.lock().unwrap_or_else(|e| e.into_inner());
                if slot.is_none() {
                    *slot = Some(p.at.as_secs_f64() * 1000.0);
                }
            })
        };
        let t = Instant::now();
        let handle = store
            .open(
                &project_dir,
                &OpenOptions {
                    progress: sink,
                    ..OpenOptions::default()
                },
            )
            .expect("the project opens");
        let usable = ms(t);
        handle.wait_idle();
        opens += 1;
        open_total += usable;
        if month_ends && months.len() < only_months.unwrap_or(usize::MAX) {
            let mut month = measure(&handle, &prepared, months.len(), &project_dir);
            month.month = meta["month"].as_str().unwrap_or("").to_owned();
            month.open_ms = usable;
            month.first_report_ms = first
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .unwrap_or(0.0);
            std::thread::sleep(Duration::from_secs(3));
            month.private_mib = private_mib().unwrap_or(f64::NAN);
            let t = Instant::now();
            let (result, _) = handle.close_counted();
            month.close_ms = ms(t);
            result.expect("close");
            month.packs = count_files(&project_dir.join("ingredients/tc/changes"));
            month.pending = count_files(&project_dir.join(".tc/pending"));
            month.git_mib = crate::common::dir_size(&project_dir.join(".git")) as f64 / 1048576.0;
            month.tree_mib = tree_size(&project_dir) as f64 / 1048576.0;
            println!(
                "month {} ({}): {} changes, open {:.0} ms, save p95 {:.1} ms, book p95 {:.1} ms, step p95 {:.3} ms, close {:.0} ms, private {:.0} MiB, {:.0} s in",
                months.len() + 1,
                month.month,
                month.changes,
                month.open_ms,
                month.save_p95,
                month.book_p95,
                month.step_p95,
                month.close_ms,
                month.private_mib,
                started.elapsed().as_secs_f64()
            );
            tsv.push_str(&month.tsv());
            tsv.push('\n');
            let _ = std::fs::write(root.join("years.tsv"), &tsv);
            months.push(month);
            if only_months.is_some_and(|n| months.len() >= n) {
                break;
            }
        } else {
            handle.close().expect("close");
        }
    }
    let mut rows = verdict(&months);
    rows.push(Measured::new(
        "years.opens",
        vec![open_total / opens.max(1) as f64],
        format!("{opens} opens in {:.0} s", started.elapsed().as_secs_f64()),
    ));
    rows
}

fn read_evening(day: &Path) -> i64 {
    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(day.join("day.json")).expect("day.json"))
            .expect("day.json");
    meta["evening"].as_i64().unwrap_or(0)
}

fn copy_tree(from: &Path, to: &Path) {
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    for entry in entries.flatten() {
        let target = to.join(entry.file_name());
        match entry.file_type() {
            Ok(t) if t.is_dir() => {
                std::fs::create_dir_all(&target).expect("a folder");
                copy_tree(&entry.path(), &target);
            }
            Ok(t) if t.is_file() => {
                std::fs::copy(entry.path(), &target).expect("a pack");
            }
            _ => {}
        }
    }
}

fn percentile_row(times: &[f64]) -> (f64, f64) {
    (median(times), p95(times))
}

fn delta_per(delta: &StoreCounters, n: usize) -> (f64, f64, f64, f64) {
    let n = n.max(1) as f64;
    (
        delta.files_opened as f64 / n,
        delta.files_created as f64 / n,
        delta.fsyncs as f64 / n,
        delta.bytes_written as f64 / n,
    )
}

/// The month-end measurement, with the project open.
fn measure(
    handle: &tc_store::ProjectHandle,
    prepared: &Prepared,
    index: usize,
    project_dir: &Path,
) -> Month {
    let mut month = Month {
        changes: handle.snapshot().fold().len(),
        ..Month::default()
    };
    let verse = |chapter: u16, v: u16, book: BookId| {
        RecordKey::Verse(
            book,
            ChapterNo::new(chapter).expect("a chapter"),
            VerseNo::new(v).expect("a verse"),
        )
    };
    let verses: Vec<RecordKey> = (1..=prepared.genesis50_verses.min(26))
        .map(|v| verse(50, v, BookId::Genesis))
        .collect();
    // 50 verse saves of words of one length, so the bytes written are the same every month.
    let before = handle.counters();
    let mut times = Vec::new();
    for n in 0..50usize {
        let key = &verses[n % verses.len()];
        let shown = handle.mint(key, None).expect("a token");
        let t = Instant::now();
        let reply = handle
            .submit(Command::SaveVerse {
                shown,
                words: format!("A measured save in month {index:03} number {n:03} here"),
            })
            .wait();
        times.push(ms(t));
        reply.expect("the save is accepted");
    }
    let delta = handle.counters().since(&before);
    (month.save_p50, month.save_p95) = percentile_row(&times);
    (
        month.save_opened,
        month.save_created,
        month.save_fsyncs,
        month.save_bytes,
    ) = delta_per(&delta, 50);
    // Decisions on Genesis 50's checks: the first month creates them, later months toggle
    // the bookmark. The resource comes from a decided check of the same tool.
    let fold = handle.snapshot().fold().clone();
    let resource_of = |tool: ToolId| -> Option<ResourceRef> {
        fold.decisions_of(tool, BookId::Genesis)
            .into_iter()
            .find_map(|k| match fold.projected(&k) {
                Some(Record::Check {
                    value: CheckValue::Decision(d),
                    ..
                }) => Some(d.resource.clone()),
                _ => None,
            })
    };
    let mut decision_times = Vec::new();
    for (n, spec) in prepared.checks.iter().take(50).enumerate() {
        let key = RecordKey::Check(spec.key.clone());
        let verse_key = verse(50, 1, BookId::Genesis);
        let CheckTarget::Verse { verse: v, .. } = &spec.key.target else {
            continue;
        };
        let verse_key = v
            .number()
            .map_or(verse_key, |v| verse(50, v, BookId::Genesis));
        let Some(resource) = resource_of(spec.key.tool) else {
            continue;
        };
        let existing = fold.projected(&key);
        let value = match existing {
            Some(Record::Check {
                value: CheckValue::Decision(d),
                ..
            }) => DecisionEdit {
                basis: None,
                resource: d.resource.clone(),
                changes: vec![DecisionChange::Bookmarked(!d.bookmarked)],
            },
            _ => DecisionEdit {
                basis: Some(DecisionBasis {
                    context: spec.context.clone(),
                    category: Some(spec.category.as_str().into()),
                }),
                resource,
                changes: vec![
                    DecisionChange::NothingToSelect(true),
                    DecisionChange::Status(Some(UserStatus::Valid)),
                ],
            },
        };
        let (Ok(shown), Ok(verse_shown)) = (handle.mint(&key, None), handle.mint(&verse_key, None))
        else {
            continue;
        };
        let t = Instant::now();
        let reply = handle
            .submit(Command::SaveDecision {
                shown,
                verse_shown,
                value,
            })
            .wait();
        decision_times.push(ms(t));
        if let Err(e) = reply {
            eprintln!("decision {n}: {e:?}");
        }
    }
    (month.decision_p50, month.decision_p95) = percentile_row(&decision_times);
    // Open a book and a chapter.
    let mut books = Vec::new();
    let before = handle.counters();
    for _ in 0..20 {
        let t = Instant::now();
        let read = handle.read_book(BookId::Genesis).wait();
        books.push(ms(t));
        assert!(read.usfm.is_some());
    }
    month.book_bytes = handle.counters().since(&before).bytes_read as f64 / 20.0;
    (month.book_p50, month.book_p95) = percentile_row(&books);
    let mut psalms = Vec::new();
    if handle
        .snapshot()
        .fold()
        .projected_books()
        .contains(&BookId::Psalms)
    {
        for _ in 0..20 {
            let t = Instant::now();
            let _ = handle.read_book(BookId::Psalms).wait();
            psalms.push(ms(t));
        }
        month.psalms_p95 = p95(&psalms);
    }
    let snapshot = handle.snapshot();
    let mut chapters = Vec::new();
    for _ in 0..50 {
        let t = Instant::now();
        let _ = snapshot.fold().chapter_keys(BookId::Genesis, 50);
        let _ = snapshot.fold().chapter_usfm(BookId::Genesis, 50);
        chapters.push(ms(t));
    }
    month.chapter_p95 = p95(&chapters);
    // The fold's step on a Genesis 50 verse, with a published snapshot alive.
    let (step_times, counts, counts_ms) = fold_steps(snapshot.fold(), &verses[0]);
    (month.step_p50, month.step_p95) = percentile_row(&step_times);
    month.step_counts = counts;
    month.counts_ms = median(&counts_ms);
    let _ = handle.leave(Leave::Mode).wait();
    handle.wait_idle();
    let _ = project_dir;
    month
}

fn fold_steps(fold: &FoldState, key: &RecordKey) -> (Vec<f64>, String, Vec<f64>) {
    let mut fold = fold.clone();
    fold.own_counters();
    let device = DeviceId::local([0x59; 15]);
    let mut seq = 0u64;
    let (mut times, mut counts_ms) = (Vec::new(), Vec::new());
    let mut last = String::new();
    for i in 0..200usize {
        let Some(Record::Verse {
            book,
            chapter,
            verse,
            value:
                VerseValue::Text {
                    marker,
                    layout,
                    last: end,
                    from,
                    ..
                },
        }) = fold.projected(key)
        else {
            break;
        };
        seq += 1;
        let mut id = [0u8; 16];
        id[..8].copy_from_slice(&seq.to_be_bytes());
        id[15] = 0x59;
        let record = Record::Verse {
            book,
            chapter,
            verse,
            value: VerseValue::Text {
                marker,
                words: format!("a fold step {i}"),
                layout,
                last: end,
                from,
            },
        };
        let mut parents = fold.shown_heads(key, None);
        parents.sort();
        let Ok(change) = Change::new(ChangeBody {
            v: tc_journal::FORMAT_MINOR,
            record,
            parents,
            device,
            seq: DeviceSeq(seq),
            at: WallTime(1_900_000_000_000 + seq as i64),
            batch: BatchRef::new(BatchId::from_bytes(id), 1).expect("a batch"),
            cause: Cause::Edit,
        }) else {
            break;
        };
        let before = fold.counters();
        let t = Instant::now();
        fold.add([FoldInput::from(change)]);
        let published = fold.clone();
        let usfm = fold.chapter_usfm(BookId::Genesis, 50);
        times.push(ms(t));
        drop(published);
        last = format!("{:?}", fold.counters().since(&before));
        std::hint::black_box(usfm);
        let t = Instant::now();
        let _ = fold.conflict_counts().len();
        counts_ms.push(ms(t));
    }
    (times, last, counts_ms)
}

/// The assertions of the run, as rows: the per-save counts equal every month, and the time of
/// saving and of opening a book in the last six months is at most 1.25 times that of months 4
/// to 9 (the fold's step, whose counted work is equal every month, at most two and a half).
fn verdict(months: &[Month]) -> Vec<Measured> {
    let mut rows = Vec::new();
    let slice = |range: std::ops::Range<usize>, f: &dyn Fn(&Month) -> f64| -> Vec<f64> {
        months
            .iter()
            .enumerate()
            .filter(|(i, _)| range.contains(i))
            .map(|(_, m)| f(m))
            .collect()
    };
    let mut failures = Vec::new();
    // The last six months against months 4 to 9 (the first three are warming up). A run of
    // 750 working days is 35 months, so "the last six" is months 30 to 35.
    let late_from = months.len().saturating_sub(6).max(9);
    for (name, limit, f) in [
        (
            "save p95",
            1.25,
            (&|m: &Month| m.save_p95) as &dyn Fn(&Month) -> f64,
        ),
        ("book open p95", 1.25, &|m: &Month| m.book_p95),
        // The fold step's counted work is identical every month (checked below); its time
        // follows the depth of the persistent tables and the cache, which grow with the
        // logarithm of the history, so a ratio above two and a half is a change of kind.
        ("fold step p95", 2.5, &|m: &Month| m.step_p95),
    ] {
        let early = median(&slice(3..9, f));
        let late = median(&slice(late_from..months.len(), f));
        let ratio = late / early;
        rows.push(Measured::new(
            &format!("years.{}", name.replace(' ', "_")),
            vec![ratio],
            format!(
                "{name}: months 4-9 median {early:.3}, months {}-{} median {late:.3}, ratio {ratio:.2} (limit {limit})",
                late_from + 1,
                months.len()
            ),
        ));
        if months.len() >= 24 && ratio > limit {
            failures.push(format!("{name} grew {ratio:.2} times"));
        }
        let xs: Vec<f64> = months.iter().map(f).collect();
        rows.push(Measured::new(
            &format!("years.slope.{}", name.replace(' ', "_")),
            vec![slope(&xs)],
            format!("least-squares slope of {name} per month"),
        ));
    }
    for (name, f) in [
        (
            "files opened per save",
            (&|m: &Month| m.save_opened) as &dyn Fn(&Month) -> f64,
        ),
        ("files created per save", &|m: &Month| m.save_created),
        ("fsyncs per save", &|m: &Month| m.save_fsyncs),
        ("bytes written per save", &|m: &Month| m.save_bytes),
    ] {
        let xs: Vec<f64> = months.iter().map(f).collect();
        let (lo, hi) = (
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        // The bytes of a save differ by a parent id now and then (a verse that has two heads);
        // the counts of files and flushes are exact.
        let slack = if name.starts_with("bytes") {
            0.02 * hi
        } else {
            1e-9
        };
        if (hi - lo).abs() > slack && !months.is_empty() {
            failures.push(format!("{name} changed: {lo} to {hi}"));
        }
        rows.push(Measured::new(
            &format!("years.counts.{}", name.replace(' ', "_")),
            vec![hi - lo],
            format!("{name}: {lo} to {hi} over {} months", months.len()),
        ));
    }
    let steps: std::collections::BTreeSet<&str> =
        months.iter().map(|m| m.step_counts.as_str()).collect();
    if steps.len() > 1 {
        failures.push(format!("the fold step's counts changed: {steps:?}"));
    }
    rows.push(Measured::new(
        "years.open",
        slice(0..months.len(), &|m| m.open_ms),
        "the open at each month's end (background work with its own budget)",
    ));
    rows.push(Measured::new(
        "years.close",
        slice(0..months.len(), &|m| m.close_ms),
        "the close at each month's end",
    ));
    if let Some(last) = months.last() {
        rows.push(Measured::new(
            "years.end",
            vec![last.git_mib, last.packs as f64],
            format!(
                "after {} months: .git {:.0} MiB, {} pack files, {} pending, working tree {:.0} MiB, {} changes",
                months.len(),
                last.git_mib,
                last.packs,
                last.pending,
                last.tree_mib,
                last.changes
            ),
        ));
    }
    for f in &failures {
        println!("FAILED: {f}");
    }
    rows.push(Measured::new(
        "years.failures",
        vec![failures.len() as f64],
        failures.join("; "),
    ));
    rows
}

/// The least-squares slope of `ys` against 0, 1, 2, ....
fn slope(ys: &[f64]) -> f64 {
    let n = ys.len() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = ys.iter().sum::<f64>() / n;
    let (mut num, mut den) = (0.0, 0.0);
    for (i, y) in ys.iter().enumerate() {
        let dx = i as f64 - mean_x;
        num += dx * (y - mean_y);
        den += dx * dx;
    }
    num / den
}
