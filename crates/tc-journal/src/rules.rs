//! The per-change rules of `docs/STORAGE.md` §4.5. They look at one change alone, so the
//! verdict does not depend on arrival order. A change that breaks one is a quarantined
//! change: kept in its file, not folded, listed.
//!
//! V1 (shape) and V2 (identity strings) are checked while the bytes are decoded, V7 (size
//! and depth) before and while they are parsed; V3 to V6 and V8 are checked here on the
//! decoded body.

use crate::change::ChangeBody;
use crate::format::{FORMAT_MINOR, PathPattern, derived_paths};
use crate::ids::{ChapterNo, DeviceId, DeviceKind, RepoPath};
use crate::record::{ChapterFront, Record, VerseValue};
use std::fmt;
use tc_usfm::codec::{has_verse_or_chapter_marker, is_valid_layout};

/// The most parents a change may name (V5).
pub const MAX_PARENTS: usize = 4096;
/// The longest change line, including its tab, hash and line feed (V7, §4.6).
pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

/// The rule a quarantined change breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    V1,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
    V8,
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Why a change is quarantined. The text is a function of the change's bytes, so every
/// device lists the same anomaly for it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, thiserror::Error)]
pub enum RuleViolation {
    /// V1: the bytes are not canonical JSON, or do not parse as a change.
    #[error("V1: {at}: {problem}")]
    Shape { at: Box<str>, problem: Box<str> },
    /// V1: a change from a newer minor uses a variant this app cannot hold, such as a new
    /// kind of verse value. It is kept unread, as made by a newer translationCore.
    #[error("V1: {at}: {name:?} was made by a newer translationCore")]
    NewerVariant { at: Box<str>, name: Box<str> },
    /// V2: an identity string is not NFC or breaks its grammar.
    #[error("V2: {at}: {problem}")]
    Identity { at: Box<str>, problem: Box<str> },
    /// V3: USFM in a value would break the book's structure.
    #[error("V3: {problem}")]
    Usfm { problem: Box<str> },
    /// V4: a device profile written by another device.
    #[error("V4: the profile of {profile} was written by {author}")]
    DeviceProfile { profile: DeviceId, author: DeviceId },
    /// V5: parents not sorted, repeated, or too many.
    #[error("V5: {problem}")]
    Parents { problem: Box<str> },
    /// V6: a comment or batch release names parents.
    #[error("V6: a {kind} change has parents; it is grow-only")]
    GrowOnly { kind: Box<str> },
    /// V7: nested deeper than 64 levels.
    #[error("V7: the change nests deeper than 64 levels")]
    TooDeep,
    /// V7: the line is over 4 MiB.
    #[error("V7: the change line has {bytes} bytes; the limit is 4 MiB")]
    LineTooLong { bytes: u64 },
    /// V8: a carried file at a path the app derives or owns.
    #[error("V8: {path}: {problem}")]
    CarriedPath { path: Box<str>, problem: Box<str> },
}

impl RuleViolation {
    pub fn rule(&self) -> Rule {
        match self {
            Self::Shape { .. } | Self::NewerVariant { .. } => Rule::V1,
            Self::Identity { .. } => Rule::V2,
            Self::Usfm { .. } => Rule::V3,
            Self::DeviceProfile { .. } => Rule::V4,
            Self::Parents { .. } => Rule::V5,
            Self::GrowOnly { .. } => Rule::V6,
            Self::TooDeep | Self::LineTooLong { .. } => Rule::V7,
            Self::CarriedPath { .. } => Rule::V8,
        }
    }

    /// True when the change was made by a newer translationCore and is kept unread, rather
    /// than broken.
    pub fn is_newer(&self) -> bool {
        matches!(self, Self::NewerVariant { .. })
    }
}

/// Checks V3 to V6 and V8 on a decoded change.
pub fn check(body: &ChangeBody) -> Result<(), RuleViolation> {
    check_usfm(&body.record)?;
    check_profile(body)?;
    check_parents(body)?;
    check_carried(body)
}

fn usfm(problem: String) -> RuleViolation {
    RuleViolation::Usfm {
        problem: problem.into(),
    }
}

/// V3. Also the structure rules for book headers and chapter fronts: a header holds no
/// chapter marker, and a front starts with its own chapter's marker and holds no other
/// verse or chapter marker.
fn check_usfm(record: &Record) -> Result<(), RuleViolation> {
    match record {
        Record::Verse { verse, value, .. } => match value {
            VerseValue::Text {
                words,
                layout,
                last,
                ..
            } => {
                if has_verse_or_chapter_marker(words) {
                    return Err(usfm("the words hold a verse or chapter marker".into()));
                }
                if has_verse_or_chapter_marker(layout) {
                    return Err(usfm("the layout holds a verse or chapter marker".into()));
                }
                if !is_valid_layout(words, layout) {
                    return Err(usfm(
                        "the layout is not a line break followed by lines of paragraph markers"
                            .into(),
                    ));
                }
                if last < verse {
                    return Err(usfm(format!("verse {verse} cannot end its span at {last}")));
                }
                Ok(())
            }
            VerseValue::Covered { by } if by >= verse => Err(usfm(format!(
                "verse {verse} cannot be covered by verse {by}"
            ))),
            _ => Ok(()),
        },
        Record::BookHeader { value, .. } => {
            if first_chapter_marker(&value.0).is_some() {
                return Err(usfm("the book header holds a chapter marker".into()));
            }
            Ok(())
        }
        Record::ChapterFront {
            chapter,
            value: ChapterFront::Present(front),
            ..
        } => check_front(*chapter, &front.0),
        _ => Ok(()),
    }
}

