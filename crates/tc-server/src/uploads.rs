//! Uploads (APP-DESIGN §5.5, AD26): `POST /api/v1/uploads` with one file as the raw request
//! body, streamed into `uploads/` in app data and registered once its last byte is stored.
//!
//! A partial upload is never complete: the bytes go to `uploads/<id>.part`, the file is
//! renamed to `uploads/<id>` after the last byte, and only then is it registered and
//! answered. A cap, a cut connection, a dropped handler or a full disk leaves no `.part`.
//! The person's name for the file is a label; it is never a path.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt as _;
use percent_encoding::percent_decode_str;
use tc_api::Reply;
use tc_api::error::ApiError;
use tc_api::ids::UploadId;
use tc_api::refusal::{Refusal, UploadRefusal};
use tc_api::upload::{UPLOAD_FLOW_HEADER, UPLOAD_NAME_HEADER, UploadFlow, UploadView};
use tokio::io::{AsyncWriteExt as _, BufWriter};

use crate::routes::{declared_length, error_reply, json_response, too_large};
use crate::state::Shared;

/// The longest name the server keeps for a file, in bytes of UTF-8: what file systems allow.
const MAX_NAME: usize = 255;

/// Bytes gathered before one write: hyper hands the body over in small pieces, and every
/// write is a task on the blocking pool.
const WRITE_BUFFER: usize = 1024 * 1024;

/// A file the UI uploaded and the server stored whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upload {
    pub id: UploadId,
    /// The name the person's file had. Never a path.
    pub name: String,
    pub size: u64,
    pub flow: UploadFlow,
    /// `uploads/<id>`.
    pub path: PathBuf,
}

/// The uploads of this run. Nothing survives a restart: `uploads/` is emptied at every
/// start, so an id only ever names a file this run stored.
///
/// An upload is Stored here from its last byte until one of three things happens:
/// - a job takes it ([`Uploads::take`]): it leaves this registry, so neither a drop nor a
///   sweep can delete the file the job reads, and the job deletes the file when done;
/// - the person's dialog has no more use for it ([`Uploads::drop_stored`], `uploads/drop`):
///   it closed, the row was removed, or another file took its place;
/// - it is still here after the keep time, because the page never said (it was closed, or
///   lost its connection): [`Uploads::sweep`] forgets it and the caller deletes the file.
pub struct Uploads {
    dir: PathBuf,
    keep: Duration,
    entries: Mutex<HashMap<UploadId, Stored>>,
}

struct Stored {
    upload: Upload,
    since: Instant,
}

impl Uploads {
    pub(crate) fn new(dir: PathBuf, keep: Duration) -> Self {
        Self {
            dir,
            keep,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<UploadId, Stored>> {
        // The map is only inserted into and removed from, so a panic elsewhere while the
        // lock was held cannot have left it half changed.
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The upload with this id, if this run stored one and nothing took or swept it.
    pub fn get(&self, id: &UploadId) -> Option<Upload> {
        self.entries().get(id).map(|stored| stored.upload.clone())
    }

    /// Hands the upload over: it is no longer registered, no sweep touches it, and the caller
    /// owns its file and deletes it when done.
    pub fn take(&self, id: &UploadId) -> Option<Upload> {
        self.entries().remove(id).map(|stored| stored.upload)
    }

    /// Deletes the upload when it is still Stored, and says whether it was. An upload a job
    /// took, or one swept or dropped already, is left alone: the job owns its file. Blocking:
    /// one unlink.
    pub fn drop_stored(&self, id: &UploadId) -> bool {
        let Some(stored) = self.entries().remove(id) else {
            return false;
        };
        // Already gone is as good as deleted; a file that cannot be removed now (held by a
        // virus scanner) is emptied at the next start.
        let _ = std::fs::remove_file(&stored.upload.path);
        true
    }

    /// Forgets the uploads stored for the keep time or longer at `now`, and returns their
    /// paths, which the caller deletes.
    pub fn sweep(&self, now: Instant) -> Vec<PathBuf> {
        let keep = self.keep;
        let mut expired = Vec::new();
        self.entries().retain(|_, stored| {
            let old = now.saturating_duration_since(stored.since) >= keep;
            if old {
                expired.push(stored.upload.path.clone());
            }
            !old
        });
        expired
    }

    fn register(&self, upload: Upload) {
        self.entries().insert(
            upload.id.clone(),
            Stored {
                upload,
                since: Instant::now(),
            },
        );
    }
}

/// Why a file name is refused.
pub(crate) enum NameError {
    /// The header is missing or repeated.
    Missing,
    NotText,
    BadEncoding,
    NotUtf8,
    Empty,
    TooLong,
    Control,
}

impl NameError {
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            Self::Missing => "the header is missing or repeated",
            Self::NotText => "the header is not visible ASCII",
            Self::BadEncoding => "the header is not valid percent-encoding",
            Self::NotUtf8 => "the name is not UTF-8",
            Self::Empty => "the name is empty",
            Self::TooLong => "the name is longer than 255 bytes",
            Self::Control => "the name holds a control character",
        }
    }
}

/// The checks every file name shares, upload or download.
pub(crate) fn check_name(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        Err(NameError::Empty)
    } else if name.len() > MAX_NAME {
        Err(NameError::TooLong)
    } else if name.chars().any(char::is_control) {
        Err(NameError::Control)
    } else {
        Ok(())
    }
}

