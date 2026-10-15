use crate::path::PathRule;

/// Why `metadata.json` could not be read into the model or written from it.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("metadata.json is not JSON: {0}")]
    Json(#[source] serde_json::Error),
    /// A JSON parser keeps only one of two equal keys, so reading on would lose the other.
    #[error("metadata.json has the key at {pointer:?} more than once")]
    DuplicateKey { pointer: String },
    #[error("metadata.json does not fit the burrito model: {0}")]
    Model(#[source] serde_json::Error),
}

/// Why a string is not a valid value for one of the strict burrito types.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValueError {
    #[error("ingredient path {path:?}: {rule}")]
    Path { path: String, rule: PathRule },
    #[error(
        "{0:?} is not a media type the burrito schema accepts (lowercase type/subtype, no dots, no parameters)"
    )]
    MimeType(String),
    #[error("{0:?} is not a lowercase hexadecimal md5 digest")]
    Md5(String),
    #[error("{0:?} is not a book code the burrito schema accepts")]
    Book(String),
    #[error("{0:?} is not a scope range such as 3, 3-5, 3:1, 3:1-9 or 3:1-4:2")]
    ScopeRange(String),
}
