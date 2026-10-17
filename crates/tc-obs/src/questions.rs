//! The OBS Translation Questions of a story (tC4 `deriveObsItems` with
//! `translationQuestions`, `src/data/derive.ts:585-597`, #331). The tN and tW check lists
//! of the stories are tc-check's (`notes_for_stories`, `word_links_for_stories`).

use tc_helps::tsv::{TsvScope, parse_questions};
use tc_helps::{CellProblem, Error, HelpRef, ObsRef, Question};

use crate::ids::StoryNo;

/// The columns of the questions table (`TQ_HEADER`, `src/data/derive.ts:136`).
const COLUMNS: usize = 7;
/// The `Occurrence` column, from 0 (`src/data/derive.ts:325`).
const OCCURRENCE: usize = 4;

/// Reads an OBS Translation Questions table as tC4 reads it: every row is a question
/// (`deriveTqItems`, `src/data/derive.ts:323-348`), except a row whose `Reference` does not
/// address a frame (`story:frame`, story 1 to 50, frame 0 the title), which tC4 drops
/// (`derive.ts:592-594`, R-10.4.1). Only a header that is not the questions header refuses
/// the table (`tsvRows`, `derive.ts:138-148`).
///
/// tc-helps refuses a whole table for one row it cannot read; tC4 reads past such a row, so
/// one bad row must not cost fifty stories their questions:
///
/// - A row's cells are taken by position (`derive.ts:325`): a missing cell is empty
///   (tC4's `?? ''`) and a cell past the seventh is not read.
/// - An `Occurrence` that is not a count (`x`, `1.5`, ` 1`) is held as
///   [`Occurrence::Unrecorded`](tc_helps::Occurrence::Unrecorded). tC4 holds `NaN`
///   (`derive.ts:336`), and no view of a question reads it: the questions tab shows the
///   question, the response and the frame (`src/views/HelpsPanel.jsx:294-309`).
/// - A row whose `ID` is not a row id (1 to 16 ASCII letters and digits) is left out, and
///   the rest kept. tC4 shows its question; a tc-helps question cannot hold that ID, and
///   this crate does not make one up.
///
/// tC4 reads only the first two parts of a locator and any integers, so it keeps `1:2:3`
/// as frame 2 of story 1, and `0:1` and `51:1` for stories no project has. No session of a
/// story shows those, and a tc-helps question cannot hold them, so they are left out too.
///
/// The questions are read for the screen only: tC4 derives them on each load and never
/// stores or writes them (`src/state.jsx:2349-2353`, `:2386`).
pub fn parse_story_questions(tsv: &str) -> Result<Vec<Question>, Error> {
    let mut lines = tsv.split('\n').filter(|line| !skipped(line));
    let Some(header) = lines.next() else {
        // No header at all: tc-helps's refusal, as tC4 throws on it.
        return parse_questions(tsv, TsvScope::Obs);
    };
    parse_questions(header, TsvScope::Obs)?;
    let mut questions = Vec::new();
    for line in lines.filter(|line| addresses_a_frame(line)) {
        let mut cells: Vec<&str> = line.split('\t').take(COLUMNS).collect();
        cells.resize(COLUMNS, "");
        let read = match read_row(header, &cells) {
            Err(Error::TsvCell {
                problem: CellProblem::Occurrence(_),
                ..
            }) => {
                cells[OCCURRENCE] = "";
                read_row(header, &cells)
            }
            read => read,
        };
        match read {
            Ok(question) => questions.extend(question),
            Err(Error::TsvCell {
                problem: CellProblem::RowId(_),
                ..
            }) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(questions)
}

/// One row of seven cells, read under the table's header. tc-helps drops a CR at the end of
/// each cell wherever it stands, as tC4 does (`derive.ts:147`), so the cells read as they
/// did on their own line.
fn read_row(header: &str, cells: &[&str]) -> Result<Option<Question>, Error> {
    let table = format!("{header}\n{}", cells.join("\t"));
    Ok(parse_questions(&table, TsvScope::Obs)?.pop())
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
