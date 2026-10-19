//! The draft buffer in the browser's storage (APP-DESIGN 6.6): items keyed by field, entries
//! that keep what this build does not know, removal only of the text a save stored, and the
//! mirror's lines.

use std::path::Path;

use proptest::prelude::*;
use serde_json::{Value, json};
use tc_api::buffer::SentSave;
use tc_api::buffer::{
    BUFFER_ITEM_PREFIX, BufferEntry, BufferKey, BufferLine, FieldKey, FieldKind, FieldPlace,
};
use tc_api::ids::RequestId;
use tc_api::ids::{ProjectId, ProjectRef, ServerInstance, WallMs};
use tc_ui::app::buffer::{
    MemStore, Removal, Store, StoreError, changed_line, current_lines, entries, entries_of,
    input_to_entry, item_key, merge, nfc, parse_item_key, read, remove, removed_line,
    removed_line_if, write, write_entry,
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
    tc_ui::app::offer::copy(
        &mut store,
        &[],
        (
            &ProjectRef::parse("en_ult").unwrap(),
            &ProjectId::parse(ID).unwrap(),
        ),
        (
            &ProjectRef::parse("copy").unwrap(),
            &ProjectId::parse(OTHER_ID).unwrap(),
        ),
    );
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
        removed_line(&key("TIT 1:4"), inst()).unwrap(),
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
            BufferLine::Removed {
                key,
                instance,
                text: None,
            } => removed_line(key, *instance).unwrap(),
            BufferLine::Removed {
                key,
                instance,
                text: Some(text),
            } => removed_line_if(key, *instance, text).unwrap(),
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
    // The keystroke still reaches the shell, as this run's entry.
    let input = input_to_entry(&mut store, &k, "x", WallMs(3), &entry("", 0));
    assert!(!input.stored);
    let parsed: BufferLine = serde_json::from_str(&input.line.unwrap()).unwrap();
    assert_eq!(
        parsed,
        BufferLine::Changed {
            key: k.clone(),
            entry: BufferEntry {
                at: WallMs(3),
                ..entry("x", 0)
            }
        }
    );
    // So does the machine's own write.
    let written = write_entry(&mut store, &k, &entry("y", 4));
    assert_eq!(
        written.failure, None,
        "another run's item is the rule, not a fault"
    );
    assert!(written.line.unwrap().contains("\"y\""));
    assert_eq!(store.items, before);
}

