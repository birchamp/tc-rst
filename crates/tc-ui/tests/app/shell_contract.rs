//! The shell contract (`docs/contracts/SHELL.md` v1): the page and the Electronite shell reduce
//! `drafts-buffer.log` by the same rules, and the page calls only bridge members the contract
//! names. The shell's own tests read the same vectors file.

use std::collections::BTreeSet;

use regex::Regex;
use serde_json::Value;
use tc_ui::app::buffer::current_lines;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

const VECTORS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/shell/buffer-vectors.json"
);

fn read_json(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_buffer_reduction_matches_the_shell_contract_vectors() {
    let file = read_json(VECTORS);
    assert_eq!(file["version"], 1, "INT-7: the vectors are version 1");
    let vectors = file["vectors"].as_array().unwrap();
    assert!(vectors.len() >= 10, "INT-7: step 1 names ten vectors");
    for vector in vectors {
        let name = vector["name"].as_str().unwrap();
        let lines = strings(&vector["lines"]);
        let want: BTreeSet<String> = vector["keep"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                let i = usize::try_from(i.as_u64().unwrap()).unwrap();
                lines.get(i).unwrap().clone()
            })
            .collect();
        let got: BTreeSet<String> = current_lines(&lines).into_iter().collect();
        assert_eq!(
            got, want,
            "INV-SHELL-CONTRACT-PINNED: vector {name:?}: current_lines kept {got:?}, the contract keeps {want:?}"
        );
    }
}

fn names_in(path: &str, pattern: &str) -> BTreeSet<String> {
    let text = std::fs::read_to_string(format!("{ROOT}/{path}")).unwrap();
    Regex::new(pattern)
        .unwrap()
        .captures_iter(&text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_owned()))
        .collect()
}

#[test]
fn the_bridge_members_the_page_uses_match_the_shell_contract() {
    let file = read_json(&format!("{ROOT}/docs/contracts/shell-bridge.json"));
    assert_eq!(file["version"], 1, "INT-7: the bridge names are version 1");
    let methods: BTreeSet<String> = strings(&file["methods"]).into_iter().collect();
    let called: BTreeSet<String> = strings(&file["calledByThePage"]).into_iter().collect();

    let bridge = names_in(
        "crates/tc-ui/src/app/web/bridge.rs",
        r#"method\("([A-Za-z]+)"\)"#,
    );
    assert_eq!(
        bridge, called,
        "INV-SHELL-CONTRACT-PINNED: bridge.rs calls {bridge:?}, shell-bridge.json calledByThePage says {called:?}"
    );
    let unknown: Vec<_> = called.difference(&methods).collect();
    assert!(
        unknown.is_empty(),
        "INV-SHELL-CONTRACT-PINNED: calledByThePage names {unknown:?}, which methods does not list"
    );
    let boot = names_in("crates/tc-ui/app/boot.js", r"tc4Shell\??\.([A-Za-z]+)");
    let unknown: Vec<_> = boot.difference(&methods).collect();
    assert!(
        unknown.is_empty(),
        "INV-SHELL-CONTRACT-PINNED: boot.js uses tc4Shell members {unknown:?}, which shell-bridge.json does not list"
    );
    let shell_md = std::fs::read_to_string(format!("{ROOT}/docs/contracts/SHELL.md")).unwrap();
    let missing: Vec<_> = methods
        .iter()
        .filter(|name| !shell_md.contains(&format!("`{name}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "INV-SHELL-CONTRACT-PINNED: docs/contracts/SHELL.md does not name {missing:?}"
    );
}
