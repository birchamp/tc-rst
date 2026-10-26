//! Import a translationCore 3 project, and Update a book from USFM (unit F1; STORAGE §13.4,
//! §13.6). The handlers check what the person typed and the file before anything is written,
//! with the rules create uses (the folder a name makes, a name in use, a language code a
//! Scripture Burrito can hold, #835), and hold the uploads; the [`ImportPort`] reads and
//! writes. [`TcImport`] is the real port, over tc-import and the store.

use std::collections::BTreeMap;
use std::sync::Arc;

use tc_api::create::Details;
use tc_api::error::Feature;
use tc_api::ids::{JobId, ProjectRef};
use tc_api::import::{
    CarryOver, FoundBook, FoundVersion, ImportDetails, ImportFile, ImportFound, ImportReview,
    Review, Run, UpdateBook, UpdateReport, VerseAt, VersionSlot, Versions, VersionsView,
};
use tc_api::jobs::{JobKind, Phase};
use tc_api::refusal::{CreateRefusal, ImportRefusal};
use tc_api::shown::Direction;
use tc_api::{ApiError, Refusal};
use tc_check::Collection;
use tc_check::carry::{self, Outcome, StoredDecision};
use tc_core::BookId;
use tc_import::SourceFile;
use tc_import::tc3::{DecisionPins, Slot, Tc3Import, VersionRequest};
use tc_journal::fold::ProjectKind;
use tc_journal::keys::{MetaField, ToolId};
use tc_journal::outside::UpdatePart;
use tc_journal::record::CheckValue;
use tc_journal::{Cause, ImportSource, RecordKey};
use tc_journal::{Record, keys::PinGroup};
use tc_resources::{BookCoverage, GatewayLanguage, ProjectPins, ResourcePin, SetSlot};
use tc_usfm::codec::{BookParts, VersePart};
use tc_versification::StandardScheme;
use unicode_normalization::UnicodeNormalization;

use crate::app::App;
use crate::backend::{BoxFuture, CommandError, CreateFailure, NewProjectSpec, StoreCommand};
use crate::dispatch::Handle;
use crate::flows::FlowRequest;
use crate::jobs::{JobCtx, JobEnd, JobSpec};
use crate::plans::{self, ListSource, PlanError, PlanSources, UnitSpec};
use crate::ports::ImportPort;
use crate::session::ProjectSession;
use crate::wire;

/// The font a new project names when the person chose none, as New Bible's first choice.
const DEFAULT_FONT: &str = "Noto Sans (default)";

fn refused(refusal: ImportRefusal) -> ApiError {
    ApiError::Refused(Refusal::Import(refusal))
}

/// tc-import's refusal in the words the person reads.
pub fn import_refusal(refusal: tc_import::ImportRefusal) -> ImportRefusal {
    use tc_import::ImportRefusal as R;
    match refusal {
        R::Truncated { file } => ImportRefusal::NotZip { file },
        R::CorruptEntry { file, entry } => ImportRefusal::CorruptEntry { file, entry },
        R::NoManifest { file } => ImportRefusal::NoManifest { file },
        R::NoText { file } => ImportRefusal::NoText { file },
        R::Languages(languages) => ImportRefusal::Languages { languages },
        R::BookTwice { book } => ImportRefusal::Twice { code: book },
        R::Unreadable { file, problem } => ImportRefusal::Unreadable { file, problem },
        R::TextChanged { book } => ImportRefusal::TextChanged { book },
    }
}

/// The create form's details for an import: create's own rules make the folder and the
/// first records.
fn create_details(details: &ImportDetails) -> Details {
    Details {
        name: details.name.trim().to_owned(),
        language_name: String::new(),
        code: details.code.trim().to_owned(),
        door43_code: None,
        direction: Direction::Ltr,
        font: DEFAULT_FONT.to_owned(),
    }
}

/// The verses of a book and those with words, as Home's drafted test counts them.
fn verse_counts(parts: &BookParts) -> (u32, u32) {
    let mut verses = 0;
    let mut drafted = 0;
    for chapter in parts.chapters.values() {
        for part in chapter.verses.values() {
            if let VersePart::Text(verse) = part {
                verses += 1;
                let words = tc_core::js::trim(&verse.words);
                if !words.is_empty() && words != "___" {
                    drafted += 1;
                }
            }
        }
    }
    (verses, drafted)
}

