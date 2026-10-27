//! Exchange by file (`docs/STORAGE.md` §10): "Save work for a teammate" writes the project's
//! last commit as a Scripture Burrito zip, verified after it is written; "Receive work from a
//! file" takes the zip's packs in through the store, as a sync's intake does, and commits.
//!
//! Not built yet: E5's read-back bypassing the page cache (the file is read back and checked
//! through the cache), and R7's `.tc/intake.json` marker (each pack is taken whole, and
//! receiving a file again adds nothing).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tc_journal::fold::{ConflictId, DerivedFile, PersonLabel};
use tc_store::{CheckpointReason, IntakeReport, ProjectHandle, ReceivedPack};

const PACKS: &str = "ingredients/tc/changes/";
const PROJECT_JSON: &str = "ingredients/tc/project.json";
const FORMAT_JSON: &str = "ingredients/tc/format.json";
const METADATA: &str = "metadata.json";
/// Carried files at the root and foreign entries travel under this dot path (§10.1 E2).
const ROOT: &str = ".tc-root/";

/// R1's caps.
const MAX_ENTRY: u64 = 512 << 20;
const MAX_TOTAL: u64 = 4 << 30;
const MAX_ENTRIES: usize = 200_000;

#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    #[error("the project has no history to export yet")]
    NoHistory,
    /// "This file is damaged. Ask for a new copy." Nothing was written.
    #[error("this file is damaged: {0}")]
    Damaged(String),
    /// "This file is for <name>, not <name>."
    #[error("this file holds another project")]
    OtherProject,
    #[error("this file needs a newer translationCore (project format {major})")]
    NeedsNewerApp { major: String },
    /// The exported file did not read back as written; it was deleted.
    #[error("the file did not read back as written: {0}")]
    NotVerified(String),
    /// The latest saves could not be committed (another installation is saving into the
    /// folder, the project is read-only, …): nothing was exported.
    #[error("the latest work could not be committed: {0}")]
    NotCommitted(String),
    /// The intake failed after some of the file's packs were folded: what was received stays
    /// in the project, so the person is never told nothing changed (PLAN Q8).
    #[error("the file was taken in only partly, and what was received was kept: {0}")]
    PartlyReceived(String),
    #[error("{0}")]
    Io(String),
    #[error(transparent)]
    Store(#[from] tc_store::StoreError),
}

/// Why the store skipped the checkpoint, in a translator's words (the reason reaches the
/// screen inside the refusal's sentence).
fn skip_words(why: tc_store::CheckpointSkip) -> String {
    match why {
        tc_store::CheckpointSkip::ReadOnly => "the project is read-only".to_owned(),
        tc_store::CheckpointSkip::SecondWriter => {
            "another translationCore is saving into this project".to_owned()
        }
        other => format!("{other:?}"),
    }
}

fn io(e: impl std::fmt::Display) -> ExchangeError {
    ExchangeError::Io(e.to_string())
}

/// §10.1: checkpoints, then writes HEAD's tree to `<destination>/<abbreviation>-<date>-<person>.zip`,
/// read back and checked before it is reported saved. Returns the file.
pub async fn export(
    project: &ProjectHandle,
    destination: &Path,
    person: &str,
) -> Result<PathBuf, ExchangeError> {
    // The file holds the last commit: a checkpoint that could not commit the latest saves
    // stops the export, so a teammate never gets less than the person sees.
    let checkpoint = project.checkpoint(CheckpointReason::BeforeExport).await?;
    if let Some(why) = checkpoint
        .skipped
        .filter(|why| *why != tc_store::CheckpointSkip::NoChanges)
    {
        return Err(ExchangeError::NotCommitted(skip_words(why)));
    }
    let facts = project.facts();
    let git_dir = facts.git_dir.ok_or(ExchangeError::NoHistory)?;
    let destination = destination.to_owned();
    let person = person.to_owned();
    let device = facts.device.to_string();
    let id = facts.id;
    tokio::task::spawn_blocking(move || write_export(&git_dir, &destination, &person, &device, id))
        .await
        .map_err(io)?
}

