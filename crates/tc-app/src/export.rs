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

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tc_api::ApiError;
use tc_api::error::Feature;
use tc_api::event::CheckpointOutcome;
use tc_api::export::{
    Book, BookPrint, DraftedFrame, ExportFacts, Exported, Preflight, PreflightView, PrintFrame,
    PrintStory, PrintStoryItem, Producer, Run, RunExport, SkipReason, SkippedVerse, Stories,
    StoriesPrint, StoryRun, Subject,
};
use tc_api::ids::{DownloadId, JobId, RefPart, StoryNo};
use tc_api::jobs::{JobKind, Phase};
use tc_api::refusal::{ExportRefusal, Refusal};
use tc_export::book::{BookChapter, BookVerse, has_undrafted, print_items, printed_items};
use tc_export::{ExportError, ExportInput, Produced};
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
use crate::story::{journal_story, read_story, story_numbers};

/// A verse's label as the book writes it: `4`, or `4-5` for a span.
fn label(verse: u16, last: u16) -> String {
    if last > verse {
        format!("{verse}-{last}")
    } else {
        verse.to_string()
    }
}

impl Handle<Book> for App {
    async fn handle(&self, req: Book) -> Result<BookPrint, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
        let numbers = read.chapters_of(req.book);
        if numbers.is_empty() {
            return Err(ApiError::BadRequest {
                field: "book".to_owned(),
                reason: format!("{} is not in the project", req.book.code()),
            });
        }
        let chapters: Vec<BookChapter> = numbers
            .into_iter()
            .map(|number| BookChapter {
                number,
                verses: crate::translate::texts(&read, req.book, number)
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
            .collect();
        let items = print_items(&printed_items(&chapters)).map_err(|e| {
            self.internal(
                "a verse's number could not be written",
                crate::reports::facts(
                    serde_json::json!({"verse": e.label, "reason": e.source.to_string()}),
                ),
            )
        })?;
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

impl Handle<Stories> for App {
    async fn handle(&self, req: Stories) -> Result<StoriesPrint, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
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
            stories.push(read_story(&read, story)?);
        }
        if stories.is_empty() {
            return Err(ApiError::BadRequest {
                field: "at".to_owned(),
                reason: "the project holds no stories".to_owned(),
            });
        }
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
                    },
                })
            })
            .collect(),
        skipped_total: facts.skipped_total,
    }
}

/// The stories' files as the project holds them, by their path from the project folder.
// shortcut: the 50 story files only; tC4 also zips `content/front` and `content/back`, which
// the project keeps as carried files whose bytes only the store can read. Add them when the
// store hands the app a carried file's bytes.
fn story_files(
    read: &crate::counters::CountingSnapshot,
) -> Result<Vec<(String, Vec<u8>)>, ApiError> {
    let mut files = Vec::new();
    for number in story_numbers() {
        let Ok(story) = StoryNo::new(number) else {
            continue;
        };
        let file = DerivedFile::Story(journal_story(story)?);
        if let Some(bytes) = read.render(&file) {
            files.push((file.path(), bytes));
        }
    }
    Ok(files)
}

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

/// What an export's job made, before it is handed to the downloads.
struct Made {
    bytes: Vec<u8>,
    file_name: String,
    content_type: &'static str,
    facts: tc_export::ExportFacts,
}

impl App {
    /// Reads the subject and makes the file. Reads only.
    async fn make_export(
        &self,
        session: &Arc<ProjectSession>,
        producer: Producer,
        subject: Subject,
    ) -> Result<Made, JobEnd> {
        let today = self.inner.clock.now();
        let (producer, input, name, ext, content_type) = match (producer, subject) {
            (Producer::AlignedUsfm | Producer::PlainUsfm, Subject::Book { book }) => {
                let input = crate::align::export::book_input(self, session, book).await?;
                let id = book.code().to_lowercase();
                let (producer, name) = if producer == Producer::AlignedUsfm {
                    (tc_export::Producer::AlignedUsfm, format!("{id}-aligned"))
                } else {
                    (tc_export::Producer::PlainUsfm, id)
                };
                (
                    producer,
                    ExportInput::Book(input),
                    name,
                    "usfm",
                    "text/plain",
                )
            }
            (Producer::StoryMarkdown, Subject::Stories) => {
                let files = story_files(&session.read())?;
                if files.is_empty() {
                    return Err(JobEnd::Failed(ApiError::BadRequest {
                        field: "subject".to_owned(),
                        reason: "the project holds no stories".to_owned(),
                    }));
                }
                (
                    tc_export::Producer::StoryMarkdown,
                    ExportInput::StoryFiles(files),
                    format!("{}-stories", session.scope().project.as_str()),
                    "zip",
                    "application/zip",
                )
            }
            (producer, _) => {
                return Err(JobEnd::Failed(ApiError::BadRequest {
                    field: "subject".to_owned(),
                    reason: format!("{producer:?} does not export this subject"),
                }));
            }
        };
        let (produced, facts) = tc_export::produce(producer, &input).map_err(|e| match e {
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
            content_type,
            facts,
        })
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
}

impl Handle<RunExport> for App {
    async fn handle(&self, req: RunExport) -> Result<JobId, ApiError> {
        // The print document needs the page setup and the interface's words, which only the
        // UI holds and no request carries yet.
        if req.producer == Producer::PrintHtml {
            return Err(ApiError::Unavailable(Feature::Export));
        }
        let session = self.read(&req.at)?;
        let Some(downloads) = self.inner.downloads.get().cloned() else {
            return Err(self.internal(
                "the server gave no downloads folder",
                crate::reports::facts(serde_json::json!({})),
            ));
        };
        let spec =
            JobSpec::new(JobKind::Export, Phase::Export).project(session.scope().project.clone());
        let app = self.clone();
        Ok(self
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
                    content_type,
                    facts,
                } = app.make_export(&session, req.producer, req.subject).await?;
                let written = downloads.dir().join(format!("{}.part", req.request));
                let name = file_name.clone();
                let registered = tokio::task::spawn_blocking(move || {
                    std::fs::write(&written, &bytes).map_err(|e| e.to_string())?;
                    downloads.add(&written, &name, content_type)
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r);
                let download = registered.map_err(|reason| {
                    app.report_export_failure(reason);
                    failed(ExportRefusal::ReadFailed)
                })?;
                Ok(Exported {
                    download,
                    file_name,
                    facts: wire_facts(&facts),
                })
            }))
    }
}
