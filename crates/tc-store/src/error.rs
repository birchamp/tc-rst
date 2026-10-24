//! The store's I/O error: what failed, on which path, and why.

use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not {op} {}: {source}", path.display())]
    Io {
        op: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{0}")]
    Other(String),
}

impl StoreError {
    pub fn io(op: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            op,
            path: path.to_owned(),
            source,
        }
    }

    /// The I/O error, if this is an I/O error.
    pub fn io_error(&self) -> Option<&io::Error> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Other(_) => None,
        }
    }

    /// The I/O error kind, if this is an I/O error.
    pub fn kind(&self) -> Option<io::ErrorKind> {
        match self {
            Self::Io { source, .. } => Some(source.kind()),
            Self::Other(_) => None,
        }
    }
}

/// Adds what was being done to an I/O result.
pub(crate) trait IoContext<T> {
    fn at(self, op: &'static str, path: &Path) -> Result<T, StoreError>;
}

impl<T> IoContext<T> for io::Result<T> {
    fn at(self, op: &'static str, path: &Path) -> Result<T, StoreError> {
        self.map_err(|e| StoreError::io(op, path, e))
    }
}
