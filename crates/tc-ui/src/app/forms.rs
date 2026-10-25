//! The forms of Home's dialogs (unit B2): New Bible and New Open Bible Stories (tC4's `np`),
//! Add a book (`ab`, its first step) and Import (`im`, its first step). Pure; C6 and F1 add
//! the steps after the first and the creates.

use std::collections::BTreeSet;

use tc_api::create::{Claims, Details, UsfmProblem, UsfmVerdict};
use tc_api::home::{HomeCard, LanguageSuggestion};
use tc_api::ids::{ProjectRef, RequestId, UploadId};
use tc_api::refusal::CreateRefusal;
use tc_api::shown::{Direction, Known};
use tc_core::BookId;

/// The chapters of each book in canonical order (tC4 `BOOK_CHAPTERS`).
pub const CHAPTERS: [u16; 66] = [
    50, 40, 27, 36, 34, 24, 21, 4, 31, 24, 22, 25, 29, 36, 10, 13, 10, 42, 150, 31, 12, 8, 66, 52,
    5, 48, 12, 14, 3, 9, 1, 4, 7, 3, 3, 3, 2, 14, 4, 28, 16, 24, 21, 28, 16, 16, 13, 6, 6, 4, 4, 5,
    3, 6, 4, 3, 1, 13, 5, 5, 3, 5, 1, 1, 1, 22,
];

/// The chapters of `book`.
pub fn chapters(book: BookId) -> u16 {
    CHAPTERS
        .get(usize::from(book.ordinal()) - 1)
        .copied()
        .unwrap_or_default()
}

/// The line under the book picker (tC4 `addBook.info`, `addBook.infoOne`): a book of one
/// chapter says "1 chapter" and not "1 chapters" (#577).
pub fn book_info_key(book: BookId) -> crate::i18n::Key {
    if chapters(book) == 1 {
        crate::i18n::keys::add_book::INFO_ONE
    } else {
        crate::i18n::keys::add_book::INFO
    }
}

/// The fonts New Bible offers (`SCRIPT_FONTS`, `tC4:src/state.jsx:327`).
pub const SCRIPT_FONTS: [&str; 5] = [
    "Noto Sans (default)",
    "Charis SIL",
    "Scheherazade New — Arabic script",
    "Awami Nastaliq — Nastaliq",
    "Padauk — Myanmar",
];

/// Which project a new-project form makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewKind {
    Bible,
    Stories,
}

/// tC4's `np`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProjectForm {
    pub kind: NewKind,
    pub name: String,
    pub lang_name: String,
    pub code: String,
    /// Door43's code of the chosen language when the Code box holds another (Q35): the
    /// project keeps it beside its own (C6 stores it).
    pub door43_code: Option<String>,
    /// The Door43 code a chosen language has when no code could be made of it (Q35).
    pub unusable_code: Option<String>,
    pub dir: Direction,
    pub font: String,
    pub versification: String,
    pub versifications: Vec<String>,
    /// Why the versification list could not be read: the picker holds `eng` alone and says so.
    pub versifications_error: Option<String>,
    pub show_advanced: bool,
    pub busy: bool,
    pub error: Option<String>,
    /// The id of the attempt in flight or lost, kept so that a press after a lost reply is
    /// answered from the server's reply cache and creates nothing twice (A-27). An edit or an
    /// answer ends the attempt.
    pub request: Option<RequestId>,
}

impl NewProjectForm {
    fn new(kind: NewKind) -> Self {
        Self {
            kind,
            name: String::new(),
            lang_name: String::new(),
            code: String::new(),
            door43_code: None,
            unusable_code: None,
            dir: Direction::Ltr,
            font: SCRIPT_FONTS[0].to_owned(),
            versification: "eng".to_owned(),
            versifications: vec!["eng".to_owned()],
            versifications_error: None,
            show_advanced: false,
            busy: false,
            error: None,
            request: None,
        }
    }

