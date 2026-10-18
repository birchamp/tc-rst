use crate::json::StrictJsonError;
use crate::path::PathRule;

/// Why `metadata.json` could not be read into the model or written from it.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("metadata.json is not JSON: {0}")]
    Json(#[source] serde_json::Error),
    /// A JSON parser keeps only one of two equal keys, so reading on would lose the other.
    #[error("metadata.json has the key at {pointer:?} more than once")]
    DuplicateKey { pointer: String },
    /// The number cannot be held exactly (`1e400`, or an integer with more digits than an
    /// f64 keeps), so writing it back would change it.
    #[error("the number at {pointer:?} cannot be kept exactly, so writing it back would change it")]
    UnrepresentableNumber { pointer: String },
    #[error("the string at {pointer:?} escapes half of a UTF-16 surrogate pair, which is not text")]
    LoneSurrogate { pointer: String },
    #[error("metadata.json nests deeper than {max} levels at {pointer:?}", max = crate::json::MAX_DEPTH)]
    TooDeep { pointer: String },
    #[error("metadata.json does not fit the burrito model: {0}")]
    Model(#[source] serde_json::Error),
    /// An `extra` map holds a key its struct models, such as `size` in an ingredient's
    /// `extra`. Writing would let one value silently replace the other, so nothing is
    /// written.
    #[error("the extra fields at {pointer:?} repeat a field the model already writes")]
    ShadowedField { pointer: String },
}

impl From<StrictJsonError> for Error {
    fn from(error: StrictJsonError) -> Self {
        match error {
            StrictJsonError::Syntax(error) => Self::Json(error),
            StrictJsonError::DuplicateKey { pointer } => Self::DuplicateKey { pointer },
            StrictJsonError::UnrepresentableNumber { pointer } => {
                Self::UnrepresentableNumber { pointer }
            }
            StrictJsonError::LoneSurrogate { pointer } => Self::LoneSurrogate { pointer },
            StrictJsonError::TooDeep { pointer } => Self::TooDeep { pointer },
        }
    }
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
    #[error("{0:?} is not a book code the burrito schema accepts")]
    Book(String),
    #[error("{0:?} is not a scope range such as 3, 3-5, 3:1, 3:1-9 or 3:1-4:2")]
    ScopeRange(String),
}
