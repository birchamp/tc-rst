//! Creating a project and adding books (unit C6; `tC4:src/state.jsx` `createBible`,
//! `createObs`, `addBooks`, `addUsfmBooks`).
//!
//! A new project is one `Store::create` (R2): its details, versification, settings, pins and,
//! for stories, the fifty template stories, written as one batch in a staging folder that is
//! renamed into place without replacing anything. Books are added through the registry's one
//! open of the folder (AD38), so a card clicked meanwhile shares the session; a blank book is
//! one `AddBook`, a book from a file is one `AddBookText`, whole or absent (tC4 #767).

use std::path::PathBuf;

use serde_json::{Value, json};
use tc_api::create::{
    AddBlankBooks, AddUsfmBooks, Bible, BooksAdded, Claims, Created, Details, Obs, ReviewUsfm,
    UsfmAdded, UsfmFile, UsfmOutcome, UsfmProblem, UsfmRow, UsfmVerdict,
};
use tc_api::ids::{ProjectRef, UploadId, WallMs};
use tc_api::jobs::{OpenPhase, Phase};
use tc_api::refusal::CreateRefusal;
use tc_api::shown::Direction;
use tc_api::{ApiError, Refusal};
use tc_core::BookId;
use tc_journal::fold::ProjectKind;
use tc_journal::keys::{MetaField, MetaPath, SettingKey};
use tc_journal::record::VersificationValue;
use tc_journal::{Json, Record, RecordKey};
use tc_resources::PaneBook;
use tc_versification::{BookCode, StandardScheme, Versification};

use crate::app::App;
use crate::backend::{CommandError, CreateFailure, NewProjectSpec, StoreCommand};
use crate::dispatch::Handle;
use crate::feedback::iso_utc_millis;
use crate::jobs::ProgressSink;
use crate::language_search::{LanguageIndex, language_name};
use crate::session::ProjectSession;
use crate::wire;

/// The folder inside app data that the server stores uploads in (`POST /api/v1/uploads`).
const UPLOADS: &str = "uploads";

fn json(value: Value) -> Result<Json, String> {
    tc_journal::json::parse(value.to_string().as_bytes()).map_err(|e| e.to_string())
}

fn direction_text(direction: Direction) -> &'static str {
    match direction {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    }
}

/// The first records of a new project (tC4 `createBible` and `createObs`): the name and the
/// short name in English, the language with its direction, when and in what locale it was
/// made, the versification of a Bible, the three settings the platform does not record, the
/// Door43 code beside the project's own when the dialog offered another (Q35), and the pins of
/// the bundled suite, so a project made here and never opened has its checking language.
pub fn project_records(
    details: &Details,
    folder: &ProjectRef,
    kind: ProjectKind,
    versification: Option<StandardScheme>,
    now: WallMs,
    listed_name: Option<&str>,
) -> Result<Vec<Record>, String> {
    let meta = |field, value| -> Result<Record, String> {
        Ok(Record::ProjectMeta {
            field,
            value: Some(json(value)?),
        })
    };
    let extra = |path: &str| -> Result<MetaField, String> {
        MetaPath::parse(path)
            .map(MetaField::Extra)
            .map_err(|e| e.to_string())
    };
    let name = details.name.trim();
    let code = details.code.trim();
    // The name the person typed; else the shipped Door43 list's name for the code (tC4's
    // platform named a regional code from its own table); else the code.
    let language = match details.language_name.trim() {
        "" => listed_name.unwrap_or(code),
        named => named,
    };
    let mut records = vec![
        meta(MetaField::Name, json!({ "en": name }))?,
        meta(MetaField::Abbreviation, json!({ "en": folder.as_str() }))?,
        meta(
            MetaField::Languages,
            json!([{
                "name": { "en": language },
                "scriptDirection": direction_text(details.direction),
                "tag": code,
            }]),
        )?,
        meta(extra("meta.dateCreated")?, json!(iso_utc_millis(now)))?,
        meta(extra("meta.defaultLocale")?, json!("en"))?,
    ];
    if let Some(scheme) = versification {
        records.push(Record::Versification {
            value: VersificationValue {
                name: Some(scheme.name().into()),
                bytes: scheme.json().to_owned(),
            },
        });
    }
    let mut setting = |key: &str, value: Value| -> Result<(), String> {
        records.push(Record::Setting {
            key: SettingKey::parse(key).map_err(|e| e.to_string())?,
            value: Some(json(value)?),
        });
        Ok(())
    };
    setting("textDirection", json!(direction_text(details.direction)))?;
    setting("textFont", json!(details.font))?;
    if !details.language_name.trim().is_empty() {
        setting("languageName", json!(details.language_name.trim()))?;
    }
    if let Some(door43) = &details.door43_code {
        setting(
            "door43Language",
            json!({ "code": door43, "name": language }),
        )?;
    }
    for (group, value) in crate::backend::store::creation_pins() {
        records.push(Record::Pins { group, value });
    }
    if kind == ProjectKind::Stories {
        for story in tc_obs::ids::StoryNo::all() {
            let base = tc_obs::story::seed_story(tc_obs::template::template_story(story))
                .map_err(|e| e.to_string())?;
            let story = tc_journal::StoryNo::new(story.get())
                .ok_or_else(|| format!("story {} is not a story number", story.get()))?;
            records.push(Record::StoryBase { story, value: base });
        }
    }
    Ok(records)
}

