//! Packs, pending files, validation, salvage, size limits and consolidation
//! (`docs/STORAGE.md` §4.2 to §4.7).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use proptest::prelude::*;
use std::collections::BTreeSet;
use std::sync::Arc;
use tc_journal::consolidate::{
    OwnPack, Redundancy, SMALL_PACK_BYTES, consolidate, consolidation_groups, redundancy,
};
use tc_journal::json::Json;
use tc_journal::line::{LINE_OVERHEAD, encode_line};
use tc_journal::pack::{
    EncodedPack, MAX_PACK_BYTES, PackError, PackFacts, PackPath, PackRead, PackSpec, PackStatus,
    PackWriteError, YearMonth, encode_pack, read_pack, seal,
};
use tc_journal::pending::{
    PendingHeader, PendingStatus, PendingWriteError, Purpose, Reconciles, encode_pending,
    read_pending,
};
use tc_journal::rules::Rule;
use tc_journal::{
    Change, ChangeId, DeviceId, DeviceKind, Md5, PackId, PackName, Record, RepoPath, SettingKey,
    Verified, WallTime, parse_change,
};

const OCT_2026: i64 = 1_791_000_000_000;

fn spec(sealer: DeviceId) -> PackSpec {
    PackSpec {
        project: project(),
        sealer,
        sealed: WallTime(OCT_2026),
    }
}

fn changes(n: u64) -> Vec<Verified> {
    (1..=n)
        .map(|i| {
            verse_change(
                local((i % 3) as u8 + 1),
                i,
                (i % 20) as u16 + 1,
                &format!("texto {i}"),
            )
            .into()
        })
        .collect()
}

fn reread(pack: &EncodedPack) -> PackRead {
    read_pack(&pack.path.to_string(), &pack.bytes, project())
}

// ---------------------------------------------------------------------------------------
// Packs
// ---------------------------------------------------------------------------------------

#[test]
fn a_sealed_pack_reads_back_whole_and_sorted() {
    let lines = changes(12);
    let mut doubled = lines.clone();
    doubled.extend(lines.iter().cloned());
    let packs = seal(&spec(local(9)), &doubled).unwrap().packs;
    assert_eq!(packs.len(), 1);
    let pack = &packs[0];
    assert_eq!(pack.path.sealer, local(9));
    assert_eq!(pack.path.month.to_string(), "2026-10");
    assert_eq!(pack.path.name, pack.id.name());
    assert_eq!(pack.id, pack_id_of(&pack.bytes));
    assert_eq!(
        pack.path.to_string(),
        format!(
            "ingredients/tc/changes/{}/2026-10/{}.ndjson",
            local(9),
            pack.id.name()
        )
    );
    let read = reread(pack);
    assert_eq!(read.status, PackStatus::Valid);
    let header = read.header.as_ref().unwrap();
    assert_eq!(header.count, 12);
    assert_eq!(header.sealer, local(9));
    assert_eq!(header.sealed, WallTime(OCT_2026));
    assert!(header.replaces.is_empty());
    assert_eq!(read.entries.len(), 12);
    let keys: Vec<_> = read.entries.iter().map(Verified::sort_key).collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "lines sorted by (device, seq, id)");
    assert_eq!(
        read.entries
            .iter()
            .map(Verified::id)
            .collect::<BTreeSet<_>>(),
        lines.iter().map(Verified::id).collect::<BTreeSet<_>>()
    );
    let text = std::str::from_utf8(&pack.bytes).unwrap();
    assert!(text.starts_with(&format!(
        "{{\"count\":12,\"project\":\"{}\",\"replaces\":[],\"sealed\":{OCT_2026},\"sealer\":\"{}\",\"tcpack\":1}}\t",
        project(),
        local(9)
    )));
}

#[test]
fn a_pack_with_a_quarantined_change_is_still_valid() {
    let mut lines = changes(3);
    let bad = r#"{"what":"not a change"}"#;
    lines.push(tc_journal::line::read_line(&line_of(bad)[..line_of(bad).len() - 1]).unwrap());
    let pack = &seal(&spec(local(9)), &lines).unwrap().packs[0];
    let read = reread(pack);
    assert_eq!(read.status, PackStatus::Valid);
    assert_eq!(read.entries.len(), 4);
    assert_eq!(
        read.entries
            .iter()
            .filter(|e| matches!(e, Verified::Quarantined(_)))
            .count(),
        1
    );
    // The quarantined line sorts first: its device and seq could not be read.
    assert!(matches!(read.entries[0], Verified::Quarantined(_)));
}

#[test]
fn p1_the_path_must_name_a_local_sealer_a_month_and_the_hash() {
    let pack = &seal(&spec(local(9)), &changes(2)).unwrap().packs[0];
    let good = pack.path.to_string();
    assert_eq!(PackPath::parse(&good), Some(pack.path));
    let actor = DeviceId::derived(DeviceKind::Tc4Actor, b"x");
    for bad in [
        good.replace("ingredients/tc/changes", "ingredients/tc/change"),
        good.replace("2026-10", "2026-13"),
        good.replace("2026-10", "26-10"),
        good.replace(".ndjson", ".json"),
        good.replace(&local(9).to_string(), &actor.to_string()),
        good.replace("/2026-10/", "/2026-10/x/"),
        good.replace(&pack.id.name().to_hex(), &pack.id.to_hex()),
    ] {
        assert_eq!(PackPath::parse(&bad), None, "{bad}");
        let read = read_pack(&bad, &pack.bytes, project());
        assert_eq!(
            read.status,
            PackStatus::Damaged(PackError::BadPath),
            "{bad}"
        );
        assert_eq!(read.entries.len(), 2, "salvaged");
    }
}

