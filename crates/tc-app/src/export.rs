//! The `export` service's reads (unit B8): what the Community Checking preview prints of the
//! open book or the open story project, by tc-export's and tc-obs' print rules (A10). A book's
//! print reads the verse records Translate's rows are made from (`translate::texts`), never a
//! second reading of the USFM, so the preview states each verse as the editor holds it; each
//! verse's text is tc-usfm's `shown_text_v1`, the port of tC4's verse text (#782: the words
//! inside nested alignments too). Nothing here writes the project.
//!
//! D2's exports: `export/preflight` counts the open conflicts of what an export reads, and
//! `export/run` checkpoints, makes the file with `tc_export::produce` and registers it with the
//! server's downloads. An export writes only into the downloads folder, never the project.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tc_api::ApiError;
use tc_api::event::CheckpointOutcome;
use tc_api::export::{
    Book, BookPrint, DraftedFrame, ExportFacts, Exported, ExportedFile, Preflight, PreflightView,
    PrintFrame, PrintItem, PrintRequest, PrintStory, PrintStoryItem, PrintWords, Producer, Run,
    RunExport, SkipReason, SkippedVerse, Stories, StoriesPrint, StoryRun, Subject,
};
use tc_api::ids::{DownloadId, JobId, RefPart, StoryNo};
use tc_api::jobs::{JobKind, JobOutcome, JobState, Phase};
use tc_api::model::print::Pictures;
use tc_api::refusal::{ExportRefusal, Refusal};
use tc_export::book::{BookChapter, BookVerse, has_undrafted, print_items, printed_items};
use tc_export::{ExportError, ExportInput, PrintBookInput, PrintStoriesInput, PrintText, Produced};
use tc_helps::obs::ObsStory;
use tc_journal::RecordKey;
use tc_journal::fold::DerivedFile;
use tc_obs::print::{PrintedFrame, PrintedStory};

use crate::app::App;
use crate::backend::CheckpointReason;
use crate::dispatch::Handle;
use crate::jobs::{JobEnd, JobSpec};
use crate::reports::{ReportOp, ReportOutcome};
use crate::session::ProjectSession;
use crate::story::{MediaFile, journal_story, read_story, story_numbers};

/// A verse's label as the book writes it: `4`, or `4-5` for a span.
fn label(verse: u16, last: u16) -> String {
    if last > verse {
        format!("{verse}-{last}")
    } else {
        verse.to_string()
    }
}

/// The book as the editor holds it, chapter by chapter: what a print of it reads.
fn book_chapters(
    read: &crate::counters::CountingSnapshot,
    book: tc_core::BookId,
) -> Result<Vec<BookChapter>, ApiError> {
    let numbers = read.chapters_of(book);
    if numbers.is_empty() {
        return Err(ApiError::BadRequest {
            field: "book".to_owned(),
            reason: format!("{} is not in the project", book.code()),
        });
    }
    Ok(numbers
        .into_iter()
        .map(|number| BookChapter {
            number,
            verses: crate::translate::texts(read, book, number)
                .into_iter()
                .map(|text| {
                    let drafted = crate::drafted::words_drafted(&text.words);
                    BookVerse {
                        label: label(text.verse, text.last),
                        drafted,
                        text: if drafted {
                            tc_usfm::shown_text_v1(&tc_usfm::codec::join_verse(
                                &text.words,
                                &text.layout,
                            ))
                        } else {
                            String::new()
                        },
                    }
                })
                .collect(),
        })
        .collect())
}

impl App {
    /// A book's printed items, as tc-export's print rules state them.
    fn book_items(&self, chapters: &[BookChapter]) -> Result<Vec<PrintItem>, ApiError> {
        print_items(&printed_items(chapters)).map_err(|e| {
            self.internal(
                "a verse's number could not be written",
                crate::reports::facts(
                    serde_json::json!({"verse": e.label, "reason": e.source.to_string()}),
                ),
            )
        })
    }
}

impl Handle<Book> for App {
    async fn handle(&self, req: Book) -> Result<BookPrint, ApiError> {
        let session = self.read(&req.at)?;
        let chapters = book_chapters(&session.read(), req.book)?;
        let items = self.book_items(&chapters)?;
        Ok(BookPrint {
            items,
            undrafted: has_undrafted(&chapters),
        })
    }
}

