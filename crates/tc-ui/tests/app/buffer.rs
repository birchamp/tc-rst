//! The draft buffer in the browser's storage (APP-DESIGN 6.6): items keyed by field, entries
//! that keep what this build does not know, removal only of the text a save stored, and the
//! mirror's lines.

use std::path::Path;

use proptest::prelude::*;
use serde_json::{Value, json};
use tc_api::buffer::{
    BUFFER_ITEM_PREFIX, BufferEntry, BufferKey, BufferLine, FieldKey, FieldKind, FieldPlace,
};
use tc_api::ids::{ProjectId, ProjectRef, ServerInstance, WallMs};
use tc_ui::app::buffer::{
    MemStore, Removal, Store, changed_line, entries, entries_of, input_to_entry, item_key, merge,
    nfc, parse_item_key, read, rekey, remove, removed_line, write,
};

const ID: &str = "6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50";
const OTHER_ID: &str = "00000000-1111-2222-3333-444444444444";

fn token() -> tc_api::ShownToken {
    serde_json::from_value(json!(
        "st1.eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA"
    ))
    .unwrap()
}

fn key_in(project: &str, id: &str, place: &str) -> BufferKey {
    BufferKey {
        project: ProjectRef::parse(project).unwrap(),
        id: ProjectId::parse(id).unwrap(),
        field: FieldKey {
            kind: FieldKind::Verse,
            place: FieldPlace::parse(place).unwrap(),
        },
    }
}

fn key(place: &str) -> BufferKey {
    key_in("en_ult", ID, place)
}

fn inst() -> ServerInstance {
    ServerInstance::from_bytes([1; 16])
}

fn entry(text: &str, at: i64) -> BufferEntry {
    BufferEntry {
        api: 1,
        instance: inst(),
        base: vec![token()],
        text: text.to_owned(),
        sent: None,
        at: WallMs(at),
        draft: None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn item_keys_are_unambiguous_for_any_folder_name(key in any::<BufferKey>()) {
        let item = item_key(&key).unwrap();
        prop_assert!(item.starts_with(BUFFER_ITEM_PREFIX));
        prop_assert_eq!(parse_item_key(&item), Some(key));
    }
}

#[test]
fn a_non_canonical_or_foreign_item_is_never_read_or_removed() {
    let mut store = MemStore::new();
    let canonical = item_key(&key("TIT 1:1")).unwrap();
    let spaced = canonical.replacen("{\"project\":", "{ \"project\":", 1);
    let foreign = [
        (
            spaced.clone(),
            serde_json::to_string(&entry("spaced", 1)).unwrap(),
        ),
        (
            format!(
                "tc4.drafts.v3:{}",
                canonical.strip_prefix(BUFFER_ITEM_PREFIX).unwrap()
            ),
            "{}".to_owned(),
        ),
        (format!("{BUFFER_ITEM_PREFIX}garbage"), "{}".to_owned()),
        (item_key(&key("TIT 1:2")).unwrap(), "not json".to_owned()),
        ("tc4.token".to_owned(), "x".to_owned()),
    ];
    for (item, value) in &foreign {
        store.set(item, value).unwrap();
    }
    assert!(entries(&store).is_empty(), "nothing foreign is read");

    write(&mut store, &key("TIT 1:3"), &entry("mine", 1)).unwrap();
    remove(&mut store, &key("TIT 1:3"), None, inst()).unwrap();
    assert_eq!(
        remove(&mut store, &key("TIT 1:2"), Some("x"), inst()).unwrap(),
        Removal::Kept
    );
    rekey(
        &mut store,
        (
            &ProjectRef::parse("en_ult").unwrap(),
            &ProjectId::parse(ID).unwrap(),
        ),
        (
            &ProjectRef::parse("copy").unwrap(),
            &ProjectId::parse(OTHER_ID).unwrap(),
        ),
    )
    .unwrap();
    for (item, value) in &foreign {
        assert_eq!(store.get(item).as_ref(), Some(value), "{item} is untouched");
    }
}

#[test]
fn rewriting_an_entry_keeps_keys_it_does_not_know() {
    let mut store = MemStore::new();
    let item = item_key(&key("TIT 1:1")).unwrap();
    let mut newer = serde_json::to_value(entry("old", 1)).unwrap();
    newer["later"] = json!({"kept": true});
    store.set(&item, &newer.to_string()).unwrap();

    let written = write(&mut store, &key("TIT 1:1"), &entry("new", 2)).unwrap();
    let stored: Value = serde_json::from_str(&store.get(&item).unwrap()).unwrap();
    assert_eq!(stored["later"], json!({"kept": true}));
    assert_eq!(stored["text"], "new");
    assert_eq!(
        written,
        store.get(&item).unwrap(),
        "the text written is returned"
    );
    assert_eq!(read(&store, &key("TIT 1:1")).unwrap().text, "new");
}

#[test]
fn an_entry_is_removed_only_when_its_text_is_the_text_just_saved() {
    let mut store = MemStore::new();
    let k = key("TIT 1:1");
    write(&mut store, &k, &entry("newer text", 1)).unwrap();
    // Another tab or boot.js wrote newer text after the save was sent.
    assert_eq!(
        remove(&mut store, &k, Some("saved text"), inst()).unwrap(),
        Removal::Kept
    );
    assert_eq!(read(&store, &k).unwrap().text, "newer text");

    write(&mut store, &k, &entry("e\u{301}", 2)).unwrap();
    assert_eq!(
        remove(&mut store, &k, Some("\u{e9}"), inst()).unwrap(),
        Removal::Removed,
        "equal in NFC is equal"
    );
    assert_eq!(
        remove(&mut store, &k, Some("\u{e9}"), inst()).unwrap(),
        Removal::Absent
    );
}

#[test]
fn entries_of_a_folder_need_its_project_id_too() {
    let mut store = MemStore::new();
    write(&mut store, &key("TIT 1:1"), &entry("mine", 1)).unwrap();
    write(
        &mut store,
        &key_in("en_ult", OTHER_ID, "TIT 1:1"),
        &entry("old folder", 1),
    )
    .unwrap();
    write(
        &mut store,
        &key_in("other", ID, "TIT 1:1"),
        &entry("other folder", 1),
    )
    .unwrap();
    let listed = entries_of(
        &store,
        &ProjectRef::parse("en_ult").unwrap(),
        &ProjectId::parse(ID).unwrap(),
    );
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].1.text, "mine");
    assert_eq!(entries(&store).len(), 3);
}