    /// New Bible (`openNewProject`): the versification list, or `eng` alone and the reason
    /// when it could not be read.
    pub fn bible(versifications: Result<Vec<String>, String>) -> Self {
        let mut form = Self::new(NewKind::Bible);
        match versifications {
            Ok(names) if !names.is_empty() => form.versifications = names,
            Ok(_) => {}
            Err(reason) => form.versifications_error = Some(reason),
        }
        form
    }

    /// New Open Bible Stories (`openNewObs`): no versification.
    pub fn stories() -> Self {
        Self::new(NewKind::Stories)
    }

    /// A suggestion chosen: its name, the code a project can hold (empty when none can be
    /// made) and its direction.
    pub fn choose(&mut self, suggestion: &LanguageSuggestion) {
        self.lang_name = suggestion.name.clone();
        self.code = suggestion.code.clone().unwrap_or_default();
        self.door43_code = suggestion.code.as_ref().and(suggestion.door43.clone());
        self.unusable_code = match suggestion.code {
            None => suggestion.door43.clone(),
            Some(_) => None,
        };
        self.dir = suggestion.dir;
    }

    /// A code typed in the Code box: it is the person's own, so no offer stands.
    pub fn type_code(&mut self, code: String) {
        self.code = code;
        self.door43_code = None;
        self.unusable_code = None;
    }

    /// What Create sends (tC4's `np` as the create reads it).
    pub fn details(&self) -> Details {
        Details {
            name: self.name.clone(),
            language_name: self.lang_name.clone(),
            code: self.code.clone(),
            door43_code: self.door43_code.clone(),
            direction: self.dir,
            font: self.font.clone(),
        }
    }

    /// The press of Create (tC4 `createBible`, `createObs`): nothing while an attempt runs;
    /// the form's own refusal when the name or the code is missing (`problem`); else the
    /// attempt's id and what it sends, the button disabled until the answer. `fresh` makes the
    /// id of a new attempt.
    pub fn press(
        &mut self,
        fresh: impl FnOnce() -> RequestId,
    ) -> Option<Result<(RequestId, Details), CreateRefusal>> {
        if self.busy {
            return None;
        }
        let details = self.details();
        if let Err(problem) = details.folder() {
            return Some(Err(problem));
        }
        self.busy = true;
        self.error = None;
        let request = *self.request.get_or_insert_with(fresh);
        Some(Ok((request, details)))
    }

    /// The server answered: the attempt is over, with the words of its refusal if it had one.
    pub fn answered(&mut self, error: Option<String>) {
        self.busy = false;
        self.request = None;
        self.error = error;
    }

    /// No answer came (the connection broke): the attempt may have been carried out, so the
    /// next press sends the same id and gets that attempt's reply.
    pub fn lost(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }

    /// A field changed: what the next press sends is another request.
    pub fn edited(&mut self) {
        self.request = None;
    }
}

/// The sentence under the language row after a choice (Q35).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeOffer<'a> {
    None,
    /// Door43's code is not one a project can hold; the project uses `code`.
    Adjusted {
        door43: &'a str,
        code: &'a str,
    },
    /// No code could be made of Door43's: the person types one.
    Unusable {
        door43: &'a str,
    },
}

pub fn code_offer(form: &NewProjectForm) -> CodeOffer<'_> {
    match (&form.door43_code, &form.unusable_code) {
        (Some(door43), _) => CodeOffer::Adjusted {
            door43,
            code: &form.code,
        },
        (None, Some(door43)) => CodeOffer::Unusable { door43 },
        (None, None) => CodeOffer::None,
    }
}

/// The steps of Add a book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddBookStep {
    Method,
    Pick,
    Usfm,
}

/// A file picked for Add a book: its name, and what the server made of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsfmEntry {
    /// Names the file in the list whatever is removed meanwhile: the server's answer for a file
    /// finds its row by this.
    pub id: u32,
    pub name: String,
    pub state: UsfmState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsfmState {
    /// Being sent to the server and read there.
    Reading,
    /// Uploaded and read: the file, and what it is.
    Read {
        upload: UploadId,
        verdict: UsfmVerdict,
    },
    /// It could not be sent or read.
    Unreadable(String),
}

