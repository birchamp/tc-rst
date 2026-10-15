//! Packs, pending files, validation, salvage, size limits and consolidation
//! (`docs/STORAGE.md` §4.2 to §4.7).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use proptest::prelude::*;
use std::collections::BTreeSet;
use tc_journal::consolidate::{
    OwnPack, SMALL_PACK_BYTES, consolidate, consolidation_groups, redundancy,
};
use tc_journal::json::Json;
use tc_journal::line::{LINE_OVERHEAD, encode_line};
use tc_journal::pack::{
    EncodedPack, MAX_PACK_BYTES, PackError, PackPath, PackRead, PackSpec, PackStatus,
    PackWriteError, YearMonth, encode_pack, read_pack, seal,
};
use tc_journal::pending::{
    PendingHeader, PendingStatus, PendingWriteError, Purpose, Reconciles, encode_pending,
    read_pending,
};
use tc_journal::{
    Change, ChangeId, DeviceId, DeviceKind, Md5, PackId, PackName, Record, RepoPath, SettingKey,
    Verified, WallTime,
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
    let packs = seal(&spec(local(9)), &doubled).unwrap();
    assert_eq!(packs.len(), 1);
    let pack = &packs[0];
    assert_eq!(pack.path.sealer, local(9));
    assert_eq!(pack.path.month.to_string(), "2026-10");
    assert_eq!(pack.path.name, pack.id.name());
    assert_eq!(pack.id, PackId::of(&pack.bytes));
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
    let pack = &seal(&spec(local(9)), &lines).unwrap()[0];
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
    let pack = &seal(&spec(local(9)), &changes(2)).unwrap()[0];
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
    let a = &seal(&spec(local(9)), &changes(2)).unwrap()[0];
    let b = &seal(&spec(local(9)), &changes(3)).unwrap()[0];
    let read = read_pack(&b.path.to_string(), &a.bytes, project());
    assert_eq!(read.status, PackStatus::Damaged(PackError::NameMismatch));
    assert_eq!(read.entries.len(), 2);
}