fn write_export(
    git_dir: &Path,
    destination: &Path,
    person: &str,
    device: &str,
    id: tc_journal::ProjectId,
) -> Result<PathBuf, ExchangeError> {
    let project = id.to_string();
    let project = project.as_str();
    let git = |e: git2::Error| ExchangeError::Io(e.message().to_owned());
    let repo = git2::Repository::open(git_dir).map_err(git)?;
    let head = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .ok_or(ExchangeError::NoHistory)?;
    let mut entries: Vec<(String, git2::Oid)> = Vec::new();
    head.tree()
        .map_err(git)?
        .walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
            if entry.kind() != Some(git2::ObjectType::Blob) {
                return git2::TreeWalkResult::Ok;
            }
            let path = format!("{dir}{}", entry.name().unwrap_or_default());
            if matches!(path.as_str(), ".gitignore" | ".gitattributes")
                || path.starts_with(".git/")
                || path.starts_with(".tc/")
            {
                return git2::TreeWalkResult::Ok;
            }
            let name = if path == METADATA || path.starts_with("ingredients/") {
                path
            } else {
                format!("{ROOT}{path}")
            };
            entries.push((name, entry.id()));
            git2::TreeWalkResult::Ok
        })
        .map_err(git)?;
    // metadata.json first, then the rest in path order.
    entries.sort_by(|a, b| (a.0 != METADATA, &a.0).cmp(&(b.0 != METADATA, &b.0)));
    let metadata = entries
        .iter()
        .find(|(n, _)| n == METADATA)
        .and_then(|(_, oid)| repo.find_blob(*oid).ok())
        .map(|b| b.content().to_vec())
        .unwrap_or_default();
    let file_name = unique(destination, &file_name(&metadata, project, person))?;
    let temp = destination.join(format!(
        ".~{file_name}.{}.tmp",
        tc_store::random::hex::<8>()
    ));
    let written = (|| -> Result<(), ExchangeError> {
        let file = std::fs::File::create(&temp).map_err(io)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(6));
        let mut dirs = std::collections::BTreeSet::new();
        for (name, oid) in &entries {
            let mut prefix = String::new();
            for part in name
                .split('/')
                .collect::<Vec<_>>()
                .split_last()
                .map_or(&[][..], |(_, d)| d)
            {
                prefix.push_str(part);
                prefix.push('/');
                if dirs.insert(prefix.clone()) {
                    zip.add_directory(prefix.as_str(), options).map_err(io)?;
                }
            }
            let blob = repo.find_blob(*oid).map_err(git)?;
            zip.start_file(name.as_str(), options).map_err(io)?;
            zip.write_all(blob.content()).map_err(io)?;
        }
        let created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        zip.set_comment(format!(
            "tc-exchange 1 {}",
            serde_json::json!({
                "project": project,
                "exporter": { "device": device, "name": person },
                "commit": head.id().to_string(),
                "created": created,
            })
        ))
        .map_err(io)?;
        let file = zip.finish().map_err(io)?;
        file.sync_all().map_err(io)?;
        Ok(())
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    let path = destination.join(&file_name);
    if path.exists() {
        let _ = std::fs::remove_file(&temp);
        return Err(ExchangeError::Io(format!(
            "{file_name} appeared while it was written"
        )));
    }
    std::fs::rename(&temp, &path).map_err(io)?;
    if let Ok(dir) = std::fs::File::open(destination) {
        let _ = dir.sync_all();
    }
    if let Err(e) = verify(&path, id) {
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    Ok(path)
}

/// E5: every entry inflates to its CRC, the burrito's metadata lists exactly its
/// ingredients, and every pack verifies.
fn verify(path: &Path, id: tc_journal::ProjectId) -> Result<(), ExchangeError> {
    let zip = read_zip(path)?;
    let metadata = zip
        .files
        .get(METADATA)
        .ok_or_else(|| ExchangeError::NotVerified("no metadata.json".into()))?;
    let problems = tc_burrito::validate_project(metadata, &ZipFiles(&zip.files));
    if let Some(first) = problems.first() {
        return Err(ExchangeError::NotVerified(format!("{first:?}")));
    }
    for (name, bytes) in &zip.files {
        if name.starts_with(PACKS) && !tc_store::read_received_pack(name, bytes, id).valid {
            return Err(ExchangeError::NotVerified(format!(
                "{name} does not verify"
            )));
        }
    }
    if !zip.damaged.is_empty() {
        return Err(ExchangeError::NotVerified(zip.damaged.join(", ")));
    }
    Ok(())
}

struct ZipFiles<'a>(&'a BTreeMap<String, Vec<u8>>);