/// A row of the USFM step: the file, and whether it can be added (tC4's `StatusDot` and caption).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsfmRow {
    pub id: u32,
    pub name: String,
    pub state: RowState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowState {
    Reading,
    Adds(BookId),
    Refused(UsfmProblem),
    Unreadable(String),
}

/// tC4's `ab`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddBookForm {
    pub project: ProjectRef,
    pub project_name: String,
    pub existing: Vec<BookId>,
    pub step: AddBookStep,
    pub book: BookId,
    pub multi: bool,
    pub books: BTreeSet<BookId>,
    /// The picked files, in the order they were picked (#484).
    pub files: Vec<UsfmEntry>,
    next_file: u32,
    pub busy: bool,
    pub error: Option<String>,
    /// The id of the add in flight or lost: see [`NewProjectForm::request`].
    pub request: Option<RequestId>,
}

impl AddBookForm {
    /// `openAddBook`: the method step, with the first canonical book the project does not
    /// hold as the pick (Genesis when it holds every book).
    pub fn open(card: &HomeCard) -> Self {
        let existing: Vec<BookId> = match &card.books {
            Known::Known(books) => books.iter().map(|b| b.book).collect(),
            Known::Unknown => Vec::new(),
        };
        Self::of(card.project.clone(), card.name.clone(), existing)
    }

    /// The form for a project just created, which holds no book (tC4 `createBible` ends in
    /// `openAddBook`).
    pub fn of_new(project: ProjectRef, name: String) -> Self {
        Self::of(project, name, Vec::new())
    }

    fn of(project: ProjectRef, project_name: String, existing: Vec<BookId>) -> Self {
        let book = BookId::ALL
            .into_iter()
            .find(|book| !existing.contains(book))
            .unwrap_or(BookId::Genesis);
        Self {
            project,
            project_name,
            existing,
            step: AddBookStep::Method,
            book,
            multi: false,
            books: BTreeSet::new(),
            files: Vec::new(),
            next_file: 0,
            busy: false,
            error: None,
            request: None,
        }
    }

    /// The books the pick step would add: those ticked in the grid, or the one chosen, apart
    /// from those the project holds (tC4 `addBooks`: adding must never touch a book that
    /// carries drafts).
    pub fn picked(&self) -> Vec<BookId> {
        let wanted: Vec<BookId> = if self.multi {
            BookId::ALL
                .into_iter()
                .filter(|book| self.books.contains(book))
                .collect()
        } else {
            vec![self.book]
        };
        wanted
            .into_iter()
            .filter(|book| !self.existing.contains(book))
            .collect()
    }

    /// The pick that names a book the project holds, for the warning (tC4 `existingPicked`).
    pub fn held_picks(&self) -> Vec<BookId> {
        let wanted: Vec<BookId> = if self.multi {
            self.books.iter().copied().collect()
        } else {
            vec![self.book]
        };
        wanted
            .into_iter()
            .filter(|book| self.existing.contains(book))
            .collect()
    }

    /// A book of the grid ticked or unticked.
    pub fn toggle(&mut self, book: BookId) {
        if self.existing.contains(&book) {
            return;
        }
        if !self.books.remove(&book) {
            self.books.insert(book);
        }
    }

    /// The press of Create book (tC4 `addBooks`): the books to add and the id of the attempt, or
    /// `Err` when none is picked. Nothing while an attempt runs.
    pub fn press_blank(
        &mut self,
        fresh: impl FnOnce() -> RequestId,
    ) -> Option<Result<(RequestId, Vec<BookId>), ()>> {
        if self.busy {
            return None;
        }
        let books = self.picked();
        if books.is_empty() {
            return Some(Err(()));
        }
        self.busy = true;
        self.error = None;
        Some(Ok((*self.request.get_or_insert_with(fresh), books)))
    }

    /// A file picked: its row reads until the server says what it is. Its id finds the row
    /// again.
    pub fn add_file(&mut self, name: String) -> u32 {
        let id = self.next_file;
        self.next_file += 1;
        self.files.push(UsfmEntry {
            id,
            name,
            state: UsfmState::Reading,
        });
        self.request = None;
        id
    }