/// What the review step shows of an import.
pub fn found(import: &Tc3Import) -> ImportFound {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    ImportFound {
        details: ImportDetails {
            name: import.name.clone(),
            code: import.language.clone(),
        },
        licenses: import.licenses.clone(),
        contributors: import.contributors.clone(),
        books: import
            .books
            .iter()
            .map(|book| {
                let parts = tc_usfm::codec::decompose(&book.usfm).ok();
                let (verses, drafted) = parts.as_ref().map(verse_counts).unwrap_or_default();
                FoundBook {
                    book: book.book,
                    chapters: count(parts.map_or(0, |p| p.chapters.len())),
                    verses,
                    drafted,
                    aligned: count(
                        book.alignments()
                            .iter()
                            .filter(|(_, _, value)| {
                                value.alignments.iter().any(|g| !g.bottom_words.is_empty())
                            })
                            .count(),
                    ),
                    decisions: count(
                        book.decision_count(ToolId::TranslationNotes)
                            + book.decision_count(ToolId::TranslationWords),
                    ),
                    not_kept: book.not_kept.clone(),
                }
            })
            .collect(),
        slots: candidates(&import.versions)
            .into_keys()
            .map(slot_of)
            .collect(),
    }
}

/// The installed versions the decisions are stored against: the bundled translationNotes and
/// translationWords links, the pins a new project gets (`importer.review.useInstalled`).
fn installed_pins() -> Option<DecisionPins> {
    let helps = tc_resources::suite::english_helps();
    Some(DecisionPins {
        translation_notes: helps.pin(SetSlot::TranslationNotes)?.resource(),
        translation_words: tc_resources::suite::english_twl().resource(),
    })
}

/// The verses of `book` whose alignment markup does not read apart: the store imports them as
/// text with the markup (§13.6 U4), and the report lists them.
fn markup_kept(parts: &BookParts) -> Vec<VerseAt> {
    let mut out = Vec::new();
    for (&chapter, parts) in &parts.chapters {
        for (&verse, part) in &parts.verses {
            let VersePart::Text(text) = part else {
                continue;
            };
            let content = tc_usfm::codec::join_verse(&text.words, &text.layout);
            let Some(last) = tc_journal::VerseNo::new(text.last) else {
                continue;
            };
            if content.contains("\\zaln-s")
                && tc_journal::alignment::split_aligned(&content, last).is_none()
            {
                out.push(VerseAt { chapter, verse });
            }
        }
    }
    out
}

fn verse_at(key: &RecordKey) -> Option<VerseAt> {
    match key {
        RecordKey::Verse(_, chapter, verse) => Some(VerseAt {
            chapter: chapter.get(),
            verse: verse.get(),
        }),
        _ => None,
    }
}

impl App {
    /// The uploads as the files the person chose, read on the blocking pool.
    async fn uploaded(&self, files: &[ImportFile]) -> Result<Vec<SourceFile>, ApiError> {
        let wanted: Vec<_> = files
            .iter()
            .map(|f| (f.name.clone(), self.upload_path(&f.upload)))
            .collect();
        let read = tokio::task::spawn_blocking(move || {
            wanted
                .into_iter()
                .map(|(name, path)| std::fs::read(path).map(|bytes| SourceFile { name, bytes }))
                .collect::<Result<Vec<_>, _>>()
        })
        .await
        .map_err(|e| {
            refused(ImportRefusal::Unreadable {
                file: String::new(),
                problem: e.to_string(),
            })
        })?;
        read.map_err(|_| ApiError::BadRequest {
            field: "upload".to_owned(),
            reason: "no such upload".to_owned(),
        })
    }

    fn drop_uploads(&self, files: &[ImportFile]) {
        for file in files {
            self.drop_upload(&file.upload);
        }
    }

    /// The folder `details` make, or why Import is off with them: create's rules, before
    /// anything is written.
    async fn import_target(
        &self,
        details: &ImportDetails,
    ) -> Result<Result<ProjectRef, Refusal>, ApiError> {
        let form = create_details(details);
        let folder = match form.folder() {
            Ok(folder) => folder,
            Err(refusal) => return Ok(Err(Refusal::Create(refusal))),
        };
        // INV-ONE-LANGUAGE-CODE-RULE: the rule the Burrito schema applies to every created
        // project's language (#835).
        if !tc_burrito::is_language_tag(&form.code) {
            return Ok(Err(Refusal::Create(CreateRefusal::CodeNotHeld {
                code: form.code,
                reason: "it is not a language tag a Scripture Burrito can hold".to_owned(),
            })));
        }
        if self.name_taken(&folder).await? {
            return Ok(Err(Refusal::Import(ImportRefusal::NameExists)));
        }
        Ok(Ok(folder))
    }

    /// A project the import made: it counts as use, and Home lists it.
    fn imported(&self, folder: &ProjectRef) {
        self.inner
            .prefs
            .record_last_used(folder.clone(), self.inner.clock.now());
        self.inner
            .bus
            .publish(tc_api::event::Event::ProjectsChanged);
    }
}

