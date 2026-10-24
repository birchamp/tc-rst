//! The OBS Translation Questions of a story (tC4 `deriveObsItems` with
//! `translationQuestions`, `src/data/derive.ts:585-597`, #331). The tN and tW check lists
//! of the stories are tc-check's (`notes_for_stories`, `word_links_for_stories`).

use tc_helps::tsv::{TsvScope, parse_questions_leniently};
use tc_helps::{Error, HelpRef, Question};

use crate::ids::StoryNo;

/// Reads an OBS Translation Questions table as tC4 reads it:
/// [`tc_helps::tsv::parse_questions_leniently`] for the stories.
pub fn parse_story_questions(tsv: &str) -> Result<Vec<Question>, Error> {
    parse_questions_leniently(tsv, TsvScope::Obs)
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