    /// The server read file `id`; a file removed meanwhile has no row to change.
    pub fn file_read(&mut self, id: u32, state: UsfmState) {
        if let Some(entry) = self.files.iter_mut().find(|e| e.id == id) {
            entry.state = state;
        }
    }

    /// Remove (✕): the file leaves, and the verdicts of the others are judged again.
    pub fn remove_file(&mut self, id: u32) {
        self.files.retain(|entry| entry.id != id);
        self.request = None;
    }

    /// Each file's row, judged in order (tC4 `classifyUsfmFiles`): a book the project holds or
    /// an earlier file claimed is refused in its row.
    pub fn rows(&self) -> Vec<UsfmRow> {
        let mut claims = Claims::default();
        self.files
            .iter()
            .map(|entry| {
                let state = match &entry.state {
                    UsfmState::Reading => RowState::Reading,
                    UsfmState::Unreadable(reason) => RowState::Unreadable(reason.clone()),
                    UsfmState::Read { verdict, .. } => match verdict {
                        UsfmVerdict::Refused(problem) => RowState::Refused(problem.clone()),
                        UsfmVerdict::Book { book } => {
                            match claims.claim(&entry.name, *book, self.existing.contains(book)) {
                                Ok(book) => RowState::Adds(book),
                                Err(problem) => RowState::Refused(problem),
                            }
                        }
                    },
                };
                UsfmRow {
                    id: entry.id,
                    name: entry.name.clone(),
                    state,
                }
            })
            .collect()
    }

    /// The files Add would add.
    pub fn addable(&self) -> usize {
        self.rows()
            .iter()
            .filter(|row| matches!(row.state, RowState::Adds(_)))
            .count()
    }

    /// Add is disabled while it runs, while a file is still read, and with nothing to add
    /// (tC4 `ab-usfm-add`; the label stays "Add book", #564).
    pub fn add_disabled(&self) -> bool {
        self.busy
            || self.addable() == 0
            || self
                .files
                .iter()
                .any(|entry| entry.state == UsfmState::Reading)
    }

    /// The press of Add (tC4 `addUsfmBooks`): the id of the attempt and the files to send, in
    /// order. The server judges every file again; the ones the rows refuse go too, so that
    /// their uploads are deleted with the rest.
    pub fn press_usfm(
        &mut self,
        fresh: impl FnOnce() -> RequestId,
    ) -> Option<(RequestId, Vec<tc_api::create::UsfmFile>)> {
        if self.add_disabled() {
            return None;
        }
        let files = self
            .files
            .iter()
            .filter_map(|entry| match &entry.state {
                UsfmState::Read { upload, .. } => Some(tc_api::create::UsfmFile {
                    upload: upload.clone(),
                    name: entry.name.clone(),
                }),
                _ => None,
            })
            .collect();
        self.busy = true;
        self.error = None;
        Some((*self.request.get_or_insert_with(fresh), files))
    }

    /// The server answered an add: the books it added leave the list and join the project's
    /// books; a refused or unwritten file stays in its row with the words of its reason.
    pub fn added(&mut self, added: &[(String, BookId)], error: Option<String>) {
        self.busy = false;
        self.request = None;
        self.error = error;
        for (name, book) in added {
            self.files.retain(|entry| entry.name != *name);
            if !self.existing.contains(book) {
                self.existing.push(*book);
            }
        }
    }

    /// The add may have been carried out; the next press sends the same id.
    pub fn lost(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }
}

/// The kinds of file Import takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Tc3,
    Usfm,
    Burrito,
}

impl ImportKind {
    pub const ALL: [ImportKind; 3] = [Self::Tc3, Self::Usfm, Self::Burrito];

    /// tC4's id: `tc3`, `usfm`, `burrito`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Tc3 => "tc3",
            Self::Usfm => "usfm",
            Self::Burrito => "burrito",
        }
    }
}

/// The steps of Import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStep {
    Kind,
    Files,
    Review,
    Working,
    Failed,
}