#[test]
fn p2_the_name_is_the_files_hash() {
    let a = &seal(&spec(local(9)), &changes(2)).unwrap().packs[0];
    let b = &seal(&spec(local(9)), &changes(3)).unwrap().packs[0];
    let read = read_pack(&b.path.to_string(), &a.bytes, project());
    assert_eq!(read.status, PackStatus::Damaged(PackError::NameMismatch));
    assert_eq!(read.entries.len(), 2);
}

#[test]
fn p3_foreign_projects_are_not_read_and_the_sealer_must_be_the_folder() {
    let pack = &seal(&spec(local(9)), &changes(2)).unwrap().packs[0];
    let read = read_pack(&pack.path.to_string(), &pack.bytes, other_project());
    assert_eq!(read.status, PackStatus::Foreign { project: project() });
    assert!(read.entries.is_empty());

    let moved = pack
        .path
        .to_string()
        .replace(&local(9).to_string(), &local(8).to_string());
    let read = read_pack(&moved, &pack.bytes, project());
    assert_eq!(
        read.status,
        PackStatus::Damaged(PackError::SealerMismatch {
            header: local(9),
            folder: local(8)
        })
    );
    assert_eq!(read.entries.len(), 2);
}

/// A verified header that names another project makes the file foreign whatever else is
/// wrong with it: salvaging its lines would merge two projects.
#[test]
fn a_damaged_pack_of_another_project_is_foreign_not_salvaged() {
    let pack = &seal(&spec(local(9)), &changes(3)).unwrap().packs[0];
    let mut damaged = pack.bytes.clone();
    let last = damaged.len() - 2;
    damaged[last] ^= 1;
    let read = read_pack(&pack.path.to_string(), &damaged, project());
    assert!(matches!(read.status, PackStatus::Damaged(_)));
    assert_eq!(read.entries.len(), 2);

    for bytes in [&pack.bytes, &damaged] {
        let read = read_pack("not/a/pack/path", bytes, other_project());
        assert_eq!(read.status, PackStatus::Foreign { project: project() });
        assert!(read.entries.is_empty());
    }
}

fn hand_pack(header: &str, lines: &[&Change], trailing: &[u8]) -> (String, Vec<u8>) {
    let mut bytes = line_of(header);
    for change in lines {
        bytes.extend_from_slice(&encode_line(change.id(), change.raw()));
    }
    bytes.extend_from_slice(trailing);
    let name = pack_id_of(&bytes).name();
    (
        format!("ingredients/tc/changes/{}/2026-10/{name}.ndjson", local(9)),
        bytes,
    )
}

#[test]
fn p3_and_p4_the_header_and_the_line_count() {
    let a = verse_change(local(1), 1, 1, "a");
    let b = verse_change(local(1), 2, 2, "b");
    let header = |count: u32, tcpack: u32| {
        format!(
            r#"{{"count":{count},"project":"{}","replaces":[],"sealed":0,"sealer":"{}","tcpack":{tcpack}}}"#,
            project(),
            local(9)
        )
    };
    let (path, bytes) = hand_pack(&header(2, 1), &[&a, &b], b"");
    assert_eq!(
        read_pack(&path, &bytes, project()).status,
        PackStatus::Valid
    );

    let (path, bytes) = hand_pack(&header(3, 1), &[&a, &b], b"");
    let read = read_pack(&path, &bytes, project());
    assert_eq!(
        read.status,
        PackStatus::Damaged(PackError::CountMismatch {
            expected: 3,
            found: 2
        })
    );
    assert_eq!(read.entries.len(), 2);

    let (path, bytes) = hand_pack(&header(2, 0), &[&a, &b], b"");
    assert!(matches!(
        read_pack(&path, &bytes, project()).status,
        PackStatus::Damaged(PackError::HeaderMalformed(_))
    ));

    let (path, bytes) = hand_pack(&header(2, 1), &[&a, &b], b"garbage");
    let read = read_pack(&path, &bytes, project());
    assert_eq!(
        read.status,
        PackStatus::Damaged(PackError::LineUnverified { line: 3 })
    );
    assert_eq!(read.entries.len(), 2);

    let unsorted = format!(
        r#"{{"count":2,"project":"{}","replaces":["{}","{}"],"sealed":0,"sealer":"{}","tcpack":1}}"#,
        project(),
        "b".repeat(32),
        "a".repeat(32),
        local(9)
    );
    let (path, bytes) = hand_pack(&unsorted, &[&a, &b], b"");
    assert!(matches!(
        read_pack(&path, &bytes, project()).status,
        PackStatus::Damaged(PackError::HeaderMalformed(_))
    ));

    let mut damaged_header = line_of(&header(2, 1));
    damaged_header[3] ^= 1;
    let mut bytes = damaged_header;
    bytes.extend_from_slice(&encode_line(a.id(), a.raw()));
    let path = format!(
        "ingredients/tc/changes/{}/2026-10/{}.ndjson",
        local(9),
        PackId::of(&bytes).name()
    );
    let read = read_pack(&path, &bytes, project());
    assert_eq!(
        read.status,
        PackStatus::Damaged(PackError::HeaderUnverified)
    );
    assert_eq!(read.entries, vec![Verified::Change(a.clone())]);

    let empty_path = format!(
        "ingredients/tc/changes/{}/2026-10/{}.ndjson",
        local(9),
        PackId::of(b"").name()
    );
    assert_eq!(
        read_pack(&empty_path, b"", project()).status,
        PackStatus::Damaged(PackError::Empty)
    );
}

