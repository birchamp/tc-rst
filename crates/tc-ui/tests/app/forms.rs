//! The forms of Home's dialogs (B2): the language choice and its code (Q35), Add a book's
//! first pick, and New Bible's versification list.

use tc_api::home::{BookTileView, Door43Card, HomeCard, LanguageSuggestion, ProjectKind};
use tc_api::ids::ProjectRef;
use tc_api::shown::{Direction, Known};
use tc_core::BookId;
use tc_ui::app::forms::{AddBookForm, AddBookStep, CodeOffer, NewProjectForm, code_offer};

fn suggestion(
    name: &str,
    code: Option<&str>,
    door43: Option<&str>,
    dir: Direction,
) -> LanguageSuggestion {
    LanguageSuggestion {
        name: name.into(),
        own: None,
        code: code.map(str::to_owned),
        door43: door43.map(str::to_owned),
        dir,
    }
}

#[test]
fn choosing_a_suggestion_fills_name_code_and_direction() {
    let mut form = NewProjectForm::stories();
    form.choose(&suggestion("Arabic", Some("ar"), None, Direction::Rtl));
    assert_eq!(
        (form.lang_name.as_str(), form.code.as_str(), form.dir),
        ("Arabic", "ar", Direction::Rtl)
    );
    assert_eq!(code_offer(&form), CodeOffer::None);
}

#[test]
fn a_door43_code_a_burrito_cannot_hold_is_offered_adjusted() {
    let mut form = NewProjectForm::stories();
    form.choose(&suggestion(
        "Lebanese Arabic",
        Some("acm-x-lebarabi-c"),
        Some("acm-x-lebarabic"),
        Direction::Rtl,
    ));
    assert_eq!(form.code, "acm-x-lebarabi-c");
    assert_eq!(form.door43_code.as_deref(), Some("acm-x-lebarabic"));
    assert_eq!(
        code_offer(&form),
        CodeOffer::Adjusted {
            door43: "acm-x-lebarabic",
            code: "acm-x-lebarabi-c"
        }
    );
}

#[test]
fn an_unusable_door43_code_leaves_the_code_empty_and_says_why() {
    let mut form = NewProjectForm::stories();
    form.choose(&suggestion("Iban", None, Some("iba-x-"), Direction::Ltr));
    assert_eq!(form.code, "");
    assert_eq!(code_offer(&form), CodeOffer::Unusable { door43: "iba-x-" });
}

#[test]
fn typing_a_code_drops_the_offer() {
    let mut form = NewProjectForm::stories();
    form.choose(&suggestion(
        "Lebanese Arabic",
        Some("acm-x-lebarabi-c"),
        Some("acm-x-lebarabic"),
        Direction::Rtl,
    ));
    form.type_code("apc".into());
    assert_eq!(code_offer(&form), CodeOffer::None);
    assert_eq!(form.door43_code, None);
}

#[test]
fn add_book_starts_at_the_first_book_not_in_the_project() {
    let mut card = HomeCard {
        project: ProjectRef::parse("p").unwrap(),
        name: "P".into(),
        tag: Known::Known("en".into()),
        dir: Direction::Ltr,
        kind: ProjectKind::Bible,
        door43: Door43Card::Local,
        books: Known::Known(
            [BookId::Genesis, BookId::Exodus]
                .into_iter()
                .map(|book| BookTileView {
                    book,
                    drafted: Known::Unknown,
                    conflicts: 0,
                })
                .collect(),
        ),
        stories: None,
        conflicts: Known::Unknown,
    };
    let form = AddBookForm::open(&card);
    assert_eq!(
        (form.step, form.book),
        (AddBookStep::Method, BookId::Leviticus)
    );
    assert_eq!(form.project_name, "P");
    card.books = Known::Known(
        BookId::ALL
            .into_iter()
            .map(|book| BookTileView {
                book,
                drafted: Known::Unknown,
                conflicts: 0,
            })
            .collect(),
    );
    assert_eq!(AddBookForm::open(&card).book, BookId::Genesis);
}

#[test]
fn a_failed_versification_list_offers_eng_and_says_so() {
    let form = NewProjectForm::bible(Err("the server did not answer".into()));
    assert_eq!(form.versifications, ["eng"]);
    assert_eq!(form.versification, "eng");
    assert!(form.versifications_error.is_some());
    let form = NewProjectForm::bible(Ok(vec!["eng".into(), "lxx".into()]));
    assert_eq!(form.versifications, ["eng", "lxx"]);
    assert_eq!(form.versifications_error, None);
}