/// The page of Import the person is on; the run's state adds Working and Failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportPage {
    Kind,
    Files,
    Review,
}

/// Where a chosen file's upload stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportUpload {
    Sending,
    Sent(UploadId),
    /// It could not be sent: why, in words.
    Failed(String),
}

/// A file chosen on the files step (tC4 `im.files`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEntry {
    /// Names the row whatever is removed meanwhile.
    pub id: u32,
    pub name: String,
    pub bytes: u64,
    pub upload: ImportUpload,
}

/// A refusal as the damaged panel and the failed step show it: its sentence, tC4's code for
/// it when tC4 has one, and what went wrong in detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFailure {
    pub code: Option<String>,
    pub sentence: String,
    pub detail: Option<String>,
}

/// What the review step read of the files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reviewed {
    Found(Box<tc_api::import::ImportReview>),
    /// The files cannot be imported (tC4's `damaged` finding): nothing will be made of them.
    Damaged(ImportFailure),
}

/// The resource versions of a tC3 import (tC4 `im.versions`, D82). The review never looks
/// them up on its own (D95).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionLookup {
    NotLookedUp,
    Looking,
    Found(tc_api::import::VersionsView),
    /// "Use installed versions": the slots Door43 did not give take this computer's, and what
    /// that does to their decisions once the server has counted it.
    Installed(
        tc_api::import::VersionsView,
        Option<tc_api::import::CarryOver>,
    ),
}

impl VersionLookup {
    /// tC4's `data-state` of the Resource versions row.
    pub fn data_state(&self) -> &'static str {
        match self {
            Self::NotLookedUp => "offline",
            Self::Looking => "looking",
            Self::Found(view) if view.unresolved.is_empty() => "found",
            Self::Found(_) => "missing",
            Self::Installed(..) => "installed",
        }
    }

    /// Import waits for a choice, a lookup or Use installed versions, or for the count of
    /// what the installed versions do (tC4 `im.busy`).
    pub fn open(&self) -> bool {
        match self {
            Self::NotLookedUp | Self::Looking | Self::Installed(_, None) => true,
            Self::Found(view) => !view.unresolved.is_empty(),
            Self::Installed(_, Some(_)) => false,
        }
    }

    /// The versions Door43 gave, which the project pins.
    pub fn found(&self) -> Vec<tc_api::import::FoundVersion> {
        match self {
            Self::Found(view) | Self::Installed(view, _) => view.found.clone(),
            Self::NotLookedUp | Self::Looking => Vec::new(),
        }
    }
}

/// tC4's `im.busy` and `im.error` as one state, beside the run's attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportState {
    Idle,
    /// `import/review` is out: the files step's buttons are off.
    Reviewing,
    /// The run is out (the working step); its job once the server named it.
    Running {
        job: Option<tc_api::ids::JobId>,
        progress: Option<tc_api::jobs::Progress>,
    },
    /// What the callout says.
    Problem(String),
    /// The run was refused or failed: the failed step.
    Failed(ImportFailure),
}

/// tC4's `im`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportForm {
    page: ImportPage,
    pub kind: Option<ImportKind>,
    /// The chosen files, in the order they were chosen.
    pub files: Vec<ImportEntry>,
    next_file: u32,
    pub reviewed: Option<Reviewed>,
    /// The Bible name and the language code of the review step.
    pub name: String,
    pub code: String,
    pub versions: VersionLookup,
    /// The license the project states: the files' one, or the one picked where they differ
    /// (tC4 `im.license`); `None` until picked, `Some("")` for none.
    pub license: Option<String>,
    /// The run's attempt: one request per attempt (UI-3).
    pub submit: crate::app::submit::Submit,
    pub state: ImportState,
}

impl ImportForm {
    /// `openImport`, with the request of its first attempt.
    pub fn open(request: RequestId) -> Self {
        Self {
            page: ImportPage::Kind,
            kind: None,
            files: Vec::new(),
            next_file: 0,
            reviewed: None,
            name: String::new(),
            code: String::new(),
            versions: VersionLookup::NotLookedUp,
            license: None,
            submit: crate::app::submit::Submit::new(request),
            state: ImportState::Idle,
        }
    }