/// A story number tc-obs printed, as the wire's: every story the project holds is 1 to 50.
fn story_no(app: &App, number: u8) -> Result<StoryNo, ApiError> {
    StoryNo::new(number).map_err(|e| {
        app.internal(
            "a story's number could not be written",
            crate::reports::facts(serde_json::json!({"story": number, "reason": e.to_string()})),
        )
    })
}

/// A frame number, 1 and up, as the wire's.
fn frame_no(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// The project's stories in story order, or `BadRequest` naming `field` when it holds none.
fn project_stories(
    read: &crate::counters::CountingSnapshot,
    field: &str,
) -> Result<Vec<ObsStory>, ApiError> {
    let mut stories: Vec<ObsStory> = Vec::new();
    for number in story_numbers() {
        let Ok(story) = StoryNo::new(number) else {
            continue;
        };
        if read
            .key_state(&RecordKey::StoryBase(journal_story(story)?))
            .is_none()
        {
            continue;
        }
        stories.push(read_story(read, story)?);
    }
    if stories.is_empty() {
        return Err(ApiError::BadRequest {
            field: field.to_owned(),
            reason: "the project holds no stories".to_owned(),
        });
    }
    Ok(stories)
}

impl Handle<Stories> for App {
    async fn handle(&self, req: Stories) -> Result<StoriesPrint, ApiError> {
        let session = self.read(&req.at)?;
        let stories = project_stories(&session.read(), "at")?;
        let loaded = self.loaded_obs(&session).await?;
        let mut pictures = |story: &ObsStory| {
            loaded
                .frame_pictures(story)
                .into_iter()
                .enumerate()
                .filter_map(|(at, uri)| Some((at + 1, uri?)))
                .collect()
        };
        let printed = tc_obs::print::printed(&stories, Some(&mut pictures));
        let mut items = Vec::new();
        for item in &printed.items {
            items.push(match item {
                PrintedStory::Gap(from, to) => PrintStoryItem::Gap(StoryRun {
                    from: story_no(self, *from)?,
                    to: story_no(self, *to)?,
                }),
                PrintedStory::Story { story, frames } => {
                    let sources = printed.pictures.get(&story.number);
                    PrintStoryItem::Story(PrintStory {
                        story: story_no(self, story.number)?,
                        title: story.title.clone(),
                        frames: frames
                            .iter()
                            .map(|frame| match frame {
                                PrintedFrame::Gap(from, to) => PrintFrame::Gap(Run {
                                    from: frame_no(*from),
                                    to: frame_no(*to),
                                }),
                                PrintedFrame::Drafted { n, text } => {
                                    PrintFrame::Drafted(DraftedFrame {
                                        frame: frame_no(*n),
                                        text: text.clone(),
                                        picture: sources.and_then(|s| s.get(n)).cloned(),
                                    })
                                }
                            })
                            .collect(),
                        reference: story.reference.clone().filter(|r| !r.is_empty()),
                    })
                }
            });
        }
        Ok(StoriesPrint {
            items,
            undrafted: printed.undrafted,
        })
    }
}

/// Whether `key` is a record of `subject`: a book's records, or the stories' text.
fn of_subject(subject: Subject, key: &RecordKey) -> bool {
    match subject {
        Subject::Book { book } => key.book() == Some(book),
        Subject::Stories => matches!(
            key,
            RecordKey::StoryBase(_) | RecordKey::Frame(..) | RecordKey::StoryRef(_)
        ),
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl Handle<Preflight> for App {
    async fn handle(&self, req: Preflight) -> Result<PreflightView, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
        // The fold keeps counts by book, so a subject with no conflict walks no list; the
        // stories' records count with the project's (no book).
        let book = match req.subject {
            Subject::Book { book } => Some(book),
            Subject::Stories => None,
        };
        if !read
            .conflict_counts()
            .iter()
            .any(|(&(b, _), &n)| b == book && n > 0)
        {
            return Ok(PreflightView {
                conflicts: 0,
                verses: 0,
            });
        }
        let mut conflicts = 0;
        let mut verses = BTreeSet::new();
        for conflict in read.conflicts() {
            let mut ours = std::iter::once(&conflict.key)
                .chain(&conflict.also_keys)
                .filter(|key| of_subject(req.subject, key))
                .peekable();
            if ours.peek().is_none() {
                continue;
            }
            conflicts += 1;
            verses.extend(ours.filter(|key| {
                matches!(
                    key,
                    RecordKey::Verse(..)
                        | RecordKey::StoryBase(_)
                        | RecordKey::Frame(..)
                        | RecordKey::StoryRef(_)
                )
            }));
        }
        Ok(PreflightView {
            conflicts,
            verses: count(verses.len()),
        })
    }
}

/// The server's finished files, as the app registers an export's file there. The registry is
/// the server's (`tc_server::Downloads`); the program joins the two.
pub trait DownloadFiles: Send + Sync {
    /// Where a file is written before [`DownloadFiles::add`], so registering it is a rename.
    fn dir(&self) -> PathBuf;
    /// Registers `written`, which lives in [`DownloadFiles::dir`], as `file_name`.
    fn add(
        &self,
        written: &Path,
        file_name: &str,
        content_type: &'static str,
    ) -> Result<DownloadId, String>;
}

/// What the app keeps of the server's downloads: nothing until the server gives them.
pub type Downloads = Arc<dyn DownloadFiles>;

fn failed(why: ExportRefusal) -> JobEnd {
    JobEnd::Failed(ApiError::Refused(Refusal::Export(why)))
}

/// tC4's `exportFilename`: `<subject>-<YYYY-MM-DD>.<ext>`.
// shortcut: the date is UTC where tC4 writes the local date; they differ only near midnight,
// and the app has no time zone to read until it carries one.
fn file_name(subject: &str, today: tc_api::ids::WallMs, ext: &str) -> String {
    let date = crate::feedback::iso_utc_millis(today);
    format!("{subject}-{}.{ext}", &date[..10])
}

/// A print's `<title>`: its file name without `.pdf`, since the shell names the PDF it saves
/// after the page's title (G1's `printPdf`).
fn document_title(subject: &str, today: tc_api::ids::WallMs) -> String {
    file_name(subject, today, "pdf")
        .trim_end_matches(".pdf")
        .to_owned()
}

/// tc-export's facts, as the wire's.
fn wire_facts(facts: &tc_export::ExportFacts) -> ExportFacts {
    ExportFacts {
        skipped: facts
            .skipped
            .iter()
            .filter_map(|skipped| {
                Some(SkippedVerse {
                    chapter: skipped.reference.chapter,
                    // A span is `4` or `4-5`: never empty, never a `:`.
                    verse: RefPart::parse(&skipped.reference.verses.to_string()).ok()?,
                    why: match skipped.why {
                        tc_export::SkipReason::NeedsReview => SkipReason::NeedsReview,
                        tc_export::SkipReason::InlineAlignment => SkipReason::InlineAlignment,
                        tc_export::SkipReason::WeaveRefused => SkipReason::WeaveRefused,
                        tc_export::SkipReason::MarkupKept => SkipReason::MarkupKept,
                    },
                })
            })
            .collect(),
        skipped_total: facts.skipped_total,
    }
}

/// The stories' `content/` files as the project holds them, by their path from the project
/// folder: the fifty story files the project derives, and the front and back matter it
/// carries. A carried file whose bytes this computer lacks fails the export: leaving it out
/// would drop the translator's text from it.
async fn story_files(session: &ProjectSession) -> Result<Vec<(String, Vec<u8>)>, Unread> {
    let (mut files, carried) = {
        let read = session.read();
        let mut files = Vec::new();
        for number in story_numbers() {
            let Ok(story) = StoryNo::new(number) else {
                continue;
            };
            let Some(number) = tc_journal::StoryNo::new(story.get()) else {
                continue;
            };
            let file = DerivedFile::Story(number);
            if let Some(bytes) = read.render(&file) {
                files.push((file.path(), bytes));
            }
        }
        (files, read.carried_under(CONTENT))
    };
    for (path, oid) in carried {
        match session.handle().carried_bytes(oid).await {
            Ok(Some(bytes)) => files.push((path.as_str().to_owned(), bytes)),
            Ok(None) => return Err(Unread(format!("{} has no bytes here", path.as_str()))),
            Err(error) => return Err(Unread(format!("{}: {error}", path.as_str()))),
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// A file of the project an export could not read, and why.
struct Unread(String);

/// The project's folder of story text (tC4 `obsMarkdown.ts`).
const CONTENT: &str = "ingredients/content/";

fn zip_files(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (path, bytes) in files {
        writer
            .start_file(path.as_str(), options)
            .map_err(|e| e.to_string())?;
        writer.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(writer.finish().map_err(|e| e.to_string())?.into_inner())
}

/// tC4's print stylesheet. One copy: the preview loads this file from the UI's assets, and
/// the print document embeds the same file, so both set lines alike (INV-ONE-PRINT-MODEL).
const PRINT_CSS: &str = include_str!("../../tc-ui/assets/ds/tokens/print.css");

/// A print line worded from the UI's catalog messages, filled as the UI's `t` fills them. A
/// key the request has no message for is the key itself, as `t` gives a missing key.
fn print_word(words: &PrintWords, text: PrintText) -> String {
    let key = text.key();
    let values = text.values();
    let vars: Vec<(&str, &dyn std::fmt::Display)> = values
        .iter()
        .map(|(name, value)| (*name, value as &dyn std::fmt::Display))
        .collect();
    tc_api::model::fill(words.message(key).unwrap_or(key), &vars)
}

/// A picture under `/media/obs/` as a `data:` URL: the print document stands alone, so the
/// window printing it needs no address of the app's (tC4 `inlinePicture`).
async fn inline_picture(app: &App, uri: &str) -> Option<String> {
    use base64::Engine as _;
    let (pack, path) = uri.strip_prefix("/media/obs/")?.split_once('/')?;
    let file = app.obs_picture(pack, path).await?;
    Some(format!(
        "data:{};base64,{}",
        file.content_type,
        base64::engine::general_purpose::STANDARD.encode(&file.bytes)
    ))
}

/// What an export's job made, before it is handed over.
struct Made {
    bytes: Vec<u8>,
    file_name: String,
    facts: tc_export::ExportFacts,
}

impl App {
    /// Reads the subject and makes the file. Reads only.
    async fn make_export(
        &self,
        session: &Arc<ProjectSession>,
        producer: Producer,
        subject: Subject,
        print: Option<PrintRequest>,
    ) -> Result<Made, JobEnd> {
        let today = self.inner.clock.now();
        let made = |produced: Result<(Produced, tc_export::ExportFacts), ExportError>,
                    name: String,
                    ext: &str|
         -> Result<Made, JobEnd> {
            let (produced, facts) = produced.map_err(|e| match e {
                ExportError::NothingDrafted => failed(ExportRefusal::NothingDrafted),
                other => {
                    self.report_export_failure(other.to_string());
                    failed(ExportRefusal::ReadFailed)
                }
            })?;
            let bytes = match produced {
                Produced::File(bytes) => bytes,
                Produced::Files(files) => zip_files(&files).map_err(|reason| {
                    self.report_export_failure(reason);
                    failed(ExportRefusal::ReadFailed)
                })?,
            };
            Ok(Made {
                bytes,
                file_name: file_name(&name, today, ext),
                facts,
            })
        };
        match (producer, subject, print) {
            (Producer::AlignedUsfm | Producer::PlainUsfm, Subject::Book { book }, None) => {
                let input = crate::align::export::book_input(self, session, book).await?;
                let id = book.code();
                let (producer, name) = if producer == Producer::AlignedUsfm {
                    (tc_export::Producer::AlignedUsfm, format!("{id}-aligned"))
                } else {
                    (tc_export::Producer::PlainUsfm, id.to_owned())
                };
                made(
                    tc_export::produce(producer, &ExportInput::Book(input)),
                    name,
                    "usfm",
                )
            }
            (Producer::StoryMarkdown, Subject::Stories, None) => {
                let project_name = self.export_name(session).await;
                let files = story_files(session).await.map_err(|Unread(reason)| {
                    self.report_export_failure(reason);
                    failed(ExportRefusal::ReadFailed)
                })?;
                if files.is_empty() {
                    return Err(JobEnd::Failed(ApiError::BadRequest {
                        field: "subject".to_owned(),
                        reason: "the project holds no stories".to_owned(),
                    }));
                }
                made(
                    tc_export::produce(
                        tc_export::Producer::StoryMarkdown,
                        &ExportInput::StoryFiles(files),
                    ),
                    format!("{project_name}-stories"),
                    "zip",
                )
            }
            (Producer::PrintHtml, Subject::Book { book }, Some(print)) => {
                let (chapters, dir) = {
                    let read = session.read();
                    (
                        book_chapters(&read, book)?,
                        crate::align::export::print_dir(&read),
                    )
                };
                let words = |text| print_word(&print.words, text);
                let name = book.code().to_owned();
                let input = PrintBookInput {
                    title: print.title.clone(),
                    document_title: document_title(&name, today),
                    items: self.book_items(&chapters)?,
                    setup: print.setup,
                    dir,
                    stylesheet: PRINT_CSS,
                    words: &words,
                };
                made(
                    tc_export::produce(
                        tc_export::Producer::PrintHtml,
                        &ExportInput::PrintBook(input),
                    ),
                    name,
                    "pdf",
                )
            }
            (Producer::PrintHtml, Subject::Stories, Some(print)) => {
                let project_name = self.export_name(session).await;
                let (stories, dir) = {
                    let read = session.read();
                    (
                        project_stories(&read, "subject")?,
                        crate::align::export::print_dir(&read),
                    )
                };
                let shown = print.setup.pictures == Pictures::Shown;
                let sources = if shown {
                    let loaded = self.loaded_obs(session).await?;
                    let mut sources = |story: &ObsStory| {
                        loaded
                            .frame_pictures(story)
                            .into_iter()
                            .enumerate()
                            .filter_map(|(at, uri)| Some((at + 1, uri?)))
                            .collect()
                    };
                    tc_obs::print::printed(&stories, Some(&mut sources)).pictures
                } else {
                    BTreeMap::new()
                };
                let mut pictures = BTreeMap::new();
                for (story, frames) in sources {
                    let mut inlined = BTreeMap::new();
                    for (frame, uri) in frames {
                        // tC4 fails the export on a picture it cannot read.
                        let Some(data) = inline_picture(self, &uri).await else {
                            self.report_export_failure(format!("picture {uri} could not be read"));
                            return Err(failed(ExportRefusal::ReadFailed));
                        };
                        inlined.insert(frame, data);
                    }
                    pictures.insert(story, inlined);
                }
                let printed = tc_obs::print::printed(&stories, None);
                let words = |text| print_word(&print.words, text);
                let input = PrintStoriesInput {
                    title: print.title.clone(),
                    document_title: document_title(&project_name, today),
                    items: &printed.items,
                    pictures,
                    setup: print.setup,
                    dir,
                    stylesheet: PRINT_CSS,
                    words: &words,
                };
                made(
                    tc_export::produce(
                        tc_export::Producer::PrintHtml,
                        &ExportInput::PrintStories(input),
                    ),
                    project_name,
                    "pdf",
                )
            }
            (producer, _, _) => Err(JobEnd::Failed(ApiError::BadRequest {
                field: "subject".to_owned(),
                reason: format!("{producer:?} does not export this subject"),
            })),
        }
    }

    /// tC4's `project.name`, which names a stories export: the name Home shows for the
    /// project (`card_facts`), else its folder.
    async fn export_name(&self, session: &Arc<ProjectSession>) -> String {
        let folder = session.scope().project.clone();
        let kind = crate::home::api_kind(session.handle().facts().kind);
        let store = self.inner.sessions.store().clone();
        let peeked = folder.clone();
        let metadata = tokio::task::spawn_blocking(move || store.peek(&peeked))
            .await
            .ok()
            .and_then(Result::ok)
            .map(|p| p.metadata)
            .unwrap_or_else(|| Err("the project's metadata could not be read".to_owned()));
        crate::home::card_facts(&metadata, &folder, kind).name
    }

    fn report_export_failure(&self, reason: String) {
        let now = self.inner.clock.now();
        self.inner.reports.add(
            ReportOp::Export,
            ReportOutcome::Failed { reason },
            crate::reports::facts(serde_json::json!({})),
            now,
            now,
        );
    }

    /// `GET /print/{job}`: the print document of `job` once it finished as a print export;
    /// `None` for an unknown or unfinished job, a failed one, or an export of another kind.
    pub async fn print_page(&self, job: &str) -> Option<MediaFile> {
        let n: u64 = job.parse().ok()?;
        // One job, one address: `07` is not job 7.
        if n.to_string() != job {
            return None;
        }
        let id = JobId::new(n);
        let JobState::Done(JobOutcome::ExportReady(Exported {
            file: ExportedFile::Print { .. },
            ..
        })) = self.inner.jobs.view(id)?.state
        else {
            return None;
        };
        let bytes = self.inner.prints.get(id)?;
        Some(MediaFile {
            bytes: bytes.as_ref().clone(),
            content_type: "text/html; charset=utf-8",
        })
    }
}

/// The print documents of print exports, by job, kept while their job's outcome is: what
/// `GET /print/{job}` serves.
#[derive(Default)]
pub(crate) struct Prints {
    by_job: std::sync::Mutex<HashMap<JobId, PageSlot>>,
}

/// A print job's document, set once when the job made it.
type PageSlot = Arc<OnceLock<Arc<Vec<u8>>>>;

impl Prints {
    /// Keeps the slot `job` fills with its document. The documents of jobs the registry
    /// forgot go first, so a document lasts as long as its job's outcome.
    fn keep(&self, job: JobId, page: PageSlot, live: impl Fn(JobId) -> bool) {
        let mut by_job = crate::app::lock(&self.by_job);
        by_job.retain(|id, _| live(*id));
        by_job.insert(job, page);
    }

    fn get(&self, job: JobId) -> Option<Arc<Vec<u8>>> {
        crate::app::lock(&self.by_job).get(&job)?.get().cloned()
    }
}

impl Handle<RunExport> for App {
    async fn handle(&self, req: RunExport) -> Result<JobId, ApiError> {
        // The UI sends the print's setup and words exactly when it asks for a print.
        match (req.producer, &req.print) {
            (Producer::PrintHtml, None) => {
                return Err(ApiError::BadRequest {
                    field: "print".to_owned(),
                    reason: "a print needs its page setup and words".to_owned(),
                });
            }
            (Producer::AlignedUsfm | Producer::PlainUsfm | Producer::StoryMarkdown, Some(_)) => {
                return Err(ApiError::BadRequest {
                    field: "print".to_owned(),
                    reason: "only a print takes a page setup".to_owned(),
                });
            }
            _ => {}
        }
        let session = self.read(&req.at)?;
        // A print is served from its job (`GET /print/{job}`), every other file downloaded.
        let downloads = if req.producer == Producer::PrintHtml {
            None
        } else {
            let Some(downloads) = self.inner.downloads.get().cloned() else {
                return Err(self.internal(
                    "the server gave no downloads folder",
                    crate::reports::facts(serde_json::json!({})),
                ));
            };
            Some(downloads)
        };
        let spec =
            JobSpec::new(JobKind::Export, Phase::Export).project(session.scope().project.clone());
        let page: PageSlot = Arc::default();
        let filled = page.clone();
        let app = self.clone();
        let job = self
            .inner
            .jobs
            .start_typed::<RunExport, _, _>(spec, move |_ctx| async move {
                // tC4 checkpoints before every export and delivers nothing when it fails
                // (`runExport`), so the file and the project's last commit agree.
                match session
                    .handle()
                    .checkpoint(CheckpointReason::BeforeExport)
                    .await
                {
                    CheckpointOutcome::Failed { reason } => {
                        app.report_export_failure(reason);
                        return Err(failed(ExportRefusal::CheckpointFailed));
                    }
                    CheckpointOutcome::Committed | CheckpointOutcome::Unchanged => {}
                }
                let Made {
                    bytes,
                    file_name,
                    facts,
                } = app
                    .make_export(&session, req.producer, req.subject, req.print)
                    .await?;
                let facts = wire_facts(&facts);
                let Some(downloads) = downloads else {
                    let _ = filled.set(Arc::new(bytes));
                    return Ok(Exported {
                        file: ExportedFile::Print { file_name },
                        facts,
                    });
                };
                let content_type = if req.producer == Producer::StoryMarkdown {
                    "application/zip"
                } else {
                    "text/plain"
                };
                let written = downloads.dir().join(format!("{}.part", req.request));
                let name = file_name.clone();
                let registered = tokio::task::spawn_blocking(move || {
                    let added = std::fs::write(&written, &bytes)
                        .map_err(|e| e.to_string())
                        .and_then(|()| downloads.add(&written, &name, content_type));
                    if added.is_err() {
                        // A part file left behind would stay in the downloads folder until
                        // the next launch; a failed removal is that same leftover.
                        let _ = std::fs::remove_file(&written);
                    }
                    added
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r);
                let download = registered.map_err(|reason| {
                    app.report_export_failure(reason);
                    failed(ExportRefusal::WriteFailed)
                })?;
                Ok(Exported {
                    file: ExportedFile::Download {
                        download,
                        file_name,
                    },
                    facts,
                })
            });
        if req.producer == Producer::PrintHtml {
            // The page is kept under the id the registry gave, whether or not the job has
            // filled it yet; `print_page` serves it only once the job is done.
            let jobs = self.inner.jobs.clone();
            self.inner
                .prints
                .keep(job, page, |id| jobs.view(id).is_some());
        }
        Ok(job)
    }
}