#[test]
fn a_truncated_pack_salvages_every_whole_line() {
    let pack = &seal(&spec(local(9)), &changes(5)).unwrap().packs[0];
    let cut = pack.bytes.len() - 10;
    let read = read_pack(&pack.path.to_string(), &pack.bytes[..cut], project());
    assert_eq!(
        read.status,
        PackStatus::Damaged(PackError::LineUnverified { line: 5 })
    );
    assert_eq!(read.entries.len(), 4);
}

/// Every line feed of `bytes` written as CR LF, as a tool that rewrites line endings would.
fn crlf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + bytes.len() / 16);
    for byte in bytes {
        if *byte == b'\n' {
            out.push(b'\r');
        }
        out.push(*byte);
    }
    out
}

/// A copy through a tool that rewrites line endings leaves every change intact: salvage
/// reads a line whose line feed became CR LF (§4.4), while validation still finds the file
/// damaged, so it is quarantined and its changes rewritten in canonical form.
#[test]
fn a_file_with_crlf_line_endings_salvages_every_change() {
    let lines = changes(5);
    let pack = &seal(&spec(local(9)), &lines).unwrap().packs[0];
    let path = pack.path.to_string();
    let read = read_pack(&path, &crlf(&pack.bytes), project());
    assert_eq!(
        read.status,
        PackStatus::Damaged(PackError::HeaderUnverified)
    );
    assert_eq!(read.entries, reread(pack).entries);
    let file = encode_pending(&pending_header(Purpose::Save), &lines).unwrap();
    let read = read_pending(&crlf(&file.bytes), project());
    assert_eq!(
        read.status,
        PendingStatus::Damaged(PackError::HeaderUnverified)
    );
    assert_eq!(read.entries, lines);

    // Another project's file is still foreign, and a newer file still newer.
    assert_eq!(
        read_pack(&path, &crlf(&pack.bytes), other_project()).status,
        PackStatus::Foreign { project: project() }
    );
    assert_eq!(
        read_pending(&crlf(&file.bytes), other_project()).status,
        PendingStatus::Foreign { project: project() }
    );
    let newer = format!(
        r#"{{"count":1,"project":"{}","replaces":[],"sealed":0,"sealer":"{}","tcpack":2}}"#,
        project(),
        local(9)
    );
    let a = verse_change(local(1), 1, 1, "a");
    let (path, bytes) = hand_pack(&newer, &[&a], b"");
    let read = read_pack(&path, &crlf(&bytes), project());
    assert_eq!(read.status, PackStatus::Newer { tcpack: 2 });
    assert!(read.entries.is_empty());
}

/// The bytes a span names in `file`.
fn at_span(file: &[u8], span: tc_journal::line::Span) -> &[u8] {
    let start = usize::try_from(span.offset).unwrap();
    &file[start..start + span.len as usize]
}

/// A loader that drops values reads them again by span (§14), so each span names exactly
/// its entry's canonical bytes: in a valid pack, in a salvaged one and in a pending file.
#[test]
fn every_entry_names_where_its_bytes_lie() {
    let lines = changes(6);
    let pack = &seal(&spec(local(9)), &lines).unwrap().packs[0];
    let damaged = crlf(&pack.bytes);
    let file = encode_pending(&pending_header(Purpose::Save), &lines).unwrap();
    let reads = [
        (pack.bytes.clone(), reread(pack).entries, reread(pack).spans),
        {
            let read = read_pack(&pack.path.to_string(), &damaged, project());
            (damaged.clone(), read.entries, read.spans)
        },
        {
            let read = read_pending(&file.bytes, project());
            (file.bytes.clone(), read.entries, read.spans)
        },
    ];
    for (bytes, entries, spans) in reads {
        assert_eq!(entries.len(), 6);
        assert_eq!(spans.len(), entries.len());
        for (entry, span) in entries.iter().zip(spans) {
            let change = entry.as_change().unwrap();
            assert_eq!(at_span(&bytes, span), &change.raw()[..]);
        }
    }
}

/// A change line is a change wherever it stands, so a file whose header was lost salvages
/// its first line too. A header, of a pack or of a pending file, never reads as a change.
#[test]
fn salvage_takes_the_first_line_unless_it_is_a_header() {
    let lines = changes(3);
    let bytes: Vec<u8> = lines
        .iter()
        .flat_map(|line| encode_line(line.id(), line.raw()))
        .collect();
    let path = format!(
        "ingredients/tc/changes/{}/2026-10/{}.ndjson",
        local(9),
        PackId::of(&bytes).name()
    );
    let read = read_pack(&path, &bytes, project());
    assert!(matches!(
        read.status,
        PackStatus::Damaged(PackError::HeaderMalformed(_))
    ));
    assert_eq!(read.entries, lines);
    let read = read_pending(&bytes, project());
    assert!(matches!(
        read.status,
        PendingStatus::Damaged(PackError::HeaderMalformed(_))
    ));
    assert_eq!(read.entries, lines);

    // A pending file's header on a pack, and a pack's on a pending file, are headers too.
    let pending = encode_pending(&pending_header(Purpose::Save), &lines).unwrap();
    let path = format!(
        "ingredients/tc/changes/{}/2026-10/{}.ndjson",
        local(9),
        PackId::of(&pending.bytes).name()
    );
    let read = read_pack(&path, &pending.bytes, project());
    assert!(matches!(read.status, PackStatus::Damaged(_)));
    assert_eq!(read.entries, lines);
    let pack = &seal(&spec(local(9)), &lines).unwrap().packs[0];
    let read = read_pending(&pack.bytes, project());
    assert!(matches!(read.status, PendingStatus::Damaged(_)));
    assert_eq!(read.entries, reread(pack).entries);
}