    pub fn step(&self) -> ImportStep {
        match (&self.state, self.page) {
            (ImportState::Running { .. }, _) => ImportStep::Working,
            (ImportState::Failed(_), _) => ImportStep::Failed,
            (_, ImportPage::Kind) => ImportStep::Kind,
            (_, ImportPage::Files) => ImportStep::Files,
            (_, ImportPage::Review) => ImportStep::Review,
        }
    }

    /// A card of the kind step (tC4 `importPickKind`). Only the tC3 kind has an importer in
    /// this release; the others stay on the kind step (rulings/2026-10-24-H3-H4-H7).
    pub fn pick_kind(&mut self, kind: ImportKind) {
        if kind != ImportKind::Tc3 {
            return;
        }
        self.kind = Some(kind);
        self.files.clear();
        self.reviewed = None;
        self.page = ImportPage::Files;
        self.state = ImportState::Idle;
    }

    /// "← Choose another kind".
    pub fn back_to_kind(&mut self) {
        self.page = ImportPage::Kind;
        self.state = ImportState::Idle;
    }

    /// "← Choose other files".
    pub fn back_to_files(&mut self) {
        self.page = ImportPage::Files;
        self.reviewed = None;
        self.versions = VersionLookup::NotLookedUp;
        self.state = ImportState::Idle;
    }

    /// A file chosen or dropped: its row sends until the server has it.
    pub fn add_file(&mut self, name: String, bytes: u64) -> u32 {
        let id = self.next_file;
        self.next_file += 1;
        self.files.push(ImportEntry {
            id,
            name,
            bytes,
            upload: ImportUpload::Sending,
        });
        if matches!(self.state, ImportState::Problem(_)) {
            self.state = ImportState::Idle;
        }
        id
    }

    /// The upload of file `id` ended; a file removed meanwhile has no row to change.
    pub fn file_sent(&mut self, id: u32, upload: ImportUpload) {
        if let Some(entry) = self.files.iter_mut().find(|e| e.id == id) {
            entry.upload = upload;
        }
    }

    pub fn remove_file(&mut self, id: u32) {
        self.files.retain(|e| e.id != id);
    }

    /// The uploaded files, in order.
    pub fn sent(&self) -> Vec<tc_api::import::ImportFile> {
        self.files
            .iter()
            .filter_map(|e| match &e.upload {
                ImportUpload::Sent(upload) => Some(tc_api::import::ImportFile {
                    upload: upload.clone(),
                    name: e.name.clone(),
                }),
                ImportUpload::Sending | ImportUpload::Failed(_) => None,
            })
            .collect()
    }

    /// "Review files →" is off while a file sends or could not be sent, with no file, and
    /// while a review is out.
    pub fn can_review(&self) -> bool {
        !self.files.is_empty()
            && self
                .files
                .iter()
                .all(|e| matches!(e.upload, ImportUpload::Sent(_)))
            && self.state != ImportState::Reviewing
    }

    /// The press of "Review files →": the files to read.
    pub fn press_review(&mut self) -> Option<Vec<tc_api::import::ImportFile>> {
        if !self.can_review() {
            return None;
        }
        self.state = ImportState::Reviewing;
        Some(self.sent())
    }

    /// The review's answer: the review step with the details the files hold, not looked up.
    /// A review that names no version has nothing to look up (tC4 `unresolvedSlots` empty).
    pub fn reviewed(&mut self, review: tc_api::import::ImportReview, next: RequestId) {
        self.name = review.details.name.clone();
        self.code = review.details.code.clone();
        self.license = match review.found.licenses.as_slice() {
            [one] => Some(one.clone()),
            [] => Some(String::new()),
            _ => None,
        };
        self.versions = if review.found.slots.is_empty() {
            VersionLookup::Found(tc_api::import::VersionsView {
                found: Vec::new(),
                unresolved: Vec::new(),
            })
        } else {
            VersionLookup::NotLookedUp
        };
        self.reviewed = Some(Reviewed::Found(Box::new(review)));
        self.page = ImportPage::Review;
        self.state = ImportState::Idle;
        self.submit.next(next);
    }

