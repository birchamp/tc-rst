//! Ending the process at a named point, for the reload and relaunch tests of APP-DESIGN §6.6
//! (AD35, feature `test-hooks`). A release build has none of this.

use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use http_body::{Frame, SizeHint};
use tc_api::test::CrashPoint;

/// The exit code of a test crash, which the process tests look for. tc-app's own crash points
/// use the same code (A3); the program crate asserts it when the two are joined.
pub const CRASH_EXIT_CODE: i32 = 86;

/// How a crash ends the process. A test substitutes [`RecordingEnder`], which ends nothing.
pub trait Ender: Send + Sync + 'static {
    /// A crash before the reply: ends the process now. Returns only in tests.
    fn end_now(&self, at: CrashPoint);

    /// A crash after the reply: ends the process once the reply's body was handed to the
    /// socket. Returns at once; the process ends a moment later.
    fn end_after_reply(&self);
}

/// Ends the process with [`CRASH_EXIT_CODE`], as a crash would.
pub struct ExitProcess;

impl Ender for ExitProcess {
    fn end_now(&self, _at: CrashPoint) {
        std::process::exit(CRASH_EXIT_CODE);
    }

    fn end_after_reply(&self) {
        // The body was handed to the connection, which writes it on a task of its own;
        // on loopback it is on the wire well within this moment.
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(100));
            std::process::exit(CRASH_EXIT_CODE);
        });
    }
}

/// What a [`RecordingEnder`] was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Now(CrashPoint),
    AfterReply,
}

/// Records each call with the time it was made and ends nothing.
#[derive(Default)]
pub struct RecordingEnder {
    calls: Mutex<Vec<(Ended, Instant)>>,
}

impl RecordingEnder {
    pub fn calls(&self) -> Vec<(Ended, Instant)> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn record(&self, ended: Ended) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((ended, Instant::now()));
    }
}

impl Ender for RecordingEnder {
    fn end_now(&self, at: CrashPoint) {
        self.record(Ended::Now(at));
    }

    fn end_after_reply(&self) {
        self.record(Ended::AfterReply);
    }
}

/// The size of the pieces [`EndAfterBody`] hands to hyper.
const PIECE: usize = 64 * 1024;

/// A response body that asks to end the process when hyper is done with it.
///
/// It must be `Drop`: hyper takes the last frame of a sized body, sees `is_end_stream()` and
/// drops the body without polling it to `None`, so a hook on the end of `poll_frame` would
/// never fire. Hyper takes a frame whole into its write buffer, so a reply in one big frame
/// would be dropped long before the socket had it; the body is therefore handed over in
/// pieces, and hyper stops asking while its buffer is full. The drop then comes when all but
/// the last few hundred kilobytes are with the socket, which [`ExitProcess`]'s wait covers.
pub(crate) struct EndAfterBody {
    inner: Body,
    /// What the last frame of `inner` held beyond the piece already handed over.
    rest: Bytes,
    ender: Option<Arc<dyn Ender>>,
}

impl EndAfterBody {
    pub(crate) fn new(inner: Body, ender: Arc<dyn Ender>) -> Self {
        Self {
            inner,
            rest: Bytes::new(),
            ender: Some(ender),
        }
    }
}

impl http_body::Body for EndAfterBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, axum::Error>>> {
        let this = &mut *self;
        if !this.rest.is_empty() {
            let piece = this.rest.split_to(this.rest.len().min(PIECE));
            return Poll::Ready(Some(Ok(Frame::data(piece))));
        }
        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                Ok(mut data) => {
                    let piece = data.split_to(data.len().min(PIECE));
                    this.rest = data;
                    Poll::Ready(Some(Ok(Frame::data(piece))))
                }
                Err(other) => Poll::Ready(Some(Ok(other))),
            },
            other => other,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.rest.is_empty() && self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        let inner = self.inner.size_hint();
        let rest = self.rest.len() as u64;
        let mut hint = SizeHint::new();
        hint.set_lower(inner.lower() + rest);
        if let Some(upper) = inner.upper() {
            hint.set_upper(upper + rest);
        }
        hint
    }
}

impl Drop for EndAfterBody {
    fn drop(&mut self) {
        if let Some(ender) = self.ender.take() {
            ender.end_after_reply();
        }
    }
}