fn check_front(chapter: ChapterNo, front: &str) -> Result<(), RuleViolation> {
    let own = format!("the front of chapter {chapter}");
    let Some(rest) = front.strip_prefix("\\c") else {
        return Err(usfm(format!("{own} does not start with \\c")));
    };
    let digits_start = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let after_blank = &rest[digits_start..];
    let digits_len = after_blank.len()
        - after_blank
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .len();
    let digits = &after_blank[..digits_len];
    let after = &after_blank[digits_len..];
    // The codec keeps a chapter label as written, zeros included (§13.5), so `\c 000001`
    // is chapter 1 however many zeros lead it.
    let significant = digits.trim_start_matches('0');
    let number_ok = digits_start > 0
        && !digits.is_empty()
        && significant.len() <= 5
        && significant.parse::<u32>().ok() == Some(u32::from(chapter.get()))
        && (after.is_empty() || after.starts_with([' ', '\t', '\r', '\n']));
    if !number_ok {
        return Err(usfm(format!("{own} does not start with \\c {chapter}")));
    }
    if has_verse_or_chapter_marker(after) {
        return Err(usfm(format!("{own} holds another verse or chapter marker")));
    }
    Ok(())
}

/// The position of the first `\c` marker, if any.
fn first_chapter_marker(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    (0..bytes.len()).find(|&at| {
        bytes[at] == b'\\'
            && bytes.get(at + 1) == Some(&b'c')
            && matches!(bytes.get(at + 2), None | Some(b' ' | b'\t' | b'\r' | b'\n'))
    })
}

/// V4.
fn check_profile(body: &ChangeBody) -> Result<(), RuleViolation> {
    let Record::DeviceProfile { device, .. } = &body.record else {
        return Ok(());
    };
    let by_itself = *device == body.device;
    let by_importer = body.device.kind() == DeviceKind::Importer
        && matches!(device.kind(), DeviceKind::Tc4Actor | DeviceKind::Tc3User);
    if by_itself || by_importer {
        Ok(())
    } else {
        Err(RuleViolation::DeviceProfile {
            profile: *device,
            author: body.device,
        })
    }
}

/// V5 and V6.
fn check_parents(body: &ChangeBody) -> Result<(), RuleViolation> {
    let parents = &body.parents;
    if parents.len() > MAX_PARENTS {
        return Err(RuleViolation::Parents {
            problem: format!("{} parents; the limit is {MAX_PARENTS}", parents.len()).into(),
        });
    }
    if parents.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(RuleViolation::Parents {
            problem: "the parents are not sorted, or one is named twice".into(),
        });
    }
    // A comment is added with no parents. Its later versions name the version they replace:
    // a revision of the text the comment box showed (§5.5, APP-DESIGN R24), a restore from
    // History, and the resolution of two changes under one comment id (a forgery or a broken
    // random source), which names both.
    use crate::change::Cause;
    let grow_only = match &body.record {
        Record::Comment { .. }
            if !matches!(
                body.cause,
                Cause::Resolve | Cause::Edit | Cause::Restore { .. }
            ) =>
        {
            Some("comment")
        }
        Record::BatchRelease { .. } => Some("batch release"),
        _ => None,
    };
    match grow_only {
        Some(kind) if !parents.is_empty() => Err(RuleViolation::GrowOnly { kind: kind.into() }),
        _ => Ok(()),
    }
}

/// V8. The derived paths are those of the change's own minor, or of this app's minor when
/// the change is newer, so the verdict on a change never depends on the project's
/// `format.json`. Paths are compared by their collision forms (§3.3).
fn check_carried(body: &ChangeBody) -> Result<(), RuleViolation> {
    let Record::CarriedFile { path, .. } = &body.record else {
        return Ok(());
    };
    carried_path_problem(path, known_minor(body.v)).map_or(Ok(()), |problem| {
        Err(RuleViolation::CarriedPath {
            path: path.as_str().into(),
            problem: problem.into(),
        })
    })
}

/// `v`, or this app's minor when `v` is newer.
fn known_minor(v: u16) -> u16 {
    if v > FORMAT_MINOR { FORMAT_MINOR } else { v }
}

/// The files translationCore writes besides those its format derives (§3.1, §11.4).
const OWNED: &[&str] = &["metadata.json", "ingredients/tc/**"];

pub(crate) fn carried_path_problem(path: &RepoPath, minor: u16) -> Option<String> {
    let folded = path.collision_form();
    let derived = derived_paths(minor);
    let owned: Vec<PathPattern> = OWNED.iter().filter_map(|p| PathPattern::parse(p)).collect();
    let app_files = || derived.iter().chain(&owned);
    if let Some(pattern) = app_files().find(|pattern| pattern.matches_folded(&folded)) {
        return Some(format!("translationCore writes the files at {pattern}"));
    }
    // One path cannot be both a file and a folder, so a carried file that the app's files
    // need as a folder, or that lies inside one of them, could never be written.
    for pattern in app_files() {
        let segments: Vec<&str> = pattern.as_str().split('/').collect();
        for depth in 1..segments.len() {
            let folder = segments[..depth].join("/");
            if PathPattern::parse(&folder).is_some_and(|folder| folder.matches_folded(&folded)) {
                return Some(format!(
                    "translationCore needs a folder here, for the files at {pattern}"
                ));
            }
        }
    }
    let segments: Vec<&str> = folded.split('/').collect();
    for depth in 1..segments.len() {
        let parent = segments[..depth].join("/");
        if let Some(pattern) = app_files().find(|pattern| pattern.matches_folded(&parent)) {
            return Some(format!(
                "the file is inside {parent}, which translationCore writes at {pattern}"
            ));
        }
    }
    None
}
