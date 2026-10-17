use crate::error::ValueError;
use crate::path::IngredientPath;
use md5::{Digest, Md5 as Md5Hasher};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{self, Read};
use std::str::FromStr;

/// An md5 digest, written as 32 lower-case hexadecimal digits: the one shared digest type
/// ([`tc_core::Md5`]), so a burrito's checksums and the journal's hashes are one type.
pub use tc_core::Md5;

/// A media type in the form the schema accepts: `^[-a-z0-9]+/[-a-z0-9+]+$`. That excludes
/// upper case, dots (so every `application/vnd.*` type) and parameters such as `charset`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MimeType(String);

/// Media types by lower-case file extension. `.usfm` is `text/plain`, as tC4 and the
/// Pankosmia server write it, so a server rescan does not rewrite the value; `text/x-usfm`
/// is also valid and is kept when read. Extensions not listed get
/// `application/octet-stream`, because a guessed vendor type would fail the schema.
const MIME_BY_EXTENSION: &[(&str, &str)] = &[
    ("usfm", "text/plain"),
    ("sfm", "text/plain"),
    ("vrs", "text/plain"),
    ("txt", "text/plain"),
    ("md", "text/markdown"),
    ("json", "application/json"),
    ("tsv", "text/tab-separated-values"),
    ("csv", "text/csv"),
    ("usx", "text/xml"),
    ("xml", "text/xml"),
    ("html", "text/html"),
    ("css", "text/css"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("svg", "image/svg+xml"),
    ("webp", "image/webp"),
    ("mp3", "audio/mpeg"),
    ("wav", "audio/wav"),
    ("ogg", "audio/ogg"),
    ("mp4", "video/mp4"),
    ("webm", "video/webm"),
    ("ttf", "font/ttf"),
    ("otf", "font/otf"),
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
    ("pdf", "application/pdf"),
    ("zip", "application/zip"),
];

const OCTET_STREAM: &str = "application/octet-stream";

impl MimeType {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        if is_schema_mime_type(text) {
            Ok(Self(text.to_owned()))
        } else {
            Err(ValueError::MimeType(text.to_owned()))
        }
    }

    /// The media type for a file at `path`, from its extension.
    pub fn for_path(path: &IngredientPath) -> Self {
        let extension = path.extension().map(str::to_ascii_lowercase);
        let found = MIME_BY_EXTENSION
            .iter()
            .find(|(known, _)| Some(*known) == extension.as_deref())
            .map_or(OCTET_STREAM, |(_, mime)| mime);
        // Every entry of the table passes `is_schema_mime_type` (a test checks it).
        Self(found.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the top-level type is `text`, as for USFM, USX and Markdown.
    pub fn is_text(&self) -> bool {
        self.0.starts_with("text/")
    }
}

pub(crate) fn is_schema_mime_type(text: &str) -> bool {
    let part_ok = |part: &str, plus: bool| {
        !part.is_empty()
            && part.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || (plus && b == b'+')
            })
    };
    text.split_once('/')
        .is_some_and(|(kind, sub)| part_ok(kind, false) && part_ok(sub, true))
}

impl fmt::Display for MimeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for MimeType {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl Serialize for MimeType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for MimeType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// The md5 and byte length of a file's exact bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileFacts {
    pub md5: Md5,
    pub size: u64,
}

impl FileFacts {
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            md5: Md5::of(bytes),
            // usize is at most 64 bits on every target Rust supports.
            size: bytes.len() as u64,
        }
    }

    /// Hashes a stream without holding it in memory, for large media files.
    pub fn of_reader(mut reader: impl Read) -> io::Result<Self> {
        let mut hasher = Md5Hasher::new();
        let mut buffer = vec![0; 64 * 1024];
        let mut size = 0u64;
        loop {
            let read = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            hasher.update(&buffer[..read]);
            size += read as u64;
        }
        Ok(Self {
            md5: Md5::from_bytes(hasher.finalize().into()),
            size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_known_digests_and_round_trips() {
        assert_eq!(Md5::of(b"").to_string(), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            Md5::of(b"The quick brown fox jumps over the lazy dog").to_string(),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
        let digest = Md5::parse("9e107d9d372bb6826bd81d3542a419d6").unwrap();
        assert_eq!(digest.to_string(), "9e107d9d372bb6826bd81d3542a419d6");
        for bad in [
            "9E107D9D372BB6826BD81D3542A419D6",
            "9e107d9d372bb6826bd81d3542a419d",
            "9e107d9d372bb6826bd81d3542a419d6a",
            "9e107d9d372bb6826bd81d3542a419dg",
            "",
        ] {
            assert!(Md5::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn streamed_facts_equal_in_memory_facts() {
        let bytes: Vec<u8> = (0..200_000u32).map(|n| (n % 251) as u8).collect();
        assert_eq!(
            FileFacts::of_reader(bytes.as_slice()).unwrap(),
            FileFacts::of(&bytes)
        );
    }

    #[test]
    fn every_table_type_passes_the_schema_pattern() {
        for (_, mime) in MIME_BY_EXTENSION {
            assert!(is_schema_mime_type(mime), "{mime}");
        }
        assert!(is_schema_mime_type(OCTET_STREAM));
    }

    #[test]
    fn media_types_follow_the_extension() {
        let mime = |path: &str| {
            MimeType::for_path(&IngredientPath::parse(path).unwrap())
                .as_str()
                .to_owned()
        };
        assert_eq!(mime("ingredients/TIT.usfm"), "text/plain");
        assert_eq!(mime("ingredients/TIT.USFM"), "text/plain");
        assert_eq!(mime("ingredients/content/01.md"), "text/markdown");
        assert_eq!(mime("ingredients/vrs.json"), "application/json");
        assert_eq!(mime("ingredients/notes.tsv"), "text/tab-separated-values");
        assert_eq!(mime("ingredients/report.docx"), OCTET_STREAM);
        assert_eq!(mime("ingredients/LICENSE"), OCTET_STREAM);
    }

    #[test]
    fn media_type_syntax_follows_the_schema() {
        for ok in [
            "text/plain",
            "text/x-usfm",
            "image/svg+xml",
            "application/octet-stream",
        ] {
            assert!(MimeType::parse(ok).is_ok(), "{ok}");
        }
        for bad in [
            "application/vnd.foo+json",
            "text/plain; charset=utf-8",
            "Text/Plain",
            "text",
            "text/",
            "/plain",
            "text/plain/x",
            "te+xt/plain",
        ] {
            assert!(MimeType::parse(bad).is_err(), "{bad}");
        }
    }
}
