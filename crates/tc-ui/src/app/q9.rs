//! The screens of the H5 packet (unit C7b step 2, Q-9): every review screen tC4 has no design
//! for, as fixed data, so the owner signs off the words and the choices before any of them is
//! wired. The test build's probe page draws one with `?probe=q9&screen=<name>`, and
//! `parity/app/q9-packet.mjs` photographs each. The choices' words are STORAGE §8.3's and
//! §8.5's, in English: they become catalog keys when the signed-off screens are wired (steps 3
//! to 6), so a word the owner changes is changed once.

use tc_api::conflicts::{NoticeView, ReviewTab, TabCount};
use tc_api::write::ReadOnlyReason;

use crate::app::review::{Owner, notice_text};
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
    Conflict(Q9Conflict),
    /// The Detached and Notices tabs: a list, each row with its own buttons.
    Rows {
        about: Option<String>,
        rows: Vec<Q9Row>,
    },
}

/// One conflict with its versions and its footer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Q9Conflict {
    pub place: String,
    pub kind: String,
    /// What happened, in words, where the versions alone do not say it.
    pub about: Option<String>,
    /// Buttons over every conflict of the tab.
    pub bulk: Vec<String>,
    pub versions: Vec<Q9Version>,
    pub hint: String,
    pub footer: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Q9Version {
    pub owner: Owner,
    pub who: String,
    /// Milliseconds since 1970, drawn as the review draws a version's date.
    pub at: i64,
    pub lines: Vec<String>,
    pub choices: Vec<String>,
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
pub const SCREENS: [&str; 17] = [
    "alignments",
    "checks",
    "checks-moved",
    "checks-comment",
    "settings-pins",
    "settings-setting",
    "settings-books",
    "settings-book-removed",
    "settings-last-book",
    "settings-carried",
    "settings-path-case",
    "detached",
    "notices",
    "received",
    "draft-offer",
    "read-only",
    "boot-error",
];

/// 2026-10-20 and the days after it, at 10:00 UTC.
const DAY0: i64 = 1_792_490_400_000;
const DAY: i64 = 24 * 60 * 60 * 1000;

fn s(text: &str) -> String {
    text.to_owned()
}

fn tabs() -> Vec<TabCount> {
    [
        (ReviewTab::Text, 0),
        (ReviewTab::Alignments, 1),
        (ReviewTab::Checks, 3),
        (ReviewTab::Settings, 7),
        (ReviewTab::Detached, 3),
        (ReviewTab::Notices, 3),
    ]
    .into_iter()
    .map(|(tab, count)| TabCount { tab, count })
    .collect()
}

fn version(owner: Owner, who: &str, day: i64, lines: &[&str], choices: &[&str]) -> Q9Version {
    Q9Version {
        owner,
        who: s(who),
        at: DAY0 + day * DAY,
        lines: lines.iter().map(|l| s(l)).collect(),
        choices: choices.iter().map(|c| s(c)).collect(),
    }
}

fn conflict(tab: ReviewTab, conflict: Q9Conflict) -> Q9Screen {
    Q9Screen::Review(Q9Review {
        tabs: tabs(),
        tab,
        body: Q9Body::Conflict(conflict),
    })
}

/// The screen `name` draws, or `None` for a name the packet does not have.
pub fn screen(name: &str) -> Option<Q9Screen> {
    use Owner::{Mine, Teammate};
    let screen = match name {
        "alignments" => conflict(
            ReviewTab::Alignments,
            Q9Conflict {
                place: s("Titus 1:1"),
                kind: s("Alignment · 2 versions"),
                about: None,
                bulk: vec![s("Dismiss alignments made for text that was not kept")],
                versions: vec![
                    version(
                        Teammate,
                        "Ana",
                        1,
                        &[
                            "Made for the current text of Titus 1:1.",
                            "14 of 14 words aligned · marked done",
                        ],
                        &[
                            "Use this alignment",
                            "Open in the aligner starting from this one",
                        ],
                    ),
                    version(
                        Mine,
                        "Mine",
                        0,
                        &["Made for text that was not kept.", "11 of 14 words aligned"],
                        &[
                            "Use this alignment",
                            "Open in the aligner starting from this one",
                        ],
                    ),
                ],
                hint: s("Use one alignment, or open one in the aligner and save it there."),
                footer: vec![],
            },
        ),
        "checks" => conflict(
            ReviewTab::Checks,
            Q9Conflict {
                place: s("Titus 1:1 · Translation Notes · “the chosen people of God”"),
                kind: s("Check · 2 versions"),
                about: None,
                bulk: vec![],
                versions: vec![
                    version(
                        Teammate,
                        "Ana",
                        1,
                        &[
                            "Selected: “escogidos de Dios”",
                            "Comment: “Use ‘escogidos’.”",
                        ],
                        &[],
                    ),
                    version(
                        Mine,
                        "Mine",
                        0,
                        &["Selected: “elegidos de Dios”", "Bookmarked"],
                        &[],
                    ),
                ],
                hint: s("The selection differs. The bookmark and the comment are kept from both."),
                footer: vec![s("Choose per line"), s("Keep Ana’s"), s("Keep mine")],
            },
        ),
        "checks-moved" => conflict(
            ReviewTab::Checks,
            Q9Conflict {
                place: s("Titus 1:2 · Translation Notes · “before all the ages of time”"),
                kind: s("A check that moved to a new note · 2 versions"),
                about: Some(s(
                    "Ana’s upgrade moved this check to the new note; Pedro changed the old one \
                     (status Invalid, comment “Revisar el tiempo”).",
                )),
                bulk: vec![],
                versions: vec![
                    version(
                        Teammate,
                        "Pedro",
                        2,
                        &["Old note: Invalid", "Comment: “Revisar el tiempo”"],
                        &[],
                    ),
                    version(
                        Teammate,
                        "Ana",
                        1,
                        &["Moved to the new note of Translation Notes v86"],
                        &[],
                    ),
                ],
                hint: s("Pedro’s change can go to the moved check, or stay where it was."),
                footer: vec![
                    s("Keep Pedro’s check where it was"),
                    s("Apply Pedro’s change to the moved check"),
                ],
            },
        ),
        "checks-comment" => conflict(
            ReviewTab::Checks,
            Q9Conflict {
                place: s("Titus 1:1 · Comment"),
                kind: s("Comment · 2 versions"),
                about: None,
                bulk: vec![],
                versions: vec![
                    version(
                        Teammate,
                        "Juan",
                        1,
                        &["“Preguntar al pastor por ‘siervo’.”"],
                        &["Keep this one"],
                    ),
                    version(
                        Mine,
                        "Mine",
                        0,
                        &["“Revisar ‘siervo’ con el equipo.”"],
                        &["Keep this one"],
                    ),
                ],
                hint: s("Keep both joins the two texts, each with its author’s name."),
                footer: vec![s("Keep both")],
            },
        ),
        "settings-pins" => conflict(
            ReviewTab::Settings,
            Q9Conflict {
                place: s("Source text versions · Translation Notes"),
                kind: s("Source text versions · 2 versions"),
                about: None,
                bulk: vec![],
                versions: vec![
                    version(
                        Teammate,
                        "Juan",
                        1,
                        &[
                            "es-419 Translation Notes v93",
                            "With v93: 1,180 checks valid, 61 to check again.",
                            "Not on this computer yet: Use v93 downloads it first.",
                        ],
                        &["Use v93"],
                    ),
                    version(
                        Mine,
                        "Mine",
                        0,
                        &[
                            "es-419 Translation Notes v92",
                            "With v92: 1,204 checks valid, 37 to check again.",
                        ],
                        &["Use v92"],
                    ),
                ],
                hint: s(
                    "The counts are worked out before anything changes. Nothing moves until you choose.",
                ),
                footer: vec![],
            },
        ),
        "settings-setting" => conflict(
            ReviewTab::Settings,
            Q9Conflict {
                place: s("Project setting · Script font"),
                kind: s("Project setting · 2 versions"),
                about: None,
                bulk: vec![],
                versions: vec![
                    version(Teammate, "Ana", 1, &["Charis SIL"], &["Keep this one"]),
                    version(Mine, "Mine", 0, &["Noto Sans"], &["Keep this one"]),
                ],
                hint: s("Keep one, or edit the setting with both beside it."),
                footer: vec![s("Edit")],
            },
        ),
        "settings-books" => conflict(
            ReviewTab::Settings,
            Q9Conflict {
                place: s("Jonah"),
                kind: s("Book in the project · 2 versions"),
                about: Some(s(
                    "Juan restored Jonah’s text; Maria brought Jonah back empty.",
                )),
                bulk: vec![],
                versions: vec![
                    version(Teammate, "Juan", 1, &["Restored with its text"], &[]),
                    version(Teammate, "Maria", 1, &["Started empty"], &[]),
                ],
                hint: s("Either way, every version of Jonah’s verses stays in History."),
                footer: vec![s("Start empty"), s("Restore the text")],
            },
        ),
        "settings-book-removed" => conflict(
            ReviewTab::Settings,
            Q9Conflict {
                place: s("Jonah"),
                kind: s("Book removed while someone edited it · 2 versions"),
                about: Some(s("Maria removed Jonah while Juan changed 4 of its verses.")),
                bulk: vec![],
                versions: vec![
                    version(
                        Teammate,
                        "Juan",
                        2,
                        &["Changed Jonah 1:1, 1:2, 2:10 and 3:4"],
                        &[],
                    ),
                    version(Teammate, "Maria", 1, &["Removed Jonah"], &[]),
                ],
                hint: s("Keep it removed keeps Juan’s changes in History."),
                footer: vec![
                    s("Show Juan’s changes"),
                    s("Keep it removed"),
                    s("Restore Jonah"),
                ],
            },
        ),
        "settings-last-book" => conflict(
            ReviewTab::Settings,
            Q9Conflict {
                place: s("Titus"),
                kind: s("Last book removed · 2 versions"),
                about: Some(s("Juan removed Titus, the project’s last book.")),
                bulk: vec![],
                versions: vec![
                    version(Teammate, "Juan", 1, &["Removed Titus"], &[]),
                    version(Mine, "Mine", 0, &["Titus is in the project"], &[]),
                ],
                hint: s("A project keeps at least one book."),
                footer: vec![
                    s("Add another book first, then remove Titus"),
                    s("Keep Titus"),
                ],
            },
        ),
        "settings-carried" => conflict(
            ReviewTab::Settings,
            Q9Conflict {
                place: s("ingredients/audio/jon_01.wav"),
                kind: s("File · 2 versions"),
                about: Some(s("Juan and you each replaced version 1 of this file.")),
                bulk: vec![],
                versions: vec![
                    version(
                        Teammate,
                        "Juan",
                        1,
                        &["Version 2 · 1.8 MB"],
                        &["Keep this one"],
                    ),
                    version(Mine, "Mine", 0, &["Version 2 · 2.1 MB"], &["Keep this one"]),
                ],
                hint: s("Keep both saves Juan’s version as jon_01-Juan.wav."),
                footer: vec![s("Keep both")],
            },
        ),
        "settings-path-case" => conflict(
            ReviewTab::Settings,
            Q9Conflict {
                place: s("ingredients/audio/jon_02.wav"),
                kind: s("Two file names that differ only in capital letters · 2 versions"),
                about: Some(s("Windows and macOS cannot keep both names in one folder.")),
                bulk: vec![],
                versions: vec![
                    version(
                        Teammate,
                        "Ana",
                        1,
                        &["ingredients/audio/JON_02.wav · 1.2 MB"],
                        &["Rename", "Remove"],
                    ),
                    version(
                        Mine,
                        "Mine",
                        0,
                        &["ingredients/audio/jon_02.wav · 1.4 MB"],
                        &["Rename", "Remove"],
                    ),
                ],
                hint: s("Rename asks for a new name the project does not hold yet."),
                footer: vec![],
            },
        ),
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
