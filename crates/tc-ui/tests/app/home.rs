//! Home's decisions (B2): the ports of tC4's `homeProjectCard` and `homeObsCard` tests, the
//! Share button, the Door43 place and the Resume card's words.

use tc_api::home::{
    BookTileView, Door43Card, HomeCard, LastEditView, ProjectKind, RecentStory, RepoPlace,
    ResumePlace, StoriesView, StoryTileView,
};
use tc_api::ids::{ProjectRef, RefPart, StoryNo, WallMs};
use tc_api::prefs::PlaceMode;
use tc_api::shown::{Direction, Known};
use tc_core::BookId;
use tc_ui::app::home::{
    collapsed_stories, location, obs_line, resume_line, resume_place, share_button, shared_names,
    shown_books, shows_book_toggle, story_tile_meta, story_tile_name, summary_line, where_line,
};
use tc_ui::i18n::keys;

fn tile(book: BookId, drafted: Option<u8>) -> BookTileView {
    BookTileView {
        book,
        drafted: drafted.map_or(Known::Unknown, Known::Known),
        conflicts: 0,
    }
}

fn card(books: Vec<BookTileView>) -> HomeCard {
    HomeCard {
        project: ProjectRef::parse("en_ult").unwrap(),
        name: "English ULT".into(),
        tag: Known::Known("en".into()),
        dir: Direction::Ltr,
        kind: ProjectKind::Bible,
        door43: Door43Card::Local,
        books: Known::Known(books),
        stories: None,
        conflicts: Known::Unknown,
    }
}

// The homeProjectCard port: the in-progress count is stated only when known.

#[test]
fn before_the_progress_read_the_summary_says_dash_in_progress() {
    let line = summary_line(&card(vec![
        tile(BookId::Ruth, None),
        tile(BookId::Titus, None),
    ]));
    assert_eq!(line, "en · 2 books · — in progress");
    assert!(!line.contains("0 in progress"));
}

#[test]
fn one_unknown_book_keeps_the_dash() {
    let line = summary_line(&card(vec![
        tile(BookId::Ruth, None),
        tile(BookId::Titus, Some(40)),
    ]));
    assert!(line.contains("— in progress"), "{line}");
}

#[test]
fn every_book_known_states_the_count_zero_included() {
    let line = summary_line(&card(vec![
        tile(BookId::Ruth, Some(0)),
        tile(BookId::Titus, Some(40)),
    ]));
    assert!(line.contains("1 in progress"), "{line}");
    let line = summary_line(&card(vec![
        tile(BookId::Ruth, Some(0)),
        tile(BookId::Titus, Some(0)),
    ]));
    assert!(line.contains("0 in progress"), "{line}");
    assert_eq!(
        summary_line(&card(vec![tile(BookId::Titus, Some(0))])),
        "en · 1 book · 0 in progress"
    );
}

#[test]
fn more_than_twelve_books_show_in_progress_only_when_all_known() {
    let mut books: Vec<BookTileView> = BookId::ALL[..13]
        .iter()
        .map(|b| tile(*b, Some(0)))
        .collect();
    books[3].drafted = Known::Known(20);
    assert!(shows_book_toggle(&books));
    assert_eq!(shown_books(&books, false).len(), 1);
    assert_eq!(shown_books(&books, true).len(), 13);
    books[5].drafted = Known::Unknown;
    assert!(!shows_book_toggle(&books));
    assert_eq!(
        shown_books(&books, false).len(),
        13,
        "an unread book is never hidden"
    );
    assert!(!shows_book_toggle(&books[..12]));
}

// The homeObsCard port.

fn story(n: u8, title: &str, drafted: Option<u8>) -> StoryTileView {
    StoryTileView {
        story: StoryNo::new(n).unwrap(),
        title: title.into(),
        drafted: drafted.map_or(Known::Unknown, Known::Known),
    }
}

fn fifty() -> Vec<StoryTileView> {
    (1..=50).map(|n| story(n, "", Some(0))).collect()
}

fn recent(n: u8, at: i64) -> RecentStory {
    RecentStory {
        story: StoryNo::new(n).unwrap(),
        at: WallMs(at),
    }
}

fn numbers(tiles: &[&StoryTileView]) -> Vec<u8> {
    tiles.iter().map(|t| t.story.get()).collect()
}

#[test]
fn collapsed_stories_are_the_three_most_recent_newest_first() {
    let stories = fifty();
    let shown = collapsed_stories(
        &stories,
        &[recent(7, 4), recent(2, 3), recent(40, 2), recent(9, 1)],
    );
    assert_eq!(numbers(&shown), [7, 2, 40]);
}

#[test]
fn collapsed_stories_skip_a_story_not_in_the_list() {
    let stories: Vec<StoryTileView> = (1..=5).map(|n| story(n, "", Some(0))).collect();
    let recent: Vec<RecentStory> = [4, 2].into_iter().map(|n| recent(n, 1)).collect();
    assert_eq!(numbers(&collapsed_stories(&stories, &recent)), [4, 2]);
}

