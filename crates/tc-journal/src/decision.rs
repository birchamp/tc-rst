//! Check decisions: the edit a checker makes (`docs/STORAGE.md` §5.4) and the state the fold
//! computes for a stored decision (§7.7). The Check screens and the fold both use these
//! functions, so what a screen shows and what the decision files say cannot differ.

use crate::command::{DecisionChange, DecisionEdit};
use crate::ids::Md5;
use crate::json::JsonMap;
use crate::record::{DecisionRecord, Judgement, ResourceRef, Selection, UserStatus};
use std::sync::OnceLock;
use tc_usfm::{SelectionCount, SelectionText};

/// The hash a decision records of the text it was judged against, and an alignment of the
/// verse it aligns: md5 of `plain_text_v1(content)`, over its UTF-8 bytes (§7.7). `content`
/// is a verse value's `words + layout`, or a run's.
pub fn verse_text_md5_v1(content: &str) -> Md5 {
    Md5::of(tc_usfm::plain_text_v1(content).as_bytes())
}

/// The text a decision's target or an alignment's verse names, as the fold, the Check
/// screens and the aligner read it (§7.7): its content, the hash of its plain text, the text
/// the selection validator searches, and the text a screen shows, the last two made on first
/// use, once for all the decisions of one target.
///
/// The shown text and the hashed text are tC4's two texts and differ on many verses (a
/// poetry line break inside the verse, `\qs` Selah, a heading): the screens show and split
/// into words what tC4 shows (`verseText`), and the hash is of what tC4's fold and export
/// hash (`verseTextMd5`).
#[derive(Debug)]
pub struct TargetText {
    content: String,
    plain: String,
    md5: Md5,
    selection: OnceLock<SelectionText>,
    shown: OnceLock<String>,
}

impl TargetText {
    pub fn new(content: String) -> Self {
        let plain = tc_usfm::plain_text_v1(&content);
        Self {
            md5: Md5::of(plain.as_bytes()),
            plain,
            content,
            selection: OnceLock::new(),
            shown: OnceLock::new(),
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    /// `plain_text_v1` of the content: the text the hash is of. A screen does not show it.
    pub fn plain(&self) -> &str {
        &self.plain
    }

    /// The text a screen shows ([`tc_usfm::shown_text_v1`] of the content, tC4's
    /// `verseText` of `src/data/bookModel.js`): the Check screen's tappable words and the
    /// aligner's words are this text's words.
    pub fn shown(&self) -> &str {
        self.shown
            .get_or_init(|| tc_usfm::shown_text_v1(&self.content))
    }

    /// [`verse_text_md5_v1`] of the content.
    pub fn md5(&self) -> Md5 {
        self.md5
    }

    /// `selections_v1(content, selections)`: each selection's text occurs in the content,
    /// read through usfm-js's `removeMarker`, exactly `occurrences` times.
    pub fn fits(&self, selections: &[Selection]) -> bool {
        let counts: Vec<SelectionCount<'_>> = selections
            .iter()
            .map(|s| SelectionCount {
                text: &s.text,
                occurrences: s.occurrences,
            })
            .collect();
        self.selection
            .get_or_init(|| SelectionText::v1(&self.content))
            .fits(&counts)
    }

    /// How many times `text` occurs in the content as `selections_v1` counts it: what a new
    /// selection of `text` records as its `occurrences`, so it fits until the text changes.
    /// `None` where the library has no value (it throws on the content); any count fits
    /// there.
    pub fn occurrences(&self, text: &str) -> Option<u32> {
        self.selection
            .get_or_init(|| SelectionText::v1(&self.content))
            .occurrences(text)
    }
}

/// Why a decision no longer holds. At least one field is true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Invalidation {
    /// The (tool, book) resolves to another resource, or to none, or to an unusable pin.
    pub resource_changed: bool,
    /// The target's text changed and the selected words no longer fit it.
    pub selections_lost: bool,
    /// tC3 or tC4 marked it invalidated before it was imported.
    pub imported: bool,
}

/// The computed state of a decision: the flags its decision file carries (§7.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assessment {
    /// `Some` when the decision does not hold (tC4 `invalidated`). It is kept as it is.
    pub invalidation: Option<Invalidation>,
    /// The target's text differs from the text it was judged against (tC4 `verseEdits`).
    pub verse_edits: bool,
    /// The status tC4's files show: an invalidated decision shows `Invalid`, unless the
    /// person chose To do. `None` when the person chose nothing.
    pub status: Option<UserStatus>,
    /// The person's own Invalid on an invalidated decision (tC4 `userInvalid`).
    pub user_invalid: bool,
}

