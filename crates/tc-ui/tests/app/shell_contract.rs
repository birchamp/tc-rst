//! The shell contract (`docs/contracts/SHELL.md` v1): the page and the Electronite shell reduce
//! `drafts-buffer.log` by the same rules, and the page calls only bridge members the contract
//! names. The shell's own tests read the same vectors file.

use std::collections::BTreeSet;

use serde_json::Value;
use tc_ui::app::buffer::current_lines;

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
