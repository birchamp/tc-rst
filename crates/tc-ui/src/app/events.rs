//! The event stream (APP-DESIGN 5.4, AD10): server-sent events read from a `fetch` body.
//!
//! [`SseParser`] turns the bytes of a connection into events, whatever size the chunks come
//! in. [`EventReader`] turns those into the frames the runtime applies, and holds the
//! position that a reconnect asks from: `?after=<seq>`. The server replays every frame after
//! it, or sends one `Resync` when it no longer holds them all.

use tc_api::event::{Event, EventFrame};
use tc_api::ids::ServerInstance;

/// One server-sent event (the WHATWG "interpreting an event stream").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// The last `id:` the stream set, which the spec keeps across events.
    pub id: Option<String>,
    /// The `data:` lines, joined by a line feed.
    pub data: String,
}

/// A line of the stream is longer than [`SseParser::MAX_LINE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a line of the event stream is over 16 MiB")]
pub struct LineTooLong;

/// Server-sent events from bytes in chunks of any size: a chunk may end inside a line, inside
/// a UTF-8 character, or between the CR and the LF of one line end.
///
/// Lines end at CRLF, LF or CR. A complete line is decoded as UTF-8, so a character split
/// across chunks is whole by then; bytes that are not UTF-8 become U+FFFD. One U+FEFF at the
/// start of the stream is dropped. An empty line ends an event, which is handed out only if a
/// `data` line came before it. A line that starts with `:` is a comment. `field: value` drops
/// one space after the colon, and a line with no colon is a field with an empty value. `data`
/// appends, `id` sets the last id unless the value holds NUL, and every other field is
/// ignored. The half-read event of a connection that ends is dropped: call [`reset`] when a
/// new connection starts.
///
/// [`reset`]: SseParser::reset
#[derive(Debug, Default)]
pub struct SseParser {
    line: Vec<u8>,
    /// The last byte was a CR, so an LF that comes next belongs to the same line end.
    after_cr: bool,
    past_first_line: bool,
    data: String,
    has_data: bool,
    last_id: Option<String>,
}

impl SseParser {
    /// The longest line it keeps. The server writes a frame as one line, and a frame is far
    /// smaller; the limit only bounds memory.
    pub const MAX_LINE: usize = 16 * 1024 * 1024;

    pub fn new() -> Self {
        Self::default()
    }

    /// The events that `chunk` completes. After an error the connection is over: call
    /// [`reset`](Self::reset) before reading another.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, LineTooLong> {
        let mut events = Vec::new();
        for &byte in chunk {
            if std::mem::take(&mut self.after_cr) && byte == b'\n' {
                continue;
            }
            if byte == b'\n' || byte == b'\r' {
                self.after_cr = byte == b'\r';
                let line = std::mem::take(&mut self.line);
                if let Some(event) = self.read_line(&line) {
                    events.push(event);
                }
                // Keep the buffer's room for the next line.
                self.line = line;
                self.line.clear();
            } else if self.line.len() >= Self::MAX_LINE {
                return Err(LineTooLong);
            } else {
                self.line.push(byte);
            }
        }
        Ok(events)
    }

    /// A new connection starts a new stream: the half-read line and event are dropped.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn read_line(&mut self, line: &[u8]) -> Option<SseEvent> {
        let decoded = String::from_utf8_lossy(line);
        let mut text: &str = &decoded;
        if !std::mem::replace(&mut self.past_first_line, true) {
            text = text.strip_prefix('\u{feff}').unwrap_or(text);
        }
        if text.is_empty() {
            return self.dispatch();
        }
        if text.starts_with(':') {
            return None;
        }
        let (field, value) = match text.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (text, ""),
        };
        match field {
            "data" => {
                self.data.push_str(value);
                self.data.push('\n');
                self.has_data = true;
            }
            "id" if !value.contains('\0') => self.last_id = Some(value.to_owned()),
            _ => {}
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        let had_data = std::mem::take(&mut self.has_data);
        let mut data = std::mem::take(&mut self.data);
        if !had_data {
            return None;
        }
        // Every data line ended with a line feed; the last one is not part of the data.
        data.pop();
        Some(SseEvent {
            id: self.last_id.clone(),
            data,
        })
    }
}

/// What the reader hands the runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Frame(EventFrame),
    /// An event that is not a frame this build knows. The runtime refreshes everything, as it
    /// does for `Resync`: a change it cannot read may be a teammate's, and must not be hidden.
    Unreadable {
        seq: Option<u64>,
        error: String,
    },
    /// A frame of another server run (APP-DESIGN 3.10). The runtime forgets every scope.
    Restart(ServerInstance),
}

/// The UI's position in the stream, and its rules for repeats.
///
/// - A frame is read once: one with `seq <= after` is a repeat and is skipped.
/// - A `Resync` is never skipped. It is numbered like the server's newest frame, which can be
///   lower than `after` (the server's count restarted below the reader's), and it sets
///   `after` to its own number.
/// - An event that does not parse is returned as [`Incoming::Unreadable`], and `after` moves
///   to its `id`, so the same event is not asked for again.
/// - A frame of another instance is a restart: the reader takes the new instance and `after`
///   starts again at that frame's number.
#[derive(Debug)]
pub struct EventReader {
    parser: SseParser,
    after: u64,
    instance: ServerInstance,
}

impl EventReader {
    /// `instance` is the server run `app/hello` named. Frames are read only after `hello`,
    /// whose snapshot is then no older than the first frame read (APP-DESIGN 6.3).
    pub fn new(instance: ServerInstance) -> Self {
        Self {
            parser: SseParser::new(),
            after: 0,
            instance,
        }
    }

    /// The `after` of the next connection: the number of the last frame read, 0 at first.
    pub fn after(&self) -> u64 {
        self.after
    }

    pub fn instance(&self) -> ServerInstance {
        self.instance
    }

    /// The frames that `chunk` completes, in order. An error ends the connection; frames
    /// already complete in this chunk are not returned and `after` has not moved past them,
    /// so the reconnect asks for them again.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Incoming>, LineTooLong> {
        let events = self.parser.push(chunk)?;
        Ok(events
            .into_iter()
            .filter_map(|event| self.read(event))
            .collect())
    }

    /// A new connection starts a new stream: the half-read line and event are dropped.
    pub fn reconnected(&mut self) {
        self.parser.reset();
    }

    fn read(&mut self, event: SseEvent) -> Option<Incoming> {
        let frame: EventFrame = match serde_json::from_str(&event.data) {
            Ok(frame) => frame,
            Err(error) => {
                let seq = event.id.as_deref().and_then(|id| id.parse::<u64>().ok());
                if let Some(seq) = seq {
                    self.after = self.after.max(seq);
                }
                return Some(Incoming::Unreadable {
                    seq,
                    error: error.to_string(),
                });
            }
        };
        if frame.instance != self.instance {
            self.instance = frame.instance;
            self.after = frame.seq;
            return Some(Incoming::Restart(frame.instance));
        }
        if matches!(frame.event, Event::Resync) {
            self.after = frame.seq;
            return Some(Incoming::Frame(frame));
        }
        if frame.seq <= self.after {
            return None;
        }
        self.after = frame.seq;
        Some(Incoming::Frame(frame))
    }
}