#[test]
fn an_item_this_build_cannot_read_is_never_written_and_the_keystroke_is_mirrored() {
    let mut store = MemStore::new();
    let k = key("TIT 1:1");
    let item = item_key(&k).unwrap();
    store
        .set(&item, r#"{"api":2,"text":"a newer build's"}"#)
        .unwrap();
    let before = store.items.clone();
    let input = input_to_entry(&mut store, &k, "typed", WallMs(3), &entry("", 0));
    assert!(!input.stored);
    assert!(input.line.unwrap().contains("\"typed\""));
    assert_eq!(
        write(&mut store, &k, &entry("z", 1)),
        Err(StoreError::Foreign)
    );
    assert_eq!(store.items, before);
}

#[test]
fn a_keystroke_updates_the_entry_or_makes_it_from_the_template() {
    let mut store = MemStore::new();
    let k = key("TIT 1:1");
    let template = entry("", 0);
    let input = input_to_entry(&mut store, &k, "P", WallMs(10), &template);
    assert!(input.stored);
    let line = input.line.unwrap();
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
    assert!(input_to_entry(&mut store, &k, "Pau", WallMs(12), &template).stored);
    let raw: Value = serde_json::from_str(&store.get(&item_key(&k).unwrap()).unwrap()).unwrap();
    assert_eq!(raw["text"], "Pau");
    assert_eq!(raw["later"], true, "keys this build does not know stay");

    // A full storage loses the write, never the mirror's line, and says so.
    store.fail_writes = true;
    let input = input_to_entry(&mut store, &k, "Paul", WallMs(13), &template);
    assert!(
        !input.stored,
        "the WebAssembly then writes the entry itself"
    );
    assert!(input.line.unwrap().contains("\"Paul\""));
}

fn saving(text: &str, request: u8) -> BufferEntry {
    BufferEntry {
        sent: Some(SentSave {
            request: RequestId::from_bytes([request; 16]),
            text: text.to_owned(),
            scope: None,
        }),
        ..entry(text, 1)
    }
}

/// Two tabs share a field's item. Tab A's save is in flight under its tokens; tab B's keystroke
/// writes B's own state over it (its template), so a reload of B never sends A's request or
/// takes A's answer as its own base, which would store B's text over A's unseen.
#[test]
fn a_keystroke_writes_this_pages_save_and_tokens_never_another_pages() {
    let mut store = MemStore::new();
    let k = key("TIT 1:1");
    write(&mut store, &k, &saving("A1", 7)).unwrap();
    let other_token: tc_api::ShownToken = serde_json::from_value(json!(
        "st1.eyJoZWFkcyI6W10sImtleSI6eyJ4IjoxfX0AAAAAAAAAAAAAAAAAAAAA"
    ))
    .unwrap();
    let tab_b = BufferEntry {
        base: vec![other_token.clone()],
        ..entry("", 0)
    };
    assert!(input_to_entry(&mut store, &k, "B1", WallMs(5), &tab_b).stored);
    let kept = read(&store, &k).unwrap();
    assert_eq!(kept.text, "B1");
    assert_eq!(kept.sent, None, "tab A's save is not tab B's");
    assert_eq!(kept.base, vec![other_token]);
    // The same page's save in flight stays named under newer text (A-2).
    let own = saving("B1", 8);
    write(&mut store, &k, &own).unwrap();
    let template = BufferEntry {
        text: String::new(),
        ..own.clone()
    };
    assert!(input_to_entry(&mut store, &k, "B1 more", WallMs(6), &template).stored);
    assert_eq!(read(&store, &k).unwrap().sent, own.sent);
}

/// A full storage refuses the write: whatever the stored item names, it stays, since an older
/// text of the field is better than none, and the mirror's line carries the new entry. A save the
/// item names is one this run's reply cache answers again after a reload.
#[test]
fn a_refused_write_leaves_the_stored_item_as_it_is() {
    let k = key("TIT 1:1");
    let newer = BufferEntry {
        text: "newer".into(),
        ..saving("older", 7)
    };
    let cases = [
        // The same save in flight, older text.
        (saving("older", 7), newer.clone()),
        // Another save: the item's was answered, and the field sends its next one.
        (saving("older", 7), saving("newer", 8)),
        // No save in the item, and the write names the save that starts now.
        (entry("older", 1), saving("newer", 8)),
        // The item's save was refused, and the write no longer names it.
        (saving("older", 7), entry("newer", 2)),
    ];
    for (stored, refused) in cases {
        let mut store = MemStore::new();
        write(&mut store, &k, &stored).unwrap();
        store.fail_writes = true;
        let written = write_entry(&mut store, &k, &refused);
        assert!(matches!(written.failure, Some(StoreError::Refused(_))));
        assert!(
            written.line.unwrap().contains("\"newer\""),
            "the mirror gets the text"
        );
        assert_eq!(
            read(&store, &k),
            Some(stored),
            "the older item stays: {refused:?}"
        );
    }
}

/// The shell's mirror after a restart behind a reused port: run 1 left an unsaved text, run 2
/// typed into the same field and saved it. The removal of run 2's entry leaves run 1's for the
/// offer at the next launch, which reads only the mirror (5.7, AD21).
#[test]
fn the_mirror_keeps_each_runs_entry_and_a_removal_drops_only_its_own() {
    let k = key("TIT 1:1");
    let run1 = BufferEntry {
        instance: ServerInstance::from_bytes([9; 16]),
        ..entry("R1, never saved", 5)
    };
    let changed = |e: &BufferEntry| {
        serde_json::to_string(&BufferLine::Changed {
            key: k.clone(),
            entry: e.clone(),
        })
        .unwrap()
    };
    let lines = vec![
        changed(&run1),
        changed(&entry("R2", 8)),
        "{\"type\":\"changed\",\"val".to_owned(),
        removed_line(&k, inst()).unwrap(),
    ];
    let current = current_lines(&lines);
    assert_eq!(current, vec![changed(&run1)]);
    assert_eq!(merge(Vec::new(), &current).len(), 1);
    // A removal of run 1 drops run 1's entry.
    let mut later = lines.clone();
    later.push(removed_line(&k, run1.instance).unwrap());
    assert!(current_lines(&later).is_empty());
}

#[test]
fn texts_compare_in_nfc() {
    use tc_ui::app::buffer::same_text;
    assert!(same_text("e\u{301}", "\u{e9}"));
    assert!(
        same_text("\u{5d1}\u{5bc}\u{5b0}", "\u{5d1}\u{5b0}\u{5bc}"),
        "Hebrew points in either order"
    );
    assert!(!same_text("Paul", "Paula"));
    assert!(!same_text("e\u{301}x", "\u{e9}"));
    assert!(same_text("", ""));
}

#[test]
fn merge_keeps_two_runs_entries_of_one_field() {
    let old_run = BufferEntry {
        instance: ServerInstance::from_bytes([9; 16]),
        ..entry("typed before the restart", 5)
    };
    let line = serde_json::to_string(&BufferLine::Changed {
        key: key("TIT 1:1"),
        entry: entry("typed after it", 9),
    })
    .unwrap();
    let merged = merge(vec![(key("TIT 1:1"), old_run)], &[line]);
    let mut texts: Vec<&str> = merged.iter().map(|(_, e)| e.text.as_str()).collect();
    texts.sort_unstable();
    assert_eq!(texts, ["typed after it", "typed before the restart"]);
}

/// A removal that names a text ends the run's entry only while the entry holds that text: an
/// offer's slot settled after its run typed on keeps the newer text (C1b, finding 8).
#[test]
fn a_removal_that_names_a_text_keeps_newer_text_of_its_run() {
    let k = key("TIT 1:4");
    let offered = entry("offered", 1);
    let newer = entry("typed since", 2);
    let lines = vec![
        changed_line(&k, &serde_json::to_string(&offered).unwrap()).unwrap(),
        changed_line(&k, &serde_json::to_string(&newer).unwrap()).unwrap(),
        removed_line_if(&k, inst(), "offered").unwrap(),
    ];
    let kept = current_lines(&lines);
    assert_eq!(kept.len(), 1);
    assert!(kept[0].contains("typed since"), "{kept:?}");
    let mut settled = lines[..1].to_vec();
    settled.push(removed_line_if(&k, inst(), "offered").unwrap());
    assert!(current_lines(&settled).is_empty());
}
