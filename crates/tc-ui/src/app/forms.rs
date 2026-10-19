//! The forms of Home's dialogs (unit B2): New Bible and New Open Bible Stories (tC4's `np`),
//! Add a book (`ab`, its first step) and Import (`im`, its first step). Pure; C6 and F1 add
//! the steps after the first and the creates.

use std::collections::BTreeSet;

use tc_api::home::{HomeCard, LanguageSuggestion};
use tc_api::ids::ProjectRef;
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
    pub busy: bool,
    pub error: Option<String>,
}

impl AddBookForm {
    /// `openAddBook`: the method step, with the first canonical book the project does not
    /// hold as the pick (Genesis when it holds every book).
    pub fn open(card: &HomeCard) -> Self {
        let existing: Vec<BookId> = match &card.books {
            Known::Known(books) => books.iter().map(|b| b.book).collect(),
            Known::Unknown => Vec::new(),
        };
        let book = BookId::ALL
            .into_iter()
            .find(|book| !existing.contains(book))
            .unwrap_or(BookId::Genesis);
        Self {
            project: card.project.clone(),
            project_name: card.name.clone(),
            existing,
            step: AddBookStep::Method,
            book,
            multi: false,
            books: BTreeSet::new(),
            busy: false,
            error: None,
        }
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
