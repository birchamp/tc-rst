//! Format versioning (`docs/STORAGE.md` §12): newer minors are read or carried, never
//! destroyed or reported as damaged, and older and newer apps holding the same changes
//! compute the same keys.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use tc_journal::format::{FormatDoc, FormatGate, PathPattern, derived_paths, gate};
use tc_journal::line::{encode_line, read_line};
use tc_journal::pack::{PackSpec, PackStatus, read_pack, seal};
use tc_journal::pending::{PendingHeader, PendingStatus, Purpose, encode_pending, read_pending};
use tc_journal::record::{UnknownRecord, VerseValue};
use tc_journal::{
    BookId, Cause, Change, FORMAT_MINOR, Record, RecordKey, RuleViolation, Verified, WallTime,
};

/// The lines of `newer-minor.ndjson`, read as a reader does.
fn newer_minor() -> Vec<Verified> {
    let bytes = std::fs::read(fixtures().join("newer-minor.ndjson")).unwrap();
    bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| read_line(line).expect("every fixture line verifies"))
        .collect()
}

fn change(v: &Verified) -> &Change {
    v.as_change()
        .unwrap_or_else(|| panic!("quarantined: {v:?}"))
}

fn rule(v: &Verified) -> &RuleViolation {
    match v {
        Verified::Quarantined(q) => &q.rule,
        Verified::Change(c) => panic!("accepted: {:?}", c.body()),
    }
}

#[test]
fn this_app_is_format_1_0() {
    assert_eq!(FORMAT_MINOR, 0);
    assert_eq!(tc_journal::FORMAT_MAJOR, 1);
}

#[test]
fn a_known_kind_with_newer_fields_keeps_its_own_key() {
    let lines = newer_minor();
    let newer = change(&lines[0]);
    let older = change(&lines[1]);
    assert_eq!(newer.body().v, 1);
    assert_eq!(newer.body().record.key(), older.body().record.key());
    assert_eq!(
        newer.body().record.key(),
        RecordKey::Verse(BookId::Titus, chapter(1), verse_no(1))
    );
    // The known fields read as minor 0 writes them; the unknown ones stay in the bytes, so
    // the two values are never "equal values" for the merge.
    assert_eq!(newer.body().record, older.body().record);
    assert_ne!(newer.raw(), older.raw());
    let text = std::str::from_utf8(newer.raw()).unwrap();
    assert!(text.contains("\"note\":\"a newer field\"") && text.contains("\"sig\""));
    assert_eq!(newer.body().batch, older.body().batch);
}

