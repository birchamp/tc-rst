//! D1: the question about books another program changed, in the ruling's words (owner ruling
//! OE 3, `docs/STORAGE-DELTA-2026-10-21.md` §2.11).

use tc_api::conflicts::{ReviewTab, TabCount};
use tc_api::event::SessionMode;
use tc_api::session::{KeptCopy, KeptImported, KeptWhy, OutsideEdits, WaitingBook};
use tc_api::write::ReadOnlyReason;
use tc_core::BookId;
use tc_ui::app::outside::{
    KeptOffer, asked_now, can_import, import_label, kept_import_label, kept_line, kept_offer,
    question, report_line, review_entry, with_kept,
};

fn waiting(file: &str, book: Option<BookId>, added: bool) -> WaitingBook {
    WaitingBook {
        file: file.into(),
        book,
        added,
        copy: format!(".tc/external/{file}"),
        unreadable: None,
        why: KeptWhy::Changed,
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
    let one = KeptImported {
        imported: 1,
        ..report
    };
    assert_eq!(report_line(&one), "Titus: 1 verse imported, 2 to review.");
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
    let writable = SessionMode::Writable;
    assert_eq!(
        kept_offer(&kept, std::slice::from_ref(&broken), &writable),
        KeptOffer::Import
    );
    assert_eq!(
        kept_offer(&kept, &[], &writable),
        KeptOffer::ImportOrDismiss
    );
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

/// INV-A-KEPT-COPY-STAYS-ON-A-SCREEN: a kept copy alone shows the way into the review, at its
/// notices; with verse conflicts it opens at them and counts both.
#[test]
fn a_kept_copy_alone_shows_the_way_into_the_review() {
    assert_eq!(review_entry(0, 0), None);
    assert_eq!(review_entry(0, 1), Some((1, ReviewTab::Notices)));
    assert_eq!(review_entry(2, 1), Some((3, ReviewTab::Text)));
    assert_eq!(review_entry(2, 0), Some((2, ReviewTab::Text)));
}

/// A read-only project's kept copies show their line and no *Import* or *Dismiss*, which the
/// store would refuse.
#[test]
fn a_read_only_project_offers_nothing_on_its_kept_copies() {
    let kept = KeptCopy {
        file: "ingredients/TIT.usfm".into(),
        copy: ".tc/external/x".into(),
        why: KeptWhy::Changed,
    };
    let read_only = SessionMode::ReadOnly(ReadOnlyReason::SecondWriter);
    assert_eq!(kept_offer(&kept, &[], &read_only), KeptOffer::Nothing);
}

/// H22 part 1: a copy of lost work asks in the ruling's words, with *Bring it back* (the same
/// import) and no *Dismiss*: its copy may be the work's only one.
#[test]
fn a_lost_work_copy_asks_bring_it_back() {
    let lost = KeptCopy {
        file: "ingredients/TIT.usfm".into(),
        copy: ".tc/external/x".into(),
        why: KeptWhy::Lost,
    };
    assert_eq!(
        kept_line(&lost),
        "Some recent work was missing from this project's records. Bring it back?"
    );
    assert_eq!(kept_import_label(&lost), "Bring it back");
    assert_eq!(
        kept_offer(&lost, &[], &SessionMode::Writable),
        KeptOffer::Import
    );
}

/// H22 part 1: lost work that waits is asked once, in the ruling's words, with *Bring it back*
/// and *Not now*; with changed books waiting too, it is asked first and on its own, and the
/// books' question follows.
#[test]
fn waiting_lost_work_is_asked_with_bring_it_back_before_changed_books() {
    let mut lost = waiting("ingredients/TIT.usfm", Some(BookId::Titus), false);
    lost.why = KeptWhy::Lost;
    let ruth = waiting("ingredients/RUT.usfm", Some(BookId::Ruth), false);
    let both = [ruth.clone(), lost.clone()];
    let now = asked_now(&both);
    assert_eq!(now, vec![lost.clone()]);
    assert_eq!(
        question(&now).as_deref(),
        Some("Some recent work was missing from this project's records. Bring it back?")
    );
    assert_eq!(import_label(&now), "Bring it back");
    let after = asked_now(std::slice::from_ref(&ruth));
    assert_eq!(after, vec![ruth.clone()]);
    assert_eq!(
        question(&after).as_deref(),
        Some(
            "This book has been changed externally. Would you like to import it into this project?"
        )
    );
    assert_eq!(import_label(&after), "Import");
}