/// The byte ranges of a pack's change lines, each with the line feed before and after it.
fn line_spans(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
    while start < bytes.len() {
        let end = start + bytes[start..].iter().position(|b| *b == b'\n').unwrap();
        spans.push((start - 1, end));
        start = end + 1;
    }
    spans
}

/// An intact pack passes P2 to P4; a damaged one salvages exactly the lines whose bytes
/// (and the line feeds around them) are intact, and nothing else.
#[test]
fn pack_round_trip_and_salvage() {
    let strategy = (
        prop::collection::vec(arb_body(), 1..10),
        prop::collection::vec((any::<prop::sample::Index>(), 0u8..8), 1..4),
    );
    run_with_census(
        256,
        strategy,
        &[
            "every line salvaged",
            "some lines salvaged",
            "no line salvaged",
        ],
        |(bodies, flips), census| {
            let lines: Vec<Verified> = bodies
                .into_iter()
                .map(|b| Change::new(b).unwrap().into())
                .collect();
            let pack = seal(&spec(local(9)), &lines).unwrap().packs.remove(0);
            let intact = reread(&pack);
            prop_assert_eq!(&intact.status, &PackStatus::Valid);
            prop_assert_eq!(
                intact
                    .entries
                    .iter()
                    .map(Verified::id)
                    .collect::<BTreeSet<_>>(),
                lines.iter().map(Verified::id).collect::<BTreeSet<_>>()
            );

            let mut damaged = pack.bytes.clone();
            for (at, bit) in &flips {
                let at = at.index(damaged.len());
                damaged[at] ^= 1 << bit;
            }
            if damaged == pack.bytes {
                return Ok(());
            }
            // Repeated flips can restore a byte; salvage depends on the final bytes.
            let touched: BTreeSet<usize> = damaged
                .iter()
                .zip(&pack.bytes)
                .enumerate()
                .filter_map(|(at, (after, before))| (after != before).then_some(at))
                .collect();
            let read = read_pack(&pack.path.to_string(), &damaged, project());
            prop_assert!(
                matches!(read.status, PackStatus::Damaged(_)),
                "{:?}",
                read.status
            );
            let spans = line_spans(&pack.bytes);
            let expected: BTreeSet<ChangeId> = spans
                .iter()
                .zip(intact.entries.iter())
                .filter(|((from, to), _)| touched.range(from..=to).next().is_none())
                .map(|(_, entry)| entry.id())
                .collect();
            let salvaged: BTreeSet<ChangeId> = read.entries.iter().map(Verified::id).collect();
            prop_assert_eq!(&salvaged, &expected);
            census.saw(match salvaged.len() {
                0 => "no line salvaged",
                n if n == lines.len() => "every line salvaged",
                _ => "some lines salvaged",
            });
            for entry in &read.entries {
                prop_assert!(
                    intact.entries.contains(entry),
                    "a salvaged change differs from what its author wrote"
                );
            }
            Ok(())
        },
    );
}

#[test]
fn restoring_a_flipped_byte_keeps_its_line_salvageable() {
    let lines = changes(1);
    let pack = seal(&spec(local(9)), &lines).unwrap().packs.remove(0);
    let mut damaged = pack.bytes.clone();
    let line_start = damaged.iter().position(|b| *b == b'\n').unwrap() + 1;
    damaged[line_start + 10] ^= 1 << 6;
    damaged[line_start + 10] ^= 1 << 6;
    damaged[0] ^= 1;
    let read = read_pack(&pack.path.to_string(), &damaged, project());
    assert!(matches!(read.status, PackStatus::Damaged(_)));
    assert_eq!(read.entries, lines);
}

/// A valid change whose line, with its tab, hash and line feed, is `len` bytes long.
fn line_of_size(seq: u64, len: usize) -> Verified {
    let base = verse_body(local(1), seq, 1, "x");
    let mut body = base.clone();
    body.record = Record::Setting {
        key: SettingKey::parse("big").unwrap(),
        value: Some(Json::from("")),
    };
    let empty = Change::new(body.clone()).unwrap();
    let fill = len - empty.raw().len() - LINE_OVERHEAD;
    body.record = Record::Setting {
        key: SettingKey::parse("big").unwrap(),
        value: Some(Json::from("a".repeat(fill))),
    };
    let change = Change::new(body).unwrap();
    assert_eq!(change.raw().len() + LINE_OVERHEAD, len);
    change.into()
}

/// A line of the largest size a change may have.
fn big_line(seq: u64) -> Verified {
    line_of_size(seq, tc_journal::rules::MAX_LINE_BYTES)
}

#[test]
fn a_seal_splits_at_64_mib_on_line_boundaries() {
    // Sixteen lines of the largest size fill 64 MiB exactly, so with a header they cannot
    // share one pack.
    let lines: Vec<Verified> = (1..=17).map(big_line).collect();
    let packs = seal(&spec(local(9)), &lines).unwrap().packs;
    assert_eq!(packs.len(), 2);
    let mut ids = BTreeSet::new();
    for pack in &packs {
        assert!(pack.bytes.len() <= MAX_PACK_BYTES, "{}", pack.bytes.len());
        let read = reread(pack);
        assert_eq!(read.status, PackStatus::Valid);
        ids.extend(read.entries.iter().map(Verified::id));
    }
    assert_eq!(packs[0].changes.len(), 15);
    assert_eq!(ids.len(), 17);
    assert!(matches!(
        encode_pack(&spec(local(9)), &lines, &[]),
        Err(PackWriteError::TooLarge { .. })
    ));
}

