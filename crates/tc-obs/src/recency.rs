//! The stories a translator edited most recently in one OBS project, newest first: the
//! collapsed Home card shows them (tC4 `src/data/obsRecency.ts`, #328). The list is kept
//! per computer, never in the project: it says where this person worked.

/// How many stories the collapsed card shows, and how many the list keeps
/// (`RECENT_STORIES`, `obsRecency.ts:12`).
pub const RECENT_STORIES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecentStory {
    pub story: u32,
    /// Milliseconds since the Unix epoch of the last edit.
    pub at: i64,
}

/// The list after an edit in `story` at `at`: that story moves to the front, the list is
/// ordered newest first and keeps [`RECENT_STORIES`] entries, so a fourth story pushes
/// the oldest out. Story 0 leaves the list unchanged (`recordRecentStory`,
/// `obsRecency.ts:18-22`; a story number that is not a positive integer cannot reach
/// this type).
pub fn record_recent_story(list: &[RecentStory], story: u32, at: i64) -> Vec<RecentStory> {
    if story == 0 {
        return list.to_vec();
    }
    let mut out = Vec::with_capacity(list.len() + 1);
    out.push(RecentStory { story, at });
    out.extend(list.iter().filter(|entry| entry.story != story));
    // A stable sort, as JavaScript's: equal times keep the new entry first.
    out.sort_by_key(|entry| std::cmp::Reverse(entry.at));
    out.truncate(RECENT_STORIES);
    out
}
