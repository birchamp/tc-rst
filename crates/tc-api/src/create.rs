//! Creating a project and adding books to it (unit C6; `tC4:src/views/modals/NewBible.jsx`,
//! `NewObs.jsx`, `AddBook.jsx`). Every write carries a `RequestId`, and the dialogs send one
//! request for each press of their button, which they disable while it runs (APP-DESIGN A-27).
//!
//! A project is created whole or not at all: its details, its settings and its pins are one
//! batch in a staging folder that is renamed into place without replacing anything (R2).

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::ids::{ProjectRef, RequestId, UploadId};
use crate::refusal::CreateRefusal;
use crate::shown::Direction;

/// The version of the bundled drafting sources that Add a book names (tC4 `SUITE_VERSION`).
/// tc-app's tests hold it to the pins a new project gets.
pub const SUITE_VERSION: &str = "v91";

/// What New Bible and New Open Bible Stories ask for (tC4's `np`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Details {
    pub name: String,
    pub language_name: String,
    /// The language code the project keeps: a tag a Scripture Burrito can hold.
    pub code: String,
    /// Door43's code for the language, when the Code box holds another because a Burrito
    /// cannot hold Door43's (owner's answer to Q35). The project keeps it beside its own.
    pub door43_code: Option<String>,
    pub direction: Direction,
    pub font: String,
}

impl Details {
    /// The folder the project's name makes (tC4 `validateNewBible`): the name in lower case
    /// with every run of other characters than `a` to `z` and `0` to `9` turned into one `_`,
    /// else the code the same way. The dialog shows the refusal before it sends anything, and
    /// the server checks again.
    pub fn folder(&self) -> Result<ProjectRef, CreateRefusal> {
        if self.name.trim().is_empty() {
            return Err(CreateRefusal::NameRequired);
        }
        if self.code.trim().is_empty() {
            return Err(CreateRefusal::CodeRequired);
        }
        let abbr = slug(&self.name);
        let abbr = if abbr.is_empty() {
            slug(&self.code)
        } else {
            abbr
        };
        // A slug is ASCII letters, digits and `_`: always a folder name on every system.
        ProjectRef::parse(&abbr).map_err(|_| CreateRefusal::AbbrRequired)
    }
}

/// tC4's `slug`: `value.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '')`.
fn slug(value: &str) -> String {
    let mut out = String::new();
    for c in value.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

/// `create/bible`: New Bible's Create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Bible {
    pub request: RequestId,
    pub details: Details,
    /// A standard versification's name; `eng` unless the person chose another.
    pub versification: String,
}

/// `create/obs`: New Open Bible Stories' Create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Obs {
    pub request: RequestId,
    pub details: Details,
}

/// The project a create made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub project: ProjectRef,
}

/// `create/addBlankBooks`: Add a book, "Start a blank book". Each book is seeded from the
/// project's pinned literal Bible (tC4 `seedInitialUsfm`), or from the book's own chapters
/// and verses when the project has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddBlankBooks {
    pub request: RequestId,
    pub project: ProjectRef,
    #[cfg_attr(
        feature = "proptest",
        proptest(strategy = "proptest::collection::vec(crate::arb::book(), 0..4)")
    )]
    pub books: Vec<BookId>,
}

/// The books a request added: those of the request the project did not hold already. tC4
/// skips a book the project holds, "fresh server truth wins".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct BooksAdded {
    #[cfg_attr(
        feature = "proptest",
        proptest(strategy = "proptest::collection::vec(crate::arb::book(), 0..4)")
    )]
    pub added: Vec<BookId>,
}

/// A USFM file the UI uploaded (`POST /api/v1/uploads`, flow `usfm`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsfmFile {
    pub upload: UploadId,
    /// The name the person's file had, for the row and for History.
    pub name: String,
}

/// `create/reviewUsfm`: what one uploaded file is, the moment it lands (tC4 `classifyUsfmFiles`'
/// damage checks, one call for each file so a finding stays with its file). Whether the book is
/// in the project already, or two files are one book, is the dialog's own check on the books.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewUsfm {
    pub file: UsfmFile,
}

/// What a file is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UsfmVerdict {
    /// A book of the Bible the file can add.
    Book {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
        book: BookId,
    },
    Refused(UsfmProblem),
}

/// Why a file cannot be added. The words are tC4's where it has them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UsfmProblem {
    /// No `\id` line that names a book of the Bible.
    NoId,
    /// The file is not UTF-8 text.
    NotText,
    /// The file does not cut into chapters and verses; `reason` says where.
    Damaged { reason: String },
    /// The project holds the book, or removed it: Import replaces a book.
    Exists {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
        book: BookId,
    },
    /// An earlier file of the request is the same book.
    TwoFiles {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
        book: BookId,
        first: String,
    },
}