    /// The files cannot be imported: the review step says why, and Import stays off.
    pub fn review_refused(&mut self, failure: ImportFailure) {
        self.reviewed = Some(Reviewed::Damaged(failure));
        self.page = ImportPage::Review;
        self.state = ImportState::Idle;
    }

    /// No usable answer: the files step says why.
    pub fn review_lost(&mut self, problem: String) {
        self.state = ImportState::Problem(problem);
    }

    /// The Bible name typed: the next Import is another request.
    pub fn type_name(&mut self, name: String, next: RequestId) {
        self.name = name;
        self.submit.edited(next);
    }

    /// The language code typed (tC4 trims it).
    pub fn type_code(&mut self, code: String, next: RequestId) {
        self.code = code.trim().to_owned();
        self.submit.edited(next);
    }

    /// The details Import sends.
    pub fn details(&self) -> tc_api::import::ImportDetails {
        tc_api::import::ImportDetails {
            name: self.name.trim().to_owned(),
            code: self.code.trim().to_owned(),
        }
    }

    /// The review's refusal while the details are the ones it read.
    fn review_refusal(&self) -> Option<&tc_api::Refusal> {
        match &self.reviewed {
            Some(Reviewed::Found(review)) if review.details == self.details() => {
                review.refusal.as_ref()
            }
            _ => None,
        }
    }

