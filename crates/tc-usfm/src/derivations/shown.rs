//! The walk behind [`super::shown_text_v1`]: tC4's `verseText` (`src/data/bookModel.js` at tC4
//! 9ad4d5c, #782), which is `tokensText(tokenizeVerse(vObj)).trim()` of
//! `src/data/sourceHighlight.ts`, over the usfm-js port's verse objects.
//!
//! `tokenizeVerse` walks a verse's objects in order and decides each one by the first rule
//! that applies:
//!
//! 1. A footnote (`type` "footnote", or `tag` "f") is skipped with everything inside.
//! 2. An alignment milestone (`tag` "zaln", `type` "milestone") gives nothing itself; its
//!    `children` are walked, at any depth, so every word of a nested alignment is shown.
//! 3. A word (`type` "word") gives its `text`.
//! 4. Any other object with a `text`, unless it is a section heading, gives that text and its
//!    children are not walked.
//! 5. Otherwise its `children` are walked, if it has any; an object with neither (a cross
//!    reference, a figure, a marker usfm-js keeps no text for) is skipped.
//!
//! The pieces are joined as they are, so the file's whitespace and line breaks stay.
//!
//! JavaScript recursion runs out of stack a few thousand levels down; this walk keeps its own
//! stack and has no depth limit, as the other derivations (`docs/STORAGE.md` §7.7). Where the
//! JavaScript throws, the walk returns [`Thrown`]: a `children` that is neither a list nor a
//! string (`for … of` cannot iterate it), a word's or an object's `text` that is not a string,
//! and a list met again inside itself, where the recursion would never end. A `children`
//! string is iterated character by character, and a character has no property, so it gives
//! nothing.

use super::js::{JsStr, eq};
use super::usfmjs::{Heap, Key, Parsed, Ref, Thrown, Val, truthy};

/// The shown text of every verse of `parsed`, in order, joined and not yet trimmed; `Err`
/// where tC4's walk throws.
pub(super) fn text(parsed: &Parsed) -> Result<JsStr, Thrown> {
    let heap = &parsed.heap;
    let mut walk = Walk {
        heap,
        on_path: vec![false; heap.len()],
        text: Vec::new(),
    };
    for (_, verse) in &parsed.verses {
        walk.verse(*verse)?;
    }
    Ok(walk.text)
}

/// What one verse object gives, by the rules of the module documentation.
enum Step<'h> {
    /// Rules 1 and 5: the object is left out.
    Skip,
    /// Rules 3 and 4.
    Text(&'h [u16]),
    /// Rules 2 and 5: walk this list.
    Walk(Ref),
    /// The object gives nothing and is not skipped: an empty milestone, a word with no text,
    /// or `children` that are a string.
    Nothing,
}

struct Walk<'h> {
    heap: &'h Heap,
    /// The lists on the path of the walk, to find a list inside itself.
    on_path: Vec<bool>,
    text: JsStr,
}

impl<'h> Walk<'h> {
    /// `walk(vObj.verseObjects)` for one verse, with its own stack of lists and positions.
    fn verse(&mut self, verse: Ref) -> Result<(), Thrown> {
        let mut stack: Vec<(Ref, usize)> = Vec::new();
        self.enter(verse, &mut stack)?;
        while let Some((list, at)) = stack.last_mut() {
            let Some(&object) = self.heap.elems(*list).get(*at) else {
                if let Some(seen) = self.on_path.get_mut(*list) {
                    *seen = false;
                }
                stack.pop();
                continue;
            };
            *at += 1;
            match step(self.heap, object)? {
                Step::Skip | Step::Nothing => {}
                Step::Text(text) => self.text.extend_from_slice(text),
                Step::Walk(list) => self.enter(list, &mut stack)?,
            }
        }
        Ok(())
    }

    fn enter(&mut self, list: Ref, stack: &mut Vec<(Ref, usize)>) -> Result<(), Thrown> {
        match self.on_path.get_mut(list) {
            Some(seen) if !*seen => *seen = true,
            _ => return Err(Thrown),
        }
        stack.push((list, 0));
        Ok(())
    }
}

/// The rule of `tokenizeVerse` that applies to `object`.
fn step(heap: &Heap, object: Ref) -> Result<Step<'_>, Thrown> {
    let is = |key: &Key, value: &str| heap.s(object, key).is_some_and(|s| eq(s, value));
    if is(&Key::Type, "footnote") || is(&Key::Tag, "f") {
        return Ok(Step::Skip);
    }
    if is(&Key::Tag, "zaln") && is(&Key::Type, "milestone") {
        // `walk(vo.children ?? [])`.
        return match heap.get(object, &Key::Children) {
            None | Some(Val::Str(_)) => Ok(Step::Nothing),
            Some(Val::Ref(list)) if heap.is_array(*list) => Ok(Step::Walk(*list)),
            Some(_) => Err(Thrown),
        };
    }
    if is(&Key::Type, "word") {
        // `vo.text ?? ''`, whose `slice` throws on anything but a string.
        return match heap.get(object, &Key::Text) {
            None => Ok(Step::Nothing),
            Some(Val::Str(text)) => Ok(Step::Text(text)),
            Some(_) => Err(Thrown),
        };
    }
    if let Some(text) = heap.get(object, &Key::Text)
        && !is(&Key::Type, "section")
    {
        return match text {
            Val::Str(text) => Ok(Step::Text(text)),
            _ => Err(Thrown),
        };
    }
    // `if (vo.children) walk(vo.children)`.
    match heap.get(object, &Key::Children) {
        Some(children) if truthy(children) => match children {
            Val::Str(_) => Ok(Step::Nothing),
            Val::Ref(list) if heap.is_array(*list) => Ok(Step::Walk(*list)),
            _ => Err(Thrown),
        },
        _ => Ok(Step::Skip),
    }
}