impl tc_burrito::ProjectFiles for ZipFiles<'_> {
    fn inspect(&self, path: &tc_burrito::IngredientPath) -> tc_burrito::FileState {
        match self.0.get(path.as_str()) {
            Some(bytes) => tc_burrito::FileState::Present(tc_burrito::FileFacts {
                md5: tc_burrito::Md5::of(bytes),
                size: bytes.len() as u64,
            }),
            None => tc_burrito::FileState::Missing,
        }
    }

    fn ingredient_dir_files(&self) -> Result<Vec<String>, String> {
        Ok(self
            .0
            .keys()
            .filter(|p| {
                p.strip_prefix("ingredients/").is_some_and(|rest| {
                    !rest
                        .split('/')
                        .any(|part| part.starts_with('.') || part.ends_with(".bak"))
                })
            })
            .cloned()
            .collect())
    }
}

/// A zip read under R1's rules: its files by name, and the entries that did not inflate to
/// their CRC.
struct ReadZip {
    files: BTreeMap<String, Vec<u8>>,
    damaged: Vec<String>,
}

fn read_zip(path: &Path) -> Result<ReadZip, ExchangeError> {
    let file = std::fs::File::open(path).map_err(io)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| ExchangeError::Damaged(e.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(ExchangeError::Damaged(format!("{} entries", zip.len())));
    }
    let mut files = BTreeMap::new();
    let mut damaged = Vec::new();
    let mut total = 0u64;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| ExchangeError::Damaged(e.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let raw = entry.name().replace('\\', "/");
        let symlink = entry
            .unix_mode()
            .is_some_and(|m| m & 0o170_000 == 0o120_000);
        if entry.enclosed_name().is_none()
            || raw.starts_with('/')
            || raw.split('/').any(|p| p == "..")
            || raw.chars().nth(1) == Some(':')
            || symlink
        {
            return Err(ExchangeError::Damaged(format!("the entry {raw:?}")));
        }
        if entry.size() > MAX_ENTRY {
            return Err(ExchangeError::Damaged(format!("{raw} is too large")));
        }
        total += entry.size();
        if total > MAX_TOTAL {
            return Err(ExchangeError::Damaged("the file is too large".into()));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        // The zip crate checks the CRC when the entry is read to its end.
        match entry.read_to_end(&mut bytes) {
            Ok(_) => {
                files.insert(raw, bytes);
            }
            Err(_) => damaged.push(raw),
        }
    }
    Ok(ReadZip { files, damaged })
}

/// What receiving a file did (§10.2 R8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveReport {
    pub intake: IntakeReport,
    pub received_by: BTreeMap<PersonLabel, u32>,
    /// The changes this receive took in (new before the file, in the project after it), for
    /// the Received sheet's "what replaced what".
    pub received_ids: Vec<tc_journal::ChangeId>,
    pub new_conflicts: Vec<ConflictId>,
    /// Entries whose bytes failed their CRC: never compared, reported or written (R4).
    pub damaged: Vec<String>,
    /// Derived files in the zip that its own packs do not make (`rulings/2026-10-22-H22.md`
    /// part 3): not taken in, nothing kept; "Juan's file holds a changed Titus that was not
    /// made in translationCore. It was not taken in; the file on the stick still has it."
    pub hand_edited: Vec<String>,
}