/// A line too long for any pack, which only a change that V7 quarantines can be (a
/// salvaged one, §4.4), is set aside instead of failing the seal: every other line is
/// sealed (§5.11 K1). A line that just fits alone with its header is sealed.
#[test]
fn a_line_no_pack_can_hold_is_set_aside_and_the_rest_is_sealed() {
    let line = |len: usize| {
        let json = format!("{{\"x\":\"{}\"}}", "a".repeat(len - 8));
        assert_eq!(json.len(), len);
        parse_change(ChangeId::of(json.as_bytes()), Arc::from(json.as_bytes()))
    };
    let huge = line(65 << 20);
    assert!(matches!(&huge, Verified::Quarantined(q) if q.rule.rule() == Rule::V7));
    let normal = changes(2);
    let mut all = normal.clone();
    all.push(huge.clone());
    // As a salvage batch would hold them.
    let file = encode_pending(&pending_header(Purpose::Salvage), &all).unwrap();
    let read = read_pending(&file.bytes, project());
    assert_eq!(read.status, PendingStatus::Valid);
    let sealed = seal(&spec(local(9)), &read.entries).unwrap();
    assert_eq!(sealed.set_aside, vec![huge.id()]);
    assert_eq!(sealed.packs.len(), 1);
    assert_eq!(reread(&sealed.packs[0]).entries.len(), 2);

    let header = sealed.packs[0]
        .bytes
        .iter()
        .position(|b| *b == b'\n')
        .unwrap()
        + 1;
    let header_alone = header - "2".len() + "1".len();
    let just_fits = line(MAX_PACK_BYTES - header_alone - LINE_OVERHEAD);
    let sealed = seal(&spec(local(9)), std::slice::from_ref(&just_fits)).unwrap();
    assert!(sealed.set_aside.is_empty());
    assert_eq!(sealed.packs[0].bytes.len(), MAX_PACK_BYTES);
    assert_eq!(reread(&sealed.packs[0]).status, PackStatus::Valid);
    let one_more = line(MAX_PACK_BYTES - header_alone - LINE_OVERHEAD + 1);
    let sealed = seal(&spec(local(9)), std::slice::from_ref(&one_more)).unwrap();
    assert_eq!(sealed.set_aside, vec![one_more.id()]);
    assert!(sealed.packs.is_empty());
}

/// §4.7: when the union is over 64 MiB, whole old packs are grouped in the order of their
/// first lines into new packs of at most 64 MiB, and a group of one is not rewritten.
#[test]
fn consolidation_groups_old_packs_into_packs_of_at_most_64_mib() {
    // Five lines of the largest size make a pack of about 20 MiB; three fit in 64 MiB.
    let packs: Vec<EncodedPack> = (0..4u64)
        .map(|p| {
            let lines: Vec<Verified> = (1..=5).map(|i| big_line(p * 5 + i)).collect();
            encode_pack(&spec(local(9)), &lines, &[]).unwrap()
        })
        .collect();
    let reads: Vec<PackRead> = packs.iter().map(reread).collect();
    // Given in reverse, so the grouping must follow the packs' first lines.
    let given: Vec<&PackRead> = reads.iter().rev().collect();
    let merged = consolidate(&spec(local(9)), &given).unwrap();
    assert_eq!(merged.len(), 1, "the fourth pack would be a group of one");
    let merged = &merged[0];
    assert!(merged.bytes.len() <= MAX_PACK_BYTES);
    let read = reread(merged);
    assert_eq!(read.status, PackStatus::Valid);
    let mut first_three: Vec<PackName> = packs[..3].iter().map(|p| p.path.name).collect();
    first_three.sort();
    assert_eq!(read.header.unwrap().replaces, first_three);
    let held: BTreeSet<ChangeId> = read.entries.iter().map(Verified::id).collect();
    let expected: BTreeSet<ChangeId> = reads[..3]
        .iter()
        .flat_map(|r| r.entries.iter().map(Verified::id))
        .collect();
    assert_eq!(held, expected);
}

/// The 64 MiB limit is inclusive, for a consolidation as for a seal: lines that fill
/// exactly 64 MiB with their header make one pack, and one byte more makes two.
#[test]
fn lines_that_fill_exactly_64_mib_make_one_pack() {
    // Lines over 4 MiB, which V7 quarantines unread, so the test does not parse 64 MiB of
    // JSON; a pack holds them like any other line.
    let line = |n: u8, len: usize| {
        let json = format!(
            "{{\"n\":{n},\"x\":\"{}\"}}",
            "a".repeat(len - LINE_OVERHEAD - 14)
        );
        assert_eq!(json.len() + LINE_OVERHEAD, len);
        parse_change(ChangeId::of(json.as_bytes()), Arc::from(json.as_bytes()))
    };
    let quarter = MAX_PACK_BYTES / 4;
    let header_of = |pack: &EncodedPack| pack.bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
    // A consolidation's header lists the two packs it replaces; a seal's lists none.
    let replacing =
        header_of(&encode_pack(&spec(local(9)), &changes(4), &[name(1), name(2)]).unwrap());
    let sealing = header_of(&encode_pack(&spec(local(9)), &changes(4), &[]).unwrap());
    let full: Vec<Verified> = (1..=3).map(|n| line(n, quarter)).collect();
    for (extra, packs) in [(0, 1), (1, 2)] {
        let mut lines = full.clone();
        lines.push(line(4, quarter - replacing + extra));
        let a = encode_pack(&spec(local(9)), &lines[..2], &[]).unwrap();
        let b = encode_pack(&spec(local(9)), &lines[2..], &[]).unwrap();
        let reads = [reread(&a), reread(&b)];
        let merged = consolidate(&spec(local(9)), &[&reads[0], &reads[1]]).unwrap();
        // Two packs that cannot share one are each a group of one, which is not rewritten.
        assert_eq!(merged.len(), 2 - packs, "{extra} byte(s) over");
        for pack in &merged {
            assert_eq!(pack.bytes.len(), MAX_PACK_BYTES);
            assert_eq!(pack.changes.len(), 4);
            assert_eq!(reread(pack).status, PackStatus::Valid);
        }

        let mut lines = full.clone();
        lines.push(line(4, quarter - sealing + extra));
        let sealed = seal(&spec(local(9)), &lines).unwrap().packs;
        assert_eq!(sealed.len(), packs, "{extra} byte(s) over");
        assert_eq!(sealed[0].bytes.len() == MAX_PACK_BYTES, extra == 0);
    }
}

