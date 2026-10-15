//! A decision and its computed state (STORAGE.md §7.7).
//!
//! A decision stores what it was made against: the md5 of the verse text and the
//! resource the check list came from. Whether it still holds is computed every time
//! from the current verse text and the current resolution; nothing is written when the
//! text or the pins change, and restoring them makes the decision valid again.
//!
//! Two consequences differ from tC4, by design. tC4 wrote `invalidated` into the file and,
//! when such a decision re-attached, set its status to To do unless the person had chosen
//! Invalid; here a decision whose resource and words hold again is shown as the person
//! left it. And tC4 revalidated selections on every open, so a selection tC4 recorded on
//! a word that is also part of a longer word (`the` in "the theory") read as changed
//! although the verse was not edited; here the selections are revalidated only when the
//! verse text differs from the text the decision was made against. (Selections recorded
//! here count their text as the validator does, so they hold on any edit that leaves
//! their text alone; see [`crate::selection`].)
//!
//! [`Decision`] mirrors the journal's `DecisionRecord` (STORAGE.md §2.3) without the
//! fields this crate does not read: `judgement` is `judgement` field for field,
//! `comment`, `bookmarked`, `resource` and `imported_invalid` are the fields of the same
//! names. The record's `context`, `category`, `dismissed` and `extra` stay with the
//! caller.

use std::fmt;

use md5::{Digest, Md5};

use crate::Error;
use crate::resolve::ResourceRef;
use crate::selection::{self, Selection};

/// The md5 of a verse's plain text (`plain_text_v1`), as a decision records it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextMd5([u8; 16]);

impl TextMd5 {
    pub fn of(text: &str) -> Self {
        Self(Md5::digest(text.as_bytes()).into())
    }

    /// Reads 32 hexadecimal digits in either case.
    pub fn parse(hex: &str) -> Result<Self, Error> {
        let bad = || Error::Md5(hex.to_owned());
        // `from_str_radix` alone would also take a sign: "+0" reads as 0.
        if hex.len() != 32 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let mut out = [0u8; 16];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| bad())?;
        }
        Ok(Self(out))
    }

    pub fn bytes(&self) -> [u8; 16] {
        self.0
    }
}

impl fmt::Display for TextMd5 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl fmt::Debug for TextMd5 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TextMd5({self})")
    }
}

/// What the person chose for a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserStatus {
    Valid,
    Invalid,
    Todo,
}

impl UserStatus {
    /// The status as tC4 files write it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Todo => "todo",
        }
    }
}

/// The person's judgement of a check and the verse text it was made against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgement {
    /// `None` is tC4's `selections: false`.
    pub selections: Option<Vec<Selection>>,
    pub nothing_to_select: bool,
    pub status: Option<UserStatus>,
    pub verse_text_md5: TextMd5,
}

/// A stored decision on a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub judgement: Judgement,
    /// The check comment; `None` is tC4's `comments: false`.
    pub comment: Option<String>,
    /// tC4's `reminders`.
    pub bookmarked: bool,
    /// The tN or TWL pin the check list came from when the decision was made.
    pub resource: ResourceRef,
    /// tC3 or tC4 said the decision was invalidated; a new judgement clears it.
    pub imported_invalid: bool,
}

impl Decision {
    /// True when the decision carries a non-empty check comment.
    pub fn has_comment(&self) -> bool {
        self.comment.as_deref().is_some_and(|c| !c.is_empty())
    }
}

/// Why a decision is invalidated. At least one field is true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Invalidation {
    /// The check list now comes from another resource (an upgrade, a gateway change, or
    /// a teammate's pins) and the decision was not carried to it.
    pub resource_changed: bool,
    /// The verse text changed and the selected words are no longer in it as selected.
    pub selections_lost: bool,
    /// tC3 or tC4 marked it invalidated before it was imported.
    pub imported: bool,
}

/// The computed state of a decision, with the fields tC4's decision files carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assessment {
    /// `Some` when the decision no longer holds (tC4 `invalidated: true`). It is kept,
    /// and the check counts as to do until the person decides it again.
    pub invalidation: Option<Invalidation>,
    /// The verse text differs from the text the decision was made against (tC4
    /// `verseEdits`).
    pub verse_edits: bool,
    /// The status tC4 files show: an invalidated decision shows `invalid`, unless the
    /// person had chosen To do. `None` when the person chose nothing.
    pub status: Option<UserStatus>,
    /// The person's own Invalid on an invalidated decision (tC4 `userInvalid`).
    pub user_invalid: bool,
}

/// Computes a decision's state (STORAGE.md §7.7).
///
/// `resolution` is the resource the (tool, book) resolves to now, `None` when no rung
/// covers it. `verse_text` is the plain text of the verse or span the check's target
/// names. When there is no verse text (the verse is not drafted, or not in the text),
/// the text gives no evidence either way and does not invalidate, as in tC4.
pub fn assess(
    decision: &Decision,
    resolution: Option<&ResourceRef>,
    verse_text: Option<&str>,
) -> Assessment {
    let resource_ok = resolution == Some(&decision.resource);
    let judgement = &decision.judgement;
    let verse_changed =
        verse_text.is_some_and(|text| TextMd5::of(text) != judgement.verse_text_md5);
    let selections_ok = match (verse_changed, verse_text, &judgement.selections) {
        (true, Some(text), Some(selections)) => !selection::validate(text, selections).changed,
        _ => true,
    };
    let invalidation = Invalidation {
        resource_changed: !resource_ok,
        selections_lost: !selections_ok,
        imported: decision.imported_invalid,
    };
    let invalidated =
        invalidation.resource_changed || invalidation.selections_lost || invalidation.imported;
    let status = if invalidated {
        Some(match judgement.status {
            Some(UserStatus::Todo) => UserStatus::Todo,
            _ => UserStatus::Invalid,
        })
    } else {
        judgement.status
    };
    Assessment {
        invalidation: invalidated.then_some(invalidation),
        verse_edits: verse_changed,
        status,
        user_invalid: invalidated && judgement.status == Some(UserStatus::Invalid),
    }
}