#[test]
fn merge_keeps_the_newest_entry_per_field_and_skips_torn_lines() {
    let local = vec![
        (key("TIT 1:1"), entry("page, older", 5)),
        (key("TIT 1:2"), entry("page, tie", 7)),
    ];
    let line = |k: &BufferKey, e: &BufferEntry| {
        serde_json::to_string(&BufferLine::Changed {
            key: k.clone(),
            entry: e.clone(),
        })
        .unwrap()
    };
    let newest = line(&key("TIT 1:1"), &entry("mirror, newer", 6));
    let torn = newest[..newest.len() - 5].to_owned();
    let mirror = vec![
        newest,
        line(&key("TIT 1:2"), &entry("mirror, tie", 7)),
        line(&key("TIT 1:3"), &entry("mirror only", 1)),
        removed_line(&key("TIT 1:4")).unwrap(),
        torn,
    ];
    let merged = merge(local, &mirror);
    let texts: Vec<(String, String)> = merged
        .iter()
        .map(|(k, e)| (k.field.place.to_string(), e.text.clone()))
        .collect();
    assert_eq!(
        texts,
        vec![
            ("TIT 1:1".to_owned(), "mirror, newer".to_owned()),
            ("TIT 1:2".to_owned(), "page, tie".to_owned()),
            ("TIT 1:3".to_owned(), "mirror only".to_owned()),
        ]
    );
}

fn folder(name: &str, id: &str) -> (ProjectRef, ProjectId) {
    (
        ProjectRef::parse(name).unwrap(),
        ProjectId::parse(id).unwrap(),
    )
}

#[test]
fn rekey_moves_every_entry_of_the_folder_and_nothing_else() {
    let mut store = MemStore::new();
    let mut with_later = serde_json::to_value(entry("one", 1)).unwrap();
    with_later["later"] = json!(1);
    store
        .set(&item_key(&key("TIT 1:1")).unwrap(), &with_later.to_string())
        .unwrap();
    write(&mut store, &key("TIT 1:2"), &entry("two", 1)).unwrap();
    write(
        &mut store,
        &key_in("en_ult", OTHER_ID, "TIT 1:1"),
        &entry("other id", 1),
    )
    .unwrap();
    let (from, from_id) = folder("en_ult", ID);
    let (to, to_id) = folder("en_ult (copy)", ID);
    assert_eq!(
        rekey(&mut store, (&from, &from_id), (&to, &to_id)).unwrap(),
        2
    );
    assert!(entries_of(&store, &from, &from_id).is_empty());
    let moved = entries_of(&store, &to, &to_id);
    assert_eq!(moved.len(), 2);
    let raw = store
        .get(&item_key(&key_in("en_ult (copy)", ID, "TIT 1:1")).unwrap())
        .unwrap();
    assert!(raw.contains("\"later\":1"), "moved as it is: {raw}");
    assert_eq!(
        entries_of(&store, &from, &ProjectId::parse(OTHER_ID).unwrap()).len(),
        1
    );
}

