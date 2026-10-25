//! Importing a project from another tool into the records a new project starts with
//! (`docs/STORAGE.md` §13). Pure: it reads the bytes it is given and returns records; the
//! store writes them whole or not at all (`tc_store::Store::create`), and the app asks the
//! questions (name, license, resource versions).
//!
//! Built for the first release: translationCore 3 projects ([`tc3`]). tC4 4.0 projects, raw
//! USFM and foreign Scripture Burritos come later (rulings/2026-10-24-H3-H4-H7-fast-release.md).
//! "Update a book from USFM" changes an existing project instead, and lives with the outside-edit
//! planner it reuses (`tc_journal::fold::FoldState::plan_update_book`).
//!
//! It adds no reader of its own: USFM goes through `tc_usfm::codec`, alignment records through
//! `tc_journal::fold::decode_alignment_record` and decisions through
//! `tc_journal::fold::decode_decisions`, the readers DOM-2 left one per job.

pub mod tc3;

/// One file the person chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Why an import makes no project. Each has the words the person sees and, where tC4 has
/// one, its `import.damaged.*` code (fixtures/tc4/conformance-fixtures/import/MANIFEST.json).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportRefusal {
    #[error("{file} is not a complete zip file")]
    Truncated { file: String },
    #[error("{file}: the part {entry} is damaged")]
    CorruptEntry { file: String, entry: String },
    #[error("{file} is not a translationCore 3 project: it has no manifest.json")]
    NoManifest { file: String },
    #[error("{file} holds no text of a book")]
    NoText { file: String },
    #[error("the projects are in different languages: {}", .0.join(", "))]
    Languages(Vec<String>),
    #[error("{book} is in two of the projects")]
    BookTwice { book: String },
    #[error("{file}: {problem}")]
    Unreadable { file: String, problem: String },
    /// The imported text would not be the source's, byte for byte (P9): nothing is made.
    #[error("{book}: the text would change in the import, so nothing was imported")]
    TextChanged { book: String },
}

impl ImportRefusal {
    /// tC4's code for the refusal, where it has one.
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::Truncated { .. } => Some("import.damaged.truncated"),
            Self::CorruptEntry { .. } => Some("import.damaged.corrupt-entry"),
            Self::NoManifest { .. } => Some("import.damaged.no-manifest"),
            _ => None,
        }
    }
}
