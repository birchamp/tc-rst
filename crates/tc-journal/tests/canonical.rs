//! Canonical bytes of changes (`docs/STORAGE.md` §2.5, §4.1).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use std::sync::Arc;
use tc_journal::json;
use tc_journal::line::{encode_line, read_line, verify_line};
use tc_journal::record::{MarkerTerm, MarkerWs, UnknownRecord, VerseValue};
use tc_journal::{
    BookId, Cause, Change, ChangeBody, ChangeError, ChangeId, DeviceSeq, Record, RuleViolation,
    Verified, WallTime, parse_change,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(768))]

    /// Fails if the encoder and the parser disagree on any value.
    #[test]
    fn canonical_change_round_trip(body in arb_body()) {
        let change = Change::new(body.clone()).unwrap_or_else(|e| panic!("{e}: {body:?}"));
        prop_assert_eq!(change.body(), &body);
        prop_assert_eq!(change.id(), ChangeId::of(change.raw()));

        // The bytes are canonical: parsing and writing them again changes nothing.
        let value = json::parse(change.raw()).unwrap();
        prop_assert_eq!(value.to_canonical(), change.raw().to_vec());

        // Read back as a reader does, from the line.
        let line = encode_line(change.id(), change.raw());
        prop_assert_eq!(*line.last().unwrap(), b'\n');
        prop_assert_eq!(line.iter().filter(|b| **b == b'\n').count(), 1);
        let read = read_line(&line[..line.len() - 1]).unwrap();
        prop_assert_eq!(read, Verified::Change(change.clone()));
        let again = parse_change(change.id(), change.raw().clone());
        prop_assert_eq!(again.as_change().map(Change::body), Some(&body));
    }

    /// One flipped byte in a line is never read as a change.
    #[test]
    fn a_damaged_line_never_verifies(body in arb_body(), at in any::<prop::sample::Index>(), bit in 0u8..8) {
        let change = Change::new(body).unwrap();
        let mut line = encode_line(change.id(), change.raw());
        line.pop();
        let at = at.index(line.len());
        line[at] ^= 1 << bit;
        prop_assert!(verify_line(&line).is_none());
    }
}

#[test]
fn the_example_of_section_2_5_has_these_bytes() {
    let body = ChangeBody {
        v: 0,
        record: Record::Verse {
            book: BookId::Titus,
            chapter: chapter(1),
            verse: verse_no(4),
            value: VerseValue::Text {
                marker: MarkerWs::canonical(),
                words: "Ellos deben enseñar".into(),
                layout: "\n\\p\n".into(),
                last: verse_no(5),
                from: None,
            },
        },
        parents: vec![ChangeId::from_bytes([0x9f; 32])],
        device: local(3),
        seq: DeviceSeq(4182),
        at: WallTime(1_760_520_000_000),
        batch: batch(0x6f, 2),
        cause: Cause::Edit,
    };
    let change = Change::new(body).unwrap();
    let expected = format!(
        concat!(
            r#"{{"at":1760520000000,"batch":{{"id":"{batch}","n":2}},"cause":{{"edit":{{}}}},"#,
            r#""device":"{device}","parents":["{parent}"],"record":{{"verse":{{"book":"TIT","#,
            r#""chapter":1,"value":{{"text":{{"last":5,"layout":"\n\\p\n","marker":{{"lead":" ","#,
            r#""term":"space"}},"words":"Ellos deben enseñar"}}}},"verse":4}}}},"seq":4182,"v":0}}"#
        ),
        batch = "6f".repeat(16),
        device = local(3),
        parent = "9f".repeat(32),
    );
    assert_eq!(std::str::from_utf8(change.raw()).unwrap(), expected);
}

/// The wire form of every kind is pinned by a fixture: decoding each pinned line and
/// encoding it again must give the same bytes, so a change to the encoder (which would
/// change every device's ids and needs a new major, §12.2 rule 7) fails here. Set
/// `TC_JOURNAL_BLESS=1` to write the fixture again from the generator after a deliberate
/// format change.
#[test]
fn every_kind_has_the_pinned_wire_form() {
    let path = fixtures().join("v0-every-kind.ndjson");
    if std::env::var_os("TC_JOURNAL_BLESS").is_some() {
        let mut runner = proptest::test_runner::TestRunner::deterministic();
        let mut kinds = std::collections::BTreeSet::new();
        let mut written = Vec::new();
        let mut count = 0;
        while kinds.len() < 18 || count < 64 {
            let body = arb_body().new_tree(&mut runner).unwrap().current();
            kinds.insert(body.record.key().kind_name().to_owned());
            let change = Change::new(body).unwrap();
            written.extend_from_slice(&encode_line(change.id(), change.raw()));
            count += 1;
            assert!(count < 2000, "the generator never made {kinds:?}");
        }
        std::fs::write(&path, &written).unwrap();
    }
    let pinned = std::fs::read(&path).unwrap();
    let mut kinds = std::collections::BTreeSet::new();
    for line in pinned.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        let Some(Verified::Change(change)) = read_line(line) else {
            panic!(
                "a pinned line no longer reads: {}",
                String::from_utf8_lossy(line)
            )
        };
        kinds.insert(change.body().record.key().kind_name().to_owned());
        let again = Change::new(change.body().clone()).unwrap();
        assert!(
            again.raw() == change.raw(),
            "the canonical bytes changed for {}",
            String::from_utf8_lossy(line)
        );
    }
    assert_eq!(kinds.len(), 18, "{kinds:?}");
}

