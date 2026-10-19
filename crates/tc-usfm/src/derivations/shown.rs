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
//! 4. Any other object with a `text`, unless it is a section heading, gives that text, and
//!    then its `children` are walked if they are a list.
//! 5. Otherwise its `children` are walked, if it has any; an object with neither (a cross
//!    reference, a figure, a marker usfm-js keeps no text for) is skipped.
//!
//! tC4 does not walk the children in rule 4, so it hides every word of a character span
//! after a span nested in it: usfm-js reads `\wj Worship the \+nd Lord\+nd* your God\wj*` as
//! a `wj` with the text "Worship the " and the children `+nd` "Lord" and " your God", and tC4
//! shows "Worship the ". The export counts all five words, so tC4's Check and Align bank
//! cannot select or align "Lord", "your" and "God", and a bank without them cannot be woven
//! (#796). The rebuild walks them, a declared difference (`docs/PLAN.md`); usfm-js puts a
//! span's text before its first nested marker in `text` and the rest in `children`, so the
//! words come in file order and none twice. On the ULT and UST v91 this changes no verse.
//!
//! The pieces are joined as they are, so the file's whitespace and line breaks stay, with
//! two additions, so that words never join across a marker: an object left out, or the start
//! or end of the object of any marker but a word or an alignment milestone (whose markup the
//! import's split removes).
//!
//! First (tC4 #795, the rule of tC4's open PR #800 at 5dcd147): usfm-js keeps the space or
//! line break after an object's end marker in its `nextChar`, which a skipped object takes
//! with it, so `siervo\f + \ft o esclavo\f* de` would read "siervode". A skipped object
//! therefore offers its `nextChar`, and the first one offered since the last text is put back
//! before the next text when the text before it ends in a character that is not whitespace
//! and the next text starts with one: "siervo de", while `siervo \f …\f* de` and
//! `siervo\f …\f*, de` keep the text they have. Characters are UTF-16 code units and
//! whitespace is JavaScript's `\s`, as in tC4's `slice(-1)` and `/^\S/`. PR #800 lets only a
//! footnote, a cross reference, an extended note and a figure offer it (`tag` `f`, `fe`, `ef`,
//! `x`, `ex` or `fig`); the rebuild lets every skipped object offer it, so words never join
//! across `\va`, `\vp`, `\ca`, `\rq`, `\cat`, `\ndx`, `\pro`, a lone `\xt`, `\fdc` or the
//! chunk milestone `\ts\*` either, which tC4 joins.
//!
//! Second: usfm-js keeps no `nextChar` when the end marker is followed by anything but a
//! space or a line break (`siervo\f + \ft o esclavo\f*de`), and none at the start or end of
//! a span or paragraph (`the \add Lord\add*s house`, `Yahweh\q2 que`, adjacent ruby
//! `\rb 神|かみ\rb*\rb 愛|あい\rb*`). Where none was offered and the characters on either side
//! of the marker would read as one word or number to tC4's tokenizer ([`crate::words`]), a
//! space is put back: "siervo de", "Lord s", "Yahweh que", "神 愛". Nothing is put back before
//! punctuation (`siervo\f …\f*, de`). tC4, PR #800 too, joins them.
//!
//! A joined word can be neither checked nor aligned, and the export, which splits words at
//! every marker, cannot weave it (#796), so both additions beyond PR #800 are a declared
//! difference (`docs/PLAN.md`). On the ULT and UST v91 they change one verse from tC4's text:
//! ULT 2SA 23:8 as written, where usfm-js makes an object of an unbalanced `\zaln-e\*` that
//! keeps the line break after it, and tC4 shows "spearagainst". Words still join where usfm-js
//! misreads alignment markup in other ways (UST 2SA 23:8 and 1KI 10:12), as in tC4.
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
use crate::words::TokenKind;

/// The shown text of every verse of `parsed`, in order, joined and not yet trimmed; `Err`
/// where tC4's walk throws.
pub(super) fn text(parsed: &Parsed) -> Result<JsStr, Thrown> {
    let heap = &parsed.heap;
    let mut walk = Walk {
        heap,
        on_path: vec![false; heap.len()],
        text: Vec::new(),
        gap: Vec::new(),
        apart: false,
        last: None,
    };
    for (_, verse) in &parsed.verses {
        // tC4 tokenizes each verse on its own.
        walk.gap.clear();
        walk.apart = false;
        walk.last = None;
        walk.verse(*verse)?;
    }
    Ok(walk.text)
}

/// What one verse object gives, by the rules of the module documentation.
enum Step<'h> {
    /// Rules 1 and 5: the object is left out.
    Skip,
    /// Rules 3 and 4: this text, then, for rule 4, walk this list.
    Text(&'h [u16], Option<Ref>),
    /// Rules 2 and 5: walk this list.
    Walk(Ref),
    /// The object gives nothing and is not skipped: an empty milestone, a word with no text,
    /// or `children` that are a string.
    Nothing,
}

