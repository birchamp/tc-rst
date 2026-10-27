//! The forms of Home's dialogs (B2): the language choice and its code (Q35), Add a book's
//! first pick, and New Bible's versification list.

use tc_api::create::{UsfmProblem, UsfmVerdict};
use tc_api::home::{BookTileView, Door43Card, HomeCard, LanguageSuggestion, ProjectKind};
use tc_api::ids::{ProjectRef, RequestId, UploadId};
use tc_api::refusal::CreateRefusal;
use tc_api::shown::{Direction, Known};
use tc_core::BookId;
use tc_ui::app::forms::{
    AddBookForm, AddBookStep, CodeOffer, NewKind, NewProjectForm, RowState, UsfmState,
    book_info_key, chapters, code_offer,
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

/// tC4 #577: a book of one chapter says "1 chapter", the others say how many.
#[test]
fn one_chapter_book_says_1_chapter() {
    for book in [
        BookId::Obadiah,
        BookId::Philemon,
        BookId::SecondJohn,
        BookId::ThirdJohn,
        BookId::Jude,
    ] {
        assert_eq!(chapters(book), 1, "{book}");
        assert_eq!(book_info_key(book), tc_ui::i18n::keys::add_book::INFO_ONE);
    }
    assert_eq!(chapters(BookId::Psalms), 150);
    assert_eq!(chapters(BookId::Revelation), 22);
    assert_eq!(
        book_info_key(BookId::Titus),
        tc_ui::i18n::keys::add_book::INFO
    );
    // Every book that has one chapter in tC4's table is one of those five.
    let singles: Vec<BookId> = BookId::ALL
        .into_iter()
        .filter(|b| chapters(*b) == 1)
        .collect();
    assert_eq!(singles.len(), 5);
}

/// tC4 #564: removing rows from a long list removes the rows asked for, whatever the list
/// became meanwhile, and the others keep their verdicts.
#[test]
fn removing_rows_from_25_aligned_books_removes_the_intended_rows() {
    let mut form = AddBookForm::open(&card_with(&[]));
    let ids: Vec<u32> = BookId::ALL
        .into_iter()
        .take(25)
        .enumerate()
        .map(|(n, book)| {
            let name = format!("{}.usfm", book.code());
            let id = form.add_file(name);
            form.file_read(id, read(u8::try_from(n).unwrap(), book));
            id
        })
        .collect();
    for gone in [ids[3], ids[10], ids[24], ids[0]] {
        form.remove_file(gone);
    }
    let rows = form.rows();
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    // The first 25 books are GEN to 1CH; the fourth, eleventh, twenty-fifth and first are gone.
    let all: Vec<String> = BookId::ALL
        .into_iter()
        .take(25)
        .map(|b| format!("{}.usfm", b.code()))
        .collect();
    let expected: Vec<&str> = all
        .iter()
        .enumerate()
        .filter(|(n, _)| ![0, 3, 10, 24].contains(n))
        .map(|(_, name)| name.as_str())
        .collect();
    assert_eq!(names, expected);
    assert!(
        rows.iter()
            .all(|row| matches!(row.state, RowState::Adds(_)))
    );
    assert_eq!(form.addable(), 21);
}

/// A Create's late answer belongs only to the form that sent it: not to a form of the other kind
/// or one opened after a Cancel, and still to its own form after an edit while it waits.
#[test]
fn a_late_create_answer_belongs_only_to_the_form_that_sent_it() {
    let mut form = filled();
    let (sent, _) = form.press(|| rid(1)).unwrap().unwrap();
    assert!(form.awaits(NewKind::Bible, sent));
    assert!(!form.awaits(NewKind::Stories, sent));
    form.edited();
    assert!(
        form.awaits(NewKind::Bible, sent),
        "an edit while it waits keeps the answer its own"
    );

    let mut reopened = filled();
    assert!(
        !reopened.awaits(NewKind::Bible, sent),
        "a form opened after Cancel waits for nothing"
    );
    reopened.press(|| rid(2)).unwrap().unwrap();
    assert!(
        !reopened.awaits(NewKind::Bible, sent),
        "its own press is another attempt"
    );
}

// ---- Import (F1): the form's steps, one state beside the run's attempt ----------------------

mod import {
    use super::rid;
    use tc_api::Refusal;
    use tc_api::ids::UploadId;
    use tc_api::import::{
        CarryOver, FoundBook, FoundVersion, ImportDetails, ImportFile, ImportFound, ImportReview,
        VersionSlot, VersionsView,
    };
    use tc_api::refusal::ImportRefusal;
    use tc_core::BookId;
    use tc_ui::app::forms::{
        ImportFailure, ImportForm, ImportKind, ImportState, ImportStep, ImportUpload, Reviewed,
        VersionLookup,
    };

    fn upload(n: u8) -> UploadId {
        UploadId::parse(&format!("{n:032x}")).unwrap()
    }

    fn details(name: &str, code: &str) -> ImportDetails {
        ImportDetails {
            name: name.into(),
            code: code.into(),
        }
    }

    fn review(refusal: Option<Refusal>) -> ImportReview {
        let found = details("cfm_fbt", "cfm");
        ImportReview {
            found: ImportFound {
                details: found.clone(),
                licenses: vec!["CC BY-SA 4.0".into()],
                contributors: Vec::new(),
                books: vec![FoundBook {
                    book: BookId::Titus,
                    chapters: 3,
                    verses: 46,
                    drafted: 46,
                    aligned: 0,
                    decisions: 5,
                    not_kept: Vec::new(),
                }],
                slots: vec![
                    VersionSlot::TranslationNotes,
                    VersionSlot::OriginalNewTestament,
                ],
                versification: tc_api::import::SchemeGuess {
                    scheme: "org".into(),
                    fit: tc_api::import::SchemeFit::Tied {
                        schemes: vec!["lxx".into(), "org".into()],
                    },
                    names: ["eng", "lxx", "org", "rsc", "rso", "vul"]
                        .map(String::from)
                        .to_vec(),
                },
            },
            details: found,
            folder: None,
            refusal,
            carry_over: None,
        }
    }

    const COUNTED: CarryOver = CarryOver {
        carried: 4,
        invalidated: 1,
    };

    /// "Use installed versions" and the server's count.
    fn installed(form: &mut ImportForm) {
        form.use_installed(rid(70)).expect("the count is asked");
        form.carry_counted(rid(70), Some(COUNTED));
    }

    fn versions(unresolved: Vec<VersionSlot>) -> VersionsView {
        VersionsView {
            found: vec![FoundVersion {
                slot: VersionSlot::TranslationNotes,
                repo_path: "git.door43.org/unfoldingWord/en_tn".into(),
                tag: "v87".into(),
                sha: "a".repeat(40),
            }],
            unresolved,
        }
    }

    /// On the files step with one file sent.
    fn with_file() -> ImportForm {
        let mut form = ImportForm::open(rid(1));
        form.pick_kind(ImportKind::Tc3);
        let id = form.add_file("cfm_fbt_tit_book.zip".into(), 40_000);
        form.file_sent(id, ImportUpload::Sent(upload(1)));
        form
    }

    /// On the review step of `with_file`.
    fn reviewed(refusal: Option<Refusal>) -> ImportForm {
        let mut form = with_file();
        form.press_review().unwrap();
        form.reviewed(review(refusal), rid(1));
        form
    }

    const NONE: [&str; 0] = [];

    #[test]
    fn only_the_tc3_kind_leads_to_the_files_step_and_back_returns_to_the_kind_step() {
        let mut form = ImportForm::open(rid(1));
        form.pick_kind(ImportKind::Usfm);
        form.pick_kind(ImportKind::Burrito);
        assert_eq!(
            form.step(),
            ImportStep::Kind,
            "USFM and Burrito wait for their importers"
        );
        assert_eq!(
            ImportKind::ALL.map(ImportKind::has_importer),
            [true, false, false],
            "the kind step draws USFM and Burrito disabled"
        );
        form.pick_kind(ImportKind::Tc3);
        assert_eq!(form.step(), ImportStep::Files);
        assert_eq!(form.kind, Some(ImportKind::Tc3));
        form.back_to_kind();
        assert_eq!(form.step(), ImportStep::Kind);
    }

    #[test]
    fn review_files_waits_for_every_upload_and_is_off_while_one_failed() {
        let mut form = ImportForm::open(rid(1));
        form.pick_kind(ImportKind::Tc3);
        assert!(!form.can_review(), "no file");
        let a = form.add_file("a.zip".into(), 1);
        let b = form.add_file("b.zip".into(), 2);
        assert_ne!(a, b);
        assert!(!form.can_review(), "both sending");
        form.file_sent(a, ImportUpload::Sent(upload(1)));
        assert!(!form.can_review(), "b still sending");
        form.file_sent(b, ImportUpload::Failed("too big".into()));
        assert!(!form.can_review(), "b could not be sent");
        form.remove_file(b);
        assert!(form.can_review());
        assert_eq!(
            form.sent(),
            [ImportFile {
                upload: upload(1),
                name: "a.zip".into()
            }]
        );
        form.file_sent(b, ImportUpload::Sent(upload(2)));
        assert_eq!(
            form.files.len(),
            1,
            "a removed file's late upload adds no row"
        );
    }

    #[test]
    fn a_review_press_sends_once_and_its_answer_shows_the_files_details() {
        let mut form = with_file();
        let files = form.press_review().expect("the files go");
        assert_eq!(files.len(), 1);
        assert_eq!(form.state, ImportState::Reviewing);
        assert_eq!(form.press_review(), None, "a second press while it reads");
        form.reviewed(review(None), rid(2));
        assert_eq!(form.step(), ImportStep::Review);
        assert_eq!((form.name.as_str(), form.code.as_str()), ("cfm_fbt", "cfm"));
        assert_eq!(form.versions, VersionLookup::NotLookedUp);
        assert_eq!(form.versions.data_state(), "offline");
        assert_eq!(form.state, ImportState::Idle);
    }

    #[test]
    fn a_damaged_file_shows_on_the_review_step_and_import_stays_off() {
        let mut form = with_file();
        form.press_review().unwrap();
        let failure = ImportFailure {
            code: Some("import.damaged.truncated".into()),
            sentence: "x.zip is not a complete zip file.".into(),
            detail: None,
        };
        form.review_refused(failure.clone());
        assert_eq!(form.step(), ImportStep::Review);
        assert_eq!(form.reviewed, Some(Reviewed::Damaged(failure)));
        assert_eq!(form.use_installed(rid(70)), None);
        assert!(!form.can_import(NONE));
        assert_eq!(form.press_run(NONE), None);
    }

    #[test]
    fn a_lost_review_stays_on_the_files_step_and_says_why() {
        let mut form = with_file();
        form.press_review().unwrap();
        form.review_lost("the app did not answer".into());
        assert_eq!(form.step(), ImportStep::Files);
        assert_eq!(
            form.state,
            ImportState::Problem("the app did not answer".into())
        );
        assert!(form.can_review(), "the next press reads again");
    }

    #[test]
    fn a_name_in_use_holds_import_and_another_name_frees_it() {
        let mut form = reviewed(None);
        installed(&mut form);
        assert!(form.can_import(NONE));
        form.type_name("Sample Bible".into(), rid(3));
        assert!(
            form.name_in_use(["sample_bible"]),
            "the folder the name makes"
        );
        assert!(form.name_in_use(["Sample_Bible"]), "without letter case");
        assert!(!form.can_import(["sample_bible"]));
        form.type_name("Tita tC3".into(), rid(4));
        assert!(!form.name_in_use(["sample_bible"]));
        assert!(form.can_import(["sample_bible"]));
        form.type_name("  ".into(), rid(5));
        assert!(!form.can_import(NONE), "no name");
    }

    #[test]
    fn the_reviews_refusal_holds_while_the_details_are_the_ones_it_read() {
        let mut form = reviewed(Some(Refusal::Import(ImportRefusal::NameExists)));
        installed(&mut form);
        assert!(form.name_in_use(NONE), "the server's listing said so");
        assert!(!form.can_import(NONE));
        form.type_name("Another".into(), rid(3));
        assert!(!form.name_in_use(NONE));
        assert!(form.can_import(NONE));

        let code = Refusal::Create(tc_api::refusal::CreateRefusal::CodeNotHeld {
            code: "q!".into(),
            reason: "not a tag".into(),
        });
        let mut form = reviewed(Some(code.clone()));
        installed(&mut form);
        assert_eq!(form.details_refusal(), Some(&code));
        assert!(!form.can_import(NONE));
        form.type_code("  kr ".into(), rid(3));
        assert_eq!(form.code, "kr", "the code is trimmed as tC4 trims it");
        assert_eq!(form.details_refusal(), None);
    }

    #[test]
    fn import_waits_for_the_versions_until_found_or_installed() {
        let mut form = reviewed(None);
        assert!(!form.can_import(NONE), "not looked up");
        assert!(form.look_up(rid(71)));
        assert_eq!(form.versions.data_state(), "looking");
        assert!(!form.look_up(rid(71)), "one lookup at a time");
        form.versions_found(rid(71), versions(vec![VersionSlot::TranslationWords]));
        assert_eq!(form.versions.data_state(), "missing");
        assert!(form.versions.open());
        assert!(!form.can_import(NONE));
        let asked = form.use_installed(rid(70)).expect("the count is asked");
        assert_eq!(asked.use_installed, Some(versions(vec![]).found));
        assert_eq!(form.versions.data_state(), "installed");
        assert!(!form.can_import(NONE), "Import waits for the count");
        form.carry_counted(rid(70), Some(COUNTED));
        assert!(form.can_import(NONE));
        assert_eq!(
            form.versions.found(),
            versions(vec![]).found,
            "the found pins stay"
        );

        let mut form = reviewed(None);
        form.look_up(rid(71));
        form.versions_found(rid(71), versions(vec![]));
        assert_eq!(form.versions.data_state(), "found");
        assert!(form.can_import(NONE));

        let mut form = reviewed(None);
        form.look_up(rid(71));
        form.versions_failed(rid(71), "Door43 did not answer".into());
        assert_eq!(form.versions, VersionLookup::NotLookedUp);
        assert_eq!(
            form.state,
            ImportState::Problem("Door43 did not answer".into())
        );
        form.look_up(rid(71));
        form.versions_not_looked_up();
        assert_eq!(form.versions.data_state(), "offline", "Not now");
    }

    /// A late answer to a lookup or a count of other files is dropped: the review of the files
    /// now chosen waits for its own.
    #[test]
    fn an_answer_to_an_earlier_lookup_or_count_is_dropped() {
        let mut form = reviewed(None);
        assert!(form.look_up(rid(10)));
        form.back_to_files();
        form.press_review().unwrap();
        form.reviewed(review(None), rid(2));
        assert!(form.look_up(rid(11)));
        form.versions_found(rid(10), versions(vec![]));
        form.versions_failed(rid(10), "late".into());
        assert_eq!(
            form.versions,
            VersionLookup::Looking,
            "the first lookup's answer"
        );
        assert_eq!(form.state, ImportState::Idle);
        form.versions_found(rid(11), versions(vec![VersionSlot::TranslationWords]));
        assert_eq!(form.versions.data_state(), "missing");

        form.use_installed(rid(12)).expect("the count is asked");
        form.back_to_files();
        form.press_review().unwrap();
        form.reviewed(review(None), rid(3));
        form.use_installed(rid(13)).expect("the count is asked");
        form.carry_counted(rid(12), Some(COUNTED));
        form.carry_failed(rid(12), "late".into());
        assert!(
            matches!(form.versions, VersionLookup::Installed(_, None)),
            "the first count's answer: {:?}",
            form.versions
        );
        form.carry_counted(rid(13), Some(COUNTED));
        assert!(form.can_import(NONE));
    }

    #[test]
    fn the_run_sends_one_request_per_attempt_with_the_found_versions() {
        let mut form = reviewed(None);
        form.look_up(rid(71));
        form.versions_found(rid(71), versions(vec![]));
        let run = form.press_run(NONE).expect("Import runs");
        assert_eq!(run.request, rid(1));
        assert_eq!(run.details, details("cfm_fbt", "cfm"));
        assert_eq!(run.found, versions(vec![]).found);
        assert_eq!(run.files.len(), 1);
        assert_eq!(form.step(), ImportStep::Working);
        assert_eq!(form.press_run(NONE), None, "a second press while it runs");

        form.run_lost("no answer".into());
        assert_eq!(form.step(), ImportStep::Review);
        assert_eq!(form.state, ImportState::Problem("no answer".into()));
        assert_eq!(
            form.press_run(NONE).unwrap().request,
            rid(1),
            "after a lost reply the same request, which the reply cache answers"
        );

        let failure = ImportFailure {
            code: Some("import.name-exists".into()),
            sentence: "A project with this name is already on this computer.".into(),
            detail: None,
        };
        form.run_refused(failure.clone(), rid(2));
        assert_eq!(form.step(), ImportStep::Failed);
        assert_eq!(form.state, ImportState::Failed(failure));
        assert_eq!(
            form.press_run(NONE).unwrap().request,
            rid(2),
            "Try the import again is a new attempt"
        );
    }

    #[test]
    fn a_run_that_ends_with_nothing_to_say_returns_to_the_review_as_a_new_attempt() {
        let mut form = reviewed(None);
        installed(&mut form);
        form.press_run(NONE).unwrap();
        form.run_started(tc_api::ids::JobId::new(7));
        assert!(matches!(
            form.state,
            ImportState::Running { job: Some(_), .. }
        ));
        form.run_stopped(rid(9));
        assert_eq!(form.step(), ImportStep::Review);
        assert_eq!(form.state, ImportState::Idle);
        assert_eq!(form.press_run(NONE).unwrap().request, rid(9));
    }

    #[test]
    fn an_edit_makes_the_next_import_another_request() {
        let mut form = reviewed(None);
        installed(&mut form);
        form.type_name("Tita".into(), rid(5));
        assert_eq!(form.press_run(NONE).unwrap().request, rid(5));
    }

    /// H13: the review's guess is the default, Import sends the one chosen, and a count of
    /// what the installed versions do, made in the other versification, is asked again.
    #[test]
    fn import_sends_the_versification_the_review_page_shows() {
        let mut form = reviewed(None);
        assert_eq!(form.versification, "org");
        let asked = form.use_installed(rid(70)).unwrap();
        assert_eq!(asked.versification.as_deref(), Some("org"));
        form.carry_counted(rid(70), Some(COUNTED));
        form.pick_versification("eng".into(), rid(6));
        assert_eq!(form.versions, VersionLookup::NotLookedUp, "counted again");
        assert_eq!(form.press_run(NONE), None, "Import waits for the count");
        let asked = form.use_installed(rid(71)).unwrap();
        assert_eq!(asked.versification.as_deref(), Some("eng"));
        form.carry_counted(rid(71), Some(COUNTED));
        let run = form.press_run(NONE).unwrap();
        assert_eq!((run.versification.as_str(), run.request), ("eng", rid(6)));
    }

    /// tC4 `NOT_LOOKED_UP` and `importUseInstalled`: not looked up, every slot the files name
    /// takes the installed versions, and the review is asked what that does to the decisions.
    #[test]
    fn use_installed_before_a_lookup_moves_every_named_slot_and_asks_for_the_count() {
        let mut form = reviewed(None);
        let asked = form.use_installed(rid(70)).expect("the count is asked");
        assert_eq!(asked.use_installed, Some(Vec::new()));
        assert_eq!(asked.details, Some(details("cfm_fbt", "cfm")));
        assert_eq!(asked.files.len(), 1);
        assert_eq!(
            form.versions,
            VersionLookup::Installed(
                VersionsView {
                    found: Vec::new(),
                    unresolved: vec![
                        VersionSlot::TranslationNotes,
                        VersionSlot::OriginalNewTestament
                    ],
                },
                None
            )
        );
        assert_eq!(form.use_installed(rid(70)), None, "one count at a time");
        form.carry_failed(rid(70), "no answer".into());
        assert_eq!(form.versions, VersionLookup::NotLookedUp, "offered again");
        assert_eq!(form.state, ImportState::Problem("no answer".into()));
        installed(&mut form);
        assert!(matches!(
            form.versions,
            VersionLookup::Installed(_, Some(COUNTED))
        ));
        assert!(form.can_import(NONE));
    }

    /// A review whose files name no version has nothing to look up (tC4 `unresolvedSlots`
    /// empty: the row is found and Import needs no choice).
    #[test]
    fn a_review_that_names_no_version_needs_no_choice() {
        let mut form = with_file();
        form.press_review().unwrap();
        let mut none = review(None);
        none.found.slots.clear();
        form.reviewed(none, rid(1));
        assert_eq!(form.versions.data_state(), "found");
        assert!(form.can_import(NONE));
    }

    /// tC4 j09 :345: where the files differ on the license, Import waits for the pick, and
    /// the run states the one picked; one license is stated as it is.
    #[test]
    fn differing_licenses_wait_for_a_pick_and_the_run_states_it() {
        let mut form = reviewed(None);
        installed(&mut form);
        assert_eq!(form.license.as_deref(), Some("CC BY-SA 4.0"));
        assert_eq!(form.press_run(NONE).unwrap().license, "CC BY-SA 4.0");

        let mut form = with_file();
        form.press_review().unwrap();
        let mut two = review(None);
        two.found.licenses = vec!["CC BY-SA 4.0".into(), "CC0 1.0 Public Domain".into()];
        form.reviewed(two, rid(1));
        installed(&mut form);
        assert_eq!(form.license, None);
        assert!(!form.can_import(NONE), "Import waits for the choice");
        form.pick_license(Some("CC0 1.0 Public Domain".into()), rid(2));
        let run = form.press_run(NONE).expect("Import runs");
        assert_eq!(run.license, "CC0 1.0 Public Domain");
        assert_eq!(run.request, rid(2), "a pick is an edit");

        let mut form = with_file();
        form.press_review().unwrap();
        let mut unnamed = review(None);
        unnamed.found.licenses = vec!["CC BY-SA 4.0".into(), String::new()];
        form.reviewed(unnamed, rid(1));
        installed(&mut form);
        form.pick_license(Some(String::new()), rid(2));
        assert_eq!(form.press_run(NONE).unwrap().license, "", "No license");
    }

    #[test]
    fn choosing_other_files_drops_the_review() {
        let mut form = reviewed(None);
        form.back_to_files();
        assert_eq!(form.step(), ImportStep::Files);
        assert_eq!(form.reviewed, None);
        assert_eq!(form.files.len(), 1, "the files stay chosen");
    }
}

// ---- Update a book from USFM (F1 step 8) -----------------------------------------------------

mod update_book {
    use super::rid;
    use tc_api::ids::{Epoch, ProjectRef, Scope, UploadId};
    use tc_api::import::{UpdateReport, VerseAt};
    use tc_core::BookId;
    use tc_ui::app::forms::{ImportFailure, ImportUpload};
    use tc_ui::app::update_book::{UpdateBookForm, UpdateState, UpdateStep, verse_list};

    fn at() -> Scope {
        Scope {
            project: ProjectRef::parse("sample_bible").unwrap(),
            epoch: Epoch::new(1),
        }
    }

    fn on_book_step() -> UpdateBookForm {
        let mut form = UpdateBookForm::open(at(), BookId::Titus, rid(1));
        let id = form.choose_file("TIT.usfm".into(), 9_000, rid(1));
        form.file_sent(
            id,
            ImportUpload::Sent(UploadId::parse(&"1".repeat(32)).unwrap()),
        );
        form.to_book();
        form
    }

    fn report() -> UpdateReport {
        UpdateReport {
            book: BookId::Titus,
            imported: 12,
            to_review: 2,
            left_as_it_was: Vec::new(),
            markup_kept: Vec::new(),
            book_added: false,
        }
    }

    #[test]
    fn the_book_step_waits_for_the_file_to_be_sent() {
        let mut form = UpdateBookForm::open(at(), BookId::Titus, rid(1));
        form.to_book();
        assert_eq!(form.step(), UpdateStep::File, "no file");
        let first = form.choose_file("a.usfm".into(), 1, rid(1));
        form.to_book();
        assert_eq!(form.step(), UpdateStep::File, "still sending");
        let second = form.choose_file("b.usfm".into(), 1, rid(1));
        form.file_sent(
            first,
            ImportUpload::Sent(UploadId::parse(&"1".repeat(32)).unwrap()),
        );
        assert!(
            !form.can_choose_book(),
            "the replaced file's upload changes nothing"
        );
        form.file_sent(second, ImportUpload::Failed("too big".into()));
        assert!(!form.can_choose_book());
        assert_eq!(on_book_step().step(), UpdateStep::Book);
    }

    #[test]
    fn the_run_sends_the_chosen_book_once_and_its_end_shows_the_report() {
        let mut form = on_book_step();
        form.choose_book(BookId::Philemon, rid(2));
        let run = form.press_run().expect("the update runs");
        assert_eq!((run.request, run.book), (rid(2), BookId::Philemon));
        assert_eq!(run.at, at());
        assert_eq!(form.step(), UpdateStep::Working);
        assert_eq!(form.press_run(), None, "a second press while it runs");
        form.run_done(report());
        assert_eq!(form.step(), UpdateStep::Report);
        assert_eq!(form.state, UpdateState::Done(report()));
        assert_eq!(
            form.press_run(),
            None,
            "no second run from the report, and no Undo"
        );
    }

    #[test]
    fn a_refusal_shows_its_sentence_and_the_file_is_chosen_again() {
        let mut form = on_book_step();
        form.press_run().unwrap();
        let failure = ImportFailure {
            code: None,
            sentence: "TIT.usfm cannot be read at line 7, so the book was not updated.".into(),
            detail: Some("an unclosed \\w".into()),
        };
        form.run_refused(failure.clone(), rid(3));
        assert_eq!(form.step(), UpdateStep::Failed);
        assert_eq!(form.state, UpdateState::Failed(failure));
        form.back_to_file();
        assert_eq!(form.step(), UpdateStep::File);
        assert!(
            !form.can_choose_book(),
            "the refused file is not offered again"
        );
    }

    #[test]
    fn a_lost_reply_sends_the_same_request_again() {
        let mut form = on_book_step();
        form.press_run().unwrap();
        form.run_lost("no answer".into());
        assert_eq!(form.step(), UpdateStep::Book);
        assert_eq!(form.press_run().unwrap().request, rid(1));
        form.run_stopped(rid(4));
        assert_eq!(form.press_run(), None);
    }

    /// A cancelled run's job deleted the upload: the person chooses the file again, and the
    /// next run sends the new upload.
    #[test]
    fn after_a_cancel_the_file_is_chosen_again() {
        let mut form = on_book_step();
        form.press_run().unwrap();
        form.run_stopped(rid(4));
        assert_eq!(form.step(), UpdateStep::File);
        assert!(!form.can_choose_book(), "the deleted upload is not offered");
        assert_eq!(form.press_run(), None);
        let id = form.choose_file("TIT.usfm".into(), 9_000, rid(5));
        let upload = UploadId::parse(&"2".repeat(32)).unwrap();
        form.file_sent(id, ImportUpload::Sent(upload.clone()));
        form.to_book();
        let run = form.press_run().unwrap();
        assert_eq!((run.request, run.file.upload), (rid(5), upload));
    }

    #[test]
    fn the_report_lists_verses_in_order() {
        let at = |chapter, verse| VerseAt { chapter, verse };
        assert_eq!(
            verse_list(&[at(2, 4), at(1, 3), at(1, 10)]),
            "1:3, 1:10, 2:4"
        );
        assert_eq!(verse_list(&[]), "");
    }
}