#[test]
fn the_first_three_when_nothing_was_edited() {
    assert_eq!(numbers(&collapsed_stories(&fifty(), &[])), [1, 2, 3]);
}

#[test]
fn obs_line_says_dash_when_unknown() {
    let mut card = card(Vec::new());
    card.tag = Known::Known("es".into());
    card.kind = ProjectKind::Stories;
    let mut stories = StoriesView {
        drafted: Known::Known(3),
        stories: vec![
            story(1, "", Some(10)),
            story(2, "", Some(4)),
            story(3, "", Some(1)),
            story(4, "", Some(0)),
        ],
        recent: Vec::new(),
    };
    assert_eq!(
        obs_line(&card, &stories),
        "es · 50 stories · 3 in progress · 3% drafted"
    );
    stories.drafted = Known::Unknown;
    assert!(obs_line(&card, &stories).ends_with(" · —"));
}

#[test]
fn an_unknown_story_tile_says_dash_never_zero() {
    assert_eq!(
        story_tile_meta(&story(2, "", None), None).as_deref(),
        Some("—")
    );
    assert_eq!(story_tile_meta(&story(2, "", Some(0)), None), None);
    assert_eq!(
        story_tile_meta(&story(2, "", Some(13)), Some("4 Oct".into())).as_deref(),
        Some("13% · 4 Oct")
    );
    assert_eq!(
        story_tile_name(&story(1, "La Creación", Some(13))),
        "1 · La Creación"
    );
    assert_eq!(story_tile_name(&story(2, "", Some(0))), "Story 2");
}

#[test]
fn share_is_sync_for_every_shared_project_never_upload() {
    let mut local = card(Vec::new());
    let button = share_button(&local, None);
    assert_eq!(
        (button.label, button.action, button.shared),
        (keys::home::SHARE, "share", false)
    );
    local.door43 = Door43Card::Shared {
        place: RepoPlace {
            owner: "o".into(),
            name: "r".into(),
            server: None,
        },
    };
    for kind in [ProjectKind::Bible, ProjectKind::Stories] {
        local.kind = kind;
        let button = share_button(&local, None);
        assert_eq!((button.label, button.action), (keys::home::SYNC, "sync"));
    }
    let running = share_button(&local, Some(&local.project));
    assert_eq!(running.label, keys::home::SYNCING);
    assert!(running.busy);
    let other = ProjectRef::parse("other").unwrap();
    assert!(share_button(&local, Some(&other)).disabled);
}

#[test]
fn location_names_the_qa_server() {
    let mut place = RepoPlace {
        owner: "o".into(),
        name: "r".into(),
        server: None,
    };
    assert_eq!(location(&place), "o/r");
    place.server = Some("qa.door43.org".into());
    assert_eq!(location(&place), "qa.door43.org/o/r");
}

#[test]
fn the_resume_card_names_the_place_and_the_text() {
    let mut resume = LastEditView {
        project: ProjectRef::parse("en_ult").unwrap(),
        place: ResumePlace::Book {
            book: BookId::Titus,
            chapter: 1,
            verse: RefPart::parse("1").unwrap(),
        },
        mode: PlaceMode::Draft,
        tool: None,
        snippet: "Pablo".into(),
    };
    assert_eq!(resume_place(&resume), "Titus 1");
    assert_eq!(resume_line(&resume), "Last edited verse 1 — “Pablo”");
    resume.snippet.clear();
    assert_eq!(resume_line(&resume), "Last edited verse 1");
    resume.place = ResumePlace::Story {
        story: StoryNo::new(3).unwrap(),
        frame: 2,
    };
    assert_eq!(resume_place(&resume), "Story 3");
    assert_eq!(resume_line(&resume), "Last edited frame 2");
}

/// Ruling H14: a project has one name, so two cards can hold it; each then says where it lives.
#[test]
fn two_cards_of_one_name_each_say_where_they_live() {
    let mut on_door43 = card(vec![]);
    on_door43.project = ProjectRef::parse("en_ult-2").unwrap();
    on_door43.door43 = Door43Card::Shared {
        place: RepoPlace {
            owner: "Juan".into(),
            name: "es_tit_text".into(),
            server: None,
        },
    };
    let here = card(vec![]);
    let mut other = card(vec![]);
    other.project = ProjectRef::parse("titus").unwrap();
    other.name = "Titus".into();
    let cards = [on_door43.clone(), here.clone(), other.clone()];
    let shared = shared_names(&cards);
    assert_eq!(shared, ["English ULT"]);
    assert_eq!(
        where_line(&on_door43, &shared).as_deref(),
        Some("On Door43: Juan/es_tit_text")
    );
    assert_eq!(
        where_line(&here, &shared).as_deref(),
        Some("Only on this computer")
    );
    assert_eq!(
        where_line(&other, &shared),
        None,
        "a name of its own needs no line"
    );
}