/// What an exchange file holds: the project's name as `metadata.json` states it (the first
/// language listed) and its id. Nothing is written (§10.2 R1-R2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identified {
    pub name: String,
    pub id: tc_journal::ProjectId,
}

/// §10.2 R1-R2: reads the file's project before the person confirms the receive.
pub fn identify(path: &Path) -> Result<Identified, ExchangeError> {
    let zip = read_zip(path)?;
    let json = |name: &str| {
        zip.files
            .get(name)
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
    };
    let id = json(PROJECT_JSON)
        .and_then(|v| {
            v.get("projectId")?
                .as_str()
                .and_then(tc_journal::ProjectId::parse)
        })
        .ok_or_else(|| ExchangeError::Damaged("no project id".into()))?;
    let name = json(METADATA)
        .and_then(|v| {
            v.pointer("/identification/name")?
                .as_object()?
                .values()
                .next()?
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_default();
    Ok(Identified { name, id })
}

/// §10.2: takes a teammate's exchange file into `project`. A file of another project, or of
/// a newer format, is refused before anything is written.
pub async fn receive(
    project: &ProjectHandle,
    path: &Path,
    from: &str,
) -> Result<ReceiveReport, ExchangeError> {
    let facts = project.facts();
    let git_dir = facts.git_dir.clone().ok_or(ExchangeError::NoHistory)?;
    let id = facts.id;
    let kind = facts.kind;
    let path = path.to_owned();
    let (packs, damaged, hand_edited, changes) =
        tokio::task::spawn_blocking(move || read_received(&path, &git_dir, id, kind))
            .await
            .map_err(io)??;
    let held = project.snapshot();
    let before: std::collections::BTreeSet<ConflictId> =
        held.fold().conflicts().map(|c| c.id).collect();
    let report = match project.receive_packs(from.to_owned(), packs).await {
        Ok(report) => report,
        Err(e) => {
            let fold = project.snapshot();
            let some_held = changes
                .iter()
                .any(|(_, id, _)| fold.fold().change(id).is_some());
            return Err(if some_held {
                ExchangeError::PartlyReceived(e.to_string())
            } else {
                e.into()
            });
        }
    };
    if let Some(why) = report.skipped.filter(|why| {
        matches!(
            why,
            tc_store::CheckpointSkip::ReadOnly | tc_store::CheckpointSkip::SecondWriter
        )
    }) {
        return Err(ExchangeError::NotCommitted(skip_words(why)));
    }
    let snapshot = project.snapshot();
    // Received: new before the file, and in the project after it.
    let mut received_from: BTreeMap<tc_journal::DeviceId, u32> = BTreeMap::new();
    let mut received_ids = Vec::new();
    let mut batches: std::collections::BTreeSet<(tc_journal::DeviceId, tc_journal::BatchId)> =
        std::collections::BTreeSet::new();
    for (device, change, batch) in changes {
        if held.fold().change(&change).is_none() && snapshot.fold().change(&change).is_some() {
            if let Some(batch) = batch
                && batches.insert((device, batch))
            {
                *received_from.entry(device).or_insert(0) += 1;
            }
            received_ids.push(change);
        }
    }
    let labels = snapshot.fold().person_labels(facts.device);
    let mut received_by = BTreeMap::new();
    for (device, count) in received_from {
        let label = labels
            .get(&device)
            .cloned()
            .unwrap_or(PersonLabel::Teammate(0));
        *received_by.entry(label).or_insert(0) += count;
    }
    Ok(ReceiveReport {
        intake: report.intake,
        received_by,
        received_ids,
        new_conflicts: snapshot
            .fold()
            .conflicts()
            .map(|c| c.id)
            .filter(|c| !before.contains(c))
            .collect(),
        damaged,
        hand_edited,
    })
}

/// The packs to take, the damaged entries, the hand-edited derived files, and every change
/// of the packs with its device.
type Read_ = (
    Vec<ReceivedPack>,
    Vec<String>,
    Vec<String>,
    Vec<(
        tc_journal::DeviceId,
        tc_journal::ChangeId,
        Option<tc_journal::BatchId>,
    )>,
);

fn read_received(
    path: &Path,
    git_dir: &Path,
    id: tc_journal::ProjectId,
    kind: tc_journal::fold::ProjectKind,
) -> Result<Read_, ExchangeError> {
    let zip = read_zip(path)?;
    // R2: the open project's file only.
    let theirs = zip
        .files
        .get(PROJECT_JSON)
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
        .and_then(|v| v.get("projectId")?.as_str().map(str::to_owned));
    if theirs.as_deref() != Some(id.to_string().as_str()) {
        return Err(ExchangeError::OtherProject);
    }
    // R3.
    if let Some(bytes) = zip.files.get(FORMAT_JSON)
        && let Ok(doc) = tc_journal::format::FormatDoc::parse(bytes)
        && let tc_journal::format::FormatGate::Refused { major } = doc.gate()
    {
        return Err(ExchangeError::NeedsNewerApp {
            major: major.to_string(),
        });
    }
    let repo =
        git2::Repository::open(git_dir).map_err(|e| ExchangeError::Io(e.message().to_owned()))?;
    let blob = |bytes: &[u8]| -> Result<tc_journal::GitOid, ExchangeError> {
        let oid = repo
            .blob(bytes)
            .map_err(|e| ExchangeError::Io(e.message().to_owned()))?;
        Ok(tc_journal::GitOid::from_bytes(
            oid.as_bytes().try_into().unwrap_or([0; 20]),
        ))
    };
    let mut packs = Vec::new();
    let mut changes = Vec::new();
    let mut inputs = Vec::new();
    let mut all_valid = zip.damaged.is_empty();
    for (name, bytes) in &zip.files {
        if name.starts_with(PACKS) {
            let read = tc_store::read_received_pack(name, bytes, id);
            all_valid &= read.valid;
            for change in read.changes {
                // Bookkeeping joins the project but is never counted; work is counted once
                // per batch, one save (`crate::is_bookkeeping`).
                let body = change.body();
                let batch = (!crate::is_bookkeeping(&body.record)).then_some(body.batch.id);
                changes.push((body.device, change.id(), batch));
                inputs.push(tc_journal::fold::FoldInput::from(change));
            }
            packs.push(ReceivedPack {
                path: name.clone(),
                bytes: bytes.clone(),
                blob: blob(bytes)?,
            });
        } else if DerivedFile::from_path(name).is_none()
            && name != METADATA
            && !name.starts_with("ingredients/tc/")
        {
            // R5: a carried file's bytes, so the projection can use any version.
            blob(bytes)?;
        }
    }
    // R6 and H22 part 3: a derived file the zip's own packs do not make was edited by hand.
    // With a damaged pack the packs cannot make the files they were written from, so nothing
    // is named: the report names the damaged pack instead.
    let hand_edited = if all_valid {
        let fold = tc_journal::fold::FoldState::fold(kind, inputs);
        zip.files
            .iter()
            .filter_map(|(name, bytes)| {
                let file = DerivedFile::from_path(name)?;
                (fold.render(&file).as_deref() != Some(bytes.as_slice())).then(|| name.clone())
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok((packs, zip.damaged, hand_edited, changes))
}

/// `<abbreviation>-<YYYY-MM-DD>-<person>.zip` (E4), each part cleaned so a display name can
/// never make the export fail.
fn file_name(metadata: &[u8], project: &str, person: &str) -> String {
    let abbreviation = serde_json::from_slice::<serde_json::Value>(metadata)
        .ok()
        .and_then(|v| {
            let names = v
                .get("identification")?
                .get("abbreviation")?
                .as_object()?
                .clone();
            names.values().next()?.as_str().map(str::to_owned)
        })
        .unwrap_or_default();
    let clean = |text: &str, fallback: &str| {
        let mut out: String = text
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
            .take(40)
            .collect();
        let reserved = [
            "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "lpt1", "lpt2", "lpt3",
        ];
        if out.is_empty() || reserved.contains(&out.to_lowercase().as_str()) {
            out = fallback.chars().take(8).collect();
        }
        out
    };
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    let (y, m, d) = civil(days);
    format!(
        "{}-{y:04}-{m:02}-{d:02}-{}.zip",
        clean(&abbreviation, project),
        clean(person, project)
    )
}

/// The calendar date of `days` since 1970-01-01 (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// `name`, or `name` with `-2`, `-3`, … before `.zip` when it is taken.
fn unique(destination: &Path, name: &str) -> Result<String, ExchangeError> {
    if !destination.join(name).exists() {
        return Ok(name.to_owned());
    }
    let stem = name.strip_suffix(".zip").unwrap_or(name);
    (2..10_000)
        .map(|n| format!("{stem}-{n}.zip"))
        .find(|candidate| !destination.join(candidate).exists())
        .ok_or_else(|| ExchangeError::Io("no free file name".into()))
}

/// R2 "Create the project from this file": a newcomer builds the project in a staging folder
/// from the file's entries (`.tc-root/` back at the root), written with the store's atomic
/// writes, and places it as `name`. The first open folds the packs and makes the history.
pub fn receive_new(
    store: &tc_store::Store,
    path: &Path,
    name: &str,
) -> Result<PathBuf, ExchangeError> {
    let zip = read_zip(path)?;
    if !zip.damaged.is_empty() {
        return Err(ExchangeError::Damaged(zip.damaged.join(", ")));
    }
    if !zip.files.contains_key(PROJECT_JSON) {
        return Err(ExchangeError::OtherProject);
    }
    if let Some(bytes) = zip.files.get(FORMAT_JSON)
        && let Ok(doc) = tc_journal::format::FormatDoc::parse(bytes)
        && let tc_journal::format::FormatGate::Refused { major } = doc.gate()
    {
        return Err(ExchangeError::NeedsNewerApp {
            major: major.to_string(),
        });
    }
    let staging = store.staging(tc_store::StagingKind::Import);
    let vfs = store.vfs().clone();
    let written = (|| -> Result<(), ExchangeError> {
        for (entry, bytes) in &zip.files {
            let rel = entry.strip_prefix(ROOT).unwrap_or(entry);
            // The file's own `.git/` and `.tc/` never shape the new project: a hand-made zip
            // could connect it to a stranger's repository (join filters the same, Q10).
            if rel.starts_with(".git/") || rel.starts_with(".tc/") {
                continue;
            }
            let full = staging.join(rel);
            let (Some(dir), Some(file)) =
                (full.parent(), full.file_name().and_then(|n| n.to_str()))
            else {
                continue;
            };
            tc_store::atomic_create_dirs(&*vfs, dir).map_err(io)?;
            tc_store::atomic_write(&*vfs, dir, file, bytes, tc_store::SyncDir::Now)
                .map_err(|e| io(e.into_inner()))?;
        }
        Ok(())
    })();
    written?;
    store.place(&staging, name).map_err(io)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        reason = "test code: a failed unwrap is a failed test"
    )]

    #[test]
    fn civil_dates() {
        assert_eq!(super::civil(0), (1970, 1, 1));
        assert_eq!(super::civil(20_751), (2026, 10, 25));
        assert_eq!(super::civil(11_016), (2000, 2, 29));
    }

    #[test]
    fn a_display_name_never_makes_the_file_name_fail() {
        let name = super::file_name(b"{}", "0123456789abcdef", "CON");
        assert!(name.starts_with("01234567-"), "{name}");
        assert!(name.ends_with("-01234567.zip"), "{name}");
        let name = super::file_name(
            br#"{"identification":{"abbreviation":{"en":"tit"}}}"#,
            "p",
            "Juan P\u{e9}rez/../x",
        );
        assert!(name.starts_with("tit-"), "{name}");
        assert!(!name.contains('/'), "{name}");
    }
}