/// What a request for a project's books names, apart from the books.
fn quiet() -> ProgressSink {
    ProgressSink::new(Phase::Open(OpenPhase::ReadingChanges))
}

fn create_failed(reason: impl Into<String>) -> ApiError {
    ApiError::Refused(Refusal::Create(CreateRefusal::Failed {
        reason: reason.into(),
    }))
}

/// A command the store refused or could not write, in the words of the add-a-book dialog.
fn command_failed(error: CommandError) -> ApiError {
    match wire::error::command_error(error) {
        Ok(api) => api,
        Err(unworded) => create_failed(unworded.0),
    }
}

impl App {
    /// Whether anything in the projects folder has `folder`'s name, without letter case.
    pub(crate) async fn name_taken(&self, folder: &ProjectRef) -> Result<bool, ApiError> {
        let folder = folder.clone();
        self.with_store(move |store| {
            let listing = store.list()?;
            let same = |name: &str| name.eq_ignore_ascii_case(folder.as_str());
            Ok::<_, crate::backend::StoreError>(
                listing.projects.iter().any(|p| same(p.folder.as_str()))
                    || listing.unusable.iter().any(|u| same(&u.name))
                    || listing.others.iter().any(|name| same(name)),
            )
        })
        .await?
        .map_err(|e| create_failed(e.to_string()))
    }

    /// Creates the project `details` describe (tC4 `createBible` and `createObs`, after the
    /// form's checks): the folder its name makes must be free, then one `Store::create`.
    async fn create_project(
        &self,
        details: Details,
        kind: ProjectKind,
        versification: Option<StandardScheme>,
    ) -> Result<Created, ApiError> {
        let folder = details
            .folder()
            .map_err(|refusal| ApiError::Refused(Refusal::Create(refusal)))?;
        let name_in_use = || {
            ApiError::Refused(Refusal::Create(CreateRefusal::NameInUse {
                abbr: folder.to_string(),
            }))
        };
        if self.name_taken(&folder).await? {
            return Err(name_in_use());
        }
        let code = details.code.trim();
        let listed_name = LanguageIndex::shared().ok().and_then(|index| {
            index
                .rows()
                .find(|row| row.lc.eq_ignore_ascii_case(code))
                .map(|row| language_name(row).to_owned())
        });
        let records = project_records(
            &details,
            &folder,
            kind,
            versification,
            self.inner.clock.now(),
            listed_name.as_deref(),
        )
        .map_err(create_failed)?;
        let spec = NewProjectSpec {
            folder: folder.clone(),
            kind,
            records,
        };
        let created = self.with_store(move |store| store.create(spec)).await?;
        match created {
            Ok(()) => {}
            Err(CreateFailure::NameExists) => return Err(name_in_use()),
            Err(CreateFailure::NotHeld(reason)) => {
                return Err(ApiError::Refused(Refusal::Create(
                    CreateRefusal::CodeNotHeld {
                        code: details.code.trim().to_owned(),
                        reason,
                    },
                )));
            }
            Err(CreateFailure::Failed(reason)) => return Err(create_failed(reason)),
            Err(CreateFailure::Unavailable) => {
                return Err(ApiError::Unavailable(tc_api::error::Feature::Writes));
            }
        }
        // Creation counts as use (tC4 `markUsed`): the new card comes first on Home.
        self.inner
            .prefs
            .record_last_used(folder.clone(), self.inner.clock.now());
        self.inner
            .bus
            .publish(tc_api::event::Event::ProjectsChanged);
        Ok(Created { project: folder })
    }