/// A list on the path of the walk.
struct Frame {
    list: Ref,
    at: usize,
    /// The list is the `children` of a marker's object, so leaving it ends the marker.
    marker: bool,
}

struct Walk<'h> {
    heap: &'h Heap,
    /// The lists on the path of the walk, to find a list inside itself.
    on_path: Vec<bool>,
    text: JsStr,
    /// The `nextChar` the first object skipped since the last text offered.
    gap: JsStr,
    /// Whether a marker stands between the last text and the next.
    apart: bool,
    /// The last code unit of the last text.
    last: Option<u16>,
}

impl<'h> Walk<'h> {
    /// `walk(vObj.verseObjects)` for one verse, with its own stack of lists and positions.
    fn verse(&mut self, verse: Ref) -> Result<(), Thrown> {
        let heap = self.heap;
        let mut stack: Vec<Frame> = Vec::new();
        self.enter(verse, false, &mut stack)?;
        while let Some(frame) = stack.last_mut() {
            let Some(&object) = heap.elems(frame.list).get(frame.at) else {
                if let Some(seen) = self.on_path.get_mut(frame.list) {
                    *seen = false;
                }
                self.apart |= frame.marker;
                stack.pop();
                continue;
            };
            frame.at += 1;
            let marker = is_marker(heap, object);
            self.apart |= marker;
            match step(heap, object)? {
                Step::Skip => {
                    self.apart = true;
                    self.offer(heap.get(object, &Key::NextChar));
                }
                Step::Nothing => {}
                Step::Text(text, children) => {
                    self.push(text);
                    match children {
                        Some(list) => self.enter(list, marker, &mut stack)?,
                        None => self.apart |= marker,
                    }
                }
                Step::Walk(list) => self.enter(list, marker, &mut stack)?,
            }
        }
        Ok(())
    }

    /// PR #800's `noteGap`, for every skipped object (see the module documentation).
    fn offer(&mut self, space: Option<&Val>) {
        if self.gap.is_empty()
            && let Some(Val::Str(space)) = space
        {
            self.gap.clone_from(space);
        }
    }

    /// PR #800's `push`, and the space where no `nextChar` was kept.
    fn push(&mut self, text: &[u16]) {
        if let (Some(&first), Some(&last)) = (text.first(), text.last()) {
            if let Some(before) = self.last.filter(|&unit| !is_space(unit))
                && !is_space(first)
            {
                if !self.gap.is_empty() {
                    self.text.extend_from_slice(&self.gap);
                } else if self.apart && would_join(before, first) {
                    self.text.push(u16::from(b' '));
                }
            }
            self.gap.clear();
            self.apart = false;
            self.last = Some(last);
        }
        self.text.extend_from_slice(text);
    }

    fn enter(&mut self, list: Ref, marker: bool, stack: &mut Vec<Frame>) -> Result<(), Thrown> {
        match self.on_path.get_mut(list) {
            Some(seen) if !*seen => *seen = true,
            _ => return Err(Thrown),
        }
        stack.push(Frame {
            list,
            at: 0,
            marker,
        });
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
            Some(Val::Str(text)) => Ok(Step::Text(text, None)),
            Some(_) => Err(Thrown),
        };
    }
    if let Some(text) = heap.get(object, &Key::Text)
        && !is(&Key::Type, "section")
    {
        // tC4 stops here; the rebuild walks the children too (see the module documentation).
        let children = match heap.get(object, &Key::Children) {
            Some(Val::Ref(list)) if heap.is_array(*list) => Some(*list),
            _ => None,
        };
        return match text {
            Val::Str(text) => Ok(Step::Text(text, children)),
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

/// Whether `object` is a marker's whose start and end keep words apart: anything but plain
/// text, a word or an alignment milestone.
fn is_marker(heap: &Heap, object: Ref) -> bool {
    let is = |key: &Key, value: &str| heap.s(object, key).is_some_and(|s| eq(s, value));
    heap.s(object, &Key::Tag).is_some() && !is(&Key::Type, "word") && !is(&Key::Tag, "zaln")
}

/// Whether the code units `a` then `b` read as one word or number to tC4's tokenizer, so that
/// text ending in `a` and text starting with `b` would join into one word.
fn would_join(a: u16, b: u16) -> bool {
    let (Some(a), Some(b)) = (char::from_u32(a.into()), char::from_u32(b.into())) else {
        return false;
    };
    let pair: String = [a, b].into_iter().collect();
    matches!(
        crate::words::classify(&pair).as_slice(),
        [token] if matches!(token.kind, TokenKind::Word | TokenKind::Number)
    )
}