/// Only a Local device writes pending files (§4.2): a header that names another kind of
/// device was not written by a save, so the file is damaged and its lines salvaged.
#[test]
fn a_pending_header_names_a_local_device() {
    let lines = changes(2);
    let file = |device: DeviceId| {
        let header = format!(
            r#"{{"device":"{device}","pending":1,"project":"{}","purpose":"save"}}"#,
            project()
        );
        let mut bytes = line_of(&header);
        for line in &lines {
            bytes.extend_from_slice(&encode_line(line.id(), line.raw()));
        }
        bytes
    };
    let read = read_pending(&file(local(1)), project());
    assert_eq!(read.status, PendingStatus::Valid);
    for kind in [
        DeviceKind::Importer,
        DeviceKind::Tc4Actor,
        DeviceKind::Tc3User,
    ] {
        let device = DeviceId::derived(kind, b"not a save");
        let read = read_pending(&file(device), project());
        assert!(
            matches!(
                &read.status,
                PendingStatus::Damaged(PackError::HeaderMalformed(_))
            ),
            "{kind:?}: {:?}",
            read.status
        );
        assert_eq!(read.header, None);
        assert_eq!(read.entries, lines);
    }
}

#[test]
fn only_a_local_device_writes_packs_and_pending_files() {
    let importer = DeviceId::derived(DeviceKind::Importer, b"import\0x");
    assert_eq!(
        seal(&spec(importer), &changes(1)),
        Err(PackWriteError::NotLocal)
    );
    assert_eq!(
        encode_pack(&spec(local(1)), &[], &[]),
        Err(PackWriteError::Empty)
    );
    let header = PendingHeader {
        device: importer,
        project: project(),
        purpose: Purpose::Intake,
        reconciles: None,
    };
    assert_eq!(
        encode_pending(&header, &changes(1)),
        Err(PendingWriteError::NotLocal)
    );
}

// ---------------------------------------------------------------------------------------
// Pending files
// ---------------------------------------------------------------------------------------

fn pending_header(purpose: Purpose) -> PendingHeader {
    PendingHeader {
        device: local(1),
        project: project(),
        purpose,
        reconciles: None,
    }
}

#[test]
fn a_pending_file_reads_back_with_its_lines_in_order() {
    let lines = changes(4);
    let reversed: Vec<Verified> = lines.iter().rev().cloned().collect();
    let file = encode_pending(&pending_header(Purpose::Save), &reversed).unwrap();
    // The name hashes the header line and the line ids (§4.2), as a pack's id does.
    let hash = pack_id_of(&file.bytes).to_hex();
    assert_eq!(file.name, format!("{}-{}.batch", local(1), &hash[..32]));
    let read = read_pending(&file.bytes, project());
    assert_eq!(read.status, PendingStatus::Valid);
    assert_eq!(read.header, Some(pending_header(Purpose::Save)));
    assert_eq!(read.entries, reversed);
    let text = std::str::from_utf8(&file.bytes).unwrap();
    assert!(text.starts_with(&format!(
        "{{\"device\":\"{}\",\"pending\":1,\"project\":\"{}\",\"purpose\":\"save\"}}\t",
        local(1),
        project()
    )));
}

#[test]
fn an_outside_edit_batch_names_what_it_reconciles() {
    let header = PendingHeader {
        purpose: Purpose::Intake,
        reconciles: Some(Reconciles {
            path: RepoPath::parse("ingredients/TIT.usfm").unwrap(),
            seen_md5: Md5::from_bytes([0xab; 16]),
        }),
        ..pending_header(Purpose::Intake)
    };
    let file = encode_pending(&header, &changes(1)).unwrap();
    let read = read_pending(&file.bytes, project());
    assert_eq!(read.header, Some(header));
    assert!(std::str::from_utf8(&file.bytes).unwrap().contains(&format!(
        "\"reconciles\":{{\"path\":\"ingredients/TIT.usfm\",\"seenMd5\":\"{}\"}}",
        "ab".repeat(16)
    )));
    for purpose in [Purpose::Save, Purpose::Intake, Purpose::Salvage] {
        let file = encode_pending(&pending_header(purpose), &changes(1)).unwrap();
        assert_eq!(
            read_pending(&file.bytes, project()).header.unwrap().purpose,
            purpose
        );
    }
}

