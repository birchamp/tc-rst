//! Home's decisions (unit B2), the parts of tC4's `Home.jsx` that are not drawing: what a
//! card's summary line says, which tiles a card shows, which stories a collapsed story card
//! shows, what the Share button says and does, and the Resume card's words. Pure, so the ports
//! of tC4's `homeProjectCard` and `homeObsCard` tests run on the host.
//!
//! An unknown count is "—", never a false 0 (tC4 D30).

use tc_api::home::{
    BookTileView, Door43Card, HomeCard, LastEditView, ProjectKind, RecentStory, RepoPlace,
    ResumePlace, StoriesView, StoryTileView,
};
use tc_api::ids::ProjectRef;
use tc_api::shown::Known;

use crate::i18n::{Key, keys, t};

/// Above this many books a card shows only its books in progress until expanded
/// (`Home.jsx:18`).
pub const COLLAPSE_ABOVE: usize = 12;

/// The stories a collapsed story card shows (`Home.jsx:79`).
pub const RECENT_TILES: usize = 3;

/// The "—" of an unknown value.
pub const DASH: &str = "—";

fn percent_of(drafted: &Known<u8>) -> Option<u8> {
    match drafted {
        Known::Known(n) => Some(*n),
        Known::Unknown => None,
    }
}

/// Every book's progress is known.
pub fn all_known(books: &[BookTileView]) -> bool {
    books.iter().all(|b| percent_of(&b.drafted).is_some())
}

/// The books whose progress is known and above 0.
pub fn in_progress(books: &[BookTileView]) -> Vec<&BookTileView> {
    books
        .iter()
        .filter(|b| percent_of(&b.drafted).is_some_and(|n| n > 0))
        .collect()
}

fn books_of(card: &HomeCard) -> &[BookTileView] {
    match &card.books {
        Known::Known(books) => books,
        Known::Unknown => &[],
    }
}

fn tag_of(card: &HomeCard) -> &str {
    match &card.tag {
        Known::Known(tag) => tag,
        Known::Unknown => DASH,
    }
}

/// A Bible card's summary line: "es-419 · 2 books · 1 in progress". The in-progress count is
/// stated only when every book's progress is known; before that it is "—", as on the tiles.
pub fn summary_line(card: &HomeCard) -> String {
    let books = books_of(card);
    let count = match &card.books {
        Known::Known(books) => books.len().to_string(),
        Known::Unknown => DASH.to_owned(),
    };
    let word = t(
        if books.len() == 1 {
            keys::home::BOOK
        } else {
            keys::home::BOOKS
        },
        &[],
    );
    let known = matches!(card.books, Known::Known(_)) && all_known(books);
    let n = if known {
        in_progress(books).len().to_string()
    } else {
        DASH.to_owned()
    };
    format!(
        "{} · {count} {word} · {}",
        tag_of(card),
        t(keys::home::IN_PROGRESS, &[("n", &n)])
    )
}

/// A story card's line: "es · 50 stories · 3 in progress · 3% drafted", with "—" for an
/// unknown percentage.
pub fn obs_line(card: &HomeCard, stories: &StoriesView) -> String {
    let in_progress = stories
        .stories
        .iter()
        .filter(|s| percent_of(&s.drafted).is_some_and(|n| n > 0))
        .count()
        .to_string();
    let drafted = match &stories.drafted {
        Known::Known(n) => t(keys::home::DRAFTED, &[("n", &n.to_string())]),
        Known::Unknown => DASH.to_owned(),
    };
    format!(
        "{} · {} · {} · {drafted}",
        tag_of(card),
        t(keys::home::OBS_STORIES, &[]),
        t(keys::home::IN_PROGRESS, &[("n", &in_progress)])
    )
}

/// The books a card shows: a card of many books shows only those in progress, once every
/// book's progress is known, until it is expanded.
pub fn shown_books(books: &[BookTileView], expanded: bool) -> Vec<BookTileView> {
    if shows_book_toggle(books) && !expanded {
        in_progress(books).into_iter().cloned().collect()
    } else {
        books.to_vec()
    }
}

/// "Show all N books" shows under a card of many books once every book's progress is known.
pub fn shows_book_toggle(books: &[BookTileView]) -> bool {
    books.len() > COLLAPSE_ABOVE && all_known(books)
}

/// The story tiles a collapsed card shows: the recently edited stories, newest first, or the
/// first [`RECENT_TILES`] when nothing was edited (`collapsedStories`).
pub fn collapsed_stories<'a>(
    stories: &'a [StoryTileView],
    recent: &[RecentStory],
) -> Vec<&'a StoryTileView> {
    let edited: Vec<&StoryTileView> = recent
        .iter()
        .filter_map(|entry| stories.iter().find(|s| s.story == entry.story))
        .collect();
    if edited.is_empty() {
        stories.iter().take(RECENT_TILES).collect()
    } else {
        edited.into_iter().take(RECENT_TILES).collect()
    }
}

