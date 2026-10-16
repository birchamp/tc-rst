//! Where the fold finds a change's value (§14: values are byte ranges read on demand).
//!
//! The fold keeps a summary of each change (its key, links, batch and the shape of its
//! value) and reads the value itself only to merge, to list a conflict's candidates or to
//! project a record. A loader that reads packs from disk hands the fold where each change's
//! bytes lie ([`ValueAt`]) and a [`ValueSource`] that reads them again, so a whole-Bible
//! project does not keep half a gigabyte of change bytes in memory.

use crate::line::Span;
use std::fmt;
use std::sync::Arc;

/// Where a loader found a change's canonical JSON: a file it can read again, named by the
/// loader's own number, and the span of the JSON in it. The fold keeps it and hands it back
/// to its [`ValueSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueAt {
    pub offset: u64,
    pub file: u32,
    pub len: u32,
}

impl ValueAt {
    pub fn new(file: u32, span: Span) -> Self {
        Self {
            offset: span.offset,
            file,
            len: span.len,
        }
    }
}

/// Reads the values of changes whose bytes the fold does not keep. `tc-store` reads the pack
/// or pending file again, or the pack's blob in the object database when the file is gone or
/// damaged; a source that cannot read a value reports it as a notice of its own.
pub trait ValueSource: Send + Sync + fmt::Debug {
    /// The bytes at `at`, or `None` when they cannot be read. The fold checks them against
    /// the change's id, so a file changed since it was read never passes for the change.
    fn read(&self, at: ValueAt) -> Option<Vec<u8>>;
}

/// A change's bytes as the fold keeps them.
#[derive(Debug, Clone)]
pub(crate) enum Stored {
    /// In memory: a change added by a command, or a loader that keeps every value.
    Bytes(Arc<[u8]>),
    /// Read from the fold's source when needed.
    At(ValueAt),
}