    /// The book's file text for a blank book: the project's literal Bible pane, else the
    /// chapters and verses of its versification (tC4 `seedInitialUsfm`).
    fn blank_source(&self, session: &ProjectSession, book: BookId) -> Result<String, ApiError> {
        let read = session.read();
        let pins = crate::understand::project_pins(&read)
            .map_err(create_failed)?
            .unwrap_or_default();
        match tc_resources::literal_pane_book(&self.inner.resources, &pins, book) {
            PaneBook::Text(usfm) => Ok(usfm),
            PaneBook::Unreadable(reason) => Err(create_failed(format!(
                "the source text of {} cannot be read: {reason}",
                book.english_name()
            ))),
            PaneBook::Absent => {
                let versification = match read.projected(&RecordKey::Versification) {
                    Some(Record::Versification { value }) => {
                        Versification::from_json(&value.bytes).ok()
                    }
                    _ => None,
                };
                let versification = versification
                    .unwrap_or_else(|| StandardScheme::DEFAULT.scheme().versification().clone());
                Ok(tc_usfm::seed::skeleton(
                    book,
                    BookCode::new(book.code())
                        .ok()
                        .and_then(|code| versification.verse_counts(code))
                        .unwrap_or_default(),
                ))
            }
        }
    }

    /// The project's session for adding books: the screen's, or a side one that closes when
    /// the request ends.
    async fn books_session(
        &self,
        project: &ProjectRef,
    ) -> Result<crate::session::SideLease, ApiError> {
        self.inner
            .sessions
            .open_side(project, quiet())
            .await
            .map_err(wire::error::open_failure)
    }

    /// The side session ends and the card reads its project again.
    async fn books_added(&self, lease: crate::session::SideLease, project: &ProjectRef) {
        drop(lease);
        self.inner.sessions.settled(project).await;
        self.inner.bus.publish(tc_api::event::Event::HomeCard {
            project: project.clone(),
        });
    }

    pub(crate) fn upload_path(&self, id: &UploadId) -> PathBuf {
        self.inner.config.app_data.join(UPLOADS).join(id.as_str())
    }

    /// What an uploaded file is, and its text when it is a book. The file is read and parsed on
    /// the blocking pool: a book of several megabytes is not a job for the async threads.
    async fn upload_verdict(
        &self,
        file: &UsfmFile,
    ) -> Result<(UsfmVerdict, Option<String>), ApiError> {
        let path = self.upload_path(&file.upload);
        let read = tokio::task::spawn_blocking(move || std::fs::read(path).map(classify_usfm))
            .await
            .map_err(|e| create_failed(e.to_string()))?;
        read.map_err(|_| ApiError::BadRequest {
            field: "upload".to_owned(),
            reason: "no such upload".to_owned(),
        })
    }

    /// Takes the upload from the server's registry before a job reads it over time, so the
    /// sweep of abandoned uploads never deletes it mid-read; the job then owns the file and
    /// deletes it with [`App::drop_upload`]. Without a server (tests) there is nothing to
    /// take.
    pub(crate) fn take_upload(&self, upload: &UploadId) {
        if let Some(uploads) = self.inner.uploads.get() {
            let _ = uploads.take(upload);
        }
    }

    pub(crate) fn drop_upload(&self, upload: &UploadId) {
        // A file that is already gone, or cannot be removed now, is emptied at the next start.
        let _ = std::fs::remove_file(self.upload_path(upload));
    }
}

/// What an uploaded file is (tC4 `classifyUsfmFiles`' damage checks). The text comes back with
/// the book, so the caller reads the file once.
pub fn classify_usfm(bytes: Vec<u8>) -> (UsfmVerdict, Option<String>) {
    let refused = |problem| (UsfmVerdict::Refused(problem), None);
    let Ok(text) = String::from_utf8(bytes) else {
        return refused(UsfmProblem::NotText);
    };
    let parts = match tc_usfm::codec::decompose(&text) {
        Ok(parts) => parts,
        Err(error) => {
            return refused(UsfmProblem::Damaged {
                reason: error.to_string(),
            });
        }
    };
    let Some(book) = tc_usfm::seed::id_book(&parts.header) else {
        return refused(UsfmProblem::NoId);
    };
    if parts.chapters.is_empty() {
        return refused(UsfmProblem::Damaged {
            reason: "the file has no chapters or verses".to_owned(),
        });
    }
    (UsfmVerdict::Book { book }, Some(text))
}

