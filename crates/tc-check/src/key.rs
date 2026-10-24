//! The identity of a check: tC4's decision identity, `CheckKey` of `docs/STORAGE.md` §2.2.
//!
//! The types are the journal's ([`tc_journal::keys`]), so a key the Check screens make is
//! the key the journal stores. This module adds what the screens need beside them: tC4's
//! `reference` object, an Open Bible Stories frame as a target, and a reference part as
//! `tc_versification` maps it.

use serde_json::Value;
use tc_helps::ObsRef;
use tc_journal::ids::{FrameNo, StoryNo};

pub use tc_journal::keys::{CheckId, CheckKey, CheckTarget, IdentityError, RefPart, ToolId};
pub use tc_journal::pins::Collection;

/// The part as a tC4 file stores it: a number for digits, a string otherwise.
pub fn part_json(part: &RefPart) -> Value {
    let text = part.as_str();
    match text.parse::<u64>() {
        Ok(n) if text.bytes().all(|b| b.is_ascii_digit()) => Value::from(n),
        _ => Value::from(text),
    }
}

/// tC4's `reference` object: `{bookId, chapter, verse}` with the book in lower case, or
/// `{story, frame}`.
pub fn reference_json(target: &CheckTarget) -> Value {
    let mut map = serde_json::Map::new();
    match target {
        CheckTarget::Verse {
            book,
            chapter,
            verse,
        } => {
            map.insert("bookId".into(), book.code().to_ascii_lowercase().into());
            map.insert("chapter".into(), part_json(chapter));
            map.insert("verse".into(), part_json(verse));
        }
        CheckTarget::Frame { story, frame } => {
            map.insert("story".into(), story.get().into());
            map.insert("frame".into(), frame.get().into());
        }
    }
    Value::Object(map)
}

/// The target of a check on frame `at` of a story.
pub fn frame_target(at: ObsRef) -> Option<CheckTarget> {
    Some(CheckTarget::Frame {
        story: StoryNo::new(at.story())?,
        frame: FrameNo::new(at.frame()),
    })
}

/// The frame a story check is on.
pub fn obs_ref(target: &CheckTarget) -> Option<ObsRef> {
    match target {
        CheckTarget::Frame { story, frame } => ObsRef::new(story.get(), frame.get()),
        CheckTarget::Verse { .. } => None,
    }
}

/// The part as `tc_versification` reads it for mapping.
pub fn to_versification(part: &RefPart) -> tc_versification::RefPart {
    tc_versification::RefPart::parse(part.as_str())
}

/// A part `tc_versification` mapped to, as a check key holds it.
pub fn from_versification(part: &tc_versification::RefPart) -> Result<RefPart, IdentityError> {
    RefPart::parse(&part.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tc_core::BookId;

    #[test]
    fn targets_print_as_tc4_writes_them() {
        let target = CheckTarget::Verse {
            book: BookId::Titus,
            chapter: RefPart::from_number(1),
            verse: RefPart::parse("9-10").unwrap(),
        };
        assert_eq!(target.locator(), "1:9-10");
        assert_eq!(
            reference_json(&target).to_string(),
            r#"{"bookId":"tit","chapter":1,"verse":"9-10"}"#
        );
        let frame = frame_target(ObsRef::new(1, 0).unwrap()).unwrap();
        assert_eq!(
            reference_json(&frame).to_string(),
            r#"{"story":1,"frame":0}"#
        );
        assert_eq!(frame.locator(), "1:0");
        assert_eq!(obs_ref(&frame), ObsRef::new(1, 0));
    }
}
