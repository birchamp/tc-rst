//! The change line (`docs/STORAGE.md` §4.1):
//!
//! ```text
//! <canonical change JSON> TAB <64 lowercase hex: sha256 of the JSON bytes> LF
//! ```
//!
//! Canonical JSON never holds a raw TAB or LF, so the split is unambiguous, and the hash on
//! the line is the change id, so each line verifies on its own. Pending files and packs use
//! the same framing, including for their header line.

use crate::change::{Verified, parse_change};
use crate::ids::ChangeId;
use crate::text::{hex, sha256, unhex};
use std::sync::Arc;

/// The bytes a line adds to its JSON: a tab, 64 hex digits and a line feed.
pub const LINE_OVERHEAD: usize = 1 + 64 + 1;

/// The framed line for canonical JSON `raw`, whose id is `id`.
pub fn encode_line(id: ChangeId, raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + LINE_OVERHEAD);
    out.extend_from_slice(raw);
    out.push(b'\t');
    out.extend_from_slice(id.to_hex().as_bytes());
    out.push(b'\n');
    out
}

/// Frames any canonical JSON (a header) with its own hash.
pub(crate) fn frame(json: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(json.len() + LINE_OVERHEAD);
    out.extend_from_slice(json);
    out.push(b'\t');
    out.extend_from_slice(hex(&sha256(json)).as_bytes());
    out.push(b'\n');
    out
}

/// Splits a line (without its line feed) at its last tab and checks the hash. Returns the
/// id and the JSON bytes.
pub fn verify_line(line: &[u8]) -> Option<(ChangeId, &[u8])> {
    let tab = line.iter().rposition(|b| *b == b'\t')?;
    let (json, hash) = (&line[..tab], &line[tab + 1..]);
    let hash = unhex::<32>(std::str::from_utf8(hash).ok()?)?;
    (sha256(json) == hash).then(|| (ChangeId::from_bytes(hash), json))
}

/// [`verify_line`] for salvage: a line whose line feed became CR LF (a copy through a tool
/// that rewrites line endings) still verifies. Canonical JSON never holds a raw CR, so the
/// CR can only be the line ending's, and the JSON is untouched. Validation (P3, P4) stays
/// strict, so such a file is still damaged.
pub(crate) fn verify_salvaged(line: &[u8]) -> Option<(ChangeId, &[u8])> {
    verify_line(line).or_else(|| verify_line(line.strip_suffix(b"\r")?))
}

/// Verifies a line (without its line feed) and reads its change. `None` when the line does
/// not verify: those bytes are damaged, not a change.
pub fn read_line(line: &[u8]) -> Option<Verified> {
    let (id, json) = verify_line(line)?;
    Some(parse_change(id, Arc::from(json)))
}

/// One line of a file and whether a line feed ended it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RawLine<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) terminated: bool,
}

/// The lines of a file. An empty file has no lines; bytes after the last line feed form a
/// last, unterminated line.
pub(crate) fn split_lines(bytes: &[u8]) -> Vec<RawLine<'_>> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        match rest.iter().position(|b| *b == b'\n') {
            Some(at) => {
                out.push(RawLine {
                    bytes: &rest[..at],
                    terminated: true,
                });
                rest = &rest[at + 1..];
            }
            None => {
                out.push(RawLine {
                    bytes: rest,
                    terminated: false,
                });
                rest = &[];
            }
        }
    }
    out
}
