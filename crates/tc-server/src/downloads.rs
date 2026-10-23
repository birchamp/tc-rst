//! Finished files the UI can fetch (APP-DESIGN §5.5): an export's result is registered here
//! and fetched with `GET /download/{id}/{name}` by an `<a download>`. A file stays ten
//! minutes and may be fetched more than once in that time.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::stream;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use tc_api::ids::DownloadId;
use tokio::io::AsyncReadExt as _;

use crate::state::Shared;
use crate::uploads::{check_name, random_hex, rename_retrying};

/// Bytes read from the file for one piece of the body.
const CHUNK: usize = 256 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    /// The name is empty, too long, holds a control character, a `/` or a `\`.
    #[error("not a file name for a download: {0}")]
    BadName(String),
    #[error("the operating system gave no random bytes")]
    Random,
    #[error("the finished file could not be registered: {0}")]
    Io(#[from] io::Error),
}

struct Entry {
    name: String,
    content_type: &'static str,
    path: PathBuf,
    size: u64,
    added: Instant,
}

/// The finished files of this run.
pub struct Downloads {
    dir: PathBuf,
    keep: Duration,
    entries: Mutex<HashMap<DownloadId, Entry>>,
}

impl Downloads {
    pub(crate) fn new(dir: PathBuf, keep: Duration) -> Self {
        Self {
            dir,
            keep,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<DownloadId, Entry>> {
        // The map is only inserted into and removed from, so a panic elsewhere while the
        // lock was held cannot have left it half changed.
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Where a producer writes its file first, so [`Downloads::add`] is a rename on one
    /// volume.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Registers the finished file `written`, which lives in [`Downloads::dir`], under a new
    /// id. `file_name` is what the person's browser saves it as. Blocking: call it from a
    /// job's blocking thread.
    pub fn add(
        &self,
        written: &Path,
        file_name: &str,
        content_type: &'static str,
    ) -> Result<DownloadId, DownloadError> {
        check_name(file_name).map_err(|error| DownloadError::BadName(error.reason().to_owned()))?;
        if file_name.contains(['/', '\\']) {
            return Err(DownloadError::BadName(
                "the name holds a path separator".to_owned(),
            ));
        }
        let id = random_hex().map_err(|_| DownloadError::Random)?;
        let id = DownloadId::parse(&id).map_err(|_| DownloadError::Random)?;
        let path = self.dir.join(id.as_str());
        rename_retrying(written, &path)?;
        let size = std::fs::metadata(&path)?.len();
        self.entries().insert(
            id.clone(),
            Entry {
                name: file_name.to_owned(),
                content_type,
                path,
                size,
                added: Instant::now(),
            },
        );
        Ok(id)
    }

    /// Forgets the files older than the keep time at `now` and returns their paths, which the
    /// caller deletes.
    pub fn sweep(&self, now: Instant) -> Vec<PathBuf> {
        let keep = self.keep;
        let mut expired = Vec::new();
        self.entries().retain(|_, entry| {
            let old = now.saturating_duration_since(entry.added) >= keep;
            if old {
                expired.push(entry.path.clone());
            }
            !old
        });
        expired
    }

    fn find(&self, id: &DownloadId, name: &str) -> Option<(PathBuf, u64, &'static str)> {
        let entries = self.entries();
        let entry = entries.get(id)?;
        (entry.name == name).then(|| (entry.path.clone(), entry.size, entry.content_type))
    }
}

/// The `filename` parameter for clients that read only ASCII: every character that is not
/// plain ASCII, or would end the quoted string or start an escape, becomes `_`.
fn ascii_fallback(name: &str) -> String {
    name.chars()
        .map(|c| {
            if (c.is_ascii_graphic() || c == ' ') && !matches!(c, '"' | '\\' | '%' | ';') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn content_disposition(name: &str) -> Option<HeaderValue> {
    HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        ascii_fallback(name),
        utf8_percent_encode(name, NON_ALPHANUMERIC)
    ))
    .ok()
}

/// `GET /download/{id}/{name}`.
pub(crate) async fn download(
    State(shared): State<Arc<Shared>>,
    UrlPath((id, name)): UrlPath<(String, String)>,
) -> Response {
    let Ok(id) = DownloadId::parse(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some((path, size, content_type)) = shared.downloads.find(&id, &name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (Ok(file), Some(disposition)) = (
        tokio::fs::File::open(&path).await,
        content_disposition(&name),
    ) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // Piece by piece, so a book's PDF never sits in memory whole. The length is the one
    // registered: a file that changed afterwards ends the body early and the client sees an
    // error rather than a wrong file.
    let body = Body::from_stream(stream::unfold(
        (file, size),
        |(mut file, left)| async move {
            if left == 0 {
                return None;
            }
            let want = usize::try_from(left).map_or(CHUNK, |left| left.min(CHUNK));
            let mut piece = vec![0_u8; want];
            match file.read(&mut piece).await {
                // The file is shorter than registered, or unreadable: end with the error.
                Ok(0) => Some((
                    Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
                    (file, 0),
                )),
                Ok(n) => {
                    piece.truncate(n);
                    Some((
                        Ok(Bytes::from(piece)),
                        (file, left.saturating_sub(n as u64)),
                    ))
                }
                Err(error) => Some((Err(error), (file, 0))),
            }
        },
    ));
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(size));
    headers.insert(header::CONTENT_DISPOSITION, disposition);
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
