//! The draft buffer's format (APP-DESIGN §6.6): what the UI, `boot.js`, the shell and the
//! server rely on when they read each other's items and lines.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use serde_json::json;
use tc_api::buffer::{
    BUFFER_ITEM_PREFIX, BufferEntry, BufferKey, BufferLine, FieldKey, FieldKind, FieldPlace,
};
use tc_api::ids::{ProjectId, ProjectRef};

const TOKEN: &str = "st1.eyJoZWFkcyI6W10sImtleSI6e319AAAAAAAAAAAAAAAAAAAAAA";

fn entry_json() -> serde_json::Value {
    json!({
        "api": 1,
        "instance": "abababababababababababababababab",
        "base": [TOKEN],
        "text": "Paul",
        "sent": null,
        "at": 5,
        "draft": null,
    })
}

#[test]
fn buffer_entry_accepts_keys_it_does_not_know() {
    // A newer build may add keys (§3.10); this build still reads the entry.
    let mut value = entry_json();
    value["later"] = json!(1);
    value["sent"] = json!({
        "request": "01010101010101010101010101010101",
        "text": "Pau",
        "scope": null,
    });
    let entry: BufferEntry = serde_json::from_value(value).unwrap();
    assert_eq!(entry.text, "Paul");
    assert_eq!(entry.sent.unwrap().text, "Pau");
}

#[test]
fn the_parts_of_an_entry_this_build_writes_refuse_unknown_keys() {
    // What this build writes inside an entry is closed: a `sent` with a key it does not know
    // is not a save it can repeat byte for byte.
    let mut value = entry_json();
    value["sent"] = json!({
        "request": "01010101010101010101010101010101",
        "text": "Pau",
        "scope": null,
        "later": 1,
    });
    assert!(serde_json::from_value::<BufferEntry>(value).is_err());
}

#[test]
fn field_place_refuses_empty_long_and_control_text() {
    assert!(FieldPlace::parse("").is_err());
    assert!(FieldPlace::parse(&"a".repeat(257)).is_err());
    assert!(FieldPlace::parse(&"a".repeat(256)).is_ok());
    assert!(FieldPlace::parse("TIT 1:4\n").is_err());
    assert!(FieldPlace::parse("TIT\u{7f}").is_err());
    assert!(FieldPlace::parse("TIT 1:4 | \"x\" אב").is_ok());
    assert!(serde_json::from_value::<FieldPlace>(json!("")).is_err());
}

#[test]
fn buffer_key_json_is_canonical() {
    let key = BufferKey {
        // A folder name may hold `|` and quotes, which a joined key could not tell apart.
        project: ProjectRef::parse("a|b \"d\" é").unwrap(),
        id: ProjectId::parse("6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50").unwrap(),
        field: FieldKey {
            kind: FieldKind::Verse,
            place: FieldPlace::parse("TIT 1:4|x").unwrap(),
        },
    };
    let text = serde_json::to_string(&key).unwrap();
    assert_eq!(
        text,
        r#"{"project":"a|b \"d\" é","id":"6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50","field":{"kind":"verse","place":"TIT 1:4|x"}}"#
    );
    let back: BufferKey = serde_json::from_str(&text).unwrap();
    assert_eq!(back, key);
    assert_eq!(serde_json::to_string(&back).unwrap(), text);
    // A key refuses what it does not know, so two texts never name one field.
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["field"]["later"] = json!(1);
    assert!(serde_json::from_value::<BufferKey>(value).is_err());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Any folder name and place give an item key that reads back as the same key, so the
    /// text after the prefix names one field only.
    #[test]
    fn every_key_reads_back_from_its_item_text(key in any::<BufferKey>()) {
        let item = format!("{BUFFER_ITEM_PREFIX}{}", serde_json::to_string(&key).unwrap());
        let back: BufferKey =
            serde_json::from_str(item.strip_prefix(BUFFER_ITEM_PREFIX).unwrap()).unwrap();
        prop_assert_eq!(&back, &key);
        prop_assert_eq!(
            serde_json::to_string(&back).unwrap(),
            item.strip_prefix(BUFFER_ITEM_PREFIX).unwrap()
        );
    }

    /// A log line never holds a line feed, so a crash in an append tears only the last line.
    #[test]
    fn a_line_is_one_line(line in any::<BufferLine>()) {
        let text = serde_json::to_string(&line).unwrap();
        prop_assert!(!text.contains('\n') && !text.contains('\r'));
        let back: BufferLine = serde_json::from_str(&text).unwrap();
        prop_assert_eq!(back, line);
    }
}
