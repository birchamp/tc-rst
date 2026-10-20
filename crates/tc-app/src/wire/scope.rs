//! Records, projects, people and the scope of a change, as tc-api carries them.

use std::collections::BTreeSet;

use tc_api::ApiError;
use tc_api::check::{CheckListRef, CheckUnit, Tool};
use tc_api::event::{Aspect, ChangeCause, ChangeParts, ChangeScope, ChapterRef, RecordChange};
use tc_api::ids::{RecordRef, StoryNo, ViewSeq};
use tc_api::shown::{PersonLabel, Who};
use tc_journal::canon::{decode_key, encode_key};
use tc_journal::fold::Delta;
use tc_journal::record::CommentTarget;
use tc_journal::{BookId, CheckKey, CheckTarget, DeviceId, RecordKey, ToolId};

use crate::backend::{Origin, StoreCause};

/// A record as tc-api names it: the canonical JSON of its key. `None` for a key whose JSON
/// is longer than a `RecordRef` holds (4,096 bytes).
pub fn record_ref(key: &RecordKey) -> Option<RecordRef> {
    let json = encode_key(key).ok()?;
    RecordRef::parse(&json.to_canonical_string()).ok()
}

/// The key a request names. One that does not read is a `BadRequest`: the UI only sends back
/// what the server gave it.
pub fn record_key(record: &RecordRef) -> Result<RecordKey, ApiError> {
    let bad = |reason: String| ApiError::BadRequest {
        field: "record".to_owned(),
        reason,
    };
    let json =
        tc_journal::json::parse(record.as_str().as_bytes()).map_err(|e| bad(e.to_string()))?;
    decode_key(&json).map_err(|e| bad(e.to_string()))
}

pub fn view(seq: u64) -> ViewSeq {
    ViewSeq::new(seq)
}

/// The project's id as tc-api writes it, the lowercase hyphenated UUID.
#[expect(
    clippy::expect_used,
    reason = "a UUID's hyphenated form is always 8-4-4-4-12 lowercase hex, the form tc-api reads"
)]
pub fn project_id(id: tc_journal::ProjectId) -> tc_api::ids::ProjectId {
    tc_api::ids::ProjectId::parse(&id.to_string()).expect("a UUID in hyphenated form")
}

/// How the screens name a person (STORAGE §8.5). A teammate past "Teammate 65535" keeps that
/// number.
pub fn person_label(label: &tc_journal::fold::PersonLabel) -> PersonLabel {
    use tc_journal::fold::PersonLabel as Label;
    let who = match label {
        Label::Mine => Who::Mine,
        Label::Username(name) => Who::Username(name.to_string()),
        Label::DisplayName(name) => Who::DisplayName(name.to_string()),
        Label::Teammate(n) => Who::Teammate(u16::try_from(*n).unwrap_or(u16::MAX)),
    };
    PersonLabel {
        who,
        computer: None,
    }
}

/// Who a change came from: the request of this app that asked for it (so the field that sent
/// it shows no notice, AD37), or by the labels of its devices, each person once, in device
/// order.
pub fn change_cause(
    cause: &StoreCause,
    label: impl Fn(&DeviceId) -> Option<PersonLabel>,
) -> ChangeCause {
    match cause {
        StoreCause::Intake { devices } => {
            let mut from: Vec<PersonLabel> = Vec::new();
            for label in devices.iter().filter_map(label) {
                if !from.contains(&label) {
                    from.push(label);
                }
            }
            ChangeCause::Intake { from }
        }
        StoreCause::Command {
            origin: Origin::Request(request),
        } => ChangeCause::LocalSave { request: *request },
        StoreCause::Command {
            origin: Origin::Server,
        }
        | StoreCause::System => ChangeCause::System,
    }
}

/// A change part by part, or `Everything` when one part cannot be told exactly.
#[derive(Default)]
struct Parts {
    chapters: BTreeSet<ChapterRef>,
    books: BTreeSet<BookId>,
    stories: BTreeSet<u8>,
    checks: BTreeSet<CheckListRef>,
    pins: bool,
    settings: bool,
    meta: bool,
    conflicts: bool,
}

/// A part that only a refresh of everything describes.
struct Everything;

impl Parts {
    fn chapter(&mut self, book: BookId, chapter: u16) {
        self.chapters.insert(ChapterRef { book, chapter });
    }

    /// A decision changes its check list's rows; on a frame, its story too. tc-api has no
    /// translationQuestions tool, so a tQ decision names no list: its record is still listed.
    fn check(&mut self, check: &CheckKey) -> Result<(), Everything> {
        let unit = match &check.target {
            CheckTarget::Verse { book, .. } => CheckUnit::Book(*book),
            CheckTarget::Frame { story, .. } => {
                self.stories.insert(story.get());
                CheckUnit::Story(StoryNo::new(story.get()).map_err(|_| Everything)?)
            }
        };
        let tool = match check.tool {
            ToolId::TranslationNotes => Some(Tool::TranslationNotes),
            ToolId::TranslationWords => Some(Tool::TranslationWords),
            ToolId::TranslationQuestions => None,
        };
        if let Some(tool) = tool {
            self.checks.insert(CheckListRef { tool, unit });
        }
        Ok(())
    }

