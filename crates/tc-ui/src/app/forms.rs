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

/// tC4's `im`, the kind step's part; F1 adds the files, the review and the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportForm {
    pub step: ImportStep,
    pub kind: Option<ImportKind>,
    pub busy: bool,
    pub error: Option<String>,
}

impl ImportForm {
    /// `openImport`.
    pub fn open() -> Self {
        Self {
            step: ImportStep::Kind,
            kind: None,
            busy: false,
            error: None,
        }
    }
}