    /// tC4 `nameClash`: the folder the name makes is one Home lists (without letter case), or
    /// the review refused the name in use while the details are the ones it read.
    pub fn name_in_use<'a>(&self, listing: impl IntoIterator<Item = &'a str>) -> bool {
        if matches!(
            self.review_refusal(),
            Some(tc_api::Refusal::Import(
                tc_api::refusal::ImportRefusal::NameExists
            ))
        ) {
            return true;
        }
        let details = self.details();
        let form = Details {
            name: details.name,
            language_name: String::new(),
            code: details.code,
            door43_code: None,
            direction: Direction::Ltr,
            font: String::new(),
        };
        // The name alone makes the folder unless it is all punctuation; a missing code says
        // nothing of a clash.
        let form = Details {
            code: if form.code.is_empty() {
                "und".to_owned()
            } else {
                form.code
            },
            ..form
        };
        form.folder().is_ok_and(|folder| {
            listing
                .into_iter()
                .any(|name| name.eq_ignore_ascii_case(folder.as_str()))
        })
    }

    /// The review's refusal of the details other than a name in use (no name, a language code
    /// a Scripture Burrito cannot hold, #835), while the details are the ones it read.
    pub fn details_refusal(&self) -> Option<&tc_api::Refusal> {
        self.review_refusal().filter(|r| {
            !matches!(
                r,
                tc_api::Refusal::Import(tc_api::refusal::ImportRefusal::NameExists)
            )
        })
    }

    /// "Look up on Door43": `true` when the lookup is to run.
    pub fn look_up(&mut self) -> bool {
        if !matches!(self.reviewed, Some(Reviewed::Found(_)))
            || self.versions != VersionLookup::NotLookedUp
        {
            return false;
        }
        self.versions = VersionLookup::Looking;
        true
    }

    pub fn versions_found(&mut self, view: tc_api::import::VersionsView) {
        if self.versions == VersionLookup::Looking {
            self.versions = VersionLookup::Found(view);
        }
    }

    /// The lookup gave no answer: it can be asked again, and the callout says why.
    pub fn versions_failed(&mut self, problem: String) {
        self.versions = VersionLookup::NotLookedUp;
        self.state = ImportState::Problem(problem);
    }

    /// The internet question's Not now, or the question took over.
    pub fn versions_not_looked_up(&mut self) {
        if self.versions == VersionLookup::Looking {
            self.versions = VersionLookup::NotLookedUp;
        }
    }

    /// "Use installed versions" (tC4 `importUseInstalled`): the review to ask for what the
    /// installed versions do to the decisions. Not looked up, every slot the files name takes
    /// them.
    pub fn use_installed(&mut self) -> Option<tc_api::import::Review> {
        let view = match &self.versions {
            VersionLookup::Found(view) if !view.unresolved.is_empty() => view.clone(),
            VersionLookup::NotLookedUp => match &self.reviewed {
                Some(Reviewed::Found(review)) => tc_api::import::VersionsView {
                    found: Vec::new(),
                    unresolved: review.found.slots.clone(),
                },
                _ => return None,
            },
            _ => return None,
        };
        let found = view.found.clone();
        self.versions = VersionLookup::Installed(view, None);
        Some(tc_api::import::Review {
            files: self.sent(),
            details: Some(self.details()),
            use_installed: Some(found),
        })
    }

    pub fn carry_counted(&mut self, carry: Option<tc_api::import::CarryOver>) {
        if let (VersionLookup::Installed(_, held @ None), Some(carry)) = (&mut self.versions, carry)
        {
            *held = Some(carry);
        }
    }

    /// The count failed: the choice is offered again, and the callout says why.
    pub fn carry_failed(&mut self, problem: String) {
        if let VersionLookup::Installed(view, None) = &self.versions {
            self.versions = if view.found.is_empty() {
                VersionLookup::NotLookedUp
            } else {
                VersionLookup::Found(view.clone())
            };
            self.state = ImportState::Problem(problem);
        }
    }

    /// The license picker (tC4 `import-license`): `None` is "Choose a license".
    pub fn pick_license(&mut self, license: Option<String>, next: RequestId) {
        self.license = license;
        self.submit.edited(next);
    }

    /// tC4 `cantImport`, inverted.
    pub fn can_import<'a>(&self, listing: impl IntoIterator<Item = &'a str>) -> bool {
        matches!(self.reviewed, Some(Reviewed::Found(_)))
            && self.page == ImportPage::Review
            && !self.name.trim().is_empty()
            && !self.code.trim().is_empty()
            && !self.name_in_use(listing)
            && self.details_refusal().is_none()
            && !self.versions.open()
            && self.license.is_some()
            && !matches!(
                self.state,
                ImportState::Reviewing | ImportState::Running { .. }
            )
            && !self.submit.busy()
    }

    /// The press of Import (and of "Try the import again"): the request to send, the form on
    /// its working step.
    pub fn press_run<'a>(
        &mut self,
        listing: impl IntoIterator<Item = &'a str>,
    ) -> Option<tc_api::import::Run> {
        if !self.can_import(listing) {
            return None;
        }
        let request = self.submit.press()?;
        self.state = ImportState::Running {
            job: None,
            progress: None,
        };
        Some(tc_api::import::Run {
            request,
            files: self.sent(),
            details: self.details(),
            found: self.versions.found(),
            license: self.license.clone().unwrap_or_default(),
        })
    }

    pub fn run_started(&mut self, job: tc_api::ids::JobId) {
        if let ImportState::Running { job: held, .. } = &mut self.state {
            *held = Some(job);
        }
    }

    pub fn run_progress(&mut self, progress: tc_api::jobs::Progress) {
        if let ImportState::Running { progress: held, .. } = &mut self.state {
            *held = Some(progress);
        }
    }

    /// The run was refused or failed: the failed step, and the next press is a new attempt.
    pub fn run_refused(&mut self, failure: ImportFailure, next: RequestId) {
        self.submit.refused(failure.sentence.clone(), next);
        self.state = ImportState::Failed(failure);
    }

    /// No usable answer: back to the review with why; the next press sends the same request.
    pub fn run_lost(&mut self, problem: String) {
        self.submit.lost(problem.clone());
        self.state = ImportState::Problem(problem);
    }

    /// The run ended with nothing to say (a cancel, a new server run): back to the review.
    pub fn run_stopped(&mut self, next: RequestId) {
        self.submit.next(next);
        self.state = ImportState::Idle;
    }
}