#[test]
fn an_unknown_kind_is_folded_under_its_own_key() {
    let lines = newer_minor();
    let with_value = change(&lines[2]);
    assert_eq!(
        with_value.body().record,
        Record::Unknown(UnknownRecord {
            kind: "noteThread".into(),
            key_json: r#"{"book":"TIT","thread":"a1"}"#.into(),
            value_json: Some(r#"{"title":"Pablo"}"#.into()),
        })
    );
    let key = with_value.body().record.key();
    assert_eq!(key.kind_name(), "noteThread");
    let without_value = change(&lines[3]);
    let Record::Unknown(record) = &without_value.body().record else {
        panic!("not unknown")
    };
    assert_eq!(record.value_json, None);
    assert_ne!(without_value.body().record.key(), key);
}

#[test]
fn an_unknown_cause_reads_as_newer() {
    let lines = newer_minor();
    assert_eq!(
        change(&lines[4]).body().cause,
        Cause::Newer {
            name: "signedEdit".into()
        }
    );
    assert_eq!(
        change(&lines[5]).body().cause,
        Cause::Newer {
            name: "external".into()
        }
    );
    assert!(matches!(
        change(&lines[5]).body().record,
        Record::Verse {
            value: VerseValue::Text { .. },
            ..
        }
    ));
}

#[test]
fn a_variant_this_app_cannot_hold_is_kept_as_made_by_a_newer_app() {
    let lines = newer_minor();
    let split = rule(&lines[6]);
    assert!(split.is_newer());
    assert_eq!(
        split,
        &RuleViolation::NewerVariant {
            at: "/record/verse/value".into(),
            name: "split".into()
        }
    );
    assert_eq!(
        rule(&lines[10]),
        &RuleViolation::NewerVariant {
            at: "/record/bookHeader/book".into(),
            name: "TOB".into()
        }
    );
}

#[test]
fn the_same_unknowns_at_this_minor_are_malformed() {
    let lines = newer_minor();
    assert_eq!(
        rule(&lines[7]),
        &RuleViolation::Shape {
            at: "/record/verse/value/text".into(),
            problem: "unknown field \"note\"".into()
        }
    );
    assert_eq!(
        rule(&lines[8]),
        &RuleViolation::Shape {
            at: "/record".into(),
            problem: "unknown record kind \"noteThread\"".into()
        }
    );
    assert_eq!(
        rule(&lines[9]),
        &RuleViolation::Shape {
            at: "/cause".into(),
            problem: "unknown cause \"signedEdit\"".into()
        }
    );
    for line in &lines[7..10] {
        assert!(!rule(line).is_newer());
    }
}

#[test]
fn a_change_from_a_newer_minor_passes_through_untouched() {
    // A seal re-frames the stored bytes; it never re-serializes another device's change.
    let lines = newer_minor();
    let spec = PackSpec {
        project: project(),
        sealer: local(9),
        sealed: WallTime(1_760_520_000_000),
    };
    let packs = seal(&spec, &lines).unwrap();
    assert_eq!(packs.len(), 1);
    let read = read_pack(&packs[0].path.to_string(), &packs[0].bytes, project());
    assert_eq!(read.status, PackStatus::Valid);
    assert_eq!(read.entries.len(), lines.len());
    for line in &lines {
        assert!(read.entries.iter().any(|e| e == line));
        let framed = encode_line(line.id(), line.raw());
        assert!(
            packs[0]
                .bytes
                .windows(framed.len())
                .any(|window| window == framed.as_slice())
        );
    }
}

#[test]
fn a_newer_pack_is_carried_never_damaged() {
    let change = verse_change(local(1), 1, 1, "a");
    let header = format!(
        r#"{{"count":1,"project":"{}","sealer":"{}","tcpack":2,"z":[1]}}"#,
        project(),
        local(1)
    );
    let mut bytes = line_of(&header);
    // A newer framing need not even be change lines.
    bytes.extend_from_slice(b"\x00binary\xff\n");
    bytes.extend_from_slice(&encode_line(change.id(), change.raw()));
    let id = tc_journal::PackId::of(&bytes);
    let path = format!(
        "ingredients/tc/changes/{}/2026-10/{}.ndjson",
        local(1),
        id.name()
    );
    let read = read_pack(&path, &bytes, project());
    assert_eq!(read.status, PackStatus::Newer { tcpack: 2 });
    assert!(read.entries.is_empty());
    // Even a damaged copy of a newer pack is reported as newer.
    let read = read_pack(
        &path.replace("2026-10", "2026-11"),
        &bytes[..bytes.len() - 3],
        project(),
    );
    assert_eq!(read.status, PackStatus::Newer { tcpack: 2 });
}

#[test]
fn a_newer_pending_file_is_kept_unread() {
    let change = verse_change(local(1), 1, 1, "a");
    let mut good = encode_pending(
        &PendingHeader {
            device: local(1),
            project: project(),
            purpose: Purpose::Save,
            reconciles: None,
        },
        &[change.into()],
    )
    .unwrap()
    .bytes;
    let header = format!(
        r#"{{"device":"{}","pending":2,"project":"{}","purpose":"save"}}"#,
        local(1),
        project()
    );
    let newline = good.iter().position(|b| *b == b'\n').unwrap();
    let mut bytes = line_of(&header);
    bytes.extend_from_slice(&good.split_off(newline + 1));
    let read = read_pending(&bytes, project());
    assert_eq!(read.status, PendingStatus::Newer { pending: 2 });
    assert!(read.entries.is_empty());
}

#[test]
fn headers_ignore_fields_a_newer_minor_adds() {
    let change = verse_change(local(1), 1, 1, "a");
    let header = format!(
        r#"{{"count":1,"project":"{}","replaces":[],"sealed":0,"sealer":"{}","signature":"x","tcpack":1}}"#,
        project(),
        local(1)
    );
    let mut bytes = line_of(&header);
    bytes.extend_from_slice(&encode_line(change.id(), change.raw()));
    let id = tc_journal::PackId::of(&bytes);
    let path = format!(
        "ingredients/tc/changes/{}/1970-01/{}.ndjson",
        local(1),
        id.name()
    );
    let read = read_pack(&path, &bytes, project());
    assert_eq!(read.status, PackStatus::Valid);
    assert_eq!(read.entries, vec![Verified::Change(change)]);
}

// ---------------------------------------------------------------------------------------
// format.json
// ---------------------------------------------------------------------------------------

fn format(name: &str) -> FormatDoc {
    FormatDoc::parse(&std::fs::read(fixtures().join(name)).unwrap()).unwrap()
}

#[test]
fn format_json_is_written_in_one_byte_form() {
    let path = fixtures().join("format-1.0.json");
    let bytes = FormatDoc::current().to_bytes();
    if std::env::var_os("TC_JOURNAL_BLESS").is_some() {
        std::fs::write(&path, &bytes).unwrap();
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let doc = FormatDoc::parse(&bytes).unwrap();
    assert_eq!(doc, FormatDoc::current());
    assert_eq!(doc.to_bytes(), bytes);
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("version\":\"0."), "no app version: {text}");
}

#[test]
fn the_gate_refuses_newer_majors_and_reads_newer_minors() {
    assert_eq!(FormatDoc::current().gate(), FormatGate::Writable);
    assert_eq!(format("format-1.3-newer.json").gate(), FormatGate::Writable);
    assert_eq!(
        format("format-1.4-read-only.json").gate(),
        FormatGate::ReadOnly {
            min_reader_minor: 4
        }
    );
    assert_eq!(
        gate(&std::fs::read(fixtures().join("format-2.0.json")).unwrap()),
        Ok(FormatGate::Refused { major: 2 })
    );
    let mut old = FormatDoc::current();
    old.major = 0;
    assert_eq!(old.gate(), FormatGate::NeedsMigration { major: 0 });
    assert!(FormatDoc::parse(b"{}").is_err());
    assert!(FormatDoc::parse(b"not json").is_err());
    assert!(FormatDoc::parse(br#"{"format":"x","major":1,"minor":0,"minReaderMinor":0,"derivations":{},"derivedPaths":[]}"#).is_err());
}

#[test]
fn a_newer_format_json_survives_an_older_app() {
    let newer = format("format-1.3-newer.json");
    let back = FormatDoc::parse(&newer.to_bytes()).unwrap();
    assert_eq!(back, newer);
    assert!(back.extra.contains_key("signing"));
    assert_eq!(back.derivations.get("wordDiff"), Some(&1));
    // Rule 4: a path the newer format derives and this app does not is kept verbatim.
    assert!(newer.is_newer_derived("ingredients/tc/notes/TIT.json"));
    assert!(!newer.is_newer_derived("ingredients/TIT.usfm"));
    assert!(!FormatDoc::current().is_newer_derived("ingredients/tc/notes/TIT.json"));
}

#[test]
fn merging_format_documents_converges_on_the_higher_minor() {
    let docs = [
        FormatDoc::current(),
        format("format-1.3-newer.json"),
        format("format-1.4-read-only.json"),
    ];
    for a in &docs {
        assert_eq!(a.merge(a), *a, "idempotent");
        for b in &docs {
            assert_eq!(a.merge(b), b.merge(a), "commutative");
            for c in &docs {
                assert_eq!(a.merge(b).merge(c), a.merge(&b.merge(c)), "associative");
            }
        }
    }
    let merged = docs[0].merge(&docs[1]);
    assert_eq!((merged.minor, merged.min_reader_minor), (3, 0));
    assert!(merged.extra.contains_key("signing"));
    let merged = merged.merge(&docs[2]);
    assert_eq!((merged.minor, merged.min_reader_minor), (4, 4));
    assert_eq!(merged.derivations.get("plainText"), Some(&2));
    // Rule 6: writing never lowers a version.
    assert_eq!(merged.raised_to(1).minor, 4);
    assert_eq!(FormatDoc::current().raised_to(2).minor, 2);
}

#[test]
fn minor_0_derives_the_paths_of_section_12_1() {
    let paths: Vec<String> = derived_paths(0)
        .iter()
        .map(|p| p.as_str().to_owned())
        .collect();
    assert_eq!(
        paths,
        [
            "ingredients/*.usfm",
            "ingredients/vrs.json",
            "ingredients/checking/**",
            "ingredients/content/*.md",
            "ingredients/tc/comments/*.json",
            "ingredients/tc/conflicts.json",
            "ingredients/tc/format.json",
        ]
    );
    assert_eq!(derived_paths(7), derived_paths(0));
    assert!(PathPattern::parse("").is_none());
}