#[test]
fn p3_foreign_projects_are_not_read_and_the_sealer_must_be_the_folder() {
    let pack = &seal(&spec(local(9)), &changes(2)).unwrap()[0];
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

fn hand_pack(header: &str, lines: &[&Change], trailing: &[u8]) -> (String, Vec<u8>) {
    let mut bytes = line_of(header);
    for change in lines {
        bytes.extend_from_slice(&encode_line(change.id(), change.raw()));
    }
    bytes.extend_from_slice(trailing);
    let name = PackId::of(&bytes).name();
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
    let pack = &seal(&spec(local(9)), &changes(5)).unwrap()[0];
    let cut = pack.bytes.len() - 10;
    let read = read_pack(&pack.path.to_string(), &pack.bytes[..cut], project());
    assert_eq!(read.status, PackStatus::Damaged(PackError::NameMismatch));
    assert_eq!(read.entries.len(), 4);
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// An intact pack passes P2 to P4; a damaged one salvages exactly the lines whose bytes
    /// (and the line feeds around them) are intact, and nothing else.
    #[test]
    fn pack_round_trip_and_salvage(
        bodies in prop::collection::vec(arb_body(), 1..10),
        flips in prop::collection::vec((any::<prop::sample::Index>(), 0u8..8), 1..4),
    ) {
        let lines: Vec<Verified> = bodies.into_iter().map(|b| Change::new(b).unwrap().into()).collect();
        let pack = seal(&spec(local(9)), &lines).unwrap().remove(0);
        let intact = reread(&pack);
        prop_assert_eq!(&intact.status, &PackStatus::Valid);
        prop_assert_eq!(
            intact.entries.iter().map(Verified::id).collect::<BTreeSet<_>>(),
            lines.iter().map(Verified::id).collect::<BTreeSet<_>>()
        );

        let mut damaged = pack.bytes.clone();
        let mut touched = BTreeSet::new();
        for (at, bit) in &flips {
            let at = at.index(damaged.len());
            damaged[at] ^= 1 << bit;
            touched.insert(at);
        }
        if damaged == pack.bytes {
            return Ok(());
        }
        let read = read_pack(&pack.path.to_string(), &damaged, project());
        prop_assert!(matches!(read.status, PackStatus::Damaged(_)), "{:?}", read.status);
        let spans = line_spans(&pack.bytes);
        let expected: BTreeSet<ChangeId> = spans
            .iter()
            .zip(intact.entries.iter())
            .filter(|((from, to), _)| touched.range(from..=to).next().is_none())
            .map(|(_, entry)| entry.id())
            .collect();
        let salvaged: BTreeSet<ChangeId> = read.entries.iter().map(Verified::id).collect();
        prop_assert_eq!(&salvaged, &expected);
        for entry in &read.entries {
            prop_assert!(intact.entries.contains(entry), "a salvaged change differs from what its author wrote");
        }
    }
}

fn big_line(seq: u64) -> Verified {
    let base = verse_body(local(1), seq, 1, "x");
    let mut body = base.clone();
    body.record = Record::Setting {
        key: SettingKey::parse("big").unwrap(),
        value: Some(Json::from("")),
    };
    let empty = Change::new(body.clone()).unwrap();
    let fill = tc_journal::rules::MAX_LINE_BYTES - empty.raw().len() - LINE_OVERHEAD;
    body.record = Record::Setting {
        key: SettingKey::parse("big").unwrap(),
        value: Some(Json::from("a".repeat(fill))),
    };
    Change::new(body).unwrap().into()
}

#[test]
fn a_seal_splits_at_64_mib_on_line_boundaries() {
    // Sixteen lines of the largest size fill 64 MiB exactly, so with a header they cannot
    // share one pack.
    let lines: Vec<Verified> = (1..=17).map(big_line).collect();
    let packs = seal(&spec(local(9)), &lines).unwrap();
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
    let hash = tc_journal::PackId::of(&file.bytes).to_hex();
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

    let read = read_pending(&file.bytes, other_project());
    assert_eq!(read.status, PendingStatus::Foreign { project: project() });
    assert!(read.entries.is_empty());

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
    let a = seal(&spec(local(9)), &all[..4]).unwrap().remove(0);
    let b = seal(&spec(local(9)), &all[3..7]).unwrap().remove(0);
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
    let c = seal(&spec(local(9)), &all[7..]).unwrap().remove(0);
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

#[test]
fn the_redundancy_rule_is_a_function_of_the_file_set() {
    let all = changes(6);
    let a = reread(&seal(&spec(local(9)), &all[..3]).unwrap().remove(0));
    let b = reread(&seal(&spec(local(9)), &all[3..]).unwrap().remove(0));
    let ab = reread(&consolidate(&spec(local(9)), &[&a, &b]).unwrap().remove(0));

    let set = vec![a.clone(), b.clone(), ab.clone()];
    let result = redundancy(&set);
    assert_eq!(result.redundant, vec![(0, 2), (1, 2)]);
    assert!(result.kept.is_empty());

    // A damaged copy of a replaced pack goes with it.
    let mut damaged_a = a.clone();
    damaged_a.status = PackStatus::Damaged(PackError::NameMismatch);
    damaged_a.entries.truncate(1);
    assert_eq!(redundancy(&[damaged_a, ab.clone()]).redundant, vec![(0, 1)]);

    // A valid pack holding a change the replacing pack lacks is kept and reported.
    let mut extra = a.clone();
    extra
        .entries
        .push(verse_change(local(2), 99, 1, "nuevo").into());
    let result = redundancy(&[extra, ab.clone()]);
    assert!(result.redundant.is_empty());
    assert_eq!(result.kept.len(), 1);
    assert_eq!(result.kept[0].missing.len(), 1);

    // Only a valid pack in the same sealer folder replaces.
    let other = reread(&consolidate(&spec(local(8)), &[&a, &b]).unwrap().remove(0));
    assert!(redundancy(&[a.clone(), other]).redundant.is_empty());
    let mut broken_ab = ab.clone();
    broken_ab.status = PackStatus::Damaged(PackError::NameMismatch);
    assert!(redundancy(&[a.clone(), broken_ab]).redundant.is_empty());
    assert!(redundancy(&[ab]).redundant.is_empty());
}
