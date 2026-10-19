//! Layer 4, behaviour (APP-DESIGN 7.3): a sequence of steps recorded on both sides from the same
//! baseline screen (parity/app/behaviour.mjs), compared step by step: the label, the DOM with
//! each element's node number, the focus and the scroll lock. Node numbers are renumbered by
//! first appearance on each side, so they compare whatever else the page holds; a node one side
//! draws again shows `#new` there.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::dom::{self, Side, Snapshot};
use super::inputs::{Scope, TargetSpec};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recording {
    pub sequence: String,
    /// The baseline screen it starts from.
    pub screen: String,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub label: String,
    pub dom: Snapshot,
    pub focus: String,
    pub overflow: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceResult {
    pub sequence: String,
    pub steps: usize,
    pub same: bool,
    pub first_diff: Option<String>,
}

static NODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" #(\d+)>$").unwrap());

/// Each step's lines, with the masks' elements taken out and node numbers renumbered by first
/// appearance across the whole recording.
fn lines(
    recording: &Recording,
    rename: &[(String, String)],
    scope: Option<&Scope>,
    masks: &[(String, TargetSpec)],
    name: &str,
) -> Result<Vec<Vec<String>>, String> {
    let mut numbers: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = Vec::new();
    for step in &recording.steps {
        let side = Side {
            name,
            snapshot: &step.dom,
            rename,
        };
        let prepared = dom::prepare(side, scope, masks)
            .map_err(|e| format!("step {:?}: {}", step.label, e.message()))?;
        let renumbered = prepared
            .lines
            .into_iter()
            .map(|line| {
                NODE.replace(&line, |c: &regex::Captures<'_>| {
                    let next = numbers.len() + 1;
                    format!(" #{}>", numbers.entry(c[1].to_owned()).or_insert(next))
                })
                .into_owned()
            })
            .collect();
        out.push(renumbered);
    }
    Ok(out)
}

/// Compares two recordings of one sequence. The masks' elements on each side take no part.
pub fn compare(
    tc4: &Recording,
    rust: &Recording,
    rename: &[(String, String)],
    scope: Option<&Scope>,
    tc4_masks: &[(String, TargetSpec)],
    rust_masks: &[(String, TargetSpec)],
) -> SequenceResult {
    let mut result = SequenceResult {
        sequence: tc4.sequence.clone(),
        steps: tc4.steps.len(),
        same: false,
        first_diff: None,
    };
    let diff = (|| -> Result<Option<String>, String> {
        if tc4.screen != rust.screen {
            return Ok(Some(format!(
                "tC4 starts from {}, rust from {}",
                tc4.screen, rust.screen
            )));
        }
        if tc4.steps.len() != rust.steps.len() {
            return Ok(Some(format!(
                "tC4 has {} steps, rust {}",
                tc4.steps.len(),
                rust.steps.len()
            )));
        }
        let a = lines(tc4, &[], scope, tc4_masks, "tC4")?;
        let b = lines(rust, rename, scope, rust_masks, "rust")?;
        for (i, (x, y)) in tc4.steps.iter().zip(&rust.steps).enumerate() {
            let at = format!("step {} ({})", i, x.label);
            if x.label != y.label {
                return Ok(Some(format!("{at}: rust's step is {}", y.label)));
            }
            if let Some(line) = dom::line_diffs(&a[i], &b[i], 1).pop() {
                return Ok(Some(format!("{at}: {line}")));
            }
            if x.focus != y.focus {
                return Ok(Some(format!(
                    "{at}: focus tC4 {} | rust {}",
                    x.focus, y.focus
                )));
            }
            if x.overflow != y.overflow {
                return Ok(Some(format!(
                    "{at}: body overflow tC4 {} | rust {}",
                    x.overflow, y.overflow
                )));
            }
        }
        Ok(None)
    })();
    match diff {
        Ok(None) => result.same = true,
        Ok(Some(d)) | Err(d) => result.first_diff = Some(d),
    }
    result
}
