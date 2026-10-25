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
    FoundBook, FoundVersion, ImportDetails, ImportFile, ImportFound, ImportReview, Review, Run,
    UpdateBook, UpdateReport, VerseAt, VersionSlot, Versions, VersionsView,
};
use tc_api::jobs::{JobKind, Phase};
use tc_api::refusal::{CreateRefusal, ImportRefusal};
use tc_api::shown::Direction;
use tc_api::{ApiError, Refusal};
use tc_core::BookId;
use tc_import::SourceFile;
use tc_import::tc3::{DecisionPins, Slot, Tc3Import, VersionRequest};
use tc_journal::fold::ProjectKind;
use tc_journal::keys::ToolId;
use tc_journal::outside::UpdatePart;
use tc_journal::{Cause, ImportSource, RecordKey};
use tc_resources::SetSlot;
use tc_usfm::codec::{BookParts, VersePart};
use tc_versification::StandardScheme;

use crate::app::App;
use crate::backend::{BoxFuture, CommandError, CreateFailure, NewProjectSpec, StoreCommand};
use crate::dispatch::Handle;
use crate::flows::FlowRequest;
use crate::jobs::{JobCtx, JobEnd, JobSpec};
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
                    aligned: count(book.alignments().len()),
                    decisions: count(
                        book.decision_count(ToolId::TranslationNotes)
                            + book.decision_count(ToolId::TranslationWords),
                    ),
                    not_kept: book.not_kept.clone(),
                }
            })
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
                    .run(app.clone(), files, req.details, folder, ctx)
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
        let file = files.pop().ok_or_else(|| ApiError::BadRequest {
            field: "upload".to_owned(),
            reason: "no such upload".to_owned(),
        })?;
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

impl ImportPort for TcImport {
    fn review(&self, files: Vec<SourceFile>) -> BoxFuture<Result<ImportFound, ApiError>> {
        Box::pin(async move { Ok(found(&read_tc3(files).await?)) })
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
        folder: ProjectRef,
        ctx: JobCtx,
    ) -> BoxFuture<Result<ProjectRef, JobEnd>> {
        Box::pin(async move {
            ctx.progress.total(2);
            let import = read_tc3(files).await?;
            ctx.progress.add(1);
            ctx.cancel.check()?;
            let pins = installed_pins().ok_or(ApiError::Unavailable(Feature::Import))?;
            let mut records = crate::create::project_records(
                &create_details(&details),
                &folder,
                ProjectKind::Bible,
                Some(StandardScheme::DEFAULT),
                app.inner.clock.now(),
                None,
            )
            .map_err(|_| refused(ImportRefusal::WriteFailed))?;
            records.extend(
                import
                    .records(&pins)
                    .map_err(|e| refused(import_refusal(e)))?,
            );
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
            Ok(UpdateReport {
                book,
                imported: count(
                    fill_keys
                        .iter()
                        .filter(|k| matches!(k, RecordKey::Verse(..)))
                        .count(),
                ),
                to_review: count(review.changes.len()),
                left_as_it_was: fill.kept.iter().filter_map(verse_at).collect(),
                markup_kept,
                book_added: fill_keys
                    .iter()
                    .any(|k| matches!(k, RecordKey::BookPresence(_))),
            })
        })
    }
}
