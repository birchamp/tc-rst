//! D1: the question about books another program changed, in the ruling's words (owner ruling
//! OE 3, `docs/STORAGE-DELTA-2026-10-21.md` §2.11).

use tc_api::conflicts::{ReviewTab, TabCount};
use tc_api::session::{KeptCopy, KeptImported, KeptWhy, OutsideEdits, WaitingBook};
use tc_core::BookId;
use tc_ui::app::outside::{can_dismiss, can_import, kept_line, question, report_line, with_kept};

fn waiting(file: &str, book: Option<BookId>, added: bool) -> WaitingBook {
    WaitingBook {
        file: file.into(),
        book,
        added,
        copy: format!(".tc/external/{file}"),
        unreadable: None,
    }
}

#[test]
fn the_question_has_the_rulings_words() {
    let titus = waiting("ingredients/TIT.usfm", Some(BookId::Titus), false);
    let ruth = waiting("ingredients/RUT.usfm", Some(BookId::Ruth), true);
    assert_eq!(question(&[]), None);
    assert_eq!(
        question(std::slice::from_ref(&titus)).as_deref(),
        Some(
            "This book has been changed externally. Would you like to import it into this project?"
        )
    );
    assert_eq!(
        question(&[titus.clone(), ruth.clone()]).as_deref(),
        Some(
            "These books have been changed externally: Titus, Ruth. Would you like to import them into this project?"
        )
    );
    assert_eq!(
        question(std::slice::from_ref(&ruth)).as_deref(),
        Some(
            "A book file RUT.usfm was added outside translationCore. Would you like to import it into this project?"
        )
    );
}

#[test]
fn the_import_report_names_the_book_and_its_counts() {
    let report = KeptImported {
        file: "ingredients/TIT.usfm".into(),
        book: Some(BookId::Titus),
        imported: 12,
        to_review: 2,
        book_added: false,
        outside: OutsideEdits::default(),
    };
    assert_eq!(
        report_line(&report),
        "Titus: 12 verses imported, 2 to review."
    );
}

/// A book that cannot be read offers only *Not now*; a copy is dismissed only once its book
/// is answered; the Notices tab counts the kept copies.
#[test]
fn the_choices_follow_the_store() {
    let mut broken = waiting("ingredients/TIT.usfm", Some(BookId::Titus), false);
    broken.unreadable = Some("TIT 1:3 appears twice (line 212)".into());
    assert!(!can_import(std::slice::from_ref(&broken)));
    let kept = KeptCopy {
        file: "ingredients/TIT.usfm".into(),
        copy: ".tc/external/x".into(),
        why: KeptWhy::Changed,
    };
    assert!(!can_dismiss(&kept, std::slice::from_ref(&broken)));
    assert!(can_dismiss(&kept, &[]));
    assert_eq!(
        kept_line(&kept),
        "Titus was changed outside translationCore. A copy is kept."
    );
    let tabs = [
        TabCount {
            tab: ReviewTab::Text,
            count: 1,
        },
        TabCount {
            tab: ReviewTab::Notices,
            count: 0,
        },
    ];
    assert_eq!(with_kept(&tabs, 2)[1].count, 2);
    assert_eq!(with_kept(&tabs, 2)[0].count, 1);
}
