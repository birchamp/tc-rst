//! The parsing layer of the fold never panics (`docs/STORAGE.md` I5, §16.1
//! `fold_never_panics`): arbitrary bytes as packs, pending files, change lines, tokens and
//! `format.json` come back as values or reported defects, and what is read back is
//! consistent with the bytes.
//!
//! The generators build files from sketches (a header with each of its fields right or
//! wrong, lines that verify or not), name a pack by the hash of its bytes so validation
//! reaches the header and the lines, and mix in arbitrary bytes. Each run counts the
//! statuses it produced and fails when one was never produced (§16.1).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use common::*;
use proptest::prelude::*;
use std::sync::Arc;
use tc_journal::format::FormatDoc;
use tc_journal::line::{encode_line, read_line, verify_line};
use tc_journal::pack::{PackError, PackSpec, PackStatus, read_pack, seal};
use tc_journal::pending::{PendingHeader, PendingStatus, Purpose, encode_pending, read_pending};
use tc_journal::{
    Change, ChangeId, DeviceId, DeviceKind, PackId, ShownToken, TokenKey, Verified, WallTime, json,
    parse_change,
};

/// Every entry a reader returns is a line that verifies: its id is the hash of its bytes.
fn consistent(entries: &[Verified]) -> Result<(), TestCaseError> {
    for entry in entries {
        prop_assert_eq!(entry.id(), ChangeId::of(entry.raw()));
        // A change of this minor writes back to the bytes it was read from.
        if let Verified::Change(change) = entry
            && change.body().v == 0
        {
            let again = Change::new(change.body().clone()).unwrap();
            prop_assert_eq!(again.raw(), change.raw());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Sketches of files
// ---------------------------------------------------------------------------------------

/// A line of a sketched file.
#[derive(Debug, Clone)]
enum LineSketch {
    /// A change line that verifies.
    Change(u8),
    /// A line that verifies but is no change: a quarantined change.
    NotAChange,
    /// A change line with a damaged byte.
    Damaged(u8),
    /// Bytes with no hash.
    Garbage(Vec<u8>),
}

fn line_sketch() -> impl Strategy<Value = LineSketch> {
    prop_oneof![
        6 => (0u8..4).prop_map(LineSketch::Change),
        1 => Just(LineSketch::NotAChange),
        1 => (0u8..4).prop_map(LineSketch::Damaged),
        1 => prop::collection::vec(any::<u8>(), 0..12).prop_map(LineSketch::Garbage),
    ]
}

fn sketch_change(n: u8) -> Change {
    verse_change(
        local(1),
        u64::from(n),
        u16::from(n) + 1,
        &format!("texto {n}"),
    )
}

fn write_line(sketch: &LineSketch, out: &mut Vec<u8>) {
    match sketch {
        LineSketch::Change(n) => {
            let change = sketch_change(*n);
            out.extend_from_slice(&encode_line(change.id(), change.raw()));
        }
        LineSketch::NotAChange => out.extend_from_slice(&line_of(r#"{"what":"not a change"}"#)),
        LineSketch::Damaged(n) => {
            let change = sketch_change(*n);
            let mut line = encode_line(change.id(), change.raw());
            line[3] ^= 0x20;
            out.extend_from_slice(&line);
        }
        LineSketch::Garbage(bytes) => {
            out.extend(bytes.iter().map(|b| if *b == b'\n' { b' ' } else { *b }));
            out.push(b'\n');
        }
    }
}

/// A header field: right, or wrong in a way validation must notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Right,
    /// A newer framing version, another project, another Local sealer, or a count off by one.
    Other,
    /// A value of the wrong kind or grammar.
    Malformed,
}

fn field() -> impl Strategy<Value = Field> {
    prop_oneof![8 => Just(Field::Right), 1 => Just(Field::Other), 1 => Just(Field::Malformed)]
}

#[derive(Debug, Clone)]
struct FileSketch {
    version: Field,
    project: Field,
    writer: Field,
    count: Field,
    /// The header's hash verifies.
    header_verifies: bool,
    lines: Vec<LineSketch>,
    /// The file ends with a line feed.
    terminated: bool,
}

fn file_sketch() -> impl Strategy<Value = FileSketch> {
    (
        (field(), field(), field(), field()),
        prop::bool::weighted(0.9),
        prop::collection::vec(line_sketch(), 0..5),
        prop::bool::weighted(0.85),
    )
        .prop_map(
            |((version, project, writer, count), header_verifies, lines, terminated)| FileSketch {
                version,
                project,
                writer,
                count,
                header_verifies,
                lines,
                terminated,
            },
        )
}

fn header_bytes(json: &str, verifies: bool) -> Vec<u8> {
    let mut line = line_of(json);
    if !verifies {
        line[2] ^= 0x01;
    }
    line
}

fn finish(mut bytes: Vec<u8>, sketch: &FileSketch) -> Vec<u8> {
    for line in &sketch.lines {
        write_line(line, &mut bytes);
    }
    if !sketch.terminated {
        bytes.pop();
    }
    bytes
}

fn project_json(field: Field) -> String {
    match field {
        Field::Right => format!("\"{}\"", project()),
        Field::Other => format!("\"{}\"", other_project()),
        Field::Malformed => "\"6F1C\"".into(),
    }
}

fn pack_bytes(sketch: &FileSketch) -> Vec<u8> {
    let count = sketch.lines.len() as i64
        + match sketch.count {
            Field::Right => 0,
            Field::Other => 1,
            Field::Malformed => -1,
        };
    let count = if count < 0 {
        "\"x\"".to_owned()
    } else {
        count.to_string()
    };
    let tcpack = match sketch.version {
        Field::Right => "1",
        Field::Other => "2",
        Field::Malformed => "-1",
    };
    let sealer = match sketch.writer {
        Field::Right => local(9),
        Field::Other => local(8),
        Field::Malformed => DeviceId::derived(DeviceKind::Importer, b"not a sealer"),
    };
    let header = format!(
        r#"{{"count":{count},"project":{},"replaces":[],"sealed":0,"sealer":"{sealer}","tcpack":{tcpack}}}"#,
        project_json(sketch.project)
    );
    finish(header_bytes(&header, sketch.header_verifies), sketch)
}

fn pending_bytes(sketch: &FileSketch) -> Vec<u8> {
    let version = match sketch.version {
        Field::Right => "1",
        Field::Other => "2",
        Field::Malformed => "\"1\"",
    };
    let device = match sketch.writer {
        Field::Right => local(1),
        Field::Other => local(2),
        Field::Malformed => DeviceId::derived(DeviceKind::Tc4Actor, b"not a save"),
    };
    let purpose = match sketch.count {
        Field::Right => "save",
        Field::Other => "salvage",
        Field::Malformed => "merge",
    };
    let header = format!(
        r#"{{"device":"{device}","pending":{version},"project":{},"purpose":"{purpose}"}}"#,
        project_json(sketch.project)
    );
    finish(header_bytes(&header, sketch.header_verifies), sketch)
}

/// Bytes that look like the format: header-like and change-like lines with random parts.
fn odd_file() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just(format!(r#"{{"count":1,"project":"{}","replaces":[],"sealed":0,"sealer":"{}","tcpack":1}}"#, project(), local(9))),
        Just(format!(r#"{{"device":"{}","pending":1,"project":"{}","purpose":"save"}}"#, local(1), project())),
        Just(format!(r#"{{"count":1,"project":"{}","replaces":[],"sealed":0,"sealer":"{}","tcpack":1}}"#, other_project(), local(9))),
        Just(r#"{"tcpack":2}"#.to_owned()),
        Just(r#"{"pending":2}"#.to_owned()),
        Just(r#"{"count":18446744073709551615,"tcpack":1}"#.to_owned()),
        Just(r#"{"pending":99999999999999999999}"#.to_owned()),
        Just(r#"{"tcpack":-1}"#.to_owned()),
        Just("[".repeat(200)),
        Just(r#"{"at":0,"batch":{"id":"00000000000000000000000000000000","n":4294967295},"cause":{"zzz":{}},"device":"d","parents":[],"record":{"verse":{}},"seq":0,"v":65535}"#.to_owned()),
        Just(String::from_utf8(sketch_change(1).raw().to_vec()).unwrap()),
        any::<String>(),
    ];
    prop::collection::vec((piece, any::<bool>(), any::<bool>()), 0..6).prop_map(|lines| {
        let mut out = Vec::new();
        for (json, hashed, newline) in lines {
            out.extend_from_slice(json.as_bytes());
            if hashed {
                out.push(b'\t');
                out.extend_from_slice(ChangeId::of(json.as_bytes()).to_hex().as_bytes());
            }
            if newline {
                out.push(b'\n');
            }
        }
        out
    })
}

/// Where a pack is read from.
#[derive(Debug, Clone)]
enum PackPlace {
    /// The path its own bytes name, so P1 and P2 pass.
    Named,
    /// The path of other bytes: P2 fails.
    Misnamed,
    /// Any text: P1 fails.
    Anywhere(String),
}

fn pack_place() -> impl Strategy<Value = PackPlace> {
    prop_oneof![
        8 => Just(PackPlace::Named),
        1 => Just(PackPlace::Misnamed),
        1 => prop_oneof![
            any::<String>(),
            "ingredients/tc/changes/[a-z0-9]{1,30}/[0-9-]{7}/[0-9a-f]{32}\\.ndjson",
        ]
        .prop_map(PackPlace::Anywhere),
    ]
}

fn pack_path(place: &PackPlace, bytes: &[u8]) -> String {
    let named = |bytes: &[u8]| {
        format!(
            "ingredients/tc/changes/{}/2026-10/{}.ndjson",
            local(9),
            pack_id_of(bytes).name()
        )
    };
    match place {
        PackPlace::Named => named(bytes),
        PackPlace::Misnamed => named(b"other bytes"),
        PackPlace::Anywhere(path) => path.clone(),
    }
}

fn packs() -> impl Strategy<Value = (PackPlace, Vec<u8>)> {
    let bytes = prop_oneof![
        6 => file_sketch().prop_map(|sketch| pack_bytes(&sketch)),
        1 => Just(Vec::new()),
        2 => odd_file(),
        1 => any::<Vec<u8>>(),
    ];
    (pack_place(), bytes)
}

fn pending_files() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        6 => file_sketch().prop_map(|sketch| pending_bytes(&sketch)),
        1 => Just(Vec::new()),
        2 => odd_file(),
        1 => any::<Vec<u8>>(),
    ]
}

fn damage(error: &PackError) -> &'static str {
    match error {
        PackError::BadPath => "bad path",
        PackError::NameMismatch => "name mismatch",
        PackError::Empty => "empty",
        PackError::HeaderUnverified => "header unverified",
        PackError::HeaderMalformed(_) => "header malformed",
        PackError::SealerMismatch { .. } => "sealer mismatch",
        PackError::CountMismatch { .. } => "count mismatch",
        PackError::LineUnverified { .. } => "line unverified",
        PackError::Truncated => "truncated",
    }
}

fn saw_entries(entries: &[Verified], census: &Census) {
    if entries.iter().any(|e| matches!(e, Verified::Change(_))) {
        census.saw("a change read");
    }
    if entries
        .iter()
        .any(|e| matches!(e, Verified::Quarantined(_)))
    {
        census.saw("a quarantined change read");
    }
}

// ---------------------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------------------

#[test]
fn fold_never_panics_on_arbitrary_packs() {
    run_with_census(
        1024,
        packs(),
        &[
            "valid",
            "newer",
            "foreign",
            "bad path",
            "name mismatch",
            "empty",
            "header unverified",
            "header malformed",
            "sealer mismatch",
            "count mismatch",
            "line unverified",
            "truncated",
            "a change read",
            "a quarantined change read",
        ],
        |(place, bytes), census| {
            let path = pack_path(&place, &bytes);
            let read = read_pack(&path, &bytes, project());
            // A file that frames a pack has the id §4.3 gives it; any other, the SHA-256 of
            // its bytes.
            if read.status == PackStatus::Valid {
                prop_assert_eq!(read.id, pack_id_of(&bytes));
            } else {
                prop_assert!(read.id == PackId::of(&bytes) || read.id == pack_id_of(&bytes));
            }
            match &read.status {
                PackStatus::Valid => census.saw("valid"),
                PackStatus::Newer { .. } => census.saw("newer"),
                PackStatus::Foreign { .. } => census.saw("foreign"),
                PackStatus::Damaged(error) => census.saw(damage(error)),
            }
            if !matches!(read.status, PackStatus::Valid | PackStatus::Damaged(_)) {
                prop_assert!(read.entries.is_empty());
            }
            saw_entries(&read.entries, census);
            consistent(&read.entries)
        },
    );
}

#[test]
fn fold_never_panics_on_arbitrary_pending_files() {
    run_with_census(
        1024,
        pending_files(),
        &[
            "valid",
            "newer",
            "foreign",
            "empty",
            "header unverified",
            "header malformed",
            "line unverified",
            "truncated",
            "a change read",
            "a quarantined change read",
        ],
        |bytes, census| {
            let read = read_pending(&bytes, project());
            match &read.status {
                PendingStatus::Valid => census.saw("valid"),
                PendingStatus::Newer { .. } => census.saw("newer"),
                PendingStatus::Foreign { .. } => census.saw("foreign"),
                PendingStatus::Damaged(error) => census.saw(damage(error)),
            }
            if matches!(
                read.status,
                PendingStatus::Newer { .. } | PendingStatus::Foreign { .. }
            ) {
                prop_assert!(read.entries.is_empty());
            }
            saw_entries(&read.entries, census);
            consistent(&read.entries)
        },
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn fold_never_panics_on_arbitrary_lines(bytes in any::<Vec<u8>>(), text in any::<String>()) {
        prop_assert!(verify_line(&bytes).is_none() || bytes.contains(&b'\t'));
        let _ = read_line(&bytes);
        let _ = json::parse(&bytes);
        let _ = json::parse(text.as_bytes());
        let read = parse_change(ChangeId::of(&bytes), Arc::from(bytes.as_slice()));
        prop_assert_eq!(read.id(), ChangeId::of(&bytes));
        let _ = FormatDoc::parse(&bytes);
        let key = TokenKey::derive(&[7; 32], project());
        let _ = ShownToken::parse(&key, &text);
    }
}

/// Where a mutated file ends.
#[derive(Debug, Clone)]
enum Cut {
    Whole,
    Anywhere(prop::sample::Index),
    /// Right after a line feed: whole lines are lost.
    AfterLine(prop::sample::Index),
    /// Right before a line feed: the last line loses its line feed.
    BeforeLineFeed(prop::sample::Index),
}

fn cut(bytes: &mut Vec<u8>, cut: &Cut) {
    let feeds: Vec<usize> = bytes
        .iter()
        .enumerate()
        .filter_map(|(at, b)| (*b == b'\n').then_some(at))
        .collect();
    let end = match cut {
        Cut::Whole => bytes.len(),
        Cut::Anywhere(at) => at.index(bytes.len() + 1).max(1),
        Cut::AfterLine(at) if !feeds.is_empty() => feeds[at.index(feeds.len())] + 1,
        Cut::BeforeLineFeed(at) if !feeds.is_empty() => feeds[at.index(feeds.len())].max(1),
        _ => bytes.len(),
    };
    bytes.truncate(end);
}

/// Mutations of valid files: every outcome is a status, never a panic, and a valid file
/// whose bytes were changed is never read as valid. Each mutated pack is also read at the
/// path its new bytes would name. A changed byte breaks the header (P3) or a line (P4) before
/// the name is checked (P2), so no mutation here reaches P2; lines moved or replaced whole
/// are `a_pack_with_lines_moved_or_replaced_is_damaged` (tests/packs.rs).
#[test]
fn mutated_files_are_never_read_as_valid() {
    let cuts = prop_oneof![
        Just(Cut::Whole),
        any::<prop::sample::Index>().prop_map(Cut::Anywhere),
        any::<prop::sample::Index>().prop_map(Cut::AfterLine),
        any::<prop::sample::Index>().prop_map(Cut::BeforeLineFeed),
    ];
    let strategy = (
        prop::collection::vec(arb_body(), 1..5),
        prop_oneof![
            Just(Vec::new()),
            prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..6),
        ],
        cuts,
    );
    run_with_census(
        1024,
        strategy,
        &[
            "pack header unverified",
            "pack line unverified",
            "pack truncated",
            "pack count mismatch",
            "pending header unverified",
            "pending line unverified",
            "pending truncated",
            "pending valid",
        ],
        |(bodies, edits, end), census| {
            let lines: Vec<Verified> = bodies
                .into_iter()
                .map(|b| Change::new(b).unwrap().into())
                .collect();
            let spec = PackSpec {
                project: project(),
                sealer: local(9),
                sealed: WallTime(0),
            };
            let pack = seal(&spec, &lines).unwrap().packs.remove(0);
            let pending = encode_pending(
                &PendingHeader {
                    device: local(1),
                    project: project(),
                    purpose: Purpose::Save,
                    reconciles: None,
                },
                &lines,
            )
            .unwrap();
            for (index, original) in [&pack.bytes, &pending.bytes].into_iter().enumerate() {
                let mut bytes = original.clone();
                for (at, value) in &edits {
                    let at = at.index(bytes.len());
                    bytes[at] = *value;
                }
                cut(&mut bytes, &end);
                if bytes == *original {
                    continue;
                }
                let at_its_place = read_pack(&pack.path.to_string(), &bytes, project());
                let at_its_name =
                    read_pack(&pack_path(&PackPlace::Named, &bytes), &bytes, project());
                let pending_read = read_pending(&bytes, project());
                for read in [&at_its_place, &at_its_name] {
                    prop_assert_ne!(&read.status, &PackStatus::Valid);
                    if index == 0
                        && let PackStatus::Damaged(error) = &read.status
                    {
                        census.saw(match damage(error) {
                            "header unverified" => "pack header unverified",
                            "line unverified" => "pack line unverified",
                            "truncated" => "pack truncated",
                            "count mismatch" => "pack count mismatch",
                            _ => continue,
                        });
                    }
                }
                if index == 1 {
                    match &pending_read.status {
                        PendingStatus::Valid => census.saw("pending valid"),
                        PendingStatus::Damaged(PackError::HeaderUnverified) => {
                            census.saw("pending header unverified")
                        }
                        PendingStatus::Damaged(PackError::LineUnverified { .. }) => {
                            census.saw("pending line unverified")
                        }
                        PendingStatus::Damaged(PackError::Truncated) => {
                            census.saw("pending truncated")
                        }
                        _ => {}
                    }
                }
                for read in [
                    &at_its_place.entries,
                    &at_its_name.entries,
                    &pending_read.entries,
                ] {
                    consistent(read)?;
                    for entry in read {
                        prop_assert!(
                            lines.contains(entry),
                            "a read change differs from what its author wrote"
                        );
                    }
                }
            }
            Ok(())
        },
    );
}
