//! The screens of the H5 packet (unit C7b step 2, Q-9) that the owner's ruling kept: the review
//! screens tC4 has no design for, as fixed data (ruling H5 dropped the conflict cards of every
//! kind outside verse text, whose newest version now wins). The test build's probe page draws one with `?probe=q9&screen=<name>`, and
//! `parity/app/q9-packet.mjs` photographs each. The choices' words are STORAGE §8.3's and
//! §8.5's, in English: they become catalog keys when the signed-off screens are wired (steps 3
//! to 6), so a word the owner changes is changed once.

use tc_api::conflicts::{NoticeView, ReviewTab, TabCount};
use tc_api::write::ReadOnlyReason;

use crate::app::review::notice_text;
use crate::i18n::{keys, t};

/// One drawn screen of the packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Q9Screen {
    Review(Q9Review),
    /// The sheet after a sync or a file of work (STORAGE §8.5).
    Received(Q9Question),
    DraftOffer(Q9Question),
    ReadOnly(Q9Question),
    /// The boot error's reason.
    BootError(String),
}

/// The review on one tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Q9Review {
    pub tabs: Vec<TabCount>,
    pub tab: ReviewTab,
    pub body: Q9Body,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Q9Body {
    /// The Detached and Notices tabs: a list, each row with its own buttons.
    Rows {
        about: Option<String>,
        rows: Vec<Q9Row>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Q9Row {
    pub head: String,
    pub line: String,
    pub choices: Vec<String>,
}

/// A sentence, what it lists, and its buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Q9Question {
    pub sentence: String,
    pub lines: Vec<String>,
    pub choices: Vec<String>,
}

/// Every screen of the packet, in the packet's order.
pub const SCREENS: [&str; 6] = [
    "detached",
    "notices",
    "received",
    "draft-offer",
    "read-only",
    "boot-error",
];

fn s(text: &str) -> String {
    text.to_owned()
}

fn tabs() -> Vec<TabCount> {
    [
        (ReviewTab::Text, 0),
        (ReviewTab::Detached, 3),
        (ReviewTab::Notices, 3),
    ]
    .into_iter()
    .map(|(tab, count)| TabCount { tab, count })
    .collect()
}

/// The screen `name` draws, or `None` for a name the packet does not have.
pub fn screen(name: &str) -> Option<Q9Screen> {
    let screen = match name {
        "detached" => Q9Screen::Review(Q9Review {
            tabs: tabs(),
            tab: ReviewTab::Detached,
            body: Q9Body::Rows {
                about: Some(t(keys::review::detached::ABOUT, &[])),
                rows: vec![
                    Q9Row {
                        head: s("Titus 1:9"),
                        line: t(keys::review::detached::DECISION, &[]),
                        choices: vec![s("Show"), s("Dismiss")],
                    },
                    Q9Row {
                        head: s("Titus 3:16"),
                        line: t(keys::review::detached::ALIGNMENT, &[]),
                        choices: vec![s("Open in the aligner at Titus 3:15"), s("Dismiss")],
                    },
                    Q9Row {
                        head: s("Titus 1:17"),
                        line: t(keys::review::detached::COMMENT, &[]),
                        choices: vec![s("Show"), s("Dismiss")],
                    },
                ],
            },
        }),
        "notices" => Q9Screen::Review(Q9Review {
            tabs: tabs(),
            tab: ReviewTab::Notices,
            body: Q9Body::Rows {
                about: None,
                rows: vec![
                    Q9Row {
                        head: String::new(),
                        line: notice_text(&NoticeView::IncompleteChanges { have: 212, of: 230 }),
                        choices: vec![s("Apply what was recovered"), s("Dismiss")],
                    },
                    Q9Row {
                        head: String::new(),
                        line: notice_text(&NoticeView::DamagedChanges { recovered: 212 }),
                        choices: vec![s("Dismiss")],
                    },
                    Q9Row {
                        head: String::new(),
                        line: notice_text(&NoticeView::FileMissing {
                            file: s("ingredients/audio/jon_01.wav"),
                        }),
                        choices: vec![s("Dismiss")],
                    },
                ],
            },
        }),
        "received" => Q9Screen::Received(Q9Question {
            sentence: s(
                "Received 212 changes from Juan and Ana. 3 verses, 1 alignment and 2 checks \
                 were changed by more than one person.",
            ),
            lines: vec![notice_text(&NoticeView::IncompleteChanges {
                have: 212,
                of: 230,
            })],
            choices: vec![s("Later"), s("Review now")],
        }),
        "draft-offer" => Q9Screen::DraftOffer(Q9Question {
            sentence: t(keys::drafts::offer::FOUND, &[("place", &"Titus 1:4")]),
            lines: vec![s("Por esta causa te dejé en Creta, para que corrigieses…")],
            choices: vec![
                t(keys::drafts::offer::DISCARD, &[]),
                t(keys::drafts::offer::KEEP, &[]),
            ],
        }),
        "read-only" => {
            let banner = crate::app::offer::banner(ReadOnlyReason::OpenOnAnotherComputer);
            Q9Screen::ReadOnly(Q9Question {
                sentence: t(banner.sentence, &[]),
                lines: vec![],
                choices: banner
                    .actions
                    .into_iter()
                    .map(|action| t(crate::app::offer::action_words(action), &[]))
                    .collect(),
            })
        }
        "boot-error" => Q9Screen::BootError(crate::app::boot::stop_reason(
            "4a1c2e9 (API 1)",
            "77e5b11 (API 2)",
        )),
        _ => return None,
    };
    Some(screen)
}
