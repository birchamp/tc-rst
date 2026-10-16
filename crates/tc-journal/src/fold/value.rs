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
    /// the fingerprint it took of the change's verified bytes, so a file changed since it
    /// was read never passes for the change.
    fn read(&self, at: ValueAt) -> Option<Vec<u8>>;
}

/// A fingerprint of a change's canonical bytes, taken when they were verified against the
/// change's id and checked whenever they are read again (§2.6). Each step maps the state
/// one to one, so bytes that differ in a single 8-byte word always differ in it, and other
/// changes match with a chance of about 2⁻⁶⁴. SHA-256 would also prove the id, but the
/// bytes were proved when they were read; computing it again for every value a projection
/// reads was a third of the projection's work without SHA instructions (§14).
pub(crate) fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = super::cow::FxHasher::default();
    hasher.write(bytes);
    hasher.write_usize(bytes.len());
    hasher.finish()
}

/// A change's bytes as the fold keeps them: 24 bytes for each of hundreds of thousands of
/// changes.
#[derive(Debug, Clone)]
pub(crate) enum Stored {
    /// In memory: a change added by a command, or a loader that keeps every value.
    Bytes(Box<Arc<[u8]>>),
    /// Read from the fold's source when needed, and checked against the [`fingerprint`] of
    /// the bytes the loader verified.
    At {
        file: u32,
        offset: u32,
        len: u32,
        fingerprint: u64,
    },
}

impl Stored {
    pub(crate) fn bytes(bytes: Arc<[u8]>) -> Self {
        Self::Bytes(Box::new(bytes))
    }

    /// Where `at` says, for the verified `bytes` found there, when its offset fits in 32
    /// bits (a pack holds at most 64 MiB; only a pending file of more than 4 GiB does not
    /// fit, and its values stay in memory).
    pub(crate) fn located(at: ValueAt, bytes: &[u8]) -> Option<Self> {
        Some(Self::At {
            file: at.file,
            offset: u32::try_from(at.offset).ok()?,
            len: at.len,
            fingerprint: fingerprint(bytes),
        })
    }
}