#[test]
fn rekey_that_cannot_write_removes_nothing() {
    let mut store = MemStore::new();
    write(&mut store, &key("TIT 1:1"), &entry("one", 1)).unwrap();
    let before = store.items.clone();
    store.fail_writes = true;
    let (from, from_id) = folder("en_ult", ID);
    let (to, to_id) = folder("copy", ID);
    assert!(rekey(&mut store, (&from, &from_id), (&to, &to_id)).is_err());
    assert_eq!(store.items, before);
}

fn golden() -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tc-api/tests/golden/types/buffer.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn the_lines_have_the_golden_form() {
    let golden = golden();
    for (n, line) in golden["lines"].as_array().unwrap().iter().enumerate() {
        let parsed: BufferLine = serde_json::from_value(line.clone()).unwrap();
        let built = match &parsed {
            BufferLine::Changed { key, entry } => {
                changed_line(key, &serde_json::to_string(entry).unwrap()).unwrap()
            }
            BufferLine::Removed { key } => removed_line(key).unwrap(),
        };
        let built: Value = serde_json::from_str(&built).unwrap();
        assert_eq!(&built, line, "line {n}");
    }
    for (n, item) in golden["itemKeys"].as_array().unwrap().iter().enumerate() {
        let key: BufferKey = serde_json::from_value(golden["keys"][n].clone()).unwrap();
        assert_eq!(item_key(&key).unwrap(), item.as_str().unwrap());
    }
}

#[test]
fn one_nfc_for_the_ui_and_the_server() {
    let lock =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"))
            .unwrap();
    let copies = lock.matches("name = \"unicode-normalization\"").count();
    assert_eq!(
        copies, 1,
        "the UI and the server normalize with one version of the crate"
    );
    assert_eq!(nfc("e\u{301}"), "\u{e9}");
    // Hebrew points are reordered by NFC (sheva before dagesh), which is why the clean rule
    // compares in NFC.
    assert_eq!(nfc("\u{5d1}\u{5bc}\u{5b0}"), "\u{5d1}\u{5b0}\u{5bc}");
}

#[test]
fn another_runs_entry_is_never_written_over_or_removed() {
    let mut store = MemStore::new();
    let k = key("TIT 1:1");
    let old_run = BufferEntry {
        instance: ServerInstance::from_bytes([9; 16]),
        ..entry("typed before the restart", 1)
    };
    write(&mut store, &k, &old_run).unwrap();
    let before = store.items.clone();
    assert_eq!(
        write(&mut store, &k, &entry("new run", 2)),
        Err(tc_ui::app::buffer::StoreError::Foreign)
    );
    assert_eq!(remove(&mut store, &k, None, inst()).unwrap(), Removal::Kept);
    assert_eq!(
        remove(&mut store, &k, Some("typed before the restart"), inst()).unwrap(),
        Removal::Kept
    );
    assert_eq!(
        input_to_entry(&mut store, &k, "x", WallMs(3), &entry("", 0)),
        None
    );
    assert_eq!(store.items, before);
}

#[test]
fn a_keystroke_updates_the_entry_or_makes_it_from_the_template() {
    let mut store = MemStore::new();
    let k = key("TIT 1:1");
    let template = entry("", 0);
    let line = input_to_entry(&mut store, &k, "P", WallMs(10), &template).unwrap();
    let stored = read(&store, &k).unwrap();
    assert_eq!((stored.text.as_str(), stored.at), ("P", WallMs(10)));
    let parsed: BufferLine = serde_json::from_str(&line).unwrap();
    assert_eq!(
        parsed,
        BufferLine::Changed {
            key: k.clone(),
            entry: stored
        }
    );

    let mut with_later = serde_json::to_value(entry("Pa", 11)).unwrap();
    with_later["later"] = json!(true);
    store
        .set(&item_key(&k).unwrap(), &with_later.to_string())
        .unwrap();
    input_to_entry(&mut store, &k, "Pau", WallMs(12), &template).unwrap();
    let raw: Value = serde_json::from_str(&store.get(&item_key(&k).unwrap()).unwrap()).unwrap();
    assert_eq!(raw["text"], "Pau");
    assert_eq!(raw["later"], true, "only text and at change");

    // A full storage loses the write, never the mirror's line.
    store.fail_writes = true;
    let line = input_to_entry(&mut store, &k, "Paul", WallMs(13), &template).unwrap();
    assert!(line.contains("\"Paul\""));
}
