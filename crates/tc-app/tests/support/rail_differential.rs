//! The rail differential test (APP-DESIGN §3.8, AD17, A-13): the UI's one-pass grouping
//! (`tc_api::model::rail`) over the rows `check/open` serves equals tc-check's grouping
//! (`tc_check::rail::groups`, tC4's `railGroupsOf`) over a list derived here from the same
//! installed resources and decisions, for every filter and every mode the tool offers.

use std::path::Path;

use tc_api::check::{Category, NoteCategory, RailRow, Tool, WordCategory};
use tc_api::model::rail::{self, Filter, GroupLabel, SortMode};
use tc_check::derive::{BookSpec, notes_for_book, word_links_for_book};
use tc_check::rail::{self as check_rail, GroupLabel as CheckLabel};
use tc_check::{CheckItem, CheckState, Collection, ToolId};
use tc_core::BookId;
use tc_helps::HelpsPart;
use tc_journal::fold::FoldState;
use tc_journal::{PinGroup, Record, RecordKey};
use tc_resources::resolve::{Resolved, resolve_tool};
use tc_resources::{HelpsChoice, ProjectPins, ResourceStore};

/// The list and each check's state, derived with tc-check alone.
pub fn tc_check_side(
    resources: &Path,
    fold: &FoldState,
    tool: Tool,
    book: BookId,
) -> (Vec<CheckItem>, Vec<CheckState>) {
    let tool_id = match tool {
        Tool::TranslationNotes => ToolId::TranslationNotes,
        Tool::TranslationWords => ToolId::TranslationWords,
    };
    let mut pins = ProjectPins::default();
    for group in PinGroup::ALL {
        if let Some(Record::Pins { value, .. }) = fold.projected(&RecordKey::Pins(group)) {
            pins.set_group_value(group, &value).unwrap();
        }
    }
    let store = ResourceStore::open_read_only(resources).unwrap();
    let loaded = tc_resources::load_book(&store, &pins, &HelpsChoice::Dcs, book);
    let spec = BookSpec {
        book,
        resource_frame: tc_versification::StandardScheme::Eng.scheme(),
        project_frame: Some(tc_versification::StandardScheme::Eng.scheme()),
        scope: &[],
    };
    let helps = &loaded.helps;
    let list = match tool {
        Tool::TranslationNotes => notes_for_book(helps.notes(), &spec),
        Tool::TranslationWords => {
            let articles = loaded
                .parts
                .get(&HelpsPart::WordArticles)
                .filter(|p| p.is_loaded())
                .map(|_| {
                    helps
                        .word_links()
                        .iter()
                        .filter(|l| helps.word_article(&l.article).is_some())
                        .map(|l| l.article.clone())
                        .collect()
                });
            word_links_for_book(helps.word_links(), articles.as_ref(), &spec)
        }
    };
    let Some(Resolved::Pin(resolution)) =
        resolve_tool(&pins, tool_id, Collection::Book(book), &store)
    else {
        panic!("{tool:?} does not resolve");
    };
    let decisions = fold.decisions(tool_id, Collection::Book(book));
    let session = tc_check::session::open(
        list.items,
        &decisions,
        Some(&resolution.pin.journal_pin().resource),
        |_| None,
    );
    (session.items, session.states)
}

fn api_name(category: Category) -> &'static str {
    match category {
        Category::Note(n) => match n {
            NoteCategory::Discourse => "discourse",
            NoteCategory::Numbers => "numbers",
            NoteCategory::Figures => "figures",
            NoteCategory::Culture => "culture",
            NoteCategory::Grammar => "grammar",
            NoteCategory::Other => "other",
        },
        Category::Word(w) => match w {
            WordCategory::Kt => "kt",
            WordCategory::Names => "names",
            WordCategory::Other => "other",
        },
    }
}

fn check_filter(filter: Filter) -> check_rail::Filter {
    match filter {
        Filter::All => check_rail::Filter::All,
        Filter::ToDo => check_rail::Filter::ToDo,
        Filter::Invalid => check_rail::Filter::Invalid,
        Filter::Bookmarked => check_rail::Filter::Bookmarked,
        Filter::Commented => check_rail::Filter::Commented,
    }
}

fn check_mode(mode: SortMode) -> check_rail::SortMode {
    match mode {
        SortMode::ByVerse => check_rail::SortMode::ByVerse,
        SortMode::ByWord => check_rail::SortMode::ByWord,
        SortMode::ByCategory => check_rail::SortMode::ByCategory,
    }
}

/// Fails unless both groupings agree for every filter and mode; answers how many rows the
/// rail holds.
pub fn assert_groups_agree(
    tool: Tool,
    rows: &[RailRow],
    terms: &[String],
    items: &[CheckItem],
    states: &[CheckState],
) -> usize {
    assert_eq!(
        rows.len(),
        items.len(),
        "{tool:?}: the rail holds every check"
    );
    for filter in Filter::ORDER {
        for &mode in SortMode::modes(tool) {
            let ours: Vec<(String, Vec<u32>)> = rail::groups(rows, terms.len(), filter, mode)
                .into_iter()
                .map(|g| {
                    let label = match g.label {
                        GroupLabel::Unit => "unit".to_owned(),
                        GroupLabel::Term(t) => terms[t as usize].clone(),
                        GroupLabel::Category(c) => api_name(c).to_owned(),
                    };
                    (label, g.rows.iter().map(|&at| rows[at].index).collect())
                })
                .collect();
            let theirs: Vec<(String, Vec<u32>)> =
                check_rail::groups(items, states, check_filter(filter), check_mode(mode))
                    .into_iter()
                    .map(|g| {
                        let label = match g.label {
                            CheckLabel::Unit => "unit".to_owned(),
                            CheckLabel::Term(term) => term,
                            CheckLabel::Category(c) => c.name().to_owned(),
                        };
                        (label, g.rows.iter().map(|&i| i as u32).collect())
                    })
                    .collect();
            assert_eq!(ours, theirs, "{tool:?} {filter:?} {mode:?}");
        }
    }
    rows.len()
}