/// A story tile's name: "1 · La Creación", or "Story 2" when the story has no title.
pub fn story_tile_name(tile: &StoryTileView) -> String {
    let n = tile.story.get().to_string();
    if tile.title.is_empty() {
        t(keys::story_draft::STORY_NUMBER, &[("n", &n)])
    } else {
        format!("{n} · {}", tile.title)
    }
}

/// A story tile's figure: "—" when unknown, "13% · 4 Oct" when edited here, else the
/// percentage alone (`None`).
pub fn story_tile_meta(tile: &StoryTileView, edited: Option<String>) -> Option<String> {
    match (percent_of(&tile.drafted), edited) {
        (None, _) => Some(DASH.to_owned()),
        (Some(n), Some(date)) => Some(format!("{n}% · {date}")),
        (Some(_), None) => None,
    }
}

/// What the Share button of a card says and does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareButton {
    pub label: Key,
    /// `data-action`: `sync` for a shared project, `share` for one only on this computer.
    pub action: &'static str,
    pub shared: bool,
    pub busy: bool,
    pub disabled: bool,
}

/// Every shared project offers "Sync with Door43" (PLAN Q2, Bible and story projects alike;
/// tC4 round 1 offers "Upload changes" on a shared story project). While a sync runs, its
/// card says "Syncing…" and the other shared cards' buttons are off.
pub fn share_button(card: &HomeCard, syncing: Option<&ProjectRef>) -> ShareButton {
    let shared = matches!(card.door43, Door43Card::Shared { .. });
    let busy = syncing == Some(&card.project);
    ShareButton {
        label: match (busy, shared) {
            (true, _) => keys::home::SYNCING,
            (false, true) => keys::home::SYNC,
            (false, false) => keys::home::SHARE,
        },
        action: if shared { "sync" } else { "share" },
        shared,
        busy,
        disabled: shared && syncing.is_some(),
    }
}

/// Where a shared project is: "qa.door43.org/owner/repo" in a development build whose server
/// holds it (tC4 #506), else "owner/repo".
pub fn location(place: &RepoPlace) -> String {
    match &place.server {
        Some(server) => format!("{server}/{}/{}", place.owner, place.name),
        None => format!("{}/{}", place.owner, place.name),
    }
}

/// The names more than one card holds: those cards say where each lives (ruling H14).
pub fn shared_names(cards: &[HomeCard]) -> Vec<String> {
    let mut names: Vec<&str> = cards.iter().map(|c| c.name.as_str()).collect();
    names.sort_unstable();
    let mut shared: Vec<String> = names
        .iter()
        .zip(names.iter().skip(1))
        .filter(|(a, b)| a == b)
        .map(|(a, _)| (*a).to_owned())
        .collect();
    shared.dedup();
    shared
}

/// The second line of a card whose name another card has (ruling H14): "On Door43:
/// owner/repo" or "Only on this computer". `None` for a name no other card has.
pub fn where_line(card: &HomeCard, shared: &[String]) -> Option<String> {
    shared.contains(&card.name).then(|| match &card.door43 {
        Door43Card::Shared { place } => t(
            keys::home::WHERE_ON_DOOR43,
            &[("location", &location(place))],
        ),
        Door43Card::Local => t(keys::home::ONLY_HERE, &[]),
    })
}

/// The Resume card's title after the project's name: "Titus 1", or "Story 3".
pub fn resume_place(resume: &LastEditView) -> String {
    match &resume.place {
        ResumePlace::Book { book, chapter, .. } => format!("{} {chapter}", book.english_name()),
        ResumePlace::Story { story, .. } => t(
            keys::story_draft::STORY_NUMBER,
            &[("n", &story.get().to_string())],
        ),
    }
}

/// The Resume card's line: the verse or frame, with the start of its text when there is one.
pub fn resume_line(resume: &LastEditView) -> String {
    let (story, v) = match &resume.place {
        ResumePlace::Book { verse, .. } => (false, verse.to_string()),
        ResumePlace::Story { frame, .. } => (true, frame.to_string()),
    };
    let key = match (story, resume.snippet.is_empty()) {
        (false, false) => keys::home::LAST_EDITED,
        (false, true) => keys::home::LAST_EDITED_EMPTY,
        (true, false) => keys::home::LAST_EDITED_FRAME,
        (true, true) => keys::home::LAST_EDITED_FRAME_EMPTY,
    };
    t(key, &[("v", &v), ("text", &resume.snippet)])
}

/// Whether a card is a story project's.
pub fn is_stories(card: &HomeCard) -> bool {
    card.kind == ProjectKind::Stories
}

/// The failed checkpoint's banner (tC4 `startLeaveCheckpoint`, `Home.jsx:363-368`): "Checkpoint
/// not saved: <reason>", and Ask for help when the failure is a problem in the app.
pub fn checkpoint_banner(
    failed: &tc_api::app::FailedCheckpoint,
) -> (String, Option<crate::app::feedback::HelpRequest>) {
    let text = format!("{}: {}", t(keys::app::COMMIT_ERROR, &[]), failed.reason);
    let help = failed
        .code
        .clone()
        .map(|code| crate::app::feedback::HelpRequest {
            code,
            text: text.clone(),
            report: None,
        });
    (text, help)
}