impl UsfmProblem {
    /// The catalog key of the row's sentence.
    pub fn key(&self) -> &'static str {
        match self {
            Self::NoId => "importer.usfm.noId",
            Self::NotText => "importer.usfm.notText",
            Self::Damaged { .. } => "addBook.usfmDamaged",
            Self::Exists { .. } => "addBook.usfmExistsUpdate",
            Self::TwoFiles { .. } => "importer.usfm.twoFiles",
        }
    }

    /// The values of the sentence's placeholders, for the file named `file`.
    pub fn vars(&self, file: &str) -> Vec<(&'static str, String)> {
        match self {
            Self::NoId | Self::NotText => vec![("file", file.to_owned())],
            Self::Damaged { reason } => {
                vec![("file", file.to_owned()), ("reason", reason.clone())]
            }
            Self::Exists { book } => vec![("name", book.english_name().to_owned())],
            Self::TwoFiles { book, first } => vec![
                ("first", first.clone()),
                ("file", file.to_owned()),
                ("code", book.code().to_owned()),
            ],
        }
    }
}

/// The books that the files of one request claim, in order (tC4 `classifyUsfmFiles`): the first
/// file of a book keeps it and a later file of the same book is refused as the second; a book
/// the project holds is refused too. The dialog runs this on the verdicts of its files so a
/// row's words follow every add and remove, and the server runs it again on the request.
#[derive(Debug, Clone, Default)]
pub struct Claims(std::collections::BTreeMap<BookId, String>);

impl Claims {
    /// File `name` is the book `book`; `held` says whether the project holds the book.
    pub fn claim(&mut self, name: &str, book: BookId, held: bool) -> Result<BookId, UsfmProblem> {
        if let Some(first) = self.0.get(&book) {
            return Err(UsfmProblem::TwoFiles {
                book,
                first: first.clone(),
            });
        }
        self.0.insert(book, name.to_owned());
        if held {
            Err(UsfmProblem::Exists { book })
        } else {
            Ok(book)
        }
    }
}

/// `create/addUsfmBooks`: Add books from USFM files. Each file that can be added is one batch,
/// whole or absent (tC4 #767); the others are refused in their own rows and the rest still
/// add. An uploaded file is deleted once its row is answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddUsfmBooks {
    pub request: RequestId,
    pub project: ProjectRef,
    pub files: Vec<UsfmFile>,
}

/// One row of the answer, in the request's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct UsfmRow {
    pub name: String,
    pub outcome: UsfmOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UsfmOutcome {
    Added {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
        book: BookId,
    },
    Refused(UsfmProblem),
    /// The file was read and the store did not write it; nothing of it is in the project.
    NotAdded {
        reason: String,
    },
}

/// The answer of `create/addUsfmBooks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct UsfmAdded {
    pub rows: Vec<UsfmRow>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details(name: &str, code: &str) -> Details {
        Details {
            name: name.to_owned(),
            language_name: String::new(),
            code: code.to_owned(),
            door43_code: None,
            direction: Direction::Ltr,
            font: String::new(),
        }
    }

    #[test]
    fn the_first_file_of_a_book_keeps_it_and_a_held_book_is_refused() {
        let mut claims = Claims::default();
        let titus = BookId::Titus;
        assert_eq!(claims.claim("a.usfm", titus, false), Ok(titus));
        assert_eq!(
            claims.claim("b.usfm", titus, false),
            Err(UsfmProblem::TwoFiles {
                book: titus,
                first: "a.usfm".into()
            })
        );
        assert_eq!(
            claims.claim("c.usfm", BookId::Jude, true),
            Err(UsfmProblem::Exists { book: BookId::Jude })
        );
        // A file refused as held still claims its book, as tC4's `fileOf` does.
        assert_eq!(
            claims.claim("d.usfm", BookId::Jude, true),
            Err(UsfmProblem::TwoFiles {
                book: BookId::Jude,
                first: "c.usfm".into()
            })
        );
    }

    #[test]
    fn the_folder_is_tc4s_slug_of_the_name_else_of_the_code() {
        let folder = |name, code| details(name, code).folder().map(|f| f.as_str().to_owned());
        assert_eq!(folder("Kanuri Bible", "kr"), Ok("kanuri_bible".to_owned()));
        assert_eq!(folder("  --Åre 2!!  ", "x"), Ok("re_2".to_owned()));
        assert_eq!(folder("Библия", "x-ug"), Ok("x_ug".to_owned()));
        assert_eq!(folder("Библия", "кр"), Err(CreateRefusal::AbbrRequired));
        assert_eq!(folder("", "kr"), Err(CreateRefusal::NameRequired));
        assert_eq!(folder("Kanuri", " "), Err(CreateRefusal::CodeRequired));
    }
}
