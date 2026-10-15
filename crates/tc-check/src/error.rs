/// Every way this crate refuses input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error(
        "invalid check id {0:?}: a check id is non-empty NFC text without `|` or `:` (STORAGE.md §2.2)"
    )]
    CheckId(String),

    #[error("invalid reference part {0:?}: a chapter or verse is non-empty text without `:`")]
    RefPart(String),

    #[error("invalid md5 {0:?}: expected 32 hexadecimal digits")]
    Md5(String),

    #[error(
        "invalid pin {0}: a pin needs a repoPath, a 40-digit sha and, if any, a list of book codes"
    )]
    Pin(String),

    #[error("invalid repository path {0:?}")]
    RepoPath(String),

    #[error("word {index} is not a word of the verse, which has {count} words")]
    WordIndex { index: usize, count: usize },

    #[error("unknown tool {0:?}")]
    Tool(String),
}
