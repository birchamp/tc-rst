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
//! The pieces are joined as they are, so the file's whitespace and line breaks stay, with one
//! addition (tC4 #795, the rule of tC4's open PR #800 at 5dcd147): usfm-js keeps the character
//! after an object's end marker in its `nextChar`, which a skipped object takes with it, so
//! `siervo\f + \ft o esclavo\f* de` would read "siervode". A skipped object therefore offers
//! its `nextChar`, and the first one offered since the last text is put back before the next
//! text when the text before it ends in a character that is not whitespace and the next text
//! starts with one: "siervo de", while `siervo \f …\f* de` and `siervo\f …\f*, de` keep the
//! text they have. Characters are UTF-16 code units and whitespace is JavaScript's `\s`, as
//! in tC4's `slice(-1)` and `/^\S/`.
//!
//! PR #800 lets only a footnote, a cross reference, an extended note and a figure offer it
//! (`tag` `f`, `fe`, `ef`, `x`, `ex` or `fig`). The rebuild lets every skipped object offer
//! it, so words never join across `\va`, `\vp`, `\ca`, `\rq`, `\cat`, `\ndx`, `\pro`, a lone
//! `\xt`, `\fdc` or the chunk milestone `\ts\*` either, which tC4 joins: a joined word can be
//! neither checked nor aligned, and the export cannot weave it (#796). This is a declared
//! difference (`docs/PLAN.md`); on the ULT and UST v91 it gives tC4's text on every verse.
//! Words still join where usfm-js misreads the markup around a note that stands inside an
//! alignment run right before `\zaln-e\*`, or around an unbalanced `\zaln-e\*` (the separator
//! lands in the `content` of an object usfm-js makes of the end marker), as in tC4.
//!
//! JavaScript recursion runs out of stack a few thousand levels down; this walk keeps its own
//! stack and has no depth limit, as the other derivations (`docs/STORAGE.md` §7.7). Where the
//! JavaScript throws, the walk returns [`Thrown`]: a `children` that is neither a list nor a
//! string (`for … of` cannot iterate it), a word's or an object's `text` that is not a string,
//! and a list met again inside itself, where the recursion would never end. A `children`
//! string is iterated character by character, and a character has no property, so it gives
//! nothing.

use super::js::{JsStr, eq, is_space};
use super::usfmjs::{Heap, Key, Parsed, Ref, Thrown, Val, truthy};

/// The shown text of every verse of `parsed`, in order, joined and not yet trimmed; `Err`
/// where tC4's walk throws.
pub(super) fn text(parsed: &Parsed) -> Result<JsStr, Thrown> {
    let heap = &parsed.heap;
    let mut walk = Walk {
        heap,
        on_path: vec![false; heap.len()],
        text: Vec::new(),
        gap: Vec::new(),
        last: None,
    };
    for (_, verse) in &parsed.verses {
        // tC4 tokenizes each verse on its own.
        walk.gap.clear();
        walk.last = None;
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
    /// The `nextChar` the first object skipped since the last text offered.
    gap: JsStr,
    /// The last code unit of the last text.
    last: Option<u16>,
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
                Step::Skip => self.skip(object),
                Step::Nothing => {}
                Step::Text(text) => self.push(text),
                Step::Walk(list) => self.enter(list, &mut stack)?,
            }
        }
        Ok(())
    }

    /// PR #800's `noteGap`, for every skipped object (see the module documentation).
    fn skip(&mut self, object: Ref) {
        if self.gap.is_empty()
            && let Some(Val::Str(next)) = self.heap.get(object, &Key::NextChar)
        {
            self.gap.clone_from(next);
        }
    }

    /// PR #800's `push`.
    fn push(&mut self, text: &[u16]) {
        if let (Some(&first), Some(&last)) = (text.first(), text.last()) {
            if !self.gap.is_empty()
                && self.last.is_some_and(|unit| !is_space(unit))
                && !is_space(first)
            {
                self.text.extend_from_slice(&self.gap);
            }
            self.gap.clear();
            self.last = Some(last);
        }
        self.text.extend_from_slice(text);
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
