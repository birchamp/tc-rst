//! The draft offer and the read-only banner (APP-DESIGN 6.4, 6.6, C1b): what the page sends at
//! an open, what it drops once an offer is settled, and the words it shows.

use serde_json::json;
use tc_api::buffer::{BufferEntry, BufferKey, BufferLine, FieldKey, FieldKind, FieldPlace};
use tc_api::ids::{ProjectId, ProjectRef, ServerInstance, WallMs};
use tc_api::session::{BufferSlot, DraftKind, DraftOffer, Keep, Settles};
use tc_api::write::ReadOnlyReason;
use tc_ui::app::buffer::{MemStore, Store, entries, removed_line, write_entry};
use tc_ui::app::offer::{BannerAction, banner, buffered_lines, offer_words, settle};
use tc_ui::i18n::keys;

const ID: &str = "6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50";

fn key(project: &str, place: &str) -> BufferKey {
    BufferKey {
        project: ProjectRef::parse(project).unwrap(),
        id: ProjectId::parse(ID).unwrap(),
        field: FieldKey {
            kind: FieldKind::Verse,
            place: FieldPlace::parse(place).unwrap(),
        },
    }
}

fn entry(run: u8, text: &str, at: i64) -> BufferEntry {
    BufferEntry {
        api: 1,
        instance: ServerInstance::from_bytes([run; 16]),
        base: vec![
            serde_json::from_value(json!(
                "st1.eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"
            ))
            .unwrap(),
        ],
        text: text.to_owned(),
        sent: None,
        at: WallMs(at),
        draft: None,
    }
}

fn changed(key: &BufferKey, entry: &BufferEntry) -> String {
    serde_json::to_string(&BufferLine::Changed {
        key: key.clone(),
        entry: entry.clone(),
    })
    .unwrap()
}

fn texts(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(
            |line| match serde_json::from_str::<BufferLine>(line).unwrap() {
                BufferLine::Changed { entry, .. } => entry.text,
                BufferLine::Removed { .. } => panic!("a removal is sent: {line}"),
            },
        )
        .collect()
}

/// The page sends the folder's entries from its storage and the mirror, the newest per field
/// and run, every run's (an open meets none of its own), and no other folder's.
#[test]
fn the_open_sends_the_folders_entries_of_page_and_mirror() {
    let mut store = MemStore::new();
    let tit4 = key("en_ult", "TIT 1:4");
    write_entry(&mut store, &tit4, &entry(1, "page, older", 10));
    write_entry(
        &mut store,
        &key("other", "TIT 1:4"),
        &entry(1, "another folder", 10),
    );
    let mirror = vec![
        changed(&tit4, &entry(1, "mirror, newer", 20)),
        changed(&key("en_ult", "TIT 1:5"), &entry(2, "mirror only", 5)),
        changed(&key("en_ult", "TIT 1:6"), &entry(2, "removed", 5)),
        removed_line(
            &key("en_ult", "TIT 1:6"),
            ServerInstance::from_bytes([2; 16]),
        )
        .unwrap(),
        "{\"type\":\"changed\"".to_owned(),
    ];
    let lines = buffered_lines(
        &store,
        &mirror,
        &ProjectRef::parse("en_ult").unwrap(),
        &ProjectId::parse(ID).unwrap(),
    );
    let mut got = texts(&lines);
    got.sort();
    assert_eq!(got, ["mirror only", "mirror, newer"]);
}

/// Settling an offer drops each item only while it holds the offered text and run, and tells
/// the mirror once per slot, so a newer text typed since stays.
#[test]
fn settling_drops_only_the_offered_text() {
    let mut store = MemStore::new();
    let (tit4, tit5) = (key("en_ult", "TIT 1:4"), key("en_ult", "TIT 1:5"));
    write_entry(&mut store, &tit4, &entry(1, "offered", 10));
    write_entry(&mut store, &tit5, &entry(1, "typed since", 30));
    let slots = [
        BufferSlot {
            key: tit4.clone(),
            instance: ServerInstance::from_bytes([1; 16]),
            text: "offered".to_owned(),
        },
        BufferSlot {
            key: tit5.clone(),
            instance: ServerInstance::from_bytes([1; 16]),
            text: "offered earlier".to_owned(),
        },
    ];
    let lines = settle(&mut store, &slots);
    assert_eq!(lines.len(), 2);
    let left: Vec<String> = entries(&store).into_iter().map(|(_, e)| e.text).collect();
    assert_eq!(left, ["typed since"]);
    assert!(store.keys().len() == 1);
}

#[test]
fn each_kind_of_keep_has_its_words() {
    let offer = |keep| DraftOffer {
        kind: DraftKind::Words,
        place: FieldPlace::parse("TIT 1:4").unwrap(),
        text: "To Titus".to_owned(),
        keep,
        shown: Vec::new(),
        settles: Settles::default(),
    };
    let save = offer_words(&offer(Keep::Save));
    assert_eq!(save.sentence, keys::drafts::offer::FOUND);
    assert_eq!(save.keep, keys::drafts::offer::KEEP);
    let version = offer_words(&offer(Keep::AsVersion));
    assert_eq!(version.sentence, keys::drafts::offer::UNVERIFIED);
    assert_eq!(version.keep, keys::drafts::offer::KEEP_AS_VERSION);
    assert_eq!(
        offer_words(&offer(Keep::CopyText)).keep,
        keys::drafts::offer::COPY_TEXT
    );
    assert_eq!(save.discard, keys::drafts::offer::DISCARD);
}

/// A copy is offered where a copy can be written: the folder's place or another writer is the
/// reason. "That computer is closed" only where a lock names another computer.
#[test]
fn a_banner_offers_the_copy_where_a_copy_helps() {
    use BannerAction::{Copy, OtherComputerClosed};
    for (reason, actions) in [
        (ReadOnlyReason::NeedsNewerApp, vec![]),
        (ReadOnlyReason::UnsafeDrive, vec![Copy]),
        (ReadOnlyReason::CloudSynced, vec![Copy]),
        (ReadOnlyReason::SecondWriter, vec![Copy]),
        (
            ReadOnlyReason::OpenOnAnotherComputer,
            vec![Copy, OtherComputerClosed],
        ),
        (ReadOnlyReason::FolderNotWritable, vec![]),
    ] {
        assert_eq!(banner(reason).actions, actions, "{reason:?}");
    }
    assert_eq!(
        banner(ReadOnlyReason::SecondWriter).sentence,
        keys::read_only::SECOND_WRITER
    );
}
