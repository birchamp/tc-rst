//! The result of one "Sync with Door43" on Home (X10, unit B10): tC4's `SyncResult.jsx` as
//! the decisions its view draws. An ok sync shows its three counts, who sent the changes, its
//! notes and its links; a sync that did not complete shows the one sentence of its refusal
//! (tC4 #678), never Door43's words or a path (Q36).

use tc_api::ApiError;
use tc_api::conflicts::ReviewPlace;
use tc_api::ids::ProjectRef;
use tc_api::jobs::{ReplacedKind, ReplacedView, SyncResultView};
use tc_api::refusal::{Message, Refusal};
use tc_api::shown::{Computer, PersonLabel, Who};

use crate::i18n::{Key, keys, t, t_dyn};

/// How a sync of `project` ended: tC4's `syncResult`. Never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncToast {
    pub project: ProjectRef,
    pub end: SyncEnd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncEnd {
    Synced(SyncResultView),
    /// Any error but the internet question and the sign-in, which the flow answers itself.
    Failed(ApiError),
    /// The app's own server gave no usable answer to the start of the sync.
    Unreached,
}

/// The toast's dot: red for a failed sync, Kindle when something waits for the person (a
/// review, or a download), green otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dot {
    Invalid,
    Kindle,
    Valid,
}

impl Dot {
    pub fn color(self) -> &'static str {
        match self {
            Self::Invalid => "var(--tc-invalid)",
            Self::Kindle => "var(--uw-kindle)",
            Self::Valid => "var(--tc-valid)",
        }
    }
}

/// One counted part of the toast: its test id, its count and its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counted {
    pub test_id: &'static str,
    pub n: u32,
    pub text: String,
}

fn counted(test_id: &'static str, n: u32, one: Key, many: Key) -> Counted {
    let key = if n == 1 { one } else { many };
    Counted {
        test_id,
        n,
        text: t(key, &[("n", &n)]),
    }
}

impl SyncToast {
    pub fn synced(&self) -> Option<&SyncResultView> {
        match &self.end {
            SyncEnd::Synced(view) => Some(view),
            SyncEnd::Failed(_) | SyncEnd::Unreached => None,
        }
    }

    /// tC4's `data-code`: the refusal's code, or nothing.
    pub fn code(&self) -> &'static str {
        match &self.end {
            SyncEnd::Failed(ApiError::Refused(refusal)) => refusal.code().unwrap_or(""),
            _ => "",
        }
    }

    /// The three counts of an ok sync, all three even when 0.
    pub fn counts(&self) -> Option<[Counted; 3]> {
        let view = self.synced()?;
        use keys::sync_result as k;
        Some([
            counted(
                "sync-count-received",
                view.received,
                k::RECEIVED_ONE,
                k::RECEIVED_MANY,
            ),
            counted(
                "sync-count-newest",
                view.kept_newest,
                k::NEWEST_ONE,
                k::NEWEST_MANY,
            ),
            counted(
                "sync-count-conflicts",
                view.verse_conflicts,
                k::CONFLICTS_ONE,
                k::CONFLICTS_MANY,
            ),
        ])
    }

    /// "Changes from: …", each sender by [`person_label`] (#680); nothing when no teammate
    /// sent a change.
    pub fn from(&self) -> Option<String> {
        let view = self.synced()?;
        if view.from.is_empty() {
            return None;
        }
        let names: Vec<String> = view.from.iter().map(person_label).collect();
        Some(t(keys::sync_result::FROM, &[("names", &names.join(" · "))]))
    }

    /// What the sync's newer versions replaced with no question, one row per person, kind and
    /// person replaced (owner ruling H5).
    pub fn replaced(&self) -> Vec<Counted> {
        let Some(view) = self.synced() else {
            return Vec::new();
        };
        view.replaced
            .iter()
            .map(|row| Counted {
                test_id: "sync-replaced",
                n: row.count,
                text: replaced_text(row),
            })
            .collect()
    }

    /// The sentences under the line, each only when its count is not 0: changes sent again,
    /// verses with two alignments (#791), and resource versions this computer lacks.
    pub fn notes(&self) -> Vec<Counted> {
        let Some(view) = self.synced() else {
            return Vec::new();
        };
        use keys::sync_result as k;
        [
            counted(
                "sync-restored",
                view.restored,
                k::RESTORED_ONE,
                k::RESTORED_MANY,
            ),
            counted(
                "sync-alignments",
                view.alignments,
                k::ALIGNMENTS_ONE,
                k::ALIGNMENTS_MANY,
            ),
            counted(
                "sync-missing-pins",
                view.missing_pins,
                k::MISSING_PINS_ONE,
                k::MISSING_PINS_MANY,
            ),
        ]
        .into_iter()
        .filter(|note| note.n > 0)
        .collect()
    }

    /// Review: a verse conflict, or a verse with two alignments, waits for the person.
    pub fn review(&self) -> bool {
        self.synced()
            .is_some_and(|v| v.verse_conflicts > 0 || v.alignments > 0)
    }

    /// The review tab Review opens at: the verses, else the alignments, so the review never
    /// opens on an empty tab (tC4 opens nothing without a verse conflict).
    pub fn review_tab(&self) -> tc_api::conflicts::ReviewTab {
        use tc_api::conflicts::ReviewTab;
        match self.synced() {
            Some(v) if v.verse_conflicts == 0 && v.alignments > 0 => ReviewTab::Alignments,
            _ => ReviewTab::Text,
        }
    }

    /// Download: a pinned version is not on this computer (D88 point 6).
    pub fn download(&self) -> bool {
        self.synced().is_some_and(|v| v.missing_pins > 0)
    }

    pub fn dot(&self) -> Dot {
        match self.synced() {
            None => Dot::Invalid,
            Some(_) if self.review() || self.download() => Dot::Kindle,
            Some(_) => Dot::Valid,
        }
    }

    /// The sentence after "The sync did not complete.": the refusal's own, with the note that
    /// the teammates' work was kept after intake; one fixed sentence for anything else.
    pub fn failure(&self) -> Option<String> {
        match &self.end {
            SyncEnd::Synced(_) => None,
            SyncEnd::Failed(error) => Some(failure_text(error)),
            SyncEnd::Unreached => Some(t(keys::sync_result::error::UNEXPECTED, &[])),
        }
    }
}