impl Handle<Review> for App {
    async fn handle(&self, req: Review) -> Result<ImportReview, ApiError> {
        let files = self.uploaded(&req.files).await?;
        let carry_over = match &req.use_installed {
            Some(kept) => Some(
                self.inner
                    .import
                    .carry_over(self.clone(), files.clone(), found_pins(kept)?)
                    .await?,
            ),
            None => None,
        };
        let found = self.inner.import.review(files).await?;
        let details = req.details.unwrap_or_else(|| found.details.clone());
        let (folder, refusal) = match self.import_target(&details).await? {
            Ok(folder) => (Some(folder), None),
            Err(refusal) => (create_details(&details).folder().ok(), Some(refusal)),
        };
        Ok(ImportReview {
            found,
            details,
            folder,
            refusal,
            carry_over,
        })
    }
}

impl Handle<Versions> for App {
    async fn handle(&self, req: Versions) -> Result<JobId, ApiError> {
        let files = self.uploaded(&req.files).await?;
        let app = self.clone();
        let spec = JobSpec::new(JobKind::ImportVersions, Phase::Import);
        Ok(self
            .inner
            .jobs
            .start_typed::<Versions, _, _>(spec, move |_ctx| async move {
                let port = app.inner.import.clone();
                port.versions(app, files).await
            }))
    }
}

impl Handle<Run> for App {
    /// The details are checked as the review checked them, since the folder may have been
    /// made meanwhile (tC4 j09 :367); then one job, which holds the import flow.
    async fn handle(&self, req: Run) -> Result<JobId, ApiError> {
        let folder = self
            .import_target(&req.details)
            .await?
            .map_err(ApiError::Refused)?;
        let found = found_pins(&req.found)?;
        let files = self.uploaded(&req.files).await?;
        let pass = self.inner.flows.enter(FlowRequest::Import)?;
        let app = self.clone();
        let spec = JobSpec::new(JobKind::Import, Phase::Import).pass(pass);
        Ok(self
            .inner
            .jobs
            .start_typed::<Run, _, _>(spec, move |ctx| async move {
                let port = app.inner.import.clone();
                let made = port
                    .run(
                        app.clone(),
                        files,
                        req.details,
                        found,
                        req.license,
                        folder,
                        ctx,
                    )
                    .await?;
                app.imported(&made);
                app.drop_uploads(&req.files);
                Ok(made)
            }))
    }
}

impl Handle<UpdateBook> for App {
    /// INV-UPDATE-A-BOOK-REPORTS-WHAT-IT-DID: a file that does not read, or that names another
    /// book, is refused here with its line, before the job and before anything is written
    /// (§13.6 U1).
    async fn handle(&self, req: UpdateBook) -> Result<JobId, ApiError> {
        let session = self.read(&req.at)?;
        let mut files = self.uploaded(std::slice::from_ref(&req.file)).await?;
        let mut file = files.pop().ok_or_else(|| ApiError::BadRequest {
            field: "upload".to_owned(),
            reason: "no such upload".to_owned(),
        })?;
        // Stored text is NFC, as SaveVerse and the tC3 import store it: the same words in
        // another normal form would otherwise read as a conflict.
        if let Ok(text) = std::str::from_utf8(&file.bytes) {
            file.bytes = text.nfc().collect::<String>().into_bytes();
        }
        let parts = tc_usfm::codec::decompose_bytes(&file.bytes).map_err(|e| {
            refused(ImportRefusal::UpdateUnreadable {
                file: file.name.clone(),
                line: u32::try_from(e.line).unwrap_or(u32::MAX),
                problem: e.problem.to_string(),
            })
        })?;
        if let Some(named) = tc_usfm::seed::id_book(&parts.header)
            && named != req.book
        {
            return Err(refused(ImportRefusal::UpdateOtherBook {
                file: file.name,
                named: named.code().to_owned(),
                chosen: req.book.code().to_owned(),
            }));
        }
        let app = self.clone();
        let spec = JobSpec::new(JobKind::UpdateBook, Phase::Import)
            .project(session.scope().project.clone());
        Ok(self
            .inner
            .jobs
            .start_typed::<UpdateBook, _, _>(spec, move |ctx| async move {
                let port = app.inner.import.clone();
                let report = port.update_book(session, req.book, file, ctx).await;
                app.drop_uploads(std::slice::from_ref(&req.file));
                report
            }))
    }
}

/// The real import: tc-import reads, the store writes.
pub struct TcImport;

/// Reads the files on the blocking pool.
async fn read_tc3(files: Vec<SourceFile>) -> Result<Tc3Import, ApiError> {
    tokio::task::spawn_blocking(move || tc_import::tc3::read(&files))
        .await
        .map_err(|e| {
            refused(ImportRefusal::Unreadable {
                file: String::new(),
                problem: e.to_string(),
            })
        })?
        .map_err(|e| refused(import_refusal(e)))
}

fn slot_of(slot: Slot) -> VersionSlot {
    match slot {
        Slot::TranslationNotes => VersionSlot::TranslationNotes,
        Slot::TranslationWords => VersionSlot::TranslationWords,
        Slot::OriginalNewTestament => VersionSlot::OriginalNewTestament,
        Slot::OriginalOldTestament => VersionSlot::OriginalOldTestament,
    }
}

