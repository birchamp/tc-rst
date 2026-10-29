//! Counting a zip's central-directory records. The `zip` crate keeps one entry per name,
//! so a zip that names one file twice reads as one entry: the second copy vanishes, or
//! replaces the first. A reader compares this count with the entries it was given and
//! refuses the zip when they differ.

use std::io::{ErrorKind, Read, Seek, SeekFrom};

const CENTRAL_HEADER: [u8; 4] = *b"PK\x01\x02";

/// The number of central-directory records that start at `start`, the offset the zip
/// reader reports, counting no further than `at_most`.
pub fn central_directory_records(
    reader: &mut (impl Read + Seek),
    start: u64,
    at_most: usize,
) -> std::io::Result<usize> {
    let mut at = start;
    let mut count = 0;
    let mut header = [0u8; 46];
    while count < at_most {
        reader.seek(SeekFrom::Start(at))?;
        match reader.read_exact(&mut header) {
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
            other => other?,
        }
        if header[..4] != CENTRAL_HEADER {
            break;
        }
        let field =
            |offset: usize| u64::from(u16::from_le_bytes([header[offset], header[offset + 1]]));
        // The name, extra field and comment follow the fixed 46 bytes.
        at += 46 + field(28) + field(30) + field(32);
        count += 1;
    }
    Ok(count)
}