#[test]
fn every_enum_variant_has_its_documented_name() {
    let body = |record: Record, cause: Cause| ChangeBody {
        v: 0,
        record,
        parents: vec![],
        device: local(1),
        seq: DeviceSeq(1),
        at: WallTime(0),
        batch: batch(1, 1),
        cause,
    };
    let verse = |term| Record::Verse {
        book: BookId::Genesis,
        chapter: chapter(1),
        verse: verse_no(1),
        value: VerseValue::Text {
            marker: MarkerWs::new("\t", term).unwrap(),
            words: String::new(),
            layout: String::new(),
            last: verse_no(1),
            from: None,
        },
    };
    for (term, name) in [
        (MarkerTerm::Space, "\"term\":\"space\""),
        (MarkerTerm::Tab, "\"term\":\"tab\""),
        (MarkerTerm::Lf, "\"term\":\"lf\""),
        (MarkerTerm::Cr, "\"term\":\"cr\""),
    ] {
        let change = Change::new(body(verse(term), Cause::Seed)).unwrap();
        let text = std::str::from_utf8(change.raw()).unwrap();
        assert!(text.contains(name), "{text}");
        assert!(text.contains("\"cause\":{\"seed\":{}}"), "{text}");
        assert!(text.contains("\"lead\":\"\\t\""), "{text}");
    }
    let change = Change::new(body(
        verse(MarkerTerm::Space),
        Cause::System(tc_journal::SystemReason::PinCoverageBackfill),
    ))
    .unwrap();
    assert!(
        std::str::from_utf8(change.raw())
            .unwrap()
            .contains("\"cause\":{\"system\":\"pinCoverageBackfill\"}")
    );
}

#[test]
fn a_cause_from_a_newer_app_is_never_written() {
    let mut body = verse_body(local(1), 1, 1, "a");
    body.cause = Cause::Newer {
        name: "signed".into(),
    };
    assert_eq!(Change::new(body), Err(ChangeError::NewerCause));
    let mut body = verse_body(local(1), 1, 1, "a");
    body.v = 1;
    assert_eq!(Change::new(body), Err(ChangeError::NewerMinor { v: 1 }));
}

#[test]
fn an_unknown_record_is_written_back_from_its_stored_json() {
    let mut body = verse_body(local(1), 1, 1, "a");
    body.record = Record::Unknown(UnknownRecord {
        kind: "verse".into(),
        key_json: "{}".into(),
        value_json: None,
    });
    assert!(matches!(
        Change::new(body.clone()),
        Err(ChangeError::BadUnknownRecord(_))
    ));
    body.record = Record::Unknown(UnknownRecord {
        kind: "note".into(),
        key_json: "{ }".into(),
        value_json: None,
    });
    assert!(matches!(
        Change::new(body),
        Err(ChangeError::BadUnknownRecord(_))
    ));
}

#[test]
fn bytes_that_are_not_canonical_are_quarantined_under_v1() {
    let good = verse_change(local(1), 1, 1, "a");
    let text = std::str::from_utf8(good.raw()).unwrap();
    let variants = [
        text.replacen(",", ", ", 1),
        text.replacen("\"at\":", "\"at\" :", 1),
        text.replacen("{\"at\"", "{\"v\":0,\"at\"", 1),
        text.replacen("\"words\":\"a\"", "\"words\":\"\\u0061\"", 1),
        text.replacen("\"seq\":1", "\"seq\":01", 1),
        text.replacen("\"seq\":1", "\"seq\":1.0", 1),
        format!("{text}\n"),
    ];
    for json in variants {
        let read = parse_change(ChangeId::of(json.as_bytes()), Arc::from(json.as_bytes()));
        match read {
            Verified::Quarantined(q) => {
                assert!(
                    matches!(q.rule, RuleViolation::Shape { .. }),
                    "{json}: {:?}",
                    q.rule
                )
            }
            Verified::Change(_) => panic!("accepted {json}"),
        }
    }
}

#[test]
fn quarantined_changes_keep_their_batch_and_device_when_they_read() {
    let good = verse_change(local(7), 9, 1, "a");
    let text = std::str::from_utf8(good.raw())
        .unwrap()
        .replace("\"book\":\"TIT\"", "\"book\":\"tit\"");
    let read = parse_change(ChangeId::of(text.as_bytes()), raw(&text));
    let Verified::Quarantined(q) = read else {
        panic!("a lowercase book code was accepted")
    };
    assert_eq!(q.batch, Some(batch(9, 1)));
    assert_eq!(q.device, Some(local(7)));
    assert_eq!(q.seq, Some(DeviceSeq(9)));
    assert_eq!(
        q.rule.to_string(),
        "V1: /record/verse/book: unknown book \"tit\""
    );
    assert_eq!(
        Verified::Quarantined(q.clone()).sort_key(),
        (Some(local(7)), Some(DeviceSeq(9)), q.id)
    );
}