impl Handle<Bible> for App {
    async fn handle(&self, req: Bible) -> Result<Created, ApiError> {
        let scheme =
            StandardScheme::from_name(&req.versification).ok_or_else(|| ApiError::BadRequest {
                field: "versification".to_owned(),
                reason: "not a standard versification".to_owned(),
            })?;
        self.create_project(req.details, ProjectKind::Bible, Some(scheme))
            .await
    }
}

impl Handle<Obs> for App {
    async fn handle(&self, req: Obs) -> Result<Created, ApiError> {
        self.create_project(req.details, ProjectKind::Stories, None)
            .await
    }
}

impl Handle<AddBlankBooks> for App {
    /// Each book the project does not hold is one `AddBook`, in the order asked. One that
    /// fails stops the request; the books before it stay added.
    async fn handle(&self, req: AddBlankBooks) -> Result<BooksAdded, ApiError> {
        let lease = self.books_session(&req.project).await?;
        let session = lease.session().clone();
        let mut added = Vec::new();
        let mut outcome = Ok(());
        for book in req.books {
            if added.contains(&book)
                || !session
                    .read()
                    .heads(&RecordKey::BookPresence(book))
                    .is_empty()
            {
                continue;
            }
            let step = async {
                let source = self.blank_source(&session, book)?;
                session
                    .handle()
                    .submit(StoreCommand::add_book(book, source).requested_by(req.request))
                    .await
                    .map_err(command_failed)
            };
            match step.await {
                Ok(_) => added.push(book),
                Err(error) => {
                    outcome = Err(error);
                    break;
                }
            }
        }
        self.books_added(lease, &req.project).await;
        outcome.map(|()| BooksAdded { added })
    }
}

/// The UI's dialog has no more use for an upload: it is deleted now, unless a job took it.
impl Handle<tc_api::upload::DropUpload> for App {
    async fn handle(&self, req: tc_api::upload::DropUpload) -> Result<(), ApiError> {
        let Some(uploads) = self.inner.uploads.get().cloned() else {
            return Ok(());
        };
        // One unlink of a file that may be 4 GiB: off the async threads.
        let _ = tokio::task::spawn_blocking(move || uploads.drop_stored(&req.upload)).await;
        Ok(())
    }
}

impl Handle<ReviewUsfm> for App {
    async fn handle(&self, req: ReviewUsfm) -> Result<UsfmVerdict, ApiError> {
        Ok(self.upload_verdict(&req.file).await?.0)
    }
}

impl Handle<AddUsfmBooks> for App {
    /// Each file is its own row. A file that cannot be added is refused in its row and the
    /// rest still add; a file the store did not write leaves no trace of its book (#767).
    async fn handle(&self, req: AddUsfmBooks) -> Result<UsfmAdded, ApiError> {
        let lease = self.books_session(&req.project).await?;
        let session = lease.session().clone();
        let mut claims = Claims::default();
        let mut rows = Vec::new();
        for file in &req.files {
            let (verdict, text) = match self.upload_verdict(file).await {
                Ok(read) => read,
                Err(error) => {
                    self.books_added(lease, &req.project).await;
                    return Err(error);
                }
            };
            let outcome = match verdict {
                UsfmVerdict::Refused(problem) => UsfmOutcome::Refused(problem),
                UsfmVerdict::Book { book } => {
                    let held = !session
                        .read()
                        .heads(&RecordKey::BookPresence(book))
                        .is_empty();
                    match claims.claim(&file.name, book, held) {
                        Err(problem) => UsfmOutcome::Refused(problem),
                        Ok(book) => {
                            let text = text.unwrap_or_default();
                            let command =
                                StoreCommand::add_book_text(book, text, file.name.clone())
                                    .requested_by(req.request);
                            match session.handle().submit(command).await {
                                Ok(_) => UsfmOutcome::Added { book },
                                Err(error) => UsfmOutcome::NotAdded {
                                    reason: error.to_string(),
                                },
                            }
                        }
                    }
                }
            };
            self.drop_upload(&file.upload);
            rows.push(UsfmRow {
                name: file.name.clone(),
                outcome,
            });
        }
        self.books_added(lease, &req.project).await;
        Ok(UsfmAdded { rows })
    }
}
