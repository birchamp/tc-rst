//! Helpers shared by the integration tests.

use std::collections::BTreeMap;
use tc_usfm::AlignmentGroup;

/// What an alignment means: each target word (text and occurrence) and the chain of
/// source words above it (content and occurrence). Two weaves that cut a group into runs
/// differently mean the same.
pub fn links(groups: &[AlignmentGroup]) -> BTreeMap<(String, u32), Vec<(String, u32)>> {
    let mut map = BTreeMap::new();
    for group in groups.iter().filter(|g| !g.sources.is_empty()) {
        for target in &group.targets {
            let sources = group
                .sources
                .iter()
                .map(|s| (s.content.clone(), s.occurrence))
                .collect();
            map.insert((target.text.clone(), target.occurrence), sources);
        }
    }
    map
}