#[test]
fn a_damaged_pending_file_is_salvaged_and_a_foreign_one_is_not_read() {
    let file = encode_pending(&pending_header(Purpose::Save), &changes(3)).unwrap();
    let mut damaged = file.bytes.clone();
    let second_line = damaged.iter().position(|b| *b == b'\n').unwrap() + 5;
    damaged[second_line] ^= 0x20;
    let read = read_pending(&damaged, project());
    assert_eq!(
        read.status,
        PendingStatus::Damaged(PackError::LineUnverified { line: 1 })
    );
    assert_eq!(read.entries.len(), 2);

    let truncated = &file.bytes[..file.bytes.len() - 1];
    let read = read_pending(truncated, project());
    assert_eq!(read.status, PendingStatus::Damaged(PackError::Truncated));
    assert_eq!(read.entries.len(), 3, "the last line still verifies");

    for bytes in [&file.bytes, &damaged] {
        let read = read_pending(bytes, other_project());
        assert_eq!(read.status, PendingStatus::Foreign { project: project() });
        assert!(read.entries.is_empty());
    }

    assert_eq!(
        read_pending(b"", project()).status,
        PendingStatus::Damaged(PackError::Empty)
    );
    let bad_purpose = format!(
        r#"{{"device":"{}","pending":1,"project":"{}","purpose":"merge"}}"#,
        local(1),
        project()
    );
    assert!(matches!(
        read_pending(&line_of(&bad_purpose), project()).status,
        PendingStatus::Damaged(PackError::HeaderMalformed(_))
    ));
    assert_eq!(
        read_pending(&line_of(&bad_purpose), other_project()).status,
        PendingStatus::Foreign { project: project() }
    );
}

// ---------------------------------------------------------------------------------------
// Consolidation and redundancy
// ---------------------------------------------------------------------------------------

fn name(n: u8) -> PackName {
    PackName::from_bytes([n; 16])
}

#[test]
fn consolidation_merges_completed_months_and_many_small_packs() {
    let month = |m| YearMonth::new(2026, m).unwrap();
    let own = |n, m, size| OwnPack {
        name: name(n),
        month: month(m),
        size,
    };
    let mut packs = vec![
        own(1, 8, 10),
        own(2, 8, 5_000_000),
        own(3, 9, 10),
        own(4, 7, 10),
        own(5, 7, 10),
        own(6, 7, 10),
    ];
    assert_eq!(
        consolidation_groups(&packs, month(10)),
        vec![vec![name(4), name(5), name(6)], vec![name(1), name(2)]]
    );
    for n in 0..32u8 {
        packs.push(own(100 + n, 10, SMALL_PACK_BYTES - 1));
    }
    packs.push(own(200, 10, SMALL_PACK_BYTES));
    assert_eq!(
        consolidation_groups(&packs, month(10)).len(),
        2,
        "32 small packs are not more than 32"
    );
    packs.push(own(201, 10, 1));
    let groups = consolidation_groups(&packs, month(10));
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[2].len(), 33);
    assert!(!groups[2].contains(&name(200)));
}

#[test]
fn a_consolidated_pack_holds_the_union_and_lists_what_it_replaces() {
    let all = changes(9);
    let a = seal(&spec(local(9)), &all[..4]).unwrap().packs.remove(0);
    let b = seal(&spec(local(9)), &all[3..7]).unwrap().packs.remove(0);
    let ab = consolidate(&spec(local(9)), &[&reread(&a), &reread(&b)]).unwrap();
    assert_eq!(ab.len(), 1);
    let ab = ab.into_iter().next().unwrap();
    let ab_read = reread(&ab);
    assert_eq!(ab_read.status, PackStatus::Valid);
    assert_eq!(ab_read.entries.len(), 7);
    let mut expected = vec![a.path.name, b.path.name];
    expected.sort();
    assert_eq!(ab_read.header.as_ref().unwrap().replaces, expected);

    // A later consolidation lists the names the absorbed packs listed, too.
    let c = seal(&spec(local(9)), &all[7..]).unwrap().packs.remove(0);
    let abc = consolidate(&spec(local(9)), &[&ab_read, &reread(&c)])
        .unwrap()
        .remove(0);
    let abc_read = reread(&abc);
    let replaces: BTreeSet<PackName> = abc_read.header.unwrap().replaces.into_iter().collect();
    assert_eq!(
        replaces,
        [a.path.name, b.path.name, c.path.name, ab.path.name]
            .into_iter()
            .collect()
    );
    assert_eq!(abc_read.entries.len(), 9);

    // Damaged packs and single packs are left alone.
    let mut broken = reread(&c);
    broken.status = PackStatus::Damaged(PackError::NameMismatch);
    assert!(
        consolidate(&spec(local(9)), &[&reread(&a), &broken])
            .unwrap()
            .is_empty()
    );
    assert!(
        consolidate(&spec(local(9)), &[&reread(&a)])
            .unwrap()
            .is_empty()
    );
}

/// The redundancy rule over the facts of reads of `packs`.
fn redundancy_of(packs: &[PackRead]) -> Redundancy {
    let facts: Vec<PackFacts> = packs.iter().map(PackFacts::of).collect();
    redundancy(&facts)
}