/// What a failed sync says (tC4 `syncErrorText`).
pub fn failure_text(error: &ApiError) -> String {
    let ApiError::Refused(Refusal::Sync(failure)) = error else {
        return t(keys::sync_result::error::UNEXPECTED, &[]);
    };
    let sentence = crate::i18n::refusal_text(failure);
    match failure.kept_note() {
        Some(note) => format!("{sentence} {}", t_dyn(note, &[], None)),
        None => sentence,
    }
}

/// The Received sheet's sentence for what one person's newer versions replaced with no question
/// (owner ruling H5), and where the older version is. A verse's alignment of the person's own
/// is tC4's `syncResult.alignReplaced`.
pub fn replaced_text(row: &ReplacedView) -> String {
    use keys::sync_result::replaced as k;
    let name = person_label(&row.by);
    let place = crate::app::review::place_label(&row.place);
    let place = if row.count > 1 {
        t(k::AND_MORE, &[("place", &place), ("n", &(row.count - 1))])
    } else {
        place
    };
    let mine = row.whose.who == Who::Mine;
    let whose = if mine {
        t(k::YOURS, &[])
    } else {
        t(k::THEIRS, &[("name", &person_label(&row.whose))])
    };
    let in_history = if mine {
        k::IN_HISTORY
    } else {
        k::OLDER_IN_HISTORY
    };
    let many = |one: Key, more: Key| if row.count > 1 { more } else { one };
    let args: [(&str, &dyn std::fmt::Display); 3] =
        [("place", &place), ("name", &name), ("whose", &whose)];
    let (sentence, after) = match row.kind {
        ReplacedKind::Alignment => match &row.place {
            ReviewPlace::Verse {
                book,
                chapter,
                verse,
            } if mine && row.count == 1 => (
                t(
                    keys::sync_result::ALIGN_REPLACED,
                    &[
                        ("book", &book.english_name()),
                        ("ref", &format!("{chapter}:{verse}")),
                        ("name", &name),
                    ],
                ),
                Some(in_history),
            ),
            _ => (
                t(many(k::ALIGNMENT, k::ALIGNMENTS), &args),
                Some(in_history),
            ),
        },
        ReplacedKind::Check => (t(many(k::CHECK, k::CHECKS), &args), Some(in_history)),
        ReplacedKind::Comment => (t(many(k::COMMENT, k::COMMENTS), &args), Some(in_history)),
        ReplacedKind::Setting => (t(k::SETTING, &args), Some(k::IN_SETTINGS)),
        ReplacedKind::Resources => (t(k::RESOURCES, &args), Some(k::IN_SETTINGS)),
        ReplacedKind::Book => (t(k::BOOK, &args), None),
        ReplacedKind::File => (t(many(k::FILE, k::FILES), &args), Some(k::BOTH_KEPT)),
    };
    match after {
        Some(after) => format!("{sentence} {}", t(after, &[])),
        None => sentence,
    }
}

/// How the toast and the conflict review name a person (tC4 `actorLabel`, #680): "Mine", the
/// Door43 name or the name the teammate gave, else "Teammate n" by the server's numbering,
/// with the computer when versions from two computers carry one person's label (STORAGE §8.5).
pub fn person_label(label: &PersonLabel) -> String {
    let person = match &label.who {
        Who::Mine => t(keys::conflicts::MINE, &[]),
        Who::Username(name) | Who::DisplayName(name) => name.clone(),
        Who::Teammate(n) => t(keys::conflicts::TEAMMATE, &[("n", n)]),
    };
    match label.computer {
        None => person,
        Some(Computer::This) => t(keys::review::computer::THIS, &[("person", &person)]),
        Some(Computer::Other(n)) => t(
            keys::review::computer::OTHER,
            &[("person", &person), ("n", &n)],
        ),
    }
}
