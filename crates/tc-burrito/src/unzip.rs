//! Reading a whole zip into memory under caps, for every zip a person hands the app: an
//! exchange file (`docs/STORAGE.md` §10.2 R1) and a tC3 project. A zip bomb, or a large zip
//! picked by mistake, is refused before it can run the app out of memory.

use std::io::{Read, Seek};

/// How much one zip may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZipLimits {
    pub entries: usize,
    /// Bytes of one entry, unpacked.
    pub entry: u64,
    /// Bytes of all entries, unpacked.
    pub total: u64,
}

/// R1's caps.
pub const ZIP_LIMITS: ZipLimits = ZipLimits {
    entries: 200_000,
    entry: 512 << 20,
    total: 4 << 30,
};

/// Why a zip was not read.
#[derive(Debug, thiserror::Error)]
pub enum ZipReadError {
    #[error("not a readable zip: {0}")]
    NotAZip(#[source] zip::result::ZipError),
    #[error("{count} entries")]
    TooManyEntries { count: usize },
    #[error("{name} is too large")]
    EntryTooLarge { name: String },
    #[error("the file is too large")]
    TooLarge,
}

/// One file entry of a zip.
#[derive(Debug)]
pub struct ZipEntry {
    /// The name as the zip writes it.
    pub name: String,
    /// The zip crate's check that the name stays inside the folder it unpacks to.
    pub enclosed: bool,
    pub unix_mode: Option<u32>,
    /// The bytes, or why they did not inflate to the entry's CRC-32.
    pub bytes: std::io::Result<Vec<u8>>,
}

/// The file entries of the zip in `reader`, in the zip's order; folders are left out.
pub fn read_zip(
    reader: impl Read + Seek,
    limits: ZipLimits,
) -> Result<Vec<ZipEntry>, ZipReadError> {
    let mut zip = zip::ZipArchive::new(reader).map_err(ZipReadError::NotAZip)?;
    if zip.len() > limits.entries {
        return Err(ZipReadError::TooManyEntries { count: zip.len() });
    }
    let mut out = Vec::new();
    let mut total = 0u64;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(ZipReadError::NotAZip)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let budget = limits.entry.min(limits.total - total);
        let too_large = |name: String| {
            if budget < limits.entry {
                ZipReadError::TooLarge
            } else {
                ZipReadError::EntryTooLarge { name }
            }
        };
        // The declared size refuses early; the read below holds to the budget, since a zip
        // can declare less than it inflates to.
        if entry.size() > budget {
            return Err(too_large(name));
        }
        let mut bytes = Vec::new();
        // The zip crate checks the CRC when the entry is read to its end.
        let read = entry.by_ref().take(budget + 1).read_to_end(&mut bytes);
        let len = bytes.len() as u64;
        if len > budget {
            return Err(too_large(name));
        }
        total += len;
        out.push(ZipEntry {
            enclosed: entry.enclosed_name().is_some(),
            unix_mode: entry.unix_mode(),
            bytes: read.map(|_| bytes),
            name,
        });
    }
    Ok(out)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "test code: a failed unwrap is a failed test"
)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    const SMALL: ZipLimits = ZipLimits {
        entries: 3,
        entry: 100,
        total: 150,
    };

    fn read(bytes: Vec<u8>) -> Result<Vec<ZipEntry>, ZipReadError> {
        read_zip(Cursor::new(bytes), SMALL)
    }

    #[test]
    fn entries_within_the_limits_read_whole() {
        let entries = read(zip_of(&[("a", &[1; 100]), ("b/c", &[2; 50])])).unwrap();
        let read: Vec<(&str, usize)> = entries
            .iter()
            .map(|e| (e.name.as_str(), e.bytes.as_ref().unwrap().len()))
            .collect();
        assert_eq!(read, [("a", 100), ("b/c", 50)]);
    }

    #[test]
    fn too_many_entries_are_refused() {
        let zip = zip_of(&[("a", b""), ("b", b""), ("c", b""), ("d", b"")]);
        assert!(matches!(
            read(zip),
            Err(ZipReadError::TooManyEntries { count: 4 })
        ));
    }

    #[test]
    fn a_large_entry_is_refused() {
        let zip = zip_of(&[("big", &[0; 101])]);
        assert!(matches!(read(zip), Err(ZipReadError::EntryTooLarge { name }) if name == "big"));
    }

    #[test]
    fn entries_that_add_up_past_the_total_are_refused() {
        let zip = zip_of(&[("a", &[0; 100]), ("b", &[0; 51])]);
        assert!(matches!(read(zip), Err(ZipReadError::TooLarge)));
    }

    /// A zip bomb declares a small size: the bytes actually inflated are what count.
    #[test]
    fn an_entry_that_declares_less_than_it_holds_is_refused() {
        let mut zip = zip_of(&[("bomb", &[0; 1000])]);
        let declare_ten = |zip: &mut Vec<u8>, signature: [u8; 4], offset: usize| {
            let at = zip.windows(4).position(|w| w == signature).unwrap() + offset;
            zip[at..at + 4].copy_from_slice(&10u32.to_le_bytes());
        };
        declare_ten(&mut zip, [0x50, 0x4b, 0x03, 0x04], 22);
        declare_ten(&mut zip, [0x50, 0x4b, 0x01, 0x02], 24);
        let declared = zip::ZipArchive::new(Cursor::new(zip.clone()))
            .unwrap()
            .by_index(0)
            .unwrap()
            .size();
        assert_eq!(declared, 10);
        assert!(matches!(read(zip), Err(ZipReadError::EntryTooLarge { name }) if name == "bomb"));
    }

    #[test]
    fn an_entry_that_fails_its_crc_reads_as_damaged() {
        let mut zip = zip_of(&[("a", b"hello hello hello")]);
        let at = zip
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x03, 0x04])
            .unwrap()
            + 14;
        zip[at] ^= 0xff;
        let central = zip
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .unwrap()
            + 16;
        zip[central] ^= 0xff;
        let entries = read(zip).unwrap();
        assert!(entries[0].bytes.is_err());
    }

    #[test]
    fn a_damaged_file_is_not_a_zip() {
        assert!(matches!(
            read(b"not a zip".to_vec()),
            Err(ZipReadError::NotAZip(_))
        ));
    }
}