/// The name in `X-Tc-Upload-Name`: percent-encoded UTF-8.
fn name_from(headers: &HeaderMap) -> Result<String, NameError> {
    let mut values = headers.get_all(UPLOAD_NAME_HEADER).iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        return Err(NameError::Missing);
    };
    let encoded = value.to_str().map_err(|_| NameError::NotText)?;
    // `percent_decode_str` passes a stray `%` through; this refuses it.
    let bytes = encoded.as_bytes();
    for (at, byte) in bytes.iter().enumerate() {
        let hex = |offset: usize| bytes.get(at + offset).is_some_and(u8::is_ascii_hexdigit);
        if *byte == b'%' && !(hex(1) && hex(2)) {
            return Err(NameError::BadEncoding);
        }
    }
    let name = percent_decode_str(encoded)
        .decode_utf8()
        .map_err(|_| NameError::NotUtf8)?
        .into_owned();
    check_name(&name)?;
    Ok(name)
}

/// 32 lowercase hex digits from the operating system's random source.
pub(crate) fn random_hex() -> Result<String, getrandom::Error> {
    use std::fmt::Write as _;
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)?;
    let mut hex = String::with_capacity(32);
    for byte in bytes {
        // Writing to a `String` cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

/// Renames `from` to `to`, trying again on `PermissionDenied`: on Windows a virus scanner can
/// hold a file that was just written for a moment (G0 ran with Defender on). Blocking.
pub(crate) fn rename_retrying(from: &Path, to: &Path) -> io::Result<()> {
    const TRIES: u32 = 10;
    let mut tried = 1;
    loop {
        match std::fs::rename(from, to) {
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied && tried < TRIES => {
                tried += 1;
                std::thread::sleep(Duration::from_millis(50));
            }
            other => return other,
        }
    }
}

/// An answer given before the body was read: the connection closes after it, because the
/// body is still on the wire.
fn early(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CONNECTION, HeaderValue::from_static("close"));
    response
}

fn bad_request(field: &str, reason: &str) -> Response {
    early(error_reply(
        StatusCode::OK,
        ApiError::BadRequest {
            field: field.to_owned(),
            reason: reason.to_owned(),
        },
    ))
}

fn not_written(reason: impl std::fmt::Display) -> Response {
    early(error_reply(
        StatusCode::OK,
        ApiError::Refused(Refusal::Upload(UploadRefusal::NotWritten {
            reason: reason.to_string(),
        })),
    ))
}

/// Deletes the `.part` file when dropped, unless the rename disarmed it. hyper drops a
/// handler's future when its connection dies, so cleanup on the error path alone would leave
/// files behind.
struct PartFile {
    path: PathBuf,
    armed: bool,
}

impl Drop for PartFile {
    fn drop(&mut self) {
        if self.armed {
            // One unlink: short enough to run on the async side.
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// `POST /api/v1/uploads`.
pub(crate) async fn upload(State(shared): State<Arc<Shared>>, request: Request) -> Response {
    // Order matters: each early answer comes before any of the body is stored.
    let headers = request.headers();
    let mut flows = headers.get_all(UPLOAD_FLOW_HEADER).iter();
    let flow = match (flows.next(), flows.next()) {
        (Some(value), None) => value.to_str().ok().and_then(UploadFlow::from_header),
        _ => None,
    };
    let Some(flow) = flow else {
        return bad_request(
            UPLOAD_FLOW_HEADER,
            "the header must name exactly one flow: exchange, import, usfm or guidedFix",
        );
    };
    let cap = (shared.limits.upload_cap)(flow);
    if declared_length(&request).is_some_and(|length| length > cap) {
        return too_large(request.into_body().into_data_stream());
    }
    let name = match name_from(headers) {
        Ok(name) => name,
        Err(error) => return bad_request(UPLOAD_NAME_HEADER, error.reason()),
    };

    let Ok(id) = random_hex() else {
        return not_written("the operating system gave no random bytes");
    };
    let Ok(id) = UploadId::parse(&id) else {
        return not_written("the upload id was not valid");
    };
    let dir = shared.uploads.dir.clone();
    let mut part = PartFile {
        path: dir.join(format!("{}.part", id.as_str())),
        armed: false,
    };
    let file = match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part.path)
        .await
    {
        Ok(file) => {
            part.armed = true;
            file
        }
        Err(error) => return not_written(error),
    };

    // No fsync: `uploads/` is emptied at every start, so a crash never reads a partial
    // upload back, and an fsync of 4 GiB would cost seconds.
    let mut writer = BufWriter::with_capacity(WRITE_BUFFER, file);
    let mut size: u64 = 0;
    let mut body = request.into_body().into_data_stream();
    while let Some(chunk) = body.next().await {
        // The client went away or sent a broken body: nobody is waiting for an answer.
        let Ok(chunk) = chunk else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        size += chunk.len() as u64;
        // A chunked body has no length to check first.
        if size > cap {
            return too_large(body);
        }
        if let Err(error) = writer.write_all(&chunk).await {
            return not_written(error);
        }
    }
    if let Err(error) = writer.flush().await {
        return not_written(error);
    }
    drop(writer);

    let path = dir.join(id.as_str());
    let (from, to) = (part.path.clone(), path.clone());
    match tokio::task::spawn_blocking(move || rename_retrying(&from, &to)).await {
        Ok(Ok(())) => part.armed = false,
        Ok(Err(error)) => return not_written(error),
        Err(error) => return not_written(error),
    }
    shared.uploads.register(Upload {
        id: id.clone(),
        name: name.clone(),
        size,
        flow,
        path,
    });
    let view = Reply::Ok(UploadView { id, name, size });
    match serde_json::to_vec(&view) {
        Ok(body) => json_response(StatusCode::OK, body),
        Err(error) => not_written(error),
    }
}
