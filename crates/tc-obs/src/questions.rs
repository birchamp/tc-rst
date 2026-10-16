//! The OBS Translation Questions of a story (tC4 `deriveObsItems` with
//! `translationQuestions`, `src/data/derive.ts:585-597`, #331). The tN and tW check lists
//! of the stories are tc-check's (`notes_for_stories`, `word_links_for_stories`).

use tc_helps::tsv::{TsvScope, parse_questions};
use tc_helps::{Error, HelpRef, ObsRef, Question};

use crate::ids::StoryNo;

/// Reads an OBS Translation Questions table, leaving out each row whose `Reference` does
/// not address a frame (`story:frame`, story 1 to 50, frame 0 the title) and keeping the
/// rest: tC4 drops a row whose locator is not two integers (`derive.ts:592-594`,
/// R-10.4.1). Everything else is tc-helps's reading, with its refusals of the whole
/// table: a header that is not the questions header, a row with another number of cells,
/// an unreadable ID or occurrence.
///
/// tC4 reads only the first two parts of a locator and any integers, so it keeps `1:2:3`
/// as frame 2 of story 1, and `0:1` and `51:1` for stories no project has. No session of a
/// story shows those, and a tc-helps question cannot hold them, so they are left out too.
pub fn parse_story_questions(tsv: &str) -> Result<Vec<Question>, Error> {
    let mut header = true;
    let rows: Vec<&str> = tsv
        .split('\n')
        .map(|line| {
            if skipped(line) {
                line
            } else if header {
                header = false;
                line
            } else if addresses_a_frame(line) {
                line
            } else {
                // An empty line in its place, which tc-helps skips, so a refusal of a later
                // row still names its line in the file.
                ""
            }
        })
        .collect();
    parse_questions(&rows.join("\n"), TsvScope::Obs)
}

/// The lines tc-helps's TSV reader skips, as tC4's `tsvRows` does those `trim()` empties;
/// the first line it does not skip is the header.
fn skipped(line: &str) -> bool {
    line.chars().all(|c| c.is_whitespace() || c == '\u{feff}')
}

/// Whether a row's `Reference` cell, as tc-helps reads it, is a frame's address.
fn addresses_a_frame(line: &str) -> bool {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let reference = match line.split_once('\t') {
        Some((first, _)) => first.strip_suffix('\r').unwrap_or(first),
        None => line,
    };
    ObsRef::parse(reference).is_some()
}

/// The questions keyed to a frame (`story:frame`, frame 0 the title), in table order;
/// with `story`, only that story's. A question read from a Bible table has no frame and
/// is left out.
pub fn story_questions(questions: &[Question], story: Option<StoryNo>) -> Vec<&Question> {
    questions
        .iter()
        .filter(|q| match q.reference {
            HelpRef::Obs(at) => story.is_none_or(|s| s.get() == at.story()),
            HelpRef::Bible(_) => false,
        })
        .collect()
}
