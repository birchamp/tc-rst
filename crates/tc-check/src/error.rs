/// Every way this crate refuses input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A check id or reference part that a check key cannot hold (STORAGE.md §2.2).
    #[error(transparent)]
    Identity(#[from] tc_journal::keys::IdentityError),

    #[error("word {index} is not a word of the verse, which has {count} words")]
    WordIndex { index: usize, count: usize },
}
