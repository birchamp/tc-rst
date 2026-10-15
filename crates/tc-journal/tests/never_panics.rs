//! The parsing layer of the fold never panics (`docs/STORAGE.md` I5, §16.1
//! `fold_never_panics`): arbitrary bytes as packs, pending files, change lines, tokens and
//! `format.json` come back as values or reported defects, and what is read back is
//! consistent with the bytes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use proptest::prelude::*;
use std::sync::Arc;
use tc_journal::format::FormatDoc;
use tc_journal::line::{read_line, verify_line};
use tc_journal::pack::{PackSpec, PackStatus, read_pack, seal};
use tc_journal::pending::{PendingHeader, PendingStatus, Purpose, encode_pending, read_pending};
use tc_journal::{Change, ChangeId, ShownToken, TokenKey, Verified, WallTime, json, parse_change};

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

fn pack_path() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(format!(
            "ingredients/tc/changes/{}/2026-10/{}.ndjson",
            local(9),
            "0".repeat(32)
        )),
        any::<String>(),
        "ingredients/tc/changes/[a-z0-9]{1,30}/[0-9-]{7}/[0-9a-f]{32}\\.ndjson",
    ]
}

/// Bytes that look like the format: header-like and change-like lines with random parts.
fn odd_file() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just(r#"{"count":1,"project":"6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50","replaces":[],"sealed":0,"sealer":"daeaqcaibaeaqcaibaeaqcaibae","tcpack":1}"#.to_owned()),
        Just(r#"{"device":"daeaqcaibaeaqcaibaeaqcaibae","pending":1,"project":"6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50","purpose":"save"}"#.to_owned()),
        Just(r#"{"count":18446744073709551615,"tcpack":1}"#.to_owned()),
        Just(r#"{"pending":99999999999999999999}"#.to_owned()),
        Just(r#"{"tcpack":-1}"#.to_owned()),
        Just("[".repeat(200)),
        Just(r#"{"at":0,"batch":{"id":"00000000000000000000000000000000","n":4294967295},"cause":{"zzz":{}},"device":"d","parents":[],"record":{"verse":{}},"seq":0,"v":65535}"#.to_owned()),
        Just(r#"{"at":0,"batch":{"id":"00000000000000000000000000000000","n":1},"cause":{"edit":{}},"device":"daeaqcaibaeaqcaibaeaqcaibae","parents":[],"record":{"verse":{"book":"TIT","chapter":1,"value":{"covered":{"by":9}},"verse":1}},"seq":0,"v":0}"#.to_owned()),
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn fold_never_panics_on_arbitrary_packs(path in pack_path(), bytes in prop_oneof![any::<Vec<u8>>(), odd_file()]) {
        let read = read_pack(&path, &bytes, project());
        prop_assert_eq!(read.id, tc_journal::PackId::of(&bytes));
        if !matches!(read.status, PackStatus::Valid | PackStatus::Damaged(_)) {
            prop_assert!(read.entries.is_empty());
        }
        consistent(&read.entries)?;
    }

    #[test]
    fn fold_never_panics_on_arbitrary_pending_files(bytes in prop_oneof![any::<Vec<u8>>(), odd_file()]) {
        let read = read_pending(&bytes, project());
        if matches!(read.status, PendingStatus::Newer { .. } | PendingStatus::Foreign { .. }) {
            prop_assert!(read.entries.is_empty());
        }
        consistent(&read.entries)?;
    }

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

    /// Mutations of valid files: every outcome is a status, never a panic, and a valid
    /// file whose bytes were changed is never read as valid.
    #[test]
    fn mutated_files_are_never_read_as_valid(
        bodies in prop::collection::vec(arb_body(), 1..5),
        edits in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..6),
        cut in any::<prop::sample::Index>(),
    ) {
        let lines: Vec<Verified> = bodies.into_iter().map(|b| Change::new(b).unwrap().into()).collect();
        let spec = PackSpec { project: project(), sealer: local(9), sealed: WallTime(0) };
        let pack = seal(&spec, &lines).unwrap().packs.remove(0);
        let pending = encode_pending(
            &PendingHeader { device: local(1), project: project(), purpose: Purpose::Save, reconciles: None },
            &lines,
        )
        .unwrap();
        for original in [&pack.bytes, &pending.bytes] {
            let mut bytes = original.clone();
            for (at, value) in &edits {
                let at = at.index(bytes.len());
                bytes[at] = *value;
            }
            bytes.truncate(cut.index(bytes.len() + 1).max(1));
            let pack_read = read_pack(&pack.path.to_string(), &bytes, project());
            if bytes != pack.bytes {
                prop_assert_ne!(&pack_read.status, &PackStatus::Valid);
            }
            consistent(&pack_read.entries)?;
            let pending_read = read_pending(&bytes, project());
            consistent(&pending_read.entries)?;
            for entry in pack_read.entries.iter().chain(&pending_read.entries) {
                prop_assert!(lines.contains(entry), "a read change differs from what its author wrote");
            }
        }
    }
}