/// A tag's numbers, for "newest first": `v87` after `v9`.
fn tag_order(tag: &str) -> Vec<u64> {
    tag.split(|c: char| !c.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .map(|run| run.parse().unwrap_or(u64::MAX))
        .collect()
}

/// The candidates to try for each slot, newest first (tC4 `resolveVersions`). A helps slot
/// whose books name different versions is left to the installed versions: no one pin holds
/// all their decisions (tC4 D82 point 2).
fn candidates(requests: &[VersionRequest]) -> BTreeMap<Slot, Option<Vec<(String, String)>>> {
    let mut by_slot: BTreeMap<Slot, Vec<&VersionRequest>> = BTreeMap::new();
    for request in requests {
        by_slot.entry(request.slot).or_default().push(request);
    }
    by_slot
        .into_iter()
        .map(|(slot, asks)| {
            let helps = matches!(slot, Slot::TranslationNotes | Slot::TranslationWords);
            let first = &asks[0].candidates;
            if helps && asks.iter().any(|a| &a.candidates != first) {
                return (slot, None);
            }
            let mut all: Vec<(String, String)> =
                asks.iter().flat_map(|a| a.candidates.clone()).collect();
            all.sort_by_key(|c| std::cmp::Reverse(tag_order(&c.1)));
            all.dedup();
            (slot, Some(all))
        })
        .collect()
}

/// The versions the person accepted, as pins. A translationWords version comes with its links
/// or not at all (tC4 D97 ruling 8): the decisions of the words tool are stored on the links.
fn found_pins(found: &[FoundVersion]) -> Result<Vec<(VersionSlot, ResourcePin)>, ApiError> {
    let pins = found
        .iter()
        .map(|f| {
            let bad = |what: &str| ApiError::BadRequest {
                field: "found".to_owned(),
                reason: format!("{} {}: not {what}", f.repo_path, f.tag),
            };
            let repo = f.repo_path.parse().map_err(|_| bad("a repository"))?;
            let sha = f.sha.parse().map_err(|_| bad("a commit"))?;
            let tag = f.tag.parse().map_err(|_| bad("a tag"))?;
            Ok((f.slot, ResourcePin::new(repo, sha).with_version(tag)))
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let has = |slot| pins.iter().any(|(s, _)| *s == slot);
    if has(VersionSlot::TranslationWords) != has(VersionSlot::TranslationWordsLinks) {
        return Err(ApiError::BadRequest {
            field: "found".to_owned(),
            reason: "a translationWords version comes with its links".to_owned(),
        });
    }
    Ok(pins)
}

/// The gateway a `<language>_tw` repository is of, by its owner and language (tC4
/// `gatewayOfTw`): the owner decides whether a `_twl` repository exists (D97 ruling 7).
fn gateway_of_tw(repo: &tc_door43::RepoRef) -> Option<&'static tc_resources::gateways::Gateway> {
    let language = repo.name.as_str().strip_suffix("_tw")?;
    tc_resources::gateways::find(language, repo.owner.as_str())
}

/// `git.door43.org/<owner>/<name>` as the client's repository.
fn door43_repo(repo_path: &str) -> Option<tc_door43::RepoRef> {
    repo_path.strip_prefix("git.door43.org/")?.parse().ok()
}

/// Seconds since 1970 of a Door43 date (`2024-05-01T12:34:56Z`, or with an offset), as tC4's
/// `Date.parse` reads it; `None` when it does not read.
fn epoch_seconds(text: &str) -> Option<i64> {
    let num = |at: &str, from: usize, to: usize| at.get(from..to)?.parse::<i64>().ok();
    let (y, mo, d) = (num(text, 0, 4)?, num(text, 5, 7)?, num(text, 8, 10)?);
    let (h, mi, s) = (num(text, 11, 13)?, num(text, 14, 16)?, num(text, 17, 19)?);
    let zone = text
        .get(19..)?
        .trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match zone.as_bytes().first() {
        None | Some(b'Z') => 0,
        Some(b'+') => num(zone, 1, 3)? * 3600 + num(zone, 4, 6)? * 60,
        Some(b'-') => -(num(zone, 1, 3)? * 3600 + num(zone, 4, 6)? * 60),
        Some(_) => return None,
    };
    // Days from the civil date (Howard Hinnant's algorithm), with March as the year's start.
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((mo + 9) % 12) + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + s - offset)
}

/// The links pin that goes with a found translationWords version (tC4 `twlFor`, D97 ruling
/// 8). An owner with no `_twl` repository takes the bundled `en_twl`, with no lookup.
/// Otherwise the `<language>_twl` release closest in date to the words release (the latest
/// when that one has no date), each candidate's tag resolved to its commit, closest first.
/// `None` when none can be pinned or Door43 did not answer: then the words stay unresolved.
async fn twl_for(client: &tc_door43::Door43Client, tw: &FoundVersion) -> Option<FoundVersion> {
    let repo = door43_repo(&tw.repo_path)?;
    let gateway = gateway_of_tw(&repo);
    if gateway.is_some_and(|g| !g.has_twl) {
        let pin = tc_resources::suite::english_twl();
        return Some(FoundVersion {
            slot: VersionSlot::TranslationWordsLinks,
            repo_path: pin.repo.to_string(),
            tag: pin.version.as_ref().map(ToString::to_string)?,
            sha: pin.sha.to_hex(),
        });
    }
    if !repo.name.as_str().ends_with("_tw") {
        return None;
    }
    let twl_path = format!("{}l", tw.repo_path);
    let twl_repo = door43_repo(&twl_path)?;
    let tw_releases = match client.releases(&repo).await {
        Ok(listing) => listing.items,
        Err(tc_door43::Door43Error::NotFound { .. }) => Vec::new(),
        Err(_) => return None,
    };
    let mut twl_releases = client.releases(&twl_repo).await.ok()?.items;
    let tw_at = tw_releases
        .iter()
        .find(|r| r.tag.to_string() == tw.tag)
        .and_then(|r| epoch_seconds(&r.created_at));
    twl_releases.sort_by_key(|r| match (tw_at, epoch_seconds(&r.created_at)) {
        (Some(at), Some(date)) => (date - at).abs(),
        (None, Some(date)) => -date,
        (_, None) => i64::MAX,
    });
    for release in twl_releases {
        match client.tag_commit(&twl_repo, &release.tag).await {
            Ok(sha) => {
                return Some(FoundVersion {
                    slot: VersionSlot::TranslationWordsLinks,
                    repo_path: twl_path,
                    tag: release.tag.to_string(),
                    sha: sha.to_hex(),
                });
            }
            Err(tc_door43::Door43Error::NotFound { .. }) => continue,
            Err(_) => return None,
        }
    }
    None
}

/// A found version the files do not name, as a slot and a version tC3 checked against: a
/// lookup of other files, so its pins would not hold these decisions. The links come with the
/// words they belong to ([`found_pins`]).
fn not_named(import: &Tc3Import, found: &[(VersionSlot, ResourcePin)]) -> Option<String> {
    let named = candidates(&import.versions);
    found.iter().find_map(|(slot, pin)| {
        if *slot == VersionSlot::TranslationWordsLinks {
            return None;
        }
        let tag = pin.version.as_ref().map(ToString::to_string);
        let held = named
            .iter()
            .find(|(s, _)| slot_of(**s) == *slot)
            .and_then(|(_, tried)| tried.as_ref())
            .is_some_and(|tried| {
                tried
                    .iter()
                    .any(|(repo, t)| *repo == pin.repo.to_string() && Some(t) == tag.as_ref())
            });
        (!held).then(|| format!("{slot:?} {}: not a version the files name", pin.repo))
    })
}

/// The found version of `slot`.
fn found_of(found: &[(VersionSlot, ResourcePin)], slot: VersionSlot) -> Option<&ResourcePin> {
    found.iter().find(|(s, _)| *s == slot).map(|(_, pin)| pin)
}

/// The found versions laid over a new project's pins (tC4 `applyVersions`): each keeps the
/// flavor of the pin it replaces. A helps pin records the books tC3 checked against it, and
/// the set takes the gateway language the decisions were made in.
fn lay_over(pins: &mut ProjectPins, found: &[(VersionSlot, ResourcePin)], import: &Tc3Import) {
    let over = |old: Option<&ResourcePin>, new: &ResourcePin| ResourcePin {
        flavor: Some(
            old.and_then(|o| o.flavor.clone())
                .unwrap_or_else(|| "scripture/textTranslation".to_owned()),
        ),
        ..new.clone()
    };
    let Some(primary) = pins.primary.as_mut() else {
        return;
    };
    for (slot, pin) in found {
        let set_slot = match slot {
            VersionSlot::TranslationNotes => SetSlot::TranslationNotes,
            VersionSlot::TranslationWords => SetSlot::TranslationWords,
            VersionSlot::TranslationWordsLinks => SetSlot::TranslationWordsLinks,
            VersionSlot::OriginalNewTestament => {
                pins.original_language.nt = Some(over(pins.original_language.nt.as_ref(), pin));
                continue;
            }
            VersionSlot::OriginalOldTestament => {
                pins.original_language.ot = Some(over(pins.original_language.ot.as_ref(), pin));
                continue;
            }
        };
        let laid = over(primary.pins.get(&set_slot), pin);
        primary.pins.insert(set_slot, laid);
    }
    // The bundled `en_twl` of an owner with no `_twl` was not looked up: it gets no claim.
    let words = found_of(found, VersionSlot::TranslationWords)
        .filter(|tw| gateway_of_tw(tw.repo.repo()).is_none_or(|g| g.has_twl));
    let claims = [
        (
            found_of(found, VersionSlot::TranslationNotes),
            ToolId::TranslationNotes,
            SetSlot::TranslationNotes,
        ),
        (
            words,
            ToolId::TranslationWords,
            SetSlot::TranslationWordsLinks,
        ),
    ];
    for (pin, tool, slot) in claims {
        let books: Vec<BookId> = import
            .books
            .iter()
            .filter(|b| b.decision_count(tool) > 0)
            .map(|b| b.book)
            .collect();
        if pin.is_some()
            && !books.is_empty()
            && let Some(held) = primary.pins.get_mut(&slot)
        {
            held.books = Some(BookCoverage::Books(books));
        }
    }
    let helps = [VersionSlot::TranslationNotes, VersionSlot::TranslationWords];
    if helps.iter().any(|&slot| found_of(found, slot).is_some())
        && let Ok(owner) = import.gateway.1.parse()
    {
        primary.gateway = GatewayLanguage {
            language: import.gateway.0.clone(),
            owner,
        }
        .into();
    }
}

/// tC4 `carryOverDecisions` for `tool`'s decisions when its tC3 version was not found: each
/// book's decisions are placed on the check list of `source`, the installed version they are
/// stored against. One placed on another check moves there; one with no check on the list
/// keeps its record, marked invalid, so it never counts as done against a list it was not made
/// on. None is dropped.
fn carry_over(
    records: Vec<Record>,
    tool: ToolId,
    source: &ListSource,
    sources: &dyn PlanSources,
    counts: &mut CarryOver,
) -> Result<Vec<Record>, PlanError> {
    let mut out = Vec::with_capacity(records.len());
    let mut by_book: BTreeMap<BookId, Vec<StoredDecision>> = BTreeMap::new();
    for record in records {
        match record {
            Record::Check {
                key,
                value: CheckValue::Decision(decision),
            } if key.tool == tool
                && let Some(book) = key.book() =>
            {
                by_book
                    .entry(book)
                    .or_default()
                    .push(StoredDecision::of(key, decision));
            }
            other => out.push(other),
        }
    }
    let frame = StandardScheme::DEFAULT.scheme();
    let spec = UnitSpec { frame, scope: &[] };
    for (book, stored) in by_book {
        let list = sources.list(tool, Collection::Book(book), source, &spec)?;
        let plan = carry::plan(&list.items, &source.list, &stored);
        let written: BTreeMap<RecordKey, Record> =
            plans::carry_changes(&|_| Vec::new(), &plan, &list.items, PinGroup::Primary)
                .into_iter()
                .map(|change| (change.record.key(), change.record))
                .collect();
        for entry in plan.entries {
            let unplaced = entry.outcome == Outcome::Unplaced;
            if unplaced {
                counts.invalidated += 1;
            } else {
                counts.carried += 1;
            }
            if written.contains_key(&RecordKey::Check(entry.key.clone())) {
                continue;
            }
            let mut decision = entry.decision;
            decision.imported_invalid |= unplaced;
            out.push(Record::Check {
                key: entry.key,
                value: CheckValue::Decision(decision),
            });
        }
        out.extend(written.into_values());
    }
    Ok(out)
}

/// The books' records, their decisions on the versions `found` and, for a helps slot not
/// found, carried over to the installed version (tC4 `applyVersions`), with what that did.
async fn imported_records(
    app: &App,
    import: &Tc3Import,
    found: &[(VersionSlot, ResourcePin)],
) -> Result<(Vec<Record>, CarryOver), ApiError> {
    let installed = installed_pins().ok_or(ApiError::Unavailable(Feature::Import))?;
    let pins = DecisionPins {
        translation_notes: found_of(found, VersionSlot::TranslationNotes)
            .map_or(installed.translation_notes, ResourcePin::resource),
        translation_words: found_of(found, VersionSlot::TranslationWordsLinks)
            .map_or(installed.translation_words, ResourcePin::resource),
    };
    let imported = import
        .records(&pins)
        .map_err(|e| refused(import_refusal(e)))?;
    let carried = [
        (VersionSlot::TranslationNotes, ToolId::TranslationNotes),
        (VersionSlot::TranslationWords, ToolId::TranslationWords),
    ]
    .into_iter()
    .filter(|(slot, _)| found_of(found, *slot).is_none())
    .map(|(_, tool)| tool)
    .collect::<Vec<_>>();
    let mut counts = CarryOver {
        carried: 0,
        invalidated: 0,
    };
    if carried.is_empty() {
        return Ok((imported, counts));
    }
    let store = app.inner.resources.clone();
    let notes = pins.translation_notes.clone();
    let links = pins.translation_words.clone();
    let articles = tc_resources::suite::english_helps()
        .pin(SetSlot::TranslationWords)
        .map(ResourcePin::resource);
    let unreadable = |problem: String| {
        refused(ImportRefusal::Unreadable {
            file: String::new(),
            problem,
        })
    };
    tokio::task::spawn_blocking(move || {
        let sources = crate::upgrades::StoreSources::new(&store);
        carried
            .into_iter()
            .try_fold(imported, |records, tool| {
                let source = match tool {
                    ToolId::TranslationNotes => ListSource {
                        list: notes.clone(),
                        articles: None,
                    },
                    _ => ListSource {
                        list: links.clone(),
                        articles: articles.clone(),
                    },
                };
                carry_over(records, tool, &source, &sources, &mut counts)
            })
            .map(|records| (records, counts))
    })
    .await
    .map_err(|e| unreadable(e.to_string()))?
    .map_err(|PlanError::Unreadable(problem)| unreadable(problem))
}

impl ImportPort for TcImport {
    fn review(&self, files: Vec<SourceFile>) -> BoxFuture<Result<ImportFound, ApiError>> {
        Box::pin(async move { Ok(found(&read_tc3(files).await?)) })
    }

    fn carry_over(
        &self,
        app: App,
        files: Vec<SourceFile>,
        found: Vec<(VersionSlot, ResourcePin)>,
    ) -> BoxFuture<Result<CarryOver, ApiError>> {
        Box::pin(async move {
            let import = read_tc3(files).await?;
            Ok(imported_records(&app, &import, &found).await?.1)
        })
    }

    /// Each slot's candidates are tried newest first; a slot Door43 did not answer for stays
    /// unresolved, so an older version is never pinned in place of one that was not checked.
    fn versions(
        &self,
        app: App,
        files: Vec<SourceFile>,
    ) -> BoxFuture<Result<VersionsView, JobEnd>> {
        Box::pin(async move {
            let import = read_tc3(files).await?;
            let client = app.resources_client()?;
            let mut view = VersionsView {
                found: Vec::new(),
                unresolved: Vec::new(),
            };
            for (slot, tried) in candidates(&import.versions) {
                let mut hit = None;
                for (repo_path, tag) in tried.into_iter().flatten() {
                    let repo = repo_path
                        .strip_prefix("git.door43.org/")
                        .and_then(|path| path.parse::<tc_door43::RepoRef>().ok());
                    let (Some(repo), Ok(name)) = (repo, tag.parse::<tc_door43::TagName>()) else {
                        continue;
                    };
                    match client.tag_commit(&repo, &name).await {
                        Ok(sha) => {
                            hit = Some(FoundVersion {
                                slot: slot_of(slot),
                                repo_path,
                                tag,
                                sha: sha.to_hex(),
                            });
                            break;
                        }
                        Err(tc_door43::Door43Error::NotFound { .. }) => continue,
                        Err(_) => break,
                    }
                }
                match hit {
                    Some(found) => view.found.push(found),
                    None => view.unresolved.push(slot_of(slot)),
                }
            }
            // The words tool's two pins resolve together or not at all (D97 ruling 8).
            if let Some(at) = view
                .found
                .iter()
                .position(|f| f.slot == VersionSlot::TranslationWords)
            {
                match twl_for(&client, &view.found[at]).await {
                    Some(links) => view.found.push(links),
                    None => {
                        let words = view.found.remove(at);
                        view.unresolved.push(words.slot);
                    }
                }
            }
            Ok(view)
        })
    }

    /// INV-AN-IMPORT-NEVER-LEAVES-A-HALF-PROJECT: a cancel is honoured until the store's create
    /// starts; the create builds in a staging folder and renames it into place without replacing
    /// anything, so it ends with the whole project or none.
    fn run(
        &self,
        app: App,
        files: Vec<SourceFile>,
        details: ImportDetails,
        found: Vec<(VersionSlot, ResourcePin)>,
        license: String,
        folder: ProjectRef,
        ctx: JobCtx,
    ) -> BoxFuture<Result<ProjectRef, JobEnd>> {
        Box::pin(async move {
            ctx.progress.total(2);
            let import = read_tc3(files).await?;
            if !import.licenses.contains(&license) {
                return Err(ApiError::BadRequest {
                    field: "license".to_owned(),
                    reason: "not a license the files name".to_owned(),
                }
                .into());
            }
            if let Some(reason) = not_named(&import, &found) {
                return Err(ApiError::BadRequest {
                    field: "found".to_owned(),
                    reason,
                }
                .into());
            }
            ctx.progress.add(1);
            ctx.cancel.check()?;
            let mut project_pins = tc_resources::suite::project_pins();
            lay_over(&mut project_pins, &found, &import);
            let mut records = crate::create::project_records(
                &create_details(&details),
                &folder,
                ProjectKind::Bible,
                Some(StandardScheme::DEFAULT),
                app.inner.clock.now(),
                None,
            )
            .map_err(|_| refused(ImportRefusal::WriteFailed))?;
            for record in &mut records {
                if let Record::Pins { group, value } = record {
                    *value = project_pins
                        .group_value(*group)
                        .map_err(|_| refused(ImportRefusal::WriteFailed))?;
                }
            }
            if !license.is_empty() {
                let statement =
                    serde_json::json!({ "shortStatements": [{ "statement": license }] });
                records.push(Record::ProjectMeta {
                    field: MetaField::Copyright,
                    value: Some(
                        tc_journal::json::parse(statement.to_string().as_bytes())
                            .map_err(|_| refused(ImportRefusal::WriteFailed))?,
                    ),
                });
            }
            let (imported, _) = imported_records(&app, &import, &found).await?;
            records.extend(imported);
            let spec = NewProjectSpec {
                folder: folder.clone(),
                kind: ProjectKind::Bible,
                records,
            };
            ctx.cancel.check()?;
            let cause = Cause::Import(ImportSource::Tc3 { user: None });
            let created = app
                .with_store(move |store| store.create_imported(spec, cause))
                .await?;
            match created {
                Ok(()) => {}
                Err(CreateFailure::NameExists) => {
                    return Err(refused(ImportRefusal::NameExists).into());
                }
                Err(CreateFailure::NotHeld(reason)) => {
                    return Err(
                        ApiError::Refused(Refusal::Create(CreateRefusal::CodeNotHeld {
                            code: details.code.trim().to_owned(),
                            reason,
                        }))
                        .into(),
                    );
                }
                Err(CreateFailure::Failed(_)) => {
                    return Err(refused(ImportRefusal::WriteFailed).into());
                }
                Err(CreateFailure::Unavailable) => {
                    return Err(ApiError::Unavailable(Feature::Writes).into());
                }
            }
            ctx.progress.add(1);
            Ok(folder)
        })
    }

    /// The `Fill`, then the `Review` at once (§13.6 U5). Once the `Fill` is written the
    /// `Review` runs whatever is asked, and a failure between the two says to run the update
    /// again: a second run finds the filled verses already there and writes the review.
    fn update_book(
        &self,
        session: Arc<ProjectSession>,
        book: BookId,
        file: SourceFile,
        ctx: JobCtx,
    ) -> BoxFuture<Result<UpdateReport, JobEnd>> {
        Box::pin(async move {
            ctx.cancel.check()?;
            let markup_kept = tc_usfm::codec::decompose_bytes(&file.bytes)
                .map(|parts| markup_kept(&parts))
                .unwrap_or_default();
            let part =
                |part| StoreCommand::update_book(book, file.bytes.clone(), file.name.clone(), part);
            let handle = session.handle().clone();
            let fill = handle
                .submit(part(UpdatePart::Fill))
                .await
                .map_err(|error| match wire::error::command_error(error) {
                    Ok(api) => api,
                    Err(unworded) => {
                        refused(ImportRefusal::UpdateNotWritten { reason: unworded.0 })
                    }
                })?;
            let review =
                handle
                    .submit(part(UpdatePart::Review))
                    .await
                    .map_err(|error: CommandError| {
                        refused(ImportRefusal::UpdateReviewNotWritten {
                            reason: error.to_string(),
                        })
                    })?;
            let read = session.read();
            let key_of = |id| read.body(id).map(|body| body.record.key());
            let fill_keys: Vec<RecordKey> = fill.changes.iter().filter_map(key_of).collect();
            let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
            let verses = |keys: &mut dyn Iterator<Item = RecordKey>| {
                count(keys.filter(|k| matches!(k, RecordKey::Verse(..))).count())
            };
            Ok(UpdateReport {
                book,
                imported: verses(&mut fill_keys.iter().cloned()),
                // The verses this run put in review: a rerun's verse is already a head and
                // is not written again.
                to_review: verses(&mut review.changes.iter().filter_map(key_of)),
                left_as_it_was: fill.kept.iter().filter_map(verse_at).collect(),
                markup_kept,
                book_added: fill_keys
                    .iter()
                    .any(|k| matches!(k, RecordKey::BookPresence(_))),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::epoch_seconds;

    #[test]
    fn door43_dates_read_as_seconds_since_1970() {
        assert_eq!(epoch_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch_seconds("2000-02-29T00:00:00Z"), Some(951_782_400));
        assert_eq!(epoch_seconds("2026-08-03T15:05:55Z"), Some(1_785_769_555));
        assert_eq!(
            epoch_seconds("2026-08-03T17:05:55.123+02:00"),
            Some(1_785_769_555)
        );
        assert_eq!(epoch_seconds("2026-08-03"), None);
    }
}