    /// A story's frame changed: each check of the story reads its frame's text, so both of
    /// the story's lists may have rows that changed, as a verse's do (`text`).
    fn story_text(&mut self, story: tc_journal::StoryNo) -> Result<(), Everything> {
        self.stories.insert(story.get());
        let unit = CheckUnit::Story(StoryNo::new(story.get()).map_err(|_| Everything)?);
        for tool in [Tool::TranslationNotes, Tool::TranslationWords] {
            self.checks.insert(CheckListRef { tool, unit });
        }
        Ok(())
    }

    /// A book's text changed: each check of the book reads its verse's text, and a check
    /// whose selections the new text no longer holds is invalidated, so every tN and tW
    /// list of the book may have rows that changed. tc-journal's delta names the verse, not
    /// the checks whose assessment moved, so both lists are named, and an open rail asks only
    /// for its rows changed since its view (AD17).
    fn text(&mut self, book: BookId) {
        for tool in [Tool::TranslationNotes, Tool::TranslationWords] {
            self.checks.insert(CheckListRef {
                tool,
                unit: CheckUnit::Book(book),
            });
        }
    }

    /// The parts a change to `key` touches. One `match` over every kind with no catch-all, so
    /// a new kind does not compile until someone decides its parts.
    fn key(
        &mut self,
        key: &RecordKey,
        comment_target: &impl Fn(&RecordKey) -> Option<CommentTarget>,
    ) -> Result<(), Everything> {
        match key {
            // Books that changed as a whole: present, removed, their header or their order.
            // A first record of a book (tc-journal's `Delta.books`) is not one of these.
            RecordKey::BookPresence(book)
            | RecordKey::BookHeader(book)
            | RecordKey::BookLayout(book) => {
                self.books.insert(*book);
            }
            // A check can target a chapter's front as well as a verse.
            RecordKey::ChapterFront(book, chapter) | RecordKey::Verse(book, chapter, _) => {
                self.chapter(*book, chapter.get());
                self.text(*book);
            }
            // An alignment links words; no check reads it.
            RecordKey::Alignment(book, chapter, _) => self.chapter(*book, chapter.get()),
            RecordKey::Check(check) => self.check(check)?,
            // A comment's place is in its value, not its key: one read of the record.
            RecordKey::Comment(_) => match comment_target(key).ok_or(Everything)? {
                CommentTarget::Verse { book, chapter, .. } => self.chapter(book, chapter.get()),
                CommentTarget::Check(check) => self.check(&check)?,
                CommentTarget::Frame { story, .. } => {
                    self.stories.insert(story.get());
                }
            },
            RecordKey::Pins(_) | RecordKey::ResourcesExtra(_) => self.pins = true,
            RecordKey::Setting(_) => self.settings = true,
            RecordKey::ProjectMeta(_) => self.meta = true,
            // Every check maps again under another versification.
            RecordKey::Versification => return Err(Everything),
            // A profile names a person in the review and in every "changed by" line.
            RecordKey::DeviceProfile(_) => self.conflicts = true,
            RecordKey::Frame(story, _) => self.story_text(*story)?,
            RecordKey::StoryBase(story) | RecordKey::StoryRef(story) => {
                self.stories.insert(story.get());
            }
            // No screen of this version shows carried files or a newer minor's kinds, and a
            // release's own changes come in the same delta under their keys: the record alone.
            RecordKey::CarriedFile(_) | RecordKey::BatchRelease(_) | RecordKey::Unknown(_) => {}
        }
        Ok(())
    }
}

/// tc-app's form of a fold `Delta` (§3.6). `conflicts_changed` says whether the conflict
/// counts differ before and after (a resolution removes a conflict, and `new_conflicts` lists
/// only added ones); `by` names who wrote each record; `comment_target` reads where a comment
/// is (its value says, not its key).
///
/// `delta.paths` (carried or derived paths moved) adds nothing: no screen of phase 1 shows
/// carried files.
pub fn change_scope(
    delta: &Delta,
    conflicts_changed: bool,
    by: impl Fn(&RecordKey) -> Option<PersonLabel>,
    comment_target: impl Fn(&RecordKey) -> Option<CommentTarget>,
) -> ChangeScope {
    if delta.everything {
        return ChangeScope::Everything;
    }
    let mut parts = Parts {
        pins: delta.pins,
        conflicts: conflicts_changed || !delta.new_conflicts.is_empty(),
        ..Parts::default()
    };
    for (book, chapter) in &delta.chapters {
        parts.chapter(*book, *chapter);
    }
    let mut records = Vec::with_capacity(delta.keys.len());
    for key in &delta.keys {
        if parts.key(key, &comment_target).is_err() {
            return ChangeScope::Everything;
        }
        let Some(record) = record_ref(key) else {
            return ChangeScope::Everything;
        };
        records.push(RecordChange {
            record,
            by: by(key),
        });
    }
    let Ok(stories) = parts
        .stories
        .iter()
        .map(|s| StoryNo::new(*s))
        .collect::<Result<Vec<_>, _>>()
    else {
        return ChangeScope::Everything;
    };
    let aspects = [
        (parts.pins, Aspect::Pins),
        (parts.settings, Aspect::Settings),
        (parts.meta, Aspect::Meta),
        (parts.conflicts, Aspect::Conflicts),
    ]
    .into_iter()
    .filter_map(|(touched, aspect)| touched.then_some(aspect))
    .collect();
    ChangeScope::bounded(ChangeParts {
        chapters: parts.chapters.into_iter().collect(),
        books: parts.books.into_iter().collect(),
        stories,
        checks: parts.checks.into_iter().collect(),
        records,
        aspects,
    })
}