/// The state of `decision` (§7.7). `resolution` is the usable pin the (tool, book) resolves
/// to now, `None` when no rung answers or the rung that answers holds an unusable pin
/// ([`crate::pins::Answer::resource`]). `text` is the target's text by the no-text rule;
/// with none, the text gives no evidence and nothing is invalidated by it, as in tC4.
pub fn assess(
    decision: &DecisionRecord,
    resolution: Option<&ResourceRef>,
    text: Option<&TargetText>,
) -> Assessment {
    let judgement = &decision.judgement;
    let resource_ok = resolution.is_some_and(|r| r.same_resource(&decision.resource));
    let verse_changed = text.is_some_and(|t| t.md5() != judgement.verse_text_md5);
    let selections_ok = match (verse_changed, text, &judgement.selections) {
        (true, Some(text), Some(selections)) => text.fits(selections),
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

/// Why a decision edit cannot be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecisionEditError {
    #[error("a first decision needs the check item's context and category")]
    NoBasis,
}

impl DecisionEdit {
    /// The decision after this edit (§5.4). `displayed` is the decision the screen showed,
    /// `None` for a first decision, which starts from [`DecisionEdit::basis`]. `text` is the
    /// target's text the screen showed, by the no-text rule of §7.7.
    ///
    /// A judgement (selections, nothing to select, or the status Valid or Invalid) is made
    /// against what is in front of the person: it stamps the md5 of `text`, clears an
    /// imported invalidation and records [`DecisionEdit::resource`], the pin of the check list
    /// the person judged in. To do and clearing the status change only the status, as tC4's
    /// To do button does (`src/views/Check.jsx` `markTodo` at d310bf3): an invalidated check
    /// put back to do stays invalidated, and its stale words are not taken as a judgement of
    /// the new text. A bookmark or a comment changes only itself.
    pub fn apply(
        &self,
        displayed: Option<&DecisionRecord>,
        text: Option<&str>,
    ) -> Result<DecisionRecord, DecisionEditError> {
        let md5 = || verse_text_md5_v1(text.unwrap_or(""));
        let mut next = match (displayed, &self.basis) {
            (Some(decision), _) => decision.clone(),
            (None, Some(basis)) => DecisionRecord {
                context: basis.context.clone(),
                category: basis.category.clone(),
                judgement: Judgement {
                    selections: None,
                    nothing_to_select: false,
                    status: None,
                    verse_text_md5: md5(),
                },
                comment: None,
                bookmarked: false,
                resource: self.resource.clone(),
                imported_invalid: false,
                dismissed: false,
                extra: JsonMap::new(),
            },
            (None, None) => return Err(DecisionEditError::NoBasis),
        };
        let mut judged = false;
        for change in &self.changes {
            match change {
                DecisionChange::Status(status) => {
                    judged |= matches!(status, Some(UserStatus::Valid | UserStatus::Invalid));
                    next.judgement.status = *status;
                }
                DecisionChange::Selections(selections) => {
                    judged = true;
                    next.judgement.selections = selections.clone();
                }
                DecisionChange::NothingToSelect(on) => {
                    judged = true;
                    next.judgement.nothing_to_select = *on;
                }
                DecisionChange::Comment(comment) => {
                    next.comment = comment.clone().filter(|c| !c.is_empty());
                }
                DecisionChange::Bookmarked(on) => next.bookmarked = *on,
            }
        }
        if judged {
            next.judgement.verse_text_md5 = md5();
            next.imported_invalid = false;
            next.resource = self.resource.clone();
        }
        Ok(next)
    }
}
