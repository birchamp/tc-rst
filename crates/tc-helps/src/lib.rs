//! Translation helps for tC: notes, words, word links, academy articles and questions,
//! from Door43 (DCS) or from Aquifer, read into one model.
//!
//! - [`tsv`] reads and writes the Door43 tables (notes, words links, questions) for Bible
//!   books and Open Bible Stories.
//! - [`articles`] reads translation words and Translation Academy articles, and the
//!   academy's `toc.yaml` and `config.yaml`.
//! - [`obs`] reads Open Bible Stories story files.
//! - [`metadata`] identifies a downloaded resource and holds the pins of a helps source.
//! - [`aquifer`] reads Aquifer release JSON and converts it into the same model.
//! - [`links`] resolves `rc://`, relative and `aquifer:` links to typed targets.
//! - [`markdown`] describes the markdown dialect and converts Aquifer HTML into it.
//! - [`highlight`] finds the aligned words of a verse that render a quote.
//! - [`Helps`] holds one source's helps and answers per-passage questions.

pub mod aquifer;
pub mod articles;
mod collection;
mod error;
pub mod highlight;
mod html;
mod ids;
pub mod links;
pub mod markdown;
pub mod metadata;
mod model;
pub mod obs;
mod refs;
pub mod tsv;

pub use collection::{Article, Helps};
pub use error::{
    CellProblem, Error, JsonFile, LinkProblem, PinMismatch, StoryProblem, TsvTable, WriteProblem,
    YamlFile,
};
pub use ids::{
    AcademyArticleId, AquiferContentId, CommitSha, HelpItemId, Manual, Occurrence, Quote, RowId,
    Sha256Digest, Slug, WordArticleId, WordCategory,
};
pub use model::{
    AcademyArticle, CheckBlocker, CheckReadiness, NotACheck, Note, Question, WordArticle, WordLink,
};
pub use refs::{BibleLocus, BibleRef, ChapterVerse, HelpRef, ObsRef, VerseList, VerseRange};
