//! Layer 3, what the DOM shows (APP-DESIGN 7.3): `homeCards` and `syncToast` as `observe` read
//! them at the shot, field by field.
//!
//! `syncToast.missingVersions` is left out: tC4's capture adds it by tC4's own rule
//! (`missingVersionsOf` in parity/app/env-tc4.mjs), and the Rust capture has no such rule; the
//! count it explains, `missingPins`, is compared.

use std::collections::{BTreeMap, BTreeSet};

use super::inputs::SyncToast;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeaningResult {
    pub same: bool,
    /// The masks whose declared pairs explain a difference.
    pub declared: Vec<String>,
    pub diffs: Vec<String>,
}

/// A declared pair: in `homeCards[repo]`, tC4's action and the rebuild's.
#[derive(Clone, Debug)]
pub struct DeclaredCard {
    pub mask: String,
    pub repo: String,
    pub tc4: String,
    pub rust: String,
}

fn or_none(value: Option<&String>) -> &str {
    value.map_or("none", String::as_str)
}

/// Compares what each side showed. The Rust side's card keys are renamed from folders to repo
/// paths with `rename` (folder, repo).
pub fn compare(
    tc4_cards: &BTreeMap<String, String>,
    tc4_toast: Option<&SyncToast>,
    rust_cards: &BTreeMap<String, String>,
    rust_toast: Option<&SyncToast>,
    rename: &[(String, String)],
    declared: &[DeclaredCard],
) -> MeaningResult {
    let rust_cards: BTreeMap<String, String> = rust_cards
        .iter()
        .map(|(folder, action)| {
            let repo = rename
                .iter()
                .find(|(f, _)| f == folder)
                .map_or(folder, |(_, r)| r);
            (repo.clone(), action.clone())
        })
        .collect();
    let mut out = MeaningResult {
        same: true,
        declared: Vec::new(),
        diffs: Vec::new(),
    };
    let keys: BTreeSet<&String> = tc4_cards.keys().chain(rust_cards.keys()).collect();
    for repo in keys {
        let (a, b) = (tc4_cards.get(repo), rust_cards.get(repo));
        if a == b {
            continue;
        }
        out.same = false;
        match declared
            .iter()
            .find(|d| &d.repo == repo && Some(&d.tc4) == a && Some(&d.rust) == b)
        {
            Some(d) => {
                if !out.declared.contains(&d.mask) {
                    out.declared.push(d.mask.clone());
                }
            }
            None => out.diffs.push(format!(
                "homeCards[{repo}]: tC4 {}, rust {}",
                or_none(a),
                or_none(b)
            )),
        }
    }
    match (tc4_toast, rust_toast) {
        (None, None) => {}
        (Some(_), None) => out
            .diffs
            .push("syncToast: tC4 shows the toast, rust does not".to_owned()),
        (None, Some(_)) => out
            .diffs
            .push("syncToast: rust shows a toast, tC4 does not".to_owned()),
        (Some(a), Some(b)) => {
            let field = |name: &str, x: String, y: String, diffs: &mut Vec<String>| {
                if x != y {
                    diffs.push(format!("syncToast.{name}: tC4 {x}, rust {y}"));
                }
            };
            let d = &mut out.diffs;
            field("ok", a.ok.to_string(), b.ok.to_string(), d);
            field("code", format!("{:?}", a.code), format!("{:?}", b.code), d);
            field("text", format!("{:?}", a.text), format!("{:?}", b.text), d);
            field(
                "received",
                a.received.to_string(),
                b.received.to_string(),
                d,
            );
            field(
                "newestWins",
                a.newest_wins.to_string(),
                b.newest_wins.to_string(),
                d,
            );
            field(
                "verseConflicts",
                a.verse_conflicts.to_string(),
                b.verse_conflicts.to_string(),
                d,
            );
            field("from", format!("{:?}", a.from), format!("{:?}", b.from), d);
            field(
                "restored",
                a.restored.to_string(),
                b.restored.to_string(),
                d,
            );
            field(
                "missingPins",
                a.missing_pins.to_string(),
                b.missing_pins.to_string(),
                d,
            );
            field(
                "links",
                format!("{:?}", a.links),
                format!("{:?}", b.links),
                d,
            );
        }
    }
    if !out.diffs.is_empty() {
        out.same = false;
        out.declared.clear();
    }
    out
}
