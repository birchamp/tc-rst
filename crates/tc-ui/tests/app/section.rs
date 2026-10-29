//! The section card's decisions (unit B4b): what Place says while a number is in hand (#604),
//! why a word does or does not take it, and when Save section is on. The words come from the
//! catalog, as the card reads them.

use tc_ui::app::client::{CallError, Transport};
use tc_ui::app::section::{DiscardError, Hints, can_save, hints, word_title};

#[test]
fn section_card_reads_the_idle_place_hint() {
    // A number waits in the bank and none is in hand: the idle instruction, nothing else.
    assert_eq!(
        hints(None, 1, 12),
        Hints {
            pick: true,
            drop: None,
            bank_empty: false,
        }
    );
    // Picked up: the drop instruction names the number, the idle one is hidden.
    assert_eq!(
        hints(Some("10"), 0, 12),
        Hints {
            pick: false,
            drop: Some("10".into()),
            bank_empty: true,
        }
    );
    // Every verse placed and none in hand: "Every verse is placed", no instruction.
    assert_eq!(
        hints(None, 0, 12),
        Hints {
            pick: false,
            drop: None,
            bank_empty: true,
        }
    );
    // No words yet: nothing to place a number in, so neither sentence is offered.
    assert_eq!(
        hints(None, 2, 0),
        Hints {
            pick: true,
            drop: None,
            bank_empty: false,
        }
    );
}

#[test]
fn a_word_says_why_it_takes_a_number_or_not() {
    let title = |held, at, ok| word_title(held, at, ok, "mostrando");
    assert_eq!(
        title(None, None, true),
        "",
        "nothing while no number is in hand"
    );
    assert_eq!(title(Some("10"), None, true), "Begin verse 10 at mostrando");
    assert_eq!(
        title(Some("10"), Some("9"), true),
        "Join verse 10 to verse 9 at mostrando"
    );
    assert_eq!(
        title(Some("10"), Some("11"), false),
        "Verse 11 already begins here"
    );
    assert_eq!(
        title(Some("10"), None, false),
        "Verse 10 cannot pass another verse number"
    );
}

#[test]
fn a_section_with_no_draft_has_nothing_to_save_and_an_emptied_one_may_be_saved() {
    assert!(can_save(false, false), "words typed");
    assert!(can_save(false, true));
    assert!(
        can_save(true, true),
        "every word deleted from a drafted section asks first"
    );
    assert!(
        !can_save(true, false),
        "an empty card over undrafted verses has nothing to save"
    );
}

#[test]
fn a_cancel_that_cannot_discard_says_why_from_the_catalog() {
    // Each reason is a catalog sentence, so the card says it in the shown language.
    assert_eq!(
        DiscardError::Restarted.message(),
        "The app restarted; the draft is kept for the next open."
    );
    assert_eq!(
        DiscardError::NotOpen.message(),
        "The project is no longer open; the draft is kept."
    );
    let offline = CallError::Transport(Transport::Unreachable("offline".into()));
    assert_eq!(
        DiscardError::Call(offline.clone()).message(),
        offline.to_string()
    );
}
