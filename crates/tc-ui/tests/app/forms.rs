//! The forms of Home's dialogs (B2): the language choice and its code (Q35), Add a book's
//! first pick, and New Bible's versification list.

use tc_api::create::{UsfmProblem, UsfmVerdict};
use tc_api::home::{BookTileView, Door43Card, HomeCard, LanguageSuggestion, ProjectKind};
use tc_api::ids::{ProjectRef, RequestId, UploadId};
use tc_api::refusal::CreateRefusal;
use tc_api::shown::{Direction, Known};
use tc_core::BookId;
use tc_ui::app::forms::{
    AddBookForm, AddBookStep, CodeOffer, NewProjectForm, RowState, UsfmState, code_offer,
};

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

fn rid(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

fn filled() -> NewProjectForm {
    let mut form = NewProjectForm::bible(Ok(vec!["eng".into()]));
    form.name = "Kanuri Bible".into();
    form.code = "kr".into();
    form
}

#[test]
fn create_sends_nothing_until_the_name_and_the_code_are_there() {
    let mut form = NewProjectForm::bible(Ok(vec!["eng".into()]));
    assert_eq!(
        form.press(|| rid(1)),
        Some(Err(CreateRefusal::NameRequired))
    );
    form.name = "Kanuri Bible".into();
    assert_eq!(
        form.press(|| rid(1)),
        Some(Err(CreateRefusal::CodeRequired))
    );
    form.name = "Библия".into();
    form.code = "кр".into();
    assert_eq!(
        form.press(|| rid(1)),
        Some(Err(CreateRefusal::AbbrRequired))
    );
    assert!(!form.busy && form.request.is_none());
}

/// A-27: Create is disabled while its request is in flight, and a press after a lost reply sends
/// the same request, which the server answers from its reply cache.
#[test]
fn create_is_disabled_in_flight_and_a_press_after_a_lost_reply_sends_the_same_request() {
    let mut form = filled();
    let (first, details) = form.press(|| rid(1)).unwrap().unwrap();
    assert_eq!(first, rid(1));
    assert_eq!(details.name, "Kanuri Bible");
    assert!(form.busy);
    assert_eq!(form.press(|| rid(2)), None, "a second press in flight");
    form.lost("the server did not answer".into());
    assert!(!form.busy);
    assert_eq!(form.press(|| rid(3)).unwrap().unwrap().0, rid(1));
    // An answer, a refusal included, ends the attempt: the next press is a new request.
    form.answered(Some(
        "A project folder named “kanuri_bible” already exists.".into(),
    ));
    assert!(!form.busy && form.error.is_some());
    assert_eq!(form.press(|| rid(4)).unwrap().unwrap().0, rid(4));
    assert_eq!(form.error, None);
    // An edit makes what the press sends another request too.
    form.lost("no answer".into());
    form.edited();
    assert_eq!(form.press(|| rid(5)).unwrap().unwrap().0, rid(5));
}

#[test]
fn create_sends_the_door43_code_beside_the_chosen_one() {
    let mut form = filled();
    form.choose(&suggestion(
        "Lebanese Arabic",
        Some("acm-x-lebarabi-c"),
        Some("acm-x-lebarabic"),
        Direction::Rtl,
    ));
    let (_, details) = form.press(|| rid(1)).unwrap().unwrap();
    assert_eq!(details.code, "acm-x-lebarabi-c");
    assert_eq!(details.door43_code.as_deref(), Some("acm-x-lebarabic"));
    assert_eq!(details.language_name, "Lebanese Arabic");
    assert_eq!(details.direction, Direction::Rtl);
}

fn card_with(books: &[BookId]) -> HomeCard {
    HomeCard {
        project: ProjectRef::parse("p").unwrap(),
        name: "P".into(),
        tag: Known::Known("en".into()),
        dir: Direction::Ltr,
        kind: ProjectKind::Bible,
        door43: Door43Card::Local,
        books: Known::Known(
            books
                .iter()
                .map(|book| BookTileView {
                    book: *book,
                    drafted: Known::Unknown,
                    conflicts: 0,
                })
                .collect(),
        ),
        stories: None,
        conflicts: Known::Unknown,
    }
}

#[test]
fn a_blank_pick_leaves_out_the_books_the_project_holds_and_warns_of_them() {
    let mut form = AddBookForm::open(&card_with(&[BookId::Titus]));
    form.book = BookId::Titus;
    assert_eq!(form.picked(), Vec::<BookId>::new());
    assert_eq!(form.held_picks(), vec![BookId::Titus]);
    assert_eq!(form.press_blank(|| rid(1)), Some(Err(())));
    form.multi = true;
    form.toggle(BookId::Titus);
    form.toggle(BookId::Jude);
    form.toggle(BookId::Genesis);
    form.toggle(BookId::Jude);
    assert_eq!(form.picked(), vec![BookId::Genesis]);
    let (request, books) = form.press_blank(|| rid(1)).unwrap().unwrap();
    assert_eq!((request, books), (rid(1), vec![BookId::Genesis]));
    assert_eq!(form.press_blank(|| rid(2)), None, "disabled in flight");
}

fn read(upload: u8, book: BookId) -> UsfmState {
    UsfmState::Read {
        upload: UploadId::parse(&format!("{upload:032x}")).unwrap(),
        verdict: UsfmVerdict::Book { book },
    }
}

/// tC4 #484, #564: each file is judged as the list changes; a book the project holds and a
/// second file of a book are refused in their own rows; Add is labelled the same while disabled.
#[test]
fn usfm_rows_are_judged_again_at_every_add_and_remove() {
    let mut form = AddBookForm::open(&card_with(&[BookId::Jude]));
    let a = form.add_file("a-TIT.usfm".into());
    assert!(form.add_disabled(), "a file still being read");
    form.file_read(a, read(1, BookId::Titus));
    let b = form.add_file("b-TIT.usfm".into());
    form.file_read(b, read(2, BookId::Titus));
    let c = form.add_file("c-JUD.usfm".into());
    form.file_read(c, read(3, BookId::Jude));
    let d = form.add_file("notes.txt".into());
    form.file_read(
        d,
        UsfmState::Read {
            upload: UploadId::parse(&format!("{:032x}", 4)).unwrap(),
            verdict: UsfmVerdict::Refused(UsfmProblem::NoId),
        },
    );
    let e = form.add_file("big.usfm".into());
    form.file_read(e, UsfmState::Unreadable("it was cut off".into()));
    let rows: Vec<RowState> = form.rows().into_iter().map(|row| row.state).collect();
    assert_eq!(
        rows,
        vec![
            RowState::Adds(BookId::Titus),
            RowState::Refused(UsfmProblem::TwoFiles {
                book: BookId::Titus,
                first: "a-TIT.usfm".into()
            }),
            RowState::Refused(UsfmProblem::Exists { book: BookId::Jude }),
            RowState::Refused(UsfmProblem::NoId),
            RowState::Unreadable("it was cut off".into()),
        ]
    );
    assert_eq!(form.addable(), 1);
    assert!(!form.add_disabled());
    // Removing the first file gives the book to the second.
    form.remove_file(a);
    assert_eq!(form.rows()[0].state, RowState::Adds(BookId::Titus));
    // The answer for a file that was removed meanwhile changes nothing.
    form.file_read(a, UsfmState::Unreadable("late".into()));
    assert_eq!(form.files.len(), 4);
    form.remove_file(b);
    form.remove_file(c);
    assert_eq!(form.addable(), 0);
    assert!(form.add_disabled());
}

#[test]
fn an_add_keeps_its_error_until_the_next_add_and_the_books_it_added_leave_the_list() {
    let mut form = AddBookForm::open(&card_with(&[]));
    for (name, book, upload) in [("a.usfm", BookId::Titus, 1), ("b.usfm", BookId::Jude, 2)] {
        let at = form.add_file(name.into());
        form.file_read(at, read(upload, book));
    }
    let (request, files) = form.press_usfm(|| rid(1)).unwrap();
    assert_eq!(request, rid(1));
    assert_eq!(files.len(), 2);
    assert_eq!(form.press_usfm(|| rid(2)), None, "disabled in flight");
    form.added(
        &[("a.usfm".into(), BookId::Titus)],
        Some("b.usfm was not added: the disk is full".into()),
    );
    assert_eq!(form.files.len(), 1);
    assert_eq!(form.existing, vec![BookId::Titus]);
    // The error stays while the list changes (#564)...
    let at = form.add_file("c.usfm".into());
    form.file_read(at, read(3, BookId::Philemon));
    assert!(form.error.is_some());
    // ...and goes at the next Add.
    let (request, _) = form.press_usfm(|| rid(3)).unwrap();
    assert_eq!(request, rid(3));
    assert_eq!(form.error, None);
}
