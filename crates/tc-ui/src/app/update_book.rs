//! "Update a book from USFM" (unit F1; STORAGE §13.6, owner ruling OE 5): a USFM file edited
//! anywhere comes into a book of the open project. Three steps: the file, the book, then the
//! run and its report. No Undo: History keeps every version (delta Q7). Pure; the page's
//! actions run it.

use tc_api::ids::{JobId, RequestId, Scope};
use tc_api::import::{UpdateBook, UpdateReport, VerseAt};
use tc_api::jobs::Progress;
use tc_core::BookId;

use crate::app::forms::{ImportEntry, ImportFailure, ImportUpload};
use crate::app::submit::Submit;

/// The steps the dialog shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStep {
    File,
    Book,
    Working,
    Report,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdatePage {
    File,
    Book,
}

/// Where the update stands, beside the run's attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Running {
        job: Option<JobId>,
        progress: Option<Progress>,
    },
    /// What the callout says: no usable answer came.
    Problem(String),
    Done(UpdateReport),
    /// Refused: the sentence, with the line for a file that does not read (delta U1).
    Failed(ImportFailure),
}

/// The dialog's form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateBookForm {
    /// The open project's session the update runs in.
    pub at: Scope,
    page: UpdatePage,
    pub file: Option<ImportEntry>,
    next_file: u32,
    pub book: BookId,
    pub submit: Submit,
    pub state: UpdateState,
}

impl UpdateBookForm {
    /// The dialog on its file step, with `book` (the book on screen) as the choice.
    pub fn open(at: Scope, book: BookId, request: RequestId) -> Self {
        Self {
            at,
            page: UpdatePage::File,
            file: None,
            next_file: 0,
            book,
            submit: Submit::new(request),
            state: UpdateState::Idle,
        }
    }

    pub fn step(&self) -> UpdateStep {
        match (&self.state, self.page) {
            (UpdateState::Running { .. }, _) => UpdateStep::Working,
            (UpdateState::Done(_), _) => UpdateStep::Report,
            (UpdateState::Failed(_), _) => UpdateStep::Failed,
            (_, UpdatePage::File) => UpdateStep::File,
            (_, UpdatePage::Book) => UpdateStep::Book,
        }
    }

    /// A file chosen: it replaces the one before, and sends until the server has it.
    pub fn choose_file(&mut self, name: String, bytes: u64, next: RequestId) -> u32 {
        let id = self.next_file;
        self.next_file += 1;
        self.file = Some(ImportEntry {
            id,
            name,
            bytes,
            upload: ImportUpload::Sending,
        });
        self.state = UpdateState::Idle;
        self.submit.edited(next);
        id
    }

    /// The upload of file `id` ended; a file replaced meanwhile is not changed.
    pub fn file_sent(&mut self, id: u32, upload: ImportUpload) {
        if let Some(file) = self.file.as_mut().filter(|f| f.id == id) {
            file.upload = upload;
        }
    }

    /// "Choose the book →" waits for the file to be sent.
    pub fn can_choose_book(&self) -> bool {
        self.file
            .as_ref()
            .is_some_and(|f| matches!(f.upload, ImportUpload::Sent(_)))
    }

    pub fn to_book(&mut self) {
        if self.can_choose_book() {
            self.page = UpdatePage::Book;
        }
    }

    /// "← Choose another file", also from a refusal.
    pub fn back_to_file(&mut self) {
        self.page = UpdatePage::File;
        self.state = UpdateState::Idle;
    }

    pub fn choose_book(&mut self, book: BookId, next: RequestId) {
        self.book = book;
        self.submit.edited(next);
    }

    /// The press of "Update the book": the request, the dialog on its working step.
    pub fn press_run(&mut self) -> Option<UpdateBook> {
        if self.page != UpdatePage::Book
            || matches!(
                self.state,
                UpdateState::Running { .. } | UpdateState::Done(_)
            )
        {
            return None;
        }
        let file = match &self.file {
            Some(ImportEntry {
                name,
                upload: ImportUpload::Sent(upload),
                ..
            }) => tc_api::import::ImportFile {
                upload: upload.clone(),
                name: name.clone(),
            },
            _ => return None,
        };
        let request = self.submit.press()?;
        self.state = UpdateState::Running {
            job: None,
            progress: None,
        };
        Some(UpdateBook {
            request,
            at: self.at.clone(),
            book: self.book,
            file,
        })
    }

    pub fn run_started(&mut self, job: JobId) {
        if let UpdateState::Running { job: held, .. } = &mut self.state {
            *held = Some(job);
        }
    }

    pub fn run_progress(&mut self, progress: Progress) {
        if let UpdateState::Running { progress: held, .. } = &mut self.state {
            *held = Some(progress);
        }
    }

    pub fn run_done(&mut self, report: UpdateReport) {
        self.submit.answered();
        self.state = UpdateState::Done(report);
    }

    /// Refused: the next press is a new attempt with `next`. The upload is used up.
    pub fn run_refused(&mut self, failure: ImportFailure, next: RequestId) {
        self.submit.refused(failure.sentence.clone(), next);
        self.file = None;
        self.state = UpdateState::Failed(failure);
    }

    /// No usable answer: the next press sends the same request again.
    pub fn run_lost(&mut self, problem: String) {
        self.submit.lost(problem.clone());
        self.state = UpdateState::Problem(problem);
    }

    /// Ended with nothing to say (a cancel, a new server run). The job deleted the upload when
    /// it ended, so the person chooses the file again.
    pub fn run_stopped(&mut self, next: RequestId) {
        self.submit.next(next);
        self.file = None;
        self.page = UpdatePage::File;
        self.state = UpdateState::Idle;
    }
}

/// "1:3, 2:4": the report's verses, in order.
pub fn verse_list(verses: &[VerseAt]) -> String {
    let mut sorted = verses.to_vec();
    sorted.sort();
    sorted
        .iter()
        .map(|v| format!("{}:{}", v.chapter, v.verse))
        .collect::<Vec<_>>()
        .join(", ")
}