/// The state of one check on the list: its decision, if any, as computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckState {
    pub assessment: Option<Assessment>,
    /// Whether the decision has selections (tC4 `selections !== false`).
    pub has_selections: bool,
    pub nothing_to_select: bool,
    pub bookmarked: bool,
    pub commented: bool,
}

impl CheckState {
    /// A check with no decision.
    pub fn undecided() -> Self {
        Self {
            assessment: None,
            has_selections: false,
            nothing_to_select: false,
            bookmarked: false,
            commented: false,
        }
    }

    /// The state of a check from its decision (see [`assess`]).
    pub fn of(
        decision: Option<&Decision>,
        resolution: Option<&ResourceRef>,
        verse_text: Option<&str>,
    ) -> Self {
        let Some(d) = decision else {
            return Self::undecided();
        };
        Self {
            assessment: Some(assess(d, resolution, verse_text)),
            has_selections: d.judgement.selections.is_some(),
            nothing_to_select: d.judgement.nothing_to_select,
            bookmarked: d.bookmarked,
            commented: d.has_comment(),
        }
    }

    pub fn invalidation(&self) -> Option<Invalidation> {
        self.assessment.and_then(|a| a.invalidation)
    }

    /// The status tC4 shows for the check (see [`Assessment::status`]).
    pub fn status(&self) -> Option<UserStatus> {
        self.assessment.and_then(|a| a.status)
    }

    /// tC4's single counting rule (`isDecided`): not invalidated, and either words are
    /// selected, nothing-to-select is set, or the status is Invalid. A bookmark or a
    /// comment is never a decision. As in tC4, a decision whose words are still selected
    /// counts as decided even after the person sets To do.
    pub fn is_decided(&self) -> bool {
        self.invalidation().is_none()
            && (self.has_selections
                || self.nothing_to_select
                || self.status() == Some(UserStatus::Invalid))
    }

    /// The dot the rail shows.
    pub fn dot(&self) -> Dot {
        if self.invalidation().is_some() {
            return Dot::Invalid;
        }
        match self.status() {
            Some(UserStatus::Valid) => Dot::Valid,
            Some(UserStatus::Invalid) => Dot::Invalid,
            Some(UserStatus::Todo) | None => Dot::Todo,
        }
    }
}

/// The status dot of a rail row: green, red, or grey. An invalidated check is red.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dot {
    Valid,
    Invalid,
    Todo,
}

/// A triage button of the Check screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Triage {
    /// "Mark valid" with the tapped words; no words is nothing-to-select.
    Valid(Vec<Selection>),
    Invalid,
    Todo,
}

/// A change the person makes to a check from the Check screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Triage(Triage),
    Bookmark(bool),
    /// The check comment; an empty comment removes it.
    Comment(String),
}

/// The decision after the person's edit (tC4 `recordDecision`, STORAGE.md §5.4).
///
/// A first decision starts from nothing selected, stamped with the md5 of `verse_text`
/// (the text the person was shown) and `resource` (the pin of the list the check is
/// on). Mark valid and Invalid record the judgement the way tC4's buttons do (Mark
/// valid sets the words or nothing-to-select, Invalid clears the words), stamp the
/// md5 and the resource again, and clear an imported invalidation: the person has
/// judged the check against what is in front of them.
///
/// To do changes only the status, as tC4's button does. It is not a judgement, so it
/// stamps nothing and an invalidation stays: an invalidated check put back to do stays
/// in To do, and its stale words are not taken as a decision on the new text. A
/// bookmark or a comment changes nothing else either.
pub fn apply(
    decision: Option<&Decision>,
    edit: Edit,
    verse_text: &str,
    resource: &ResourceRef,
) -> Decision {
    let mut next = decision.cloned().unwrap_or_else(|| Decision {
        judgement: Judgement {
            selections: None,
            nothing_to_select: false,
            status: None,
            verse_text_md5: TextMd5::of(verse_text),
        },
        comment: None,
        bookmarked: false,
        resource: resource.clone(),
        imported_invalid: false,
    });
    let (selections, nothing_to_select, status) = match edit {
        Edit::Bookmark(on) => {
            next.bookmarked = on;
            return next;
        }
        Edit::Comment(text) => {
            next.comment = (!text.is_empty()).then_some(text);
            return next;
        }
        Edit::Triage(Triage::Todo) => {
            next.judgement.status = Some(UserStatus::Todo);
            return next;
        }
        Edit::Triage(Triage::Valid(words)) if words.is_empty() => (None, true, UserStatus::Valid),
        Edit::Triage(Triage::Valid(words)) => (Some(words), false, UserStatus::Valid),
        Edit::Triage(Triage::Invalid) => (None, false, UserStatus::Invalid),
    };
    next.judgement = Judgement {
        selections,
        nothing_to_select,
        status: Some(status),
        verse_text_md5: TextMd5::of(verse_text),
    };
    next.resource = resource.clone();
    next.imported_invalid = false;
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_round_trips_as_hex() {
        let md5 = TextMd5::of("");
        assert_eq!(md5.to_string(), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            TextMd5::parse("D41D8CD98F00B204E9800998ECF8427E").unwrap(),
            md5
        );
        assert!(TextMd5::parse("d41d").is_err());
        assert!(TextMd5::parse("g41d8cd98f00b204e9800998ecf8427e").is_err());
        assert!(TextMd5::parse(&"+0".repeat(16)).is_err());
    }
}