#[test]
fn the_redundancy_rule_is_a_function_of_the_file_set() {
    let all = changes(6);
    let a = reread(&seal(&spec(local(9)), &all[..3]).unwrap().packs.remove(0));
    let b = reread(&seal(&spec(local(9)), &all[3..]).unwrap().packs.remove(0));
    let ab = reread(&consolidate(&spec(local(9)), &[&a, &b]).unwrap().remove(0));

    let set = vec![a.clone(), b.clone(), ab.clone()];
    let result = redundancy_of(&set);
    assert_eq!(result.redundant, vec![(0, 2), (1, 2)]);
    assert!(result.kept.is_empty());

    // A damaged copy of a replaced pack goes with it.
    let mut damaged_a = a.clone();
    damaged_a.status = PackStatus::Damaged(PackError::NameMismatch);
    damaged_a.entries.truncate(1);
    assert_eq!(
        redundancy_of(&[damaged_a, ab.clone()]).redundant,
        vec![(0, 1)]
    );

    // A valid pack holding a change the replacing pack lacks is kept and reported.
    let mut extra = a.clone();
    extra
        .entries
        .push(verse_change(local(2), 99, 1, "nuevo").into());
    let result = redundancy_of(&[extra, ab.clone()]);
    assert!(result.redundant.is_empty());
    assert_eq!(result.kept.len(), 1);
    assert_eq!(result.kept[0].missing.len(), 1);

    // Only a valid pack in the same sealer folder replaces.
    let other = reread(&consolidate(&spec(local(8)), &[&a, &b]).unwrap().remove(0));
    assert!(redundancy_of(&[a.clone(), other]).redundant.is_empty());
    let mut broken_ab = ab.clone();
    broken_ab.status = PackStatus::Damaged(PackError::NameMismatch);
    assert!(redundancy_of(&[a.clone(), broken_ab]).redundant.is_empty());
    assert!(redundancy_of(&[ab]).redundant.is_empty());
}

/// What can happen to a pack's lines without damaging any byte of them.
#[derive(Debug, Clone)]
enum LineEdit {
    Drop(prop::sample::Index),
    Duplicate(prop::sample::Index),
    Swap(prop::sample::Index, prop::sample::Index),
    /// Another valid change line in place of one.
    Replace(prop::sample::Index),
    /// A header that verifies but says other things (another `sealed` time, same count).
    OtherHeader,
    /// A flipped bit in the header line.
    DamagedHeader(prop::sample::Index, u8),
}

fn line_edit() -> impl Strategy<Value = LineEdit> {
    prop_oneof![
        any::<prop::sample::Index>().prop_map(LineEdit::Drop),
        any::<prop::sample::Index>().prop_map(LineEdit::Duplicate),
        (any::<prop::sample::Index>(), any::<prop::sample::Index>())
            .prop_map(|(a, b)| LineEdit::Swap(a, b)),
        any::<prop::sample::Index>().prop_map(LineEdit::Replace),
        Just(LineEdit::OtherHeader),
        (any::<prop::sample::Index>(), 0u8..8)
            .prop_map(|(at, bit)| LineEdit::DamagedHeader(at, bit)),
    ]
}

/// The pack's name covers every byte though no pass hashes the whole file (§4.3): a pack
/// whose lines were dropped, duplicated, reordered or replaced by other valid lines, or
/// whose header was replaced or damaged, never reads as valid at its path, and salvage
/// takes exactly the change lines the file still holds intact.
#[test]
fn a_pack_with_lines_moved_or_replaced_is_damaged() {
    run_with_census(
        256,
        (prop::collection::vec(arb_body(), 2..8), line_edit()),
        &[
            "dropped",
            "duplicated",
            "swapped",
            "replaced",
            "other header",
            "damaged header",
        ],
        |(bodies, edit), census| {
            let lines: Vec<Verified> = bodies
                .into_iter()
                .map(|b| Change::new(b).unwrap().into())
                .collect();
            let pack = seal(&spec(local(9)), &lines).unwrap().packs.remove(0);
            let text = pack.bytes.clone();
            let header_end = text.iter().position(|b| *b == b'\n').unwrap() + 1;
            let mut header = text[..header_end].to_vec();
            let mut body: Vec<Vec<u8>> = text[header_end..]
                .split_inclusive(|b| *b == b'\n')
                .map(<[u8]>::to_vec)
                .collect();
            let n = body.len();
            match &edit {
                LineEdit::Drop(at) => {
                    body.remove(at.index(n));
                    census.saw("dropped");
                }
                LineEdit::Duplicate(at) => {
                    let line = body[at.index(n)].clone();
                    body.insert(at.index(n), line);
                    census.saw("duplicated");
                }
                LineEdit::Swap(a, b) => {
                    let (a, b) = (a.index(n), b.index(n));
                    if a == b {
                        return Ok(());
                    }
                    body.swap(a, b);
                    census.saw("swapped");
                }
                LineEdit::Replace(at) => {
                    let other = verse_change(local(3), 99, 1, "another line");
                    body[at.index(n)] = encode_line(other.id(), other.raw());
                    census.saw("replaced");
                }
                LineEdit::OtherHeader => {
                    let other = seal(
                        &PackSpec {
                            sealed: WallTime(OCT_2026 + 1),
                            ..spec(local(9))
                        },
                        &lines,
                    )
                    .unwrap()
                    .packs
                    .remove(0);
                    let end = other.bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
                    header = other.bytes[..end].to_vec();
                    census.saw("other header");
                }
                LineEdit::DamagedHeader(at, bit) => {
                    let at = at.index(header.len() - 1);
                    header[at] ^= 1 << bit;
                    census.saw("damaged header");
                }
            }
            let mut edited = header.clone();
            for line in &body {
                edited.extend_from_slice(line);
            }
            prop_assert_ne!(&edited, &pack.bytes);
            let read = read_pack(&pack.path.to_string(), &edited, project());
            prop_assert!(
                matches!(read.status, PackStatus::Damaged(_)),
                "{:?} read as {:?}",
                edit,
                read.status
            );
            // Salvage takes every change line still intact, as written, and nothing else.
            let intact: BTreeSet<ChangeId> = body
                .iter()
                .filter_map(|line| tc_journal::line::verify_line(line.strip_suffix(b"\n")?))
                .map(|(id, _)| id)
                .collect();
            let salvaged: BTreeSet<ChangeId> = read.entries.iter().map(Verified::id).collect();
            prop_assert_eq!(salvaged, intact);
            Ok(())
        },
    );
}
