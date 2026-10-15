//! A port of usfm-js 3.4.3 `toJSON(text, { chunk: true })` (`lib/js/usfmToJson.js`) and
//! `removeMarker` (`lib/js/filter.js`), function for function, quirks included.
//!
//! tC4 hashes a verse's text with this parser (`verseTextMd5` in `journal/fold.mjs`), and
//! word-aligner-lib checks selections with it, so `plain_text_v1` and `selections_v1` must
//! give what the library gives on every input, not what a careful USFM reader would. The
//! library mutates shared objects as it goes (a marker object is pushed into the verse and
//! then edited through the marker list), so the port keeps JavaScript's object model: a
//! heap of objects with named properties, where arrays are objects with elements.
//!
//! Every input starts with `\v 1 ` (both callers prepend it), so the library's header
//! handling (`addHeaderMarker`, `cleanupHeaderNewLines`) is never reached and is not
//! ported: the first marker sets the current verse, and nothing sets it back to zero
//! without also setting the current chapter to a string, which is never `=== 0`.
//!
//! One deliberate difference: a marker named after a member of `Object.prototype`
//! (`\constructor`, `\toString`) is looked up in the library's tables as that member, which
//! makes it throw or treat the marker oddly. Here such a marker is an unknown marker.

use super::js::{
    JsStr, cmp_ascii, concat, ends_with, eq, index_of, is_digit, is_line_terminator, is_space,
    is_word, js, last_index_of, number_to_string, split, starts_with, substr, substring, to_number,
    trim,
};
use std::borrow::Cow;
use std::collections::BTreeMap;

use super::usfmjs_tables::{
    MARKERS_WITH_NUMBERS, PROPERTIES, SPECIAL_END_TAGS, WORD_SPECIAL_ATTRIBUTES,
};

// ---------------------------------------------------------------------------------------
// The marker table
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub(super) enum EndTag {
    None,
    One(&'static str),
    Many(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Props {
    pub ty: Option<&'static str>,
    pub end_tag: EndTag,
    pub display: bool,
    pub attrib: bool,
    pub end_attrib: bool,
    pub standalone: bool,
    pub usfm3_milestone: bool,
}

fn props(tag: Option<&[u16]>) -> Option<&'static Props> {
    let tag = tag?;
    PROPERTIES
        .binary_search_by(|(name, _)| cmp_ascii(tag, name).reverse())
        .ok()
        .map(|at| &PROPERTIES[at].1)
}

fn special_end_tag(tag: &[u16]) -> Option<&'static str> {
    SPECIAL_END_TAGS
        .binary_search_by(|(name, _)| cmp_ascii(tag, name).reverse())
        .ok()
        .map(|at| SPECIAL_END_TAGS[at].1)
}

fn supports_numbers(tag: Option<&[u16]>) -> bool {
    tag.is_some_and(|tag| MARKERS_WITH_NUMBERS.iter().any(|name| eq(tag, name)))
}

fn displayable(tag: Option<&[u16]>) -> bool {
    props(tag).is_some_and(|p| p.display)
}

/// `markerTermination` as a list (a string becomes a list of one).
fn terminations(tag: Option<&[u16]>) -> Vec<&'static str> {
    match props(tag).map(|p| p.end_tag) {
        Some(EndTag::One(t)) => vec![t],
        Some(EndTag::Many(ts)) => ts.to_vec(),
        Some(EndTag::None) | None => Vec::new(),
    }
}

/// `propTermination` as JavaScript would concatenate it (an array joins with `,`).
fn termination_text(props: Option<&Props>) -> Option<String> {
    match props?.end_tag {
        EndTag::None => None,
        EndTag::One(t) => Some(t.to_owned()),
        EndTag::Many(ts) => Some(ts.join(",")),
    }
}

// ---------------------------------------------------------------------------------------
// The object model
// ---------------------------------------------------------------------------------------

pub(super) type Ref = usize;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Val {
    Str(JsStr),
    Ref(Ref),
    Num(f64),
    Bool(bool),
}

/// The property names the parser itself uses; any other name comes from a word attribute.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Key {
    Open,
    Tag,
    Number,
    Content,
    Text,
    NextChar,
    Type,
    Children,
    Attrib,
    EndTag,
    Nesting,
    Usfm3Milestone,
    EndMarkerChar,
    Other(JsStr),
}

impl Key {
    fn named(name: &[u16]) -> Self {
        const NAMES: &[(&str, Key)] = &[
            ("open", Key::Open),
            ("tag", Key::Tag),
            ("number", Key::Number),
            ("content", Key::Content),
            ("text", Key::Text),
            ("nextChar", Key::NextChar),
            ("type", Key::Type),
            ("children", Key::Children),
            ("attrib", Key::Attrib),
            ("endTag", Key::EndTag),
            ("nesting", Key::Nesting),
            ("usfm3Milestone", Key::Usfm3Milestone),
            ("endMarkerChar", Key::EndMarkerChar),
        ];
        NAMES
            .iter()
            .find(|(n, _)| eq(name, n))
            .map_or_else(|| Key::Other(name.to_vec()), |(_, k)| k.clone())
    }
}

#[derive(Debug, Default)]
pub(super) struct Item {
    /// A map, so a word with thousands of attributes is read in time linear in its size.
    props: BTreeMap<Key, Val>,
    /// `Some` for an array.
    elems: Option<Vec<Ref>>,
}

#[derive(Debug, Default)]
pub(super) struct Heap {
    items: Vec<Item>,
}

impl Heap {
    fn obj(&mut self) -> Ref {
        self.items.push(Item::default());
        self.items.len() - 1
    }

    fn arr(&mut self) -> Ref {
        self.items.push(Item {
            props: BTreeMap::new(),
            elems: Some(Vec::new()),
        });
        self.items.len() - 1
    }

    pub(super) fn get(&self, r: Ref, key: &Key) -> Option<&Val> {
        self.items[r].props.get(key)
    }

    /// The property when it holds a string.
    pub(super) fn s(&self, r: Ref, key: &Key) -> Option<&JsStr> {
        match self.get(r, key) {
            Some(Val::Str(s)) => Some(s),
            _ => None,
        }
    }

    pub(super) fn truthy(&self, r: Ref, key: &Key) -> bool {
        self.get(r, key).is_some_and(truthy)
    }

    fn set(&mut self, r: Ref, key: Key, value: Val) {
        self.items[r].props.insert(key, value);
    }

    fn set_s(&mut self, r: Ref, key: Key, value: JsStr) {
        self.set(r, key, Val::Str(value));
    }

    fn del(&mut self, r: Ref, key: &Key) {
        self.items[r].props.remove(key);
    }

    /// `object[key] = String(object[key]) + suffix`, extending a string in place: the
    /// library builds long texts one piece at a time, and copying the text for each piece
    /// would take time quadratic in its length.
    fn append(&mut self, r: Ref, key: Key, suffix: &[u16]) {
        if let Some(Val::Str(text)) = self.items[r].props.get_mut(&key) {
            text.extend_from_slice(suffix);
            return;
        }
        let mut text = self.string_of(self.get(r, &key));
        text.extend_from_slice(suffix);
        self.set_s(r, key, text);
    }

    /// `object[key] = (object[key] || "") + suffix`, in place.
    fn append_to_truthy(&mut self, r: Ref, key: Key, suffix: &[u16]) {
        if self.truthy(r, &key) {
            self.append(r, key, suffix);
        } else {
            self.set_s(r, key, suffix.to_vec());
        }
    }

    /// Drops the last code unit of the object's text.
    fn truncate_text(&mut self, r: Ref) {
        if let Some(Val::Str(text)) = self.items[r].props.get_mut(&Key::Text) {
            text.pop();
            return;
        }
        let mut text = self.string_of(self.get(r, &Key::Text));
        text.pop();
        self.set_s(r, Key::Text, text);
    }

    /// `String(object[key])`, borrowed when it is a string.
    fn text_of(&self, r: Ref, key: &Key) -> Cow<'_, [u16]> {
        match self.get(r, key) {
            Some(Val::Str(text)) => Cow::Borrowed(text.as_slice()),
            other => Cow::Owned(self.string_of(other)),
        }
    }

    /// The elements of an array; a non-array has none.
    pub(super) fn elems(&self, r: Ref) -> &[Ref] {
        self.items[r].elems.as_deref().unwrap_or(&[])
    }

    fn is_array(&self, r: Ref) -> bool {
        self.items[r].elems.is_some()
    }

    fn push(&mut self, array: Ref, item: Ref) {
        if let Some(elems) = self.items[array].elems.as_mut() {
            elems.push(item);
        }
    }

    fn pop(&mut self, array: Ref) {
        if let Some(elems) = self.items[array].elems.as_mut() {
            elems.pop();
        }
    }

    fn last(&self, array: Ref) -> Option<Ref> {
        self.elems(array).last().copied()
    }

    /// The tag of an object, when it is a string.
    fn tag(&self, r: Ref) -> Option<&[u16]> {
        self.s(r, &Key::Tag).map(Vec::as_slice)
    }

    /// `String(value)` for concatenation; `undefined` for a missing property.
    pub(super) fn string_of(&self, value: Option<&Val>) -> JsStr {
        match value {
            None => js("undefined"),
            Some(Val::Str(s)) => s.clone(),
            Some(Val::Num(n)) => number_to_string(*n),
            Some(Val::Bool(b)) => js(if *b { "true" } else { "false" }),
            Some(Val::Ref(r)) if self.is_array(*r) => {
                let parts: Vec<JsStr> = self
                    .elems(*r)
                    .iter()
                    .map(|_| js("[object Object]"))
                    .collect();
                let mut out = Vec::new();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        out.push(u16::from(b','));
                    }
                    out.extend_from_slice(part);
                }
                out
            }
            Some(Val::Ref(_)) => js("[object Object]"),
        }
    }

    /// `a || b` over two properties: the first truthy one, else the second as it is.
    fn or(&self, r: Ref, a: &Key, b: &Key) -> Option<Val> {
        match self.get(r, a) {
            Some(v) if truthy(v) => Some(v.clone()),
            _ => self.get(r, b).cloned(),
        }
    }
}

pub(super) fn truthy(value: &Val) -> bool {
    match value {
        Val::Str(s) => !s.is_empty(),
        Val::Ref(_) => true,
        Val::Num(n) => *n != 0.0 && !n.is_nan(),
        Val::Bool(b) => *b,
    }
}

/// A local that holds a string or `undefined`.
type Opt = Option<JsStr>;

fn opt_from(value: Option<Val>, heap: &Heap) -> Opt {
    match value {
        None => None,
        Some(Val::Str(s)) => Some(s),
        Some(other) => Some(heap.string_of(Some(&other))),
    }
}

fn opt_truthy_str(value: &Opt) -> bool {
    value.as_ref().is_some_and(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------------------
// Lines
// ---------------------------------------------------------------------------------------

/// One match of `/([^\\]+)?\\(\+?\w+\s*\d*)(?!\w)\s*([^\\]+)?(\\\w\*)?/g`, as ranges of the
/// line.
#[derive(Debug, Clone)]
struct LineMatch {
    index: usize,
    end: usize,
    orphan: Option<(usize, usize)>,
    open: (usize, usize),
    content: Option<(usize, usize)>,
    close: Option<(usize, usize)>,
    next_char: Option<u16>,
}

/// The marker regex at the backslash `q`: the end of group 2, or `None`.
fn marker_at(line: &[u16], q: usize) -> Option<usize> {
    let n = line.len();
    let mut r = q + 1;
    if line.get(r) == Some(&u16::from(b'+')) {
        r += 1;
    }
    let mut a = r;
    while a < n && is_word(line[a]) {
        a += 1;
    }
    if a == r {
        return None;
    }
    // `\w+` keeps its longest run: right after it comes no `\w`, so the lookahead can
    // always be met by giving back part of `\s*\d*`, never part of `\w+`.
    let mut b = a;
    while b < n && is_space(line[b]) {
        b += 1;
    }
    if b == a {
        return Some(a);
    }
    let mut c = b;
    while c < n && is_digit(line[c]) {
        c += 1;
    }
    if c < n && is_word(line[c]) {
        Some(b - 1)
    } else {
        Some(c)
    }
}

fn line_matches(line: &[u16], last_line: bool) -> Vec<LineMatch> {
    let n = line.len();
    let backslash = u16::from(b'\\');
    let mut out = Vec::new();
    let mut p = 0;
    while p < n {
        let Some(q) = (p..n).find(|at| line[*at] == backslash) else {
            break;
        };
        let Some(open_end) = marker_at(line, q) else {
            // Every start between p and q meets this backslash first and fails the same way.
            p = q + 1;
            continue;
        };
        let mut at = open_end;
        while at < n && is_space(line[at]) {
            at += 1;
        }
        let content_start = at;
        while at < n && line[at] != backslash {
            at += 1;
        }
        let content = (at > content_start).then_some((content_start, at));
        let close = (at + 2 < n
            && line[at] == backslash
            && is_word(line[at + 1])
            && line[at + 2] == u16::from(b'*'))
        .then_some((at, at + 3));
        let end = close.map_or(at, |(_, e)| e);
        let next_char = if !last_line && end >= n {
            Some(u16::from(b'\n'))
        } else if line.get(end) == Some(&u16::from(b' ')) {
            Some(u16::from(b' '))
        } else {
            None
        };
        out.push(LineMatch {
            index: p,
            end,
            orphan: (q > p).then_some((p, q)),
            open: (q + 1, open_end),
            content,
            close,
            next_char,
        });
        p = end;
    }
    out
}

/// `parseMarkerOpen`: the tag (`\+?\w+`) and the number after it (`\s*(\d*)`).
fn parse_marker_open(open: &[u16]) -> (JsStr, JsStr) {
    let mut a = usize::from(open.first() == Some(&u16::from(b'+')));
    while a < open.len() && is_word(open[a]) {
        a += 1;
    }
    let mut b = a;
    while b < open.len() && is_space(open[b]) {
        b += 1;
    }
    let mut c = b;
    while c < open.len() && is_digit(open[c]) {
        c += 1;
    }
    (open[..a].to_vec(), open[b..c].to_vec())
}

fn remove_leading_space(text: &[u16]) -> &[u16] {
    if text.len() > 1 && text[0] == u16::from(b' ') {
        &text[1..]
    } else {
        text
    }
}

fn text_marker(heap: &mut Heap, content: JsStr) -> Ref {
    let r = heap.obj();
    heap.set_s(r, Key::Content, content);
    r
}

/// `parseLine`.
fn parse_line(heap: &mut Heap, line: &[u16], last_line: bool) -> Vec<Ref> {
    let mut out = Vec::new();
    let newline = if last_line { js("") } else { js("\n") };
    if trim(line).is_empty() {
        if !last_line {
            out.push(text_marker(heap, concat(&[line, &newline])));
        }
        return out;
    }
    let matches = line_matches(line, last_line);
    if let Some(first) = matches.first()
        && first.index > 0
    {
        let leading = substr(line, 0, Some(first.index as isize - 1)).to_vec();
        out.push(text_marker(heap, leading));
    }
    if matches.is_empty() {
        out.push(text_marker(heap, concat(&[line, &newline])));
        return out;
    }
    let mut last_object: Option<Ref> = None;
    for m in &matches {
        let mut m0_start = m.index;
        if let Some((a, b)) = m.orphan {
            let orphan = text_marker(heap, line[a..b].to_vec());
            out.push(orphan);
            m0_start = b;
        }
        let m0 = &line[m0_start..m.end];
        let group2 = &line[m.open.0..m.open.1];
        let open = trim(group2).to_vec();
        let content: Opt = m.content.map(|(a, b)| line[a..b].to_vec());
        let close: Opt = m.close.map(|(a, b)| trim(&line[a..b]).to_vec());
        let (tag, number) = parse_marker_open(&open);
        let mut obj = heap.obj();
        heap.set_s(obj, Key::Open, open.clone());
        heap.set_s(obj, Key::Tag, tag.clone());
        heap.set_s(obj, Key::Number, number.clone());
        if let Some(content) = &content {
            heap.set_s(obj, Key::Content, content.clone());
        }
        let white_space_in_open = open.as_slice() != group2;
        if white_space_in_open && number.is_empty() {
            let mut should_match = concat(&[&js("\\"), &open]);
            if let Some(c) = content.as_ref().filter(|c| !c.is_empty()) {
                should_match = concat(&[&should_match, &js(" "), c]);
            }
            if remove_leading_space(m0) != should_match.as_slice() {
                let run_to_end = m.end >= line.len();
                let mut start_pos = open.len() as isize + 2;
                let mut end_pos: isize = match &content {
                    Some(c) => index_of(m0, c, start_pos as usize).map_or(-1, |at| at as isize),
                    // `indexOf(undefined)` looks for the text "undefined", which a match
                    // without content cannot hold after its marker.
                    None => -1,
                };
                if end_pos < 0 {
                    if !run_to_end {
                        if m0 == concat(&[&js("\\"), group2]).as_slice() {
                            heap.set_s(obj, Key::NextChar, js(" "));
                        }
                    } else {
                        end_pos = start_pos;
                        start_pos -= 1;
                    }
                }
                if end_pos >= 0 {
                    let prefix = substring(m0, start_pos, end_pos);
                    if !prefix.is_empty() {
                        let rest = content.clone().unwrap_or_default();
                        heap.set_s(obj, Key::Content, concat(&[prefix, &rest]));
                    }
                }
            }
        }
        if !number.is_empty() && !supports_numbers(Some(&tag)) {
            heap.del(obj, &Key::Number);
            let new_content = match index_of(m0, &tag, 0) {
                Some(tag_pos) => substr(m0, (tag_pos + tag.len() + 1) as isize, None).to_vec(),
                None => concat(&[&number, &js(" "), &content.clone().unwrap_or_default()]),
            };
            heap.set_s(obj, Key::Content, new_content);
        }
        if let Some(close) = close.filter(|c| !c.is_empty()) {
            if let Some(content) = heap.s(obj, &Key::Content).filter(|c| !c.is_empty())
                && let Some(pos) = last_index_of(content, &close)
            {
                let cut = content[..pos].to_vec();
                heap.set_s(obj, Key::Content, cut);
            }
            out.push(obj);
            let close_tag = substr(&close, 1, None).to_vec();
            obj = heap.obj();
            heap.set_s(obj, Key::Open, close_tag.clone());
            heap.set_s(obj, Key::Tag, close_tag);
        }
        if let Some(next) = m.next_char {
            heap.set_s(obj, Key::NextChar, vec![next]);
        }
        out.push(obj);
        last_object = Some(obj);
    }
    if let Some(last) = matches.last()
        && last.end < line.len()
    {
        let mut orphan_text = concat(&[substr(line, last.end as isize, None), &newline]);
        if last_object.is_some_and(|o| heap.s(o, &Key::NextChar).is_some_and(|c| eq(c, " "))) {
            orphan_text = substr(&orphan_text, 1, None).to_vec();
        }
        out.push(text_marker(heap, orphan_text));
    }
    out
}

// ---------------------------------------------------------------------------------------
// Words and attributes
// ---------------------------------------------------------------------------------------

/// One match of `/([x-]*)([\w-]+)=['"](.*?)['"]/g`: prefix, key and value.
fn attribute_matches(text: &[u16]) -> Vec<(JsStr, JsStr, JsStr)> {
    let n = text.len();
    let is_key = |u: u16| is_word(u) || u == u16::from(b'-');
    let is_quote = |u: u16| u == u16::from(b'"') || u == u16::from(b'\'');
    let mut out = Vec::new();
    let mut p = 0;
    while p < n {
        let mut e = p;
        while e < n && is_key(text[e]) {
            e += 1;
        }
        if e == p {
            p += 1;
            continue;
        }
        let value_start = e + 2;
        let found = (e + 1 < n && text[e] == u16::from(b'=') && is_quote(text[e + 1]))
            .then(|| {
                (value_start..n)
                    .take_while(|at| !is_line_terminator(text[*at]) || is_quote(text[*at]))
                    .find(|at| is_quote(text[*at]))
            })
            .flatten();
        let Some(value_end) = found else {
            // Every start inside this run of key characters ends the run at `e` and fails
            // there the same way.
            p = e + 1;
            continue;
        };
        let run = &text[p..e];
        let x_run = run
            .iter()
            .take_while(|u| **u == u16::from(b'x') || **u == u16::from(b'-'))
            .count()
            .min(run.len() - 1);
        out.push((
            run[..x_run].to_vec(),
            run[x_run..].to_vec(),
            text[value_start..value_end].to_vec(),
        ));
        p = value_end + 1;
    }
    out
}

/// `parseWord`. `remove_prefix_of_x` is `wordSpecialAttributes` for `\w` and `null` for
/// milestones.
fn parse_word(heap: &mut Heap, word_content: Option<&[u16]>, word_attributes: bool) -> Ref {
    let word_content = word_content.unwrap_or(&[]);
    let parts = split(word_content, &js("|"));
    let word = remove_leading_space(parts[0]).to_vec();
    let object = heap.obj();
    heap.set_s(object, Key::Text, word);
    heap.set_s(object, Key::Tag, js("w"));
    heap.set_s(object, Key::Type, js("word"));
    let Some(attribute_content) = parts.get(1).filter(|a| !a.is_empty()) else {
        return object;
    };
    let matches = attribute_matches(attribute_content);
    for (x_prefix, key, value) in &matches {
        let mut key = key.clone();
        if !x_prefix.is_empty()
            && word_attributes
            && !WORD_SPECIAL_ATTRIBUTES.iter().any(|a| eq(&key, a))
        {
            key = concat(&[x_prefix, &key]);
        }
        if eq(&key, "strongs") || eq(&key, "x-strong") {
            key = js("strong");
        }
        set_attribute(heap, object, &key, value.clone());
    }
    if matches.is_empty() {
        set_attribute(heap, object, attribute_content, js(""));
    }
    object
}

/// `object[key] = value`. Assigning a string to `__proto__` does nothing in JavaScript.
fn set_attribute(heap: &mut Heap, object: Ref, key: &[u16], value: JsStr) {
    if eq(key, "__proto__") {
        return;
    }
    heap.set_s(object, Key::named(key), value);
}

// ---------------------------------------------------------------------------------------
// The parser state
// ---------------------------------------------------------------------------------------

/// `currentChapter` and `currentVerse`: the number 0, or a string.
#[derive(Debug, Clone, PartialEq)]
enum Pos {
    Zero,
    Str(JsStr),
}

impl Pos {
    fn truthy(&self) -> bool {
        matches!(self, Pos::Str(s) if !s.is_empty())
    }

    fn above_zero(&self) -> bool {
        match self {
            Pos::Zero => false,
            Pos::Str(s) => to_number(s) > 0.0,
        }
    }
}

/// What `content` or `marker` holds in the functions that take either.
#[derive(Debug, Clone)]
enum Thing {
    Text(JsStr),
    Obj(Ref),
}

struct State {
    heap: Heap,
    current_chapter: Pos,
    current_verse: Pos,
    verses: Vec<(JsStr, Ref)>,
    headers: Ref,
    phrase: Option<Vec<Ref>>,
    phrase_parent: Option<Ref>,
    on_same_chapter: bool,
    in_header: bool,
    new_align_format: bool,
    /// Set where the library throws a TypeError; the parse stops there.
    thrown: bool,
}

impl State {
    fn verse_array(&self, key: &[u16]) -> Option<Ref> {
        self.verses
            .iter()
            .find(|(k, _)| k.as_slice() == key)
            .map(|(_, r)| *r)
    }

    fn get_last_phrase(&self) -> Option<Ref> {
        self.phrase.as_ref().and_then(|p| p.last().copied())
    }

    fn get_save_to_location(&mut self) -> Ref {
        if let Some(phrase) = self.get_last_phrase() {
            return phrase;
        }
        if let Pos::Str(verse) = &self.current_verse
            && !verse.is_empty()
        {
            let verse = verse.clone();
            return match self.verse_array(&verse) {
                Some(r) => r,
                None => {
                    let r = self.heap.arr();
                    self.verses.push((verse, r));
                    r
                }
            };
        }
        self.headers
    }

    /// `createUsfmObject`.
    fn create_usfm_object(&mut self, marker: Thing, no_next: bool) -> Ref {
        let output = match marker {
            Thing::Text(text) => {
                let r = self.heap.obj();
                self.heap.set_s(r, Key::Type, js("text"));
                self.heap.set_s(r, Key::Text, text);
                return r;
            }
            Thing::Obj(r) => r,
        };
        let heap = &mut self.heap;
        let tag = heap.tag(output).map(<[u16]>::to_vec);
        let mut content = opt_from(heap.or(output, &Key::Content, &Key::Text), heap);
        let tag_props = props(tag.as_deref());
        let mut ty = tag_props.and_then(|p| p.ty);
        let mut is_text = true;
        match tag.as_deref().filter(|t| !t.is_empty()) {
            Some(tag) => {
                is_text = tag_props.is_some_and(|p| p.display);
                if ty == Some("milestone") && index_of(tag, &js("-s"), 0).is_none() {
                    ty = Some("");
                }
                if let Some(ty) = ty.filter(|t| !t.is_empty()) {
                    heap.set_s(output, Key::Type, js(ty));
                }
            }
            None => heap.set_s(output, Key::Type, js("text")),
        }
        if heap.truthy(output, &Key::Number) {
            if !supports_numbers(tag.as_deref()) {
                let mut new_content = heap.string_of(heap.get(output, &Key::Number));
                if let Some(c) = content.as_ref().filter(|c| !c.is_empty()) {
                    new_content = concat(&[&new_content, &js(" "), c]);
                }
                content = Some(new_content);
                heap.del(output, &Key::Number);
            }
        } else {
            heap.del(output, &Key::Number);
        }
        if no_next {
            heap.del(output, &Key::NextChar);
        } else if heap.truthy(output, &Key::NextChar) && opt_truthy_str(&content) {
            let next = heap.string_of(heap.get(output, &Key::NextChar));
            content = content.map(|c| concat(&[&c, &next]));
            heap.del(output, &Key::NextChar);
        }
        heap.del(output, &Key::Content);
        heap.del(output, &Key::Text);
        if let Some(c) = content.filter(|c| !c.is_empty()) {
            heap.set_s(output, if is_text { Key::Text } else { Key::Content }, c);
        }
        heap.del(output, &Key::Open);
        output
    }

    /// `pushObject`. `save_to` is an array, or `None` for "where content goes now".
    fn push_object(&mut self, save_to: Option<Ref>, object: Thing) {
        let mut save_to = save_to;
        if save_to.is_none() {
            // `state.nested` is never filled in this version of the library.
            if let Some(phrase) = self.get_last_phrase() {
                save_to = Some(phrase);
            }
        }
        let object = match object {
            Thing::Text(text) if text.is_empty() => return,
            other => self.create_usfm_object_if_text(other),
        };
        let save_to = match save_to {
            Some(r) => r,
            None => self.get_save_to_location(),
        };
        let heap = &mut self.heap;
        if heap.s(object, &Key::Type).is_some_and(|t| eq(t, "text"))
            && let Some(last) = heap.last(save_to)
            && heap.s(last, &Key::Type).is_some_and(|t| eq(t, "text"))
        {
            let text = heap.text_of(object, &Key::Text).into_owned();
            heap.append(last, Key::Text, &text);
            return;
        }
        heap.push(save_to, object);
    }

    fn create_usfm_object_if_text(&mut self, object: Thing) -> Ref {
        match object {
            Thing::Text(text) => self.create_usfm_object(Thing::Text(text), false),
            Thing::Obj(r) => r,
        }
    }

    /// `removeLastNewLine`.
    fn remove_last_new_line(&mut self, current_tag: Option<&[u16]>, end_tag: bool) {
        let save_to = self.get_save_to_location();
        let mut replace_with_space = false;
        let mut last_object = self.heap.last(save_to);
        if let Some(last) = last_object {
            if self
                .heap
                .s(last, &Key::NextChar)
                .is_some_and(|c| eq(c, "\n"))
            {
                if !self.new_align_format {
                    self.heap.del(last, &Key::NextChar);
                    if crammed_markers(&self.heap, Some(last), current_tag) {
                        replace_with_space = true;
                    }
                }
            } else if self.heap.truthy(last, &Key::Text) {
                let (new_line, one, space_and_new_line) = {
                    let text = self.heap.text_of(last, &Key::Text);
                    (
                        is_last_char_new_line(&text),
                        text.len() == 1,
                        text.len() == 2 && text[0] == u16::from(b' '),
                    )
                };
                if new_line {
                    if one {
                        self.heap.pop(save_to);
                        last_object = self.heap.last(save_to);
                    } else {
                        let is_space_only = space_and_new_line;
                        self.heap.truncate_text(last);
                        if !self.new_align_format
                            && is_space_only
                            && displayable(self.heap.tag(last))
                        {
                            self.heap.set_s(last, Key::NextChar, js(" "));
                            self.heap.del(last, &Key::Text);
                        }
                    }
                    replace_with_space = self.new_align_format
                        || (!end_tag && crammed_markers(&self.heap, last_object, current_tag));
                }
            }
        }
        if replace_with_space {
            self.push_object(None, Thing::Text(js(" ")));
        }
    }

    /// `handleWordWhiteSpace`.
    fn handle_word_white_space(&mut self) {
        let save_to = self.get_save_to_location();
        let elems = self.heap.elems(save_to).len();
        let Some(last) = self.heap.last(save_to) else {
            return;
        };
        if self
            .heap
            .s(last, &Key::NextChar)
            .is_some_and(|c| eq(c, "\n"))
        {
            if !self.new_align_format {
                self.heap.set_s(last, Key::NextChar, js(" "));
            }
        } else if self.heap.truthy(last, &Key::Text) {
            let (new_line, one, after_quote) = {
                let text = self.heap.text_of(last, &Key::Text);
                (
                    is_last_char_new_line(&text),
                    text.len() == 1,
                    is_next_to_last_char_quote(&text),
                )
            };
            if new_line {
                let start_of_line = elems == 1 && one;
                let remove_new_line = start_of_line || after_quote;
                if self.new_align_format {
                    if self.heap.s(last, &Key::Type).is_some_and(|t| eq(t, "text")) {
                        self.heap.truncate_text(last);
                        self.heap.append(last, Key::Text, &js(" "));
                    }
                } else if remove_new_line {
                    if one {
                        self.heap.pop(save_to);
                    } else {
                        self.heap.truncate_text(last);
                    }
                } else {
                    self.heap.truncate_text(last);
                    self.heap.append(last, Key::Text, &js(" "));
                }
            }
        }
    }

    /// `checkForEndMarker`: the end marker and whether the marker spans.
    fn check_for_end_marker(&mut self, marker: Ref) -> (Option<JsStr>, bool) {
        let heap = &mut self.heap;
        let mut spanned = false;
        let mut end_marker: Option<JsStr> = None;
        let mut content = heap.s(marker, &Key::Content).cloned().unwrap_or_default();
        let initial_tag = heap.tag(marker).map(<[u16]>::to_vec).unwrap_or_default();
        let mut base_tag = initial_tag.clone();
        if ends_with(&base_tag, "*") {
            base_tag.pop();
            end_marker = Some(initial_tag.clone());
        } else if starts_with(&content, "-") {
            let next = substr(&content, 1, Some(1)).to_vec();
            if eq(&next, "s") || eq(&next, "e") {
                let tag = concat(&[&initial_tag, substr(&content, 0, Some(2))]);
                heap.set_s(marker, Key::Tag, tag.clone());
                end_marker = eq(&next, "e").then_some(tag);
                base_tag = concat(&[&base_tag, &js("-s")]);
                content = substr(&content, 2, None).to_vec();
                heap.set_s(marker, Key::Content, content.clone());
            }
        } else if starts_with(&content, "*") {
            let mut trim_len = 1;
            let mut space = false;
            if eq(substr(&content, 1, Some(1)), " ") {
                trim_len += 1;
                space = true;
            }
            let tag = concat(&[&initial_tag, &js("*")]);
            heap.set_s(marker, Key::Tag, tag.clone());
            end_marker = Some(tag);
            content = substr(&content, trim_len, None).to_vec();
            if !content.is_empty() {
                if heap.truthy(marker, &Key::NextChar) {
                    let next = heap.string_of(heap.get(marker, &Key::NextChar));
                    content.extend_from_slice(&next);
                }
                heap.del(marker, &Key::NextChar);
            }
            if space {
                if !content.is_empty() {
                    heap.set_s(marker, Key::EndMarkerChar, js(" "));
                } else {
                    content = js(" ");
                }
            }
            heap.set_s(marker, Key::Content, content);
        }
        if end_marker.is_some() {
            spanned = true;
        } else {
            let tag_props = props(Some(&base_tag));
            if tag_props.is_some_and(|p| p.standalone) {
                end_marker = heap.tag(marker).map(<[u16]>::to_vec);
                spanned = true;
            } else if let Some(termination) = termination_text(tag_props) {
                spanned = true;
                let joined = concat(&[&initial_tag, &js(&termination)]);
                if heap.tag(marker) == Some(joined.as_slice()) {
                    end_marker = heap.tag(marker).map(<[u16]>::to_vec);
                }
            }
        }
        (end_marker, spanned)
    }

    fn is_non_displayable_phrase_parent(&self) -> bool {
        self.get_phrase_parent()
            .is_some_and(|parent| !displayable(self.heap.tag(parent)))
    }

    /// `saveUsfmObject`.
    fn save_usfm_object(&mut self, marker: Thing) {
        if let Some(parent) = self.get_phrase_parent() {
            if !displayable(self.heap.tag(parent)) {
                let object_text = match &marker {
                    Thing::Text(t) => Some(t.clone()),
                    Thing::Obj(r) => marker_to_text(&self.heap, *r),
                };
                self.append_content(parent, object_text);
            } else if self.heap.truthy(parent, &Key::Attrib)
                && !self.heap.truthy(parent, &Key::Usfm3Milestone)
                && matches!(marker, Thing::Text(_))
            {
                if let Thing::Text(text) = marker {
                    self.heap.append(parent, Key::Attrib, &text);
                }
            } else {
                let save_to = self.get_last_phrase();
                let object = self.create_usfm_object(marker, false);
                match save_to {
                    Some(save_to) => self.heap.push(save_to, object),
                    // A phrase parent set without a phrase stack: `null.push` throws.
                    None => self.thrown = true,
                }
            }
        } else {
            let save_to = self.get_save_to_location();
            let object = match marker {
                Thing::Obj(r) => r,
                Thing::Text(t) => {
                    // Only objects reach this branch in the library; a string is pushed
                    // as it is, and a string in a verse array has no `text`.
                    let r = self.heap.obj();
                    self.heap.set_s(r, Key::Other(js("__string")), t);
                    r
                }
            };
            self.heap.push(save_to, object);
        }
    }

    /// `(parent.content || "") + text`, with JavaScript's `undefined`.
    fn append_content(&mut self, parent: Ref, text: Opt) {
        let text = text.unwrap_or_else(|| js("undefined"));
        self.heap.append_to_truthy(parent, Key::Content, &text);
    }

    /// `incrementPhraseNesting`.
    fn increment_phrase_nesting(&mut self, parent: Option<Ref>, tag: &[u16]) -> bool {
        let Some(parent) = parent else {
            return false;
        };
        if self.heap.tag(parent) != Some(tag) {
            return false;
        }
        if !self.heap.truthy(parent, &Key::Nesting) {
            self.heap.set(parent, Key::Nesting, Val::Num(0.0));
        }
        let n = number_of(&self.heap, self.heap.get(parent, &Key::Nesting));
        self.heap.set(parent, Key::Nesting, Val::Num(n + 1.0));
        true
    }

    /// `decrementPhraseNesting`: whether the end tag closes the parent, and the count.
    fn decrement_phrase_nesting(&mut self, parent: Option<Ref>, end_tag: &[u16]) -> (bool, f64) {
        let parts = split(end_tag, &js(" "));
        let end_tag_base = if parts.len() > 1 {
            concat(&[parts[0], &js("\\*")])
        } else {
            parts[0].to_vec()
        };
        let mut matches_parent = false;
        let mut count = -1.0;
        let Some(parent) = parent else {
            return (matches_parent, count);
        };
        let parent_tag = self.heap.tag(parent).map(<[u16]>::to_vec);
        for termination in terminations(parent_tag.as_deref()) {
            if termination.is_empty() {
                continue;
            }
            let termination = js(termination);
            let tag_text = parent_tag.clone().unwrap_or_else(|| js("undefined"));
            let short = substr(&tag_text, 0, Some(tag_text.len() as isize - 2)).to_vec();
            matches_parent = termination == end_tag_base
                || concat(&[&tag_text, &termination]) == end_tag_base
                || concat(&[&short, &termination, &js("\\*")]) == end_tag_base;
            if matches_parent {
                break;
            }
        }
        if !matches_parent && special_end_tag(&end_tag_base).map(js) == parent_tag {
            matches_parent = true;
        }
        if matches_parent {
            // `count = nesting || 0; if (count) nesting = --count;` decrements any truthy
            // value, the string "0" included.
            count = match self.heap.get(parent, &Key::Nesting) {
                Some(v) if truthy(v) => {
                    let n = number_of(&self.heap, Some(v)) - 1.0;
                    self.heap.set(parent, Key::Nesting, Val::Num(n));
                    n
                }
                _ => 0.0,
            };
            if count == 0.0 || count.is_nan() {
                self.heap.del(parent, &Key::Nesting);
            }
            self.heap.del(parent, &Key::Usfm3Milestone);
        }
        (matches_parent, count)
    }

    /// `getLastItem`: the last item where content goes now, or that (empty) array itself.
    fn get_last_item(&mut self) -> Ref {
        let last = self.get_save_to_location();
        self.heap.last(last).unwrap_or(last)
    }

    /// `startSpan`.
    fn start_span(&mut self, marker: Ref, tag: &[u16], mut index: usize, markers: &[Ref]) -> usize {
        self.heap.set_s(marker, Key::Tag, tag.to_vec());
        let phrase_parent = self.get_phrase_parent();
        let tag_props = props(Some(tag));
        let is_displayable = tag_props.is_some_and(|p| p.display);
        if tag_props.is_some_and(|p| p.usfm3_milestone) {
            self.heap.set(marker, Key::Usfm3Milestone, Val::Bool(true));
        }
        if tag_props.is_some_and(|p| p.attrib) {
            let content_key = if is_displayable {
                Key::Text
            } else {
                Key::Content
            };
            let mut content: Opt =
                opt_from(self.heap.get(marker, &content_key).cloned(), &self.heap);
            if let Some(mut text) = content.clone().filter(|c| !c.is_empty()) {
                if let Some(mut pos) = index_of(&text, &js("|"), 0) {
                    let (joined, new_index) =
                        get_to_end_of_attributes(&mut self.heap, text, pos, index, markers);
                    text = joined;
                    index = new_index;
                    if trim(substr(&text, 0, Some(pos as isize))).is_empty() {
                        pos = 0;
                    }
                    self.heap.set_s(
                        marker,
                        Key::Attrib,
                        substr(&text, pos as isize, None).to_vec(),
                    );
                    text = substr(&text, 0, Some(pos as isize)).to_vec();
                }
                if !text.is_empty() {
                    self.heap.set_s(marker, content_key.clone(), text.clone());
                }
                content = Some(text);
            }
            if !opt_truthy_str(&content) {
                self.heap.del(marker, &content_key);
            }
        }
        if let Some(parent) = phrase_parent
            && !displayable(self.heap.tag(parent))
        {
            let text = marker_to_text(&self.heap, marker);
            self.append_content(parent, text);
            self.increment_phrase_nesting(Some(parent), tag);
            return index;
        }
        if is_displayable {
            self.push_object(None, Thing::Obj(marker));
            if self.phrase.is_none() {
                self.phrase = Some(Vec::new());
                self.phrase_parent = Some(marker);
            }
            let array = self.heap.arr();
            if let Some(phrase) = self.phrase.as_mut() {
                phrase.push(array);
            }
            self.heap.set(marker, Key::Children, Val::Ref(array));
        } else {
            self.save_usfm_object(Thing::Obj(marker));
            if self.phrase.is_none() {
                self.phrase_parent = Some(self.get_last_item());
            }
            self.increment_phrase_nesting(Some(marker), tag);
        }
        index
    }

    /// `getPhraseParent`.
    fn get_phrase_parent(&self) -> Option<Ref> {
        if let Some(phrase) = &self.phrase
            && phrase.len() > 1
        {
            let parent = phrase[phrase.len() - 2];
            if let Some(last) = self.heap.last(parent) {
                return Some(last);
            }
        }
        self.phrase_parent
    }

    /// `popPhrase`.
    fn pop_phrase(&mut self) -> Option<Ref> {
        match self.phrase.as_mut() {
            Some(phrase) if !phrase.is_empty() => {
                phrase.pop();
                if phrase.is_empty() {
                    self.phrase = None;
                    self.phrase_parent.take()
                } else {
                    self.get_phrase_parent()
                }
            }
            _ => {
                self.phrase_parent = None;
                None
            }
        }
    }

    /// `endSpan`.
    fn end_span(&mut self, mut index: usize, markers: &[Ref], end_marker: &[u16]) -> usize {
        let current = markers[index];
        let mut end_marker = end_marker.to_vec();
        let mut content: Opt = self.heap.s(current, &Key::Content).cloned();
        let mut phrase_parent = self.get_phrase_parent();
        let parent_props = phrase_parent.and_then(|p| props(self.heap.tag(p)));
        let parent_content_displayable = parent_props.is_some_and(|p| p.display);
        if !end_marker.is_empty() && parent_props.is_some_and(|p| p.usfm3_milestone) {
            end_marker = concat(&[&end_marker, &js("\\*")]);
        }
        if phrase_parent.is_none() || parent_content_displayable {
            self.pop_phrase();
            if let Some(parent) = phrase_parent
                && !end_marker.is_empty()
            {
                if let Some(Val::Ref(children)) = self.heap.get(parent, &Key::Children)
                    && self.heap.elems(*children).is_empty()
                {
                    self.heap.del(parent, &Key::Children);
                } else if let Some(Val::Str(children)) = self.heap.get(parent, &Key::Children)
                    && children.is_empty()
                {
                    self.heap.del(parent, &Key::Children);
                }
                let mut walk = Some(parent);
                while let Some(parent) = walk {
                    let Some(tag) = self.heap.tag(parent).map(<[u16]>::to_vec) else {
                        // `phraseParent.tag.split` on an array that stands as a parent.
                        self.thrown = true;
                        return index;
                    };
                    let tag_base = split(&tag, &js("-"))[0].to_vec();
                    if concat(&[&tag_base, &js("*")]) == end_marker
                        || concat(&[&tag_base, &js("-e\\*")]) == end_marker
                    {
                        self.heap.set_s(parent, Key::EndTag, end_marker.clone());
                        break;
                    }
                    self.heap.set_s(parent, Key::EndTag, Vec::new());
                    walk = self.get_phrase_parent();
                    self.pop_phrase();
                }
                // The library walks with the same variable, so what follows sees where the
                // walk stopped: the closed parent, or nothing.
                phrase_parent = walk;
            }
        }
        let mut check_next = props(self.heap.tag(current)).is_some_and(|p| p.standalone);
        if let Some(text) = content.clone().filter(|c| !c.is_empty()) {
            let next = text[0];
            if [u16::from(b'\\'), u16::from(b'-'), u16::from(b'*')].contains(&next) {
                let mut trim_len: usize = 0;
                if next == u16::from(b'*') {
                    trim_len = 1;
                } else if starts_with(&text, "\\*") {
                    trim_len = 2;
                } else if starts_with(&text, "-e\\*") {
                    trim_len = 4;
                } else if starts_with(&text, "-e*") {
                    trim_len = 3;
                } else if eq(&text, "-e") {
                    trim_len = 2;
                    check_next = true;
                }
                if trim_len > 0 {
                    if eq(substr(&text, trim_len as isize, Some(1)), "\n") {
                        trim_len += 1;
                    }
                    content = Some(substr(&text, trim_len as isize, None).to_vec());
                }
            }
        }
        if opt_truthy_str(&content) && props(self.heap.tag(current)).is_some_and(|p| p.end_attrib) {
            let text = content.clone().unwrap_or_default();
            match phrase_parent {
                Some(parent) => {
                    let end_tag = match self.heap.get(parent, &Key::EndTag) {
                        Some(v) if truthy(v) => self.heap.string_of(Some(v)),
                        _ => Vec::new(),
                    };
                    let parts = split(&end_tag, &js("\\*"));
                    let mut joined = concat(&[parts[0], &text]);
                    if parts.len() > 1 {
                        joined = concat(&[&joined, &js("\\*")]);
                    }
                    end_marker = joined.clone();
                    self.heap.set_s(parent, Key::EndTag, joined);
                }
                None => {
                    self.heap.set_s(current, Key::Attrib, text);
                }
            }
            self.heap.set_s(current, Key::Content, Vec::new());
            content = Some(Vec::new());
        }
        let next_item = markers.get(index + 1).copied();
        if (check_next
            || (!opt_truthy_str(&content)
                && !self.heap.truthy(current, &Key::NextChar)
                && !end_marker.is_empty()))
            && let Some(next_item) = next_item
            && !self.heap.truthy(next_item, &Key::Tag)
        {
            let mut next_content = match self.heap.get(next_item, &Key::Content) {
                Some(v) if truthy(v) => self.heap.string_of(Some(v)),
                _ => Vec::new(),
            };
            let mut trim_len: usize = 0;
            if starts_with(&next_content, "*") {
                trim_len = 1;
            } else if starts_with(&next_content, "\\*") {
                trim_len = 2;
            }
            let mut terminator = substr(&next_content, 0, Some(trim_len as isize)).to_vec();
            if self.heap.truthy(current, &Key::Attrib) {
                if starts_with(&terminator, "\\") {
                    terminator = substr(&terminator, 1, None).to_vec();
                }
                self.heap.set_s(current, Key::EndTag, terminator);
            } else {
                if index_of(&end_marker, &terminator, 0).is_none() {
                    end_marker = concat(&[&end_marker, &terminator]);
                }
                self.heap.set_s(current, Key::Tag, end_marker.clone());
            }
            let next_char = substr(&next_content, trim_len as isize, Some(1)).to_vec();
            if (eq(&next_char, " ") || eq(&next_char, "\n")) && phrase_parent.is_none() {
                trim_len += 1;
                self.heap.set_s(current, Key::NextChar, next_char);
            }
            if trim_len > 0 {
                content = Some(Vec::new());
                next_content = substr(&next_content, trim_len as isize, None).to_vec();
                self.heap
                    .set_s(next_item, Key::Content, next_content.clone());
            }
            if next_content.is_empty() {
                index += 1;
            }
        }
        if self.heap.truthy(current, &Key::NextChar) && opt_truthy_str(&content) {
            let next = self.heap.string_of(self.heap.get(current, &Key::NextChar));
            content = content.map(|c| concat(&[&c, &next]));
            self.heap.del(current, &Key::NextChar);
        }
        let mut content_is_current = false;
        match phrase_parent {
            Some(parent) => {
                let end_marker_text = concat(&[&js("\\"), &end_marker]);
                let (matches_parent, count) =
                    self.decrement_phrase_nesting(Some(parent), &end_marker);
                let finish_phrase = matches_parent && count <= 0.0;
                if !parent_content_displayable {
                    let mut next_char = match self.heap.get(current, &Key::NextChar) {
                        Some(v) if truthy(v) => self.heap.string_of(Some(v)),
                        _ => Vec::new(),
                    };
                    if opt_truthy_str(&content) && !next_char.is_empty() {
                        content = content.map(|c| concat(&[&c, &next_char]));
                        next_char = Vec::new();
                    }
                    if let Some(v) = self.heap.get(current, &Key::EndMarkerChar)
                        && truthy(v)
                    {
                        next_char = concat(&[&next_char, &self.heap.string_of(Some(v))]);
                    }
                    if !finish_phrase {
                        self.heap.append_to_truthy(
                            parent,
                            Key::Content,
                            &concat(&[&end_marker_text, &next_char]),
                        );
                    } else {
                        self.heap.set_s(parent, Key::EndTag, end_marker.clone());
                        if !next_char.is_empty() {
                            self.heap.set_s(parent, Key::NextChar, next_char);
                        }
                        self.pop_phrase();
                    }
                } else if finish_phrase {
                    self.heap.set_s(parent, Key::EndTag, end_marker.clone());
                    let next_char = self
                        .heap
                        .get(current, &Key::NextChar)
                        .filter(|v| truthy(v))
                        .or_else(|| self.heap.get(current, &Key::EndMarkerChar))
                        .cloned();
                    if let Some(next_char) = next_char.filter(truthy) {
                        let next_char = self.heap.string_of(Some(&next_char));
                        content = Some(concat(&[&next_char, &content.clone().unwrap_or_default()]));
                    }
                }
            }
            // No parent: the end marker itself is saved.
            None => content_is_current = true,
        }
        let saved = if content_is_current {
            Some(Thing::Obj(current))
        } else {
            content.filter(|c| !c.is_empty()).map(Thing::Text)
        };
        if let Some(saved) = saved {
            let object = self.create_usfm_object(saved, self.in_header);
            self.save_usfm_object(Thing::Obj(object));
        }
        index
    }

    /// `addToCurrentVerse`.
    fn add_to_current_verse(&mut self, marker: Thing) {
        match marker {
            Thing::Text(text) => {
                let object = self.create_usfm_object(Thing::Text(text), false);
                self.push_object(None, Thing::Obj(object));
            }
            Thing::Obj(r) => {
                if !self.heap.truthy(r, &Key::Tag) {
                    let object = self.create_usfm_object(Thing::Obj(r), false);
                    self.push_object(None, Thing::Obj(object));
                    return;
                }
                let object = self.create_usfm_object(Thing::Obj(r), false);
                self.save_usfm_object(Thing::Obj(object));
            }
        }
    }

    /// `terminatePhrases`.
    fn terminate_phrases(&mut self) {
        let mut parent = self.get_phrase_parent();
        while let Some(p) = parent {
            self.heap.set_s(p, Key::EndTag, Vec::new());
            self.heap.del(p, &Key::Nesting);
            self.heap.del(p, &Key::Usfm3Milestone);
            parent = self.pop_phrase();
        }
    }

    /// `parseAsVerse`.
    fn parse_as_verse(&mut self, marker: Ref) {
        self.in_header = false;
        self.terminate_phrases();
        let mut content = match self.heap.get(marker, &Key::Content) {
            Some(v) if truthy(v) => self.heap.string_of(Some(v)),
            _ => Vec::new(),
        };
        if self
            .heap
            .s(marker, &Key::NextChar)
            .is_some_and(|c| eq(c, "\n"))
        {
            content.push(u16::from(b'\n'));
        }
        self.heap.set_s(marker, Key::Content, content.clone());
        let number = self.heap.string_of(self.heap.get(marker, &Key::Number));
        let mut verse = strip_leading_zeros(&number);
        if let Some(span_len) = verse_span(&content) {
            let span = &content[..span_len];
            verse.push(span[0]);
            verse.extend(strip_leading_zeros(trim(substr(span, 1, None))));
            self.heap
                .set_s(marker, Key::Content, content[span_len..].to_vec());
        }
        self.current_verse = Pos::Str(verse.clone());
        if !self.on_same_chapter {
            if self.verse_array(&verse).is_some() {
                self.on_same_chapter = true;
            } else {
                let r = self.heap.arr();
                self.verses.push((verse, r));
                let content = self
                    .heap
                    .s(marker, &Key::Content)
                    .cloned()
                    .unwrap_or_default();
                self.push_object(None, Thing::Text(content));
            }
        }
    }

    /// `processAsText`.
    fn process_as_text(&mut self, marker: Ref) {
        let content = match self.heap.get(marker, &Key::Content) {
            Some(v) if truthy(v) => Some(self.heap.string_of(Some(v))),
            _ => None,
        };
        if self.current_chapter.above_zero()
            && let Some(content) = content.clone()
        {
            if self.get_phrase_parent().is_some() {
                self.save_usfm_object(Thing::Text(content));
            } else {
                self.add_to_current_verse(Thing::Text(content));
            }
        }
        // The header branch (`currentChapter === 0 && !currentVerse`) cannot be reached;
        // see the module documentation.
        if self.current_verse.above_zero()
            && let Some(content) = content
        {
            if let Pos::Str(verse) = &self.current_verse
                && self.verse_array(verse).is_none()
            {
                let verse = verse.clone();
                let r = self.heap.arr();
                self.verses.push((verse, r));
            }
            if self.get_phrase_parent().is_some() {
                self.save_usfm_object(Thing::Text(content));
            } else {
                let array = match &self.current_verse {
                    Pos::Str(verse) => self.verse_array(verse),
                    Pos::Zero => None,
                };
                self.push_object(array, Thing::Text(content));
            }
        }
    }

    /// `processAsChapter`.
    fn process_as_chapter(&mut self, marker: Ref) {
        self.in_header = false;
        self.terminate_phrases();
        let number = self.heap.string_of(self.heap.get(marker, &Key::Number));
        self.current_chapter = Pos::Str(strip_leading_zeros(&number));
        self.on_same_chapter = false;
        self.current_verse = Pos::Zero;
    }

    /// `processMarkerForSpans`.
    fn process_marker_for_spans(&mut self, marker: Ref, i: usize, markers: &[Ref]) -> usize {
        let (mut end_marker, mut spanned) = self.check_for_end_marker(marker);
        if end_marker.is_none()
            && let Some(start) = self.heap.tag(marker).and_then(special_end_tag)
        {
            end_marker = self.heap.tag(marker).map(<[u16]>::to_vec);
            self.heap.set_s(marker, Key::Tag, js(start));
            spanned = true;
        }
        match end_marker {
            Some(end_marker) => {
                if spanned {
                    return self.end_span(i, markers, &end_marker);
                }
                i
            }
            None if spanned => {
                let object = self.create_usfm_object(Thing::Obj(marker), false);
                let tag = self
                    .heap
                    .tag(marker)
                    .map(<[u16]>::to_vec)
                    .unwrap_or_default();
                self.start_span(object, &tag, i, markers)
            }
            None => {
                self.add_to_current_verse(Thing::Obj(marker));
                i
            }
        }
    }

    /// `processMarker`. The header branch cannot be reached (module documentation).
    fn process_marker(&mut self, marker: Ref, index: usize, markers: &[Ref]) -> usize {
        if self.current_chapter.truthy() || self.current_verse.truthy() {
            return self.process_marker_for_spans(marker, index, markers);
        }
        index
    }
}

fn number_of(heap: &Heap, value: Option<&Val>) -> f64 {
    match value {
        None => f64::NAN,
        Some(Val::Num(n)) => *n,
        Some(Val::Bool(b)) => f64::from(u8::from(*b)),
        Some(Val::Str(s)) => to_number(s),
        Some(v @ Val::Ref(_)) => to_number(&heap.string_of(Some(v))),
    }
}

fn is_last_char_new_line(text: &[u16]) -> bool {
    text.last()
        .is_some_and(|u| *u == u16::from(b'\n') || *u == u16::from(b'\r'))
}

fn is_next_to_last_char_quote(text: &[u16]) -> bool {
    text.len() >= 2 && [u16::from(b'\''), u16::from(b'"'), 0x201C].contains(&text[text.len() - 2])
}

fn crammed_markers(heap: &Heap, last: Option<Ref>, current_tag: Option<&[u16]>) -> bool {
    let Some(last) = last else {
        return false;
    };
    let tag = heap.tag(last);
    tag.is_some_and(|t| eq(t, "k") || eq(t, "w") || eq(t, "zaln")) && tag == current_tag
}

fn strip_leading_zeros(text: &[u16]) -> JsStr {
    let mut at = 0;
    while text.len() - at > 1 && text[at] == u16::from(b'0') {
        at += 1;
    }
    text[at..].to_vec()
}

/// `/(^-\d+\s)/`: the length of a span suffix at the start of a verse's content.
fn verse_span(content: &[u16]) -> Option<usize> {
    if content.first() != Some(&u16::from(b'-')) {
        return None;
    }
    let digits = content[1..].iter().take_while(|u| is_digit(**u)).count();
    (digits > 0 && content.get(1 + digits).is_some_and(|u| is_space(*u))).then_some(digits + 2)
}

/// `markerToText`; `None` is `undefined`.
fn marker_to_text(heap: &Heap, marker: Ref) -> Opt {
    let Some(tag) = heap.get(marker, &Key::Tag).filter(|v| truthy(v)) else {
        return opt_from(heap.or(marker, &Key::Text, &Key::Content), heap);
    };
    let tag = heap.string_of(Some(tag));
    let mut text = concat(&[&js("\\"), &tag]);
    if let Some(number) = heap.get(marker, &Key::Number).filter(|v| truthy(v)) {
        text = concat(&[&text, &js(" "), &heap.string_of(Some(number))]);
    }
    let content = opt_from(heap.or(marker, &Key::Content, &Key::Text), heap);
    if let Some(c) = content.as_ref().filter(|c| !c.is_empty()) {
        text = concat(&[&text, &js(" "), c]);
    }
    if let Some(attrib) = heap.get(marker, &Key::Attrib).filter(|v| truthy(v)) {
        let suffix = match index_of(&tag, &js("-"), 0) {
            Some(dash) if dash > 0 => substr(&tag, dash as isize + 1, Some(1)).to_vec(),
            _ => Vec::new(),
        };
        let spanned_tag = eq(&suffix, "s") || eq(&suffix, "e");
        if !opt_truthy_str(&content) && (suffix.is_empty() || !spanned_tag) {
            text.push(u16::from(b' '));
        }
        text = concat(&[&text, &heap.string_of(Some(attrib))]);
        if spanned_tag {
            text.push(u16::from(b'\\'));
        }
    }
    if let Some(next) = heap.get(marker, &Key::NextChar).filter(|v| truthy(v)) {
        text = concat(&[&text, &heap.string_of(Some(next))]);
    }
    Some(text)
}

/// `getToEndOfAttributes`.
fn get_to_end_of_attributes(
    heap: &mut Heap,
    mut content: JsStr,
    pos: usize,
    mut index: usize,
    markers: &[Ref],
) -> (JsStr, usize) {
    let star = js("*");
    if index_of(&content, &star, pos).is_some() {
        return (content, index);
    }
    loop {
        let next_line = index + 1;
        if next_line >= markers.len() {
            break;
        }
        let next_marker = markers[next_line];
        if heap.truthy(next_marker, &Key::Tag) || !heap.truthy(next_marker, &Key::Content) {
            break;
        }
        let next_content = heap.string_of(heap.get(next_marker, &Key::Content));
        match index_of(&next_content, &star, 0) {
            Some(0) => break,
            // Neither part holds a `*`, so the joined content does not either: searching it
            // again would make a long run of lines quadratic.
            None => {
                content.extend_from_slice(&next_content);
                index = next_line;
            }
            Some(found) => {
                let rest = substr(&next_content, found as isize, None).to_vec();
                let mut found = found;
                if next_content[found - 1] == u16::from(b'\\') {
                    found -= 1;
                }
                if found > 0 {
                    content = concat(&[&content, substr(&next_content, 0, Some(found as isize))]);
                    heap.set_s(next_marker, Key::Content, rest);
                }
                break;
            }
        }
    }
    (content, index)
}

/// `getAlignmentFormat`: whether milestones end with `\*` (the newer alignment format).
fn alignment_format(usfm: &[u16]) -> bool {
    let find = |start: &str| {
        let at = index_of(usfm, &js(start), 0)?;
        let mut end = at + start.len();
        while end < usfm.len() && usfm[end] != u16::from(b'\\') {
            end += 1;
        }
        Some(end)
    };
    let Some(end) = find("\\zaln-s").or_else(|| find("\\k-s")) else {
        return false;
    };
    usfm.get(end) == Some(&u16::from(b'\\')) && usfm.get(end + 1) == Some(&u16::from(b'*'))
}

/// `usfm.split(/\r?\n/)`.
fn split_lines(text: &[u16]) -> Vec<&[u16]> {
    let mut out = Vec::new();
    let mut start = 0;
    for (at, unit) in text.iter().enumerate() {
        if *unit == u16::from(b'\n') {
            let end = if at > start && text[at - 1] == u16::from(b'\r') {
                at - 1
            } else {
                at
            };
            out.push(&text[start..end]);
            start = at + 1;
        }
    }
    out.push(&text[start..]);
    out
}

/// The result of a chunk parse: the heap and each verse's objects, keyed as JavaScript
/// orders `Object.keys(verses)`.
pub(super) struct Parsed {
    pub(super) heap: Heap,
    pub(super) verses: Vec<(JsStr, Ref)>,
    /// The library throws on this input; `heap` and `verses` hold what it had built.
    pub(super) thrown: bool,
}

/// `usfmToJSON(usfm, { chunk: true })`, keeping only `verses`.
pub(super) fn to_json_chunk(usfm: &[u16]) -> Parsed {
    let usfm = match usfm.first() {
        Some(0xFEFF | 0xFFFE) => &usfm[1..],
        _ => usfm,
    };
    let lines = split_lines(usfm);
    let mut heap = Heap::default();
    let mut markers = Vec::new();
    let last_line = lines.len() - 1;
    for (l, line) in lines.iter().enumerate() {
        markers.extend(parse_line(&mut heap, line, l >= last_line));
    }
    let headers = heap.arr();
    let mut st = State {
        heap,
        current_chapter: Pos::Zero,
        current_verse: Pos::Zero,
        verses: Vec::new(),
        headers,
        phrase: None,
        phrase_parent: None,
        on_same_chapter: false,
        in_header: true,
        new_align_format: alignment_format(usfm),
        thrown: false,
    };
    let mut i = 0;
    while i < markers.len() {
        let marker = markers[i];
        let tag = st.heap.tag(marker).map(<[u16]>::to_vec);
        match tag.as_deref() {
            Some(t) if eq(t, "c") || eq(t, "v") => {
                if !st.heap.truthy(marker, &Key::Number) && st.heap.truthy(marker, &Key::Content) {
                    extract_number_from_content(&mut st.heap, marker);
                }
                if st.heap.truthy(marker, &Key::Number) {
                    if eq(t, "c") {
                        st.process_as_chapter(marker);
                    } else {
                        st.parse_as_verse(marker);
                    }
                } else {
                    let text = marker_to_text(&st.heap, marker);
                    set_opt(&mut st.heap, marker, Key::Content, text);
                    st.process_as_text(marker);
                }
            }
            Some(t) if eq(t, "k") || eq(t, "zaln") => {
                // `state.inHeader` is false from the first marker on.
                let content = st.heap.s(marker, &Key::Content).cloned();
                let phrase = parse_word(&mut st.heap, content.as_deref(), false);
                st.heap.set_s(phrase, Key::Type, js("milestone"));
                let text = st.heap.string_of(st.heap.get(phrase, &Key::Text));
                let milestone = trim(&text);
                if eq(milestone, "-s") {
                    st.remove_last_new_line(Some(t), false);
                    st.heap.del(phrase, &Key::Text);
                    i = st.start_span(phrase, t, i, &markers);
                } else if eq(milestone, "-e") {
                    st.remove_last_new_line(Some(t), true);
                    let end = concat(&[t, &js("-e\\*")]);
                    i = st.end_span(i, &markers, &end);
                } else {
                    i = st.process_marker_for_spans(marker, i, &markers);
                }
            }
            Some(t) if eq(t, "w") => {
                st.handle_word_white_space();
                let content = st.heap.s(marker, &Key::Content).cloned();
                let word = parse_word(&mut st.heap, content.as_deref(), true);
                if st.is_non_displayable_phrase_parent() {
                    st.save_usfm_object(Thing::Obj(marker));
                } else {
                    st.push_object(None, Thing::Obj(word));
                    if let Some(next) = st.heap.get(marker, &Key::NextChar).filter(|v| truthy(v)) {
                        let next = st.heap.string_of(Some(next));
                        st.push_object(None, Thing::Text(next));
                    }
                }
            }
            Some(t) if eq(t, "w*") => {
                if st.is_non_displayable_phrase_parent() {
                    st.save_usfm_object(Thing::Obj(marker));
                } else if let Some(next) = st.heap.get(marker, &Key::NextChar).filter(|v| truthy(v))
                {
                    let next = st.heap.string_of(Some(next));
                    if !eq(&next, " ") {
                        st.push_object(None, Thing::Text(next));
                    } else if let Some(next_marker) = markers.get(i + 1).copied()
                        && !st.heap.truthy(next_marker, &Key::Tag)
                        && st.heap.truthy(next_marker, &Key::Content)
                        && !st
                            .heap
                            .s(next_marker, &Key::Content)
                            .is_some_and(|c| starts_with(c, " "))
                    {
                        st.push_object(None, Thing::Text(next));
                    }
                }
            }
            None => {
                let content = st
                    .heap
                    .s(marker, &Key::Content)
                    .cloned()
                    .unwrap_or_default();
                if starts_with(&content, "\\*") {
                    st.heap
                        .set_s(marker, Key::Content, substr(&content, 2, None).to_vec());
                } else if starts_with(&content, "*")
                    && let Some(parent) = st.get_phrase_parent()
                    && (st.heap.truthy(parent, &Key::Usfm3Milestone)
                        || st
                            .heap
                            .tag(parent)
                            .is_some_and(|t| eq(t, "k") || eq(t, "w") || eq(t, "zaln")))
                {
                    st.heap
                        .set_s(marker, Key::Content, substr(&content, 1, None).to_vec());
                }
                if st.heap.truthy(marker, &Key::Content) {
                    st.process_as_text(marker);
                }
            }
            Some(t) => {
                let t = t.to_vec();
                let tag0 = substr(&t, 0, Some(1)).to_vec();
                let mut processed = false;
                if eq(&tag0, "v") || eq(&tag0, "c") {
                    let number = substr(&t, 1, None).to_vec();
                    let digits = number
                        .strip_prefix(&[u16::from(b'+')][..])
                        .unwrap_or(&number);
                    let is_int = !digits.is_empty() && digits.iter().all(|u| is_digit(*u));
                    if is_int {
                        st.heap.set_s(marker, Key::Tag, tag0.clone());
                        if let Some(old) = st.heap.get(marker, &Key::Number).filter(|v| truthy(v)) {
                            let old = st.heap.string_of(Some(old));
                            let mut content = old;
                            if let Some(c) =
                                st.heap.get(marker, &Key::Content).filter(|v| truthy(v))
                            {
                                content =
                                    concat(&[&content, &js(" "), &st.heap.string_of(Some(c))]);
                            }
                            st.heap.set_s(marker, Key::Content, content);
                        }
                        st.heap.set_s(marker, Key::Number, number);
                        if eq(&tag0, "v") {
                            st.parse_as_verse(marker);
                        } else {
                            st.process_as_chapter(marker);
                        }
                        processed = true;
                    } else if t.len() == 1 {
                        let text = marker_to_text(&st.heap, marker);
                        set_opt(&mut st.heap, marker, Key::Content, text);
                        st.process_as_text(marker);
                        processed = true;
                    }
                }
                if !processed {
                    i = st.process_marker(marker, i, &markers);
                }
            }
        }
        if st.thrown {
            break;
        }
        i += 1;
    }
    if !st.thrown {
        st.terminate_phrases();
    }
    let mut verses = st.verses;
    order_keys(&mut verses);
    Parsed {
        heap: st.heap,
        verses,
        thrown: st.thrown,
    }
}

fn set_opt(heap: &mut Heap, r: Ref, key: Key, value: Opt) {
    match value {
        Some(v) => heap.set_s(r, key, v),
        // `marker.content = undefined` leaves the property, holding undefined.
        None => heap.del(r, &key),
    }
}

/// `extractNumberFromContent`: the first run of digits becomes the number, and the content
/// loses its first two code units (the library slices by the length of the match array).
fn extract_number_from_content(heap: &mut Heap, marker: Ref) {
    let content = heap.string_of(heap.get(marker, &Key::Content));
    let Some(start) = content.iter().position(|u| is_digit(*u)) else {
        return;
    };
    let len = content[start..]
        .iter()
        .take_while(|u| is_digit(**u))
        .count();
    heap.set_s(marker, Key::Number, content[start..start + len].to_vec());
    heap.set_s(marker, Key::Content, substr(&content, 2, None).to_vec());
}

/// `Object.keys` order: integer indices ascending, then the other keys as inserted.
fn order_keys(verses: &mut [(JsStr, Ref)]) {
    let index_of_key = |key: &[u16]| -> Option<u64> {
        let text = super::js::to_string(key);
        let canonical = text == "0" || (!text.starts_with('0') && !text.is_empty());
        if !canonical || !text.bytes().all(|b| b.is_ascii_digit()) || text.len() > 10 {
            return None;
        }
        text.parse::<u64>()
            .ok()
            .filter(|n| *n < u64::from(u32::MAX))
    };
    verses.sort_by_key(|(key, _)| match index_of_key(key) {
        Some(n) => (0, n),
        None => (1, 0),
    });
}

// ---------------------------------------------------------------------------------------
// removeMarker (filter.js)
// ---------------------------------------------------------------------------------------
//
// The library walks a parsed verse with three mutually recursive functions:
// `replaceWordsAndMilestones`, `parseMilestone` and `parseChild`. Ported as recursion, a
// verse of 100,000 nested `\add` markers (500 KB) overflows the stack, and the nested walk
// of `replaceWordsAndMilestones`, which calls `parseMilestone` on every level it enters,
// takes time quadratic in the depth. Here each walk keeps its own stack, and what the
// nested walk needs from `parseMilestone` (whether it throws, whether its text is empty)
// is summarized once per object from the objects below it.
//
// Where the library recurses forever (an object that is its own descendant), it throws a
// RangeError; that is `Thrown` here too. Where it merely runs out of stack (a few thousand
// levels, depending on the JavaScript engine), it has no stable value, and version 1 gives
// the value of the walk with no limit (`docs/STORAGE.md` §7.7).

/// The library throws on this input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Thrown;

/// A flattened verse object: what `replaceWordsAndMilestones` leaves for `mergeVerseData`.
struct Flat {
    text: Option<Val>,
    tag: bool,
    next_char: Option<Val>,
}

/// `removeMarker(text)`: the displayed text of a verse, as word-aligner-lib reads it.
pub(super) fn remove_marker(text: &[u16]) -> Result<JsStr, Thrown> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let parsed = to_json_chunk(&concat(&[&js("\\v 1 "), text]));
    if parsed.thrown {
        return Err(Thrown);
    }
    let Some((_, verse)) = parsed.verses.iter().find(|(k, _)| eq(k, "1")) else {
        return Ok(text.to_vec());
    };
    let heap = &parsed.heap;
    let mut walk = Walk::new(heap);
    let mut spaced = false;
    let mut flattened = Vec::new();
    for object in heap.elems(*verse) {
        let flat;
        (flat, spaced) = walk.replace_words_and_milestones(*object, spaced)?;
        flattened.push(flat);
    }
    let mut verse_text: JsStr = Vec::new();
    for object in &flattened {
        if let Some(text) = object.text.as_ref().filter(|v| truthy(v)) {
            if object.tag {
                let last = verse_text.last().copied();
                if !matches!(last, None | Some(0x20 | 0x0A)) {
                    verse_text.push(u16::from(b' '));
                }
            }
            verse_text.extend(heap.string_of(Some(text)));
        }
        if let Some(next) = object.next_char.as_ref().filter(|v| truthy(v)) {
            verse_text.extend(heap.string_of(Some(next)));
        }
    }
    Ok(verse_text)
}

/// A property of a verse object, where the object may be a one-character string (an
/// element of a `children` string), whose properties are all undefined.
fn prop(heap: &Heap, object: Option<Ref>, key: &Key) -> Option<Val> {
    object.and_then(|r| heap.get(r, key).cloned())
}

fn is_type(heap: &Heap, object: Option<Ref>, ty: &str) -> bool {
    matches!(prop(heap, object, &Key::Type), Some(Val::Str(t)) if eq(&t, ty))
}

/// `object.children` as `children[i]` reads it: an array's elements, or for a string one
/// property-less element per code unit; `None` when it is undefined.
fn children(heap: &Heap, object: Option<Ref>) -> Option<Vec<Option<Ref>>> {
    match prop(heap, object, &Key::Children)? {
        Val::Ref(c) => Some(heap.elems(c).iter().map(|x| Some(*x)).collect()),
        Val::Str(s) => Some(vec![None; s.len()]),
        // A number or boolean has no `length`, so a loop over it runs no times.
        Val::Num(_) | Val::Bool(_) => Some(Vec::new()),
    }
}

fn has_truthy(heap: &Heap, object: Option<Ref>, key: &Key) -> bool {
    prop(heap, object, key).as_ref().is_some_and(truthy)
}

/// True when `text += value` (for a truthy `value`) makes the text longer.
fn grows(heap: &Heap, value: Option<&Val>) -> bool {
    value.is_some_and(|v| truthy(v) && !heap.string_of(Some(v)).is_empty())
}

fn ends_with_pause(text: &[u16]) -> bool {
    let last = substr(text, -1, None);
    [",", ".", "?", ";"].iter().any(|p| eq(last, p))
}

/// The word spacing `replaceWordsAndMilestones` returns for an object with no text of its
/// own: none after an object with a `nextChar` or with text that does not end in a pause.
fn spaced_after(heap: &Heap, object: Option<Ref>) -> bool {
    if has_truthy(heap, object, &Key::NextChar) {
        return false;
    }
    match prop(heap, object, &Key::Text).filter(truthy) {
        Some(text) => ends_with_pause(&heap.string_of(Some(&text))),
        None => true,
    }
}

/// What `parseChild` on an object does to the text of the `parseMilestone` around it,
/// while that text is still empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effect {
    /// The text is no longer empty.
    Grows,
    /// The text is still empty, and the word spacing is now `" "` or `""`.
    Empty { spaced: bool },
}

impl Effect {
    fn then(self, next: [Effect; 2]) -> Effect {
        match self {
            Effect::Grows => Effect::Grows,
            Effect::Empty { spaced } => next[usize::from(spaced)],
        }
    }
}

/// What the nested walk of `replaceWordsAndMilestones` needs to know about an object,
/// computed from the objects below it.
#[derive(Debug, Clone, Copy)]
struct Summary {
    /// `parseChild(object)` throws.
    throws: bool,
    /// `parseMilestone(object)` throws.
    milestone_throws: bool,
    /// `parseMilestone(object)` gives an empty text.
    milestone_empty: bool,
    /// `parseChild(object)` on a still empty text, with word spacing `""` and `" "`.
    effect: [Effect; 2],
}

/// An element of a `children` string: no properties at all.
const NO_OBJECT: Summary = Summary {
    throws: false,
    milestone_throws: true,
    milestone_empty: true,
    effect: [
        Effect::Empty { spaced: false },
        Effect::Empty { spaced: true },
    ],
};

struct Walk<'h> {
    heap: &'h Heap,
    summaries: Vec<Option<Summary>>,
    /// Objects on the path of the walk in progress, to find an object that is its own
    /// descendant.
    on_path: Vec<bool>,
}

impl<'h> Walk<'h> {
    fn new(heap: &'h Heap) -> Self {
        Self {
            heap,
            summaries: vec![None; heap.items.len()],
            on_path: vec![false; heap.items.len()],
        }
    }

    fn enter(&mut self, object: Option<Ref>) -> Result<(), Thrown> {
        if let Some(r) = object {
            if self.on_path[r] {
                return Err(Thrown);
            }
            self.on_path[r] = true;
        }
        Ok(())
    }

    fn leave(&mut self, object: Option<Ref>) {
        if let Some(r) = object {
            self.on_path[r] = false;
        }
    }

    /// `replaceWordsAndMilestones(object, wordSpacing)` for an object of the verse: the
    /// flattened object and whether the next word is spaced. Nested children are
    /// flattened in a copy that `mergeVerseData` never reads, but that walk still runs,
    /// because it can throw.
    fn replace_words_and_milestones(
        &mut self,
        object: Ref,
        spaced: bool,
    ) -> Result<(Flat, bool), Thrown> {
        let heap = self.heap;
        let object = Some(object);
        let mut text = Vec::new();
        if is_type(heap, object, "word") {
            if spaced {
                text.push(u16::from(b' '));
            }
            text.extend(heap.string_of(prop(heap, object, &Key::Text).as_ref()));
        } else if has_truthy(heap, object, &Key::Children) {
            if spaced {
                text.push(u16::from(b' '));
            }
            self.parse_milestone(object, &mut text)?;
        }
        if !text.is_empty() {
            let flat = Flat {
                text: Some(Val::Str(text)),
                tag: false,
                next_char: None,
            };
            return Ok((flat, true));
        }
        let spacing = spaced_after(heap, object);
        if has_truthy(heap, object, &Key::Children) {
            self.flatten_children(object)?;
        }
        let flat = Flat {
            text: prop(heap, object, &Key::Text),
            tag: has_truthy(heap, object, &Key::Tag),
            next_char: prop(heap, object, &Key::NextChar),
        };
        Ok((flat, spacing))
    }

    /// The loop of `replaceWordsAndMilestones` over an object's children, and every loop
    /// below it, for what they can throw.
    fn flatten_children(&mut self, root: Option<Ref>) -> Result<(), Thrown> {
        struct Frame {
            object: Option<Ref>,
            kids: Vec<Option<Ref>>,
            at: usize,
            /// The word spacing threaded through the children.
            spaced: bool,
            /// What the object's own call returns, once its children are done.
            returns: bool,
        }
        let heap = self.heap;
        let frame = |object: Option<Ref>, returns: bool| Frame {
            object,
            kids: children(heap, object).unwrap_or_default(),
            at: 0,
            spaced: false,
            returns,
        };
        self.enter(root)?;
        let mut stack = vec![frame(root, false)];
        while let Some(top) = stack.last_mut() {
            let Some(child) = top.kids.get(top.at).copied() else {
                // The flattened child is stored back with `children[i] = …`, which strict
                // mode refuses on a string.
                if matches!(prop(heap, top.object, &Key::Children), Some(Val::Str(_))) {
                    return Err(Thrown);
                }
                let (object, returns) = (top.object, top.returns);
                stack.pop();
                self.leave(object);
                if let Some(parent) = stack.last_mut() {
                    parent.spaced = returns;
                }
                continue;
            };
            top.at += 1;
            let spaced = top.spaced;
            let empty = if is_type(heap, child, "word") {
                !spaced
                    && heap
                        .string_of(prop(heap, child, &Key::Text).as_ref())
                        .is_empty()
            } else if has_truthy(heap, child, &Key::Children) {
                let summary = self.summary(child)?;
                if summary.milestone_throws {
                    return Err(Thrown);
                }
                !spaced && summary.milestone_empty
            } else {
                true
            };
            let returns = if empty {
                spaced_after(heap, child)
            } else {
                true
            };
            if empty && has_truthy(heap, child, &Key::Children) {
                self.enter(child)?;
                stack.push(frame(child, returns));
            } else if let Some(top) = stack.last_mut() {
                top.spaced = returns;
            }
        }
        Ok(())
    }

    /// Appends `parseMilestone(object)` to `out`. `parseChild` appends to the text of the
    /// innermost `parseMilestone`, which starts at `start` in `out`.
    fn parse_milestone(&mut self, root: Option<Ref>, out: &mut JsStr) -> Result<(), Thrown> {
        enum Frame {
            Milestone {
                object: Option<Ref>,
                kids: Vec<Option<Ref>>,
                at: usize,
                /// The child whose `nextChar` follows once its `parseChild` is done.
                pending: Option<Option<Ref>>,
            },
            Child {
                object: Option<Ref>,
                kids: Vec<Option<Ref>>,
                at: usize,
            },
        }
        struct Text {
            start: usize,
            spaced: bool,
        }
        enum Step {
            Child(Option<Ref>),
            MilestoneDone(Option<Ref>),
            ChildDone(Option<Ref>),
        }
        /// Starts `parseMilestone(object)`: its own text, then its children.
        fn open(
            walk: &mut Walk<'_>,
            object: Option<Ref>,
            out: &mut JsStr,
            frames: &mut Vec<Frame>,
            texts: &mut Vec<Text>,
        ) -> Result<(), Thrown> {
            let heap = walk.heap;
            let start = out.len();
            if let Some(text) = prop(heap, object, &Key::Text).filter(truthy) {
                out.extend(heap.string_of(Some(&text)));
            }
            // It reads `children.length`, so a milestone without children throws.
            let kids = children(heap, object).ok_or(Thrown)?;
            walk.enter(object)?;
            texts.push(Text {
                start,
                spaced: false,
            });
            frames.push(Frame::Milestone {
                object,
                kids,
                at: 0,
                pending: None,
            });
            Ok(())
        }
        let heap = self.heap;
        let mut frames: Vec<Frame> = Vec::new();
        let mut texts: Vec<Text> = Vec::new();
        open(self, root, out, &mut frames, &mut texts)?;
        loop {
            let step = match frames.last_mut() {
                None => break,
                Some(Frame::Milestone {
                    object,
                    kids,
                    at,
                    pending,
                }) => {
                    if let Some(done) = pending.take()
                        && let Some(next) = prop(heap, done, &Key::NextChar).filter(truthy)
                    {
                        out.extend(heap.string_of(Some(&next)));
                    }
                    match kids.get(*at).copied() {
                        Some(child) => {
                            *at += 1;
                            *pending = Some(child);
                            Step::Child(child)
                        }
                        None => Step::MilestoneDone(*object),
                    }
                }
                Some(Frame::Child { object, kids, at }) => match kids.get(*at).copied() {
                    Some(child) => {
                        *at += 1;
                        Step::Child(child)
                    }
                    None => Step::ChildDone(*object),
                },
            };
            match step {
                Step::MilestoneDone(object) => {
                    if let Some(next) = prop(heap, object, &Key::NextChar).filter(truthy) {
                        out.extend(heap.string_of(Some(&next)));
                    }
                    frames.pop();
                    texts.pop();
                    self.leave(object);
                    if let Some(text) = texts.last_mut() {
                        text.spaced = true;
                    }
                }
                Step::ChildDone(object) => {
                    frames.pop();
                    self.leave(object);
                }
                // `parseChild`.
                Step::Child(child) => {
                    let Some(text) = texts.last_mut() else { break };
                    if is_type(heap, child, "word") {
                        if text.spaced {
                            out.push(u16::from(b' '));
                        }
                        out.extend(heap.string_of(prop(heap, child, &Key::Text).as_ref()));
                        text.spaced = true;
                    } else if is_type(heap, child, "milestone") {
                        if text.spaced {
                            out.push(u16::from(b' '));
                        }
                        // The spacing becomes " " when this milestone is done.
                        open(self, child, out, &mut frames, &mut texts)?;
                    } else {
                        if let Some(own) = prop(heap, child, &Key::Text).filter(truthy) {
                            out.extend(heap.string_of(Some(&own)));
                            if !ends_with_pause(&out[text.start..]) {
                                text.spaced = false;
                            }
                        }
                        if has_truthy(heap, child, &Key::Children) {
                            self.enter(child)?;
                            frames.push(Frame::Child {
                                object: child,
                                kids: children(heap, child).unwrap_or_default(),
                                at: 0,
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// The summary of `object`, computed once from the summaries of its children.
    fn summary(&mut self, object: Option<Ref>) -> Result<Summary, Thrown> {
        let Some(root) = object else {
            return Ok(NO_OBJECT);
        };
        if let Some(known) = self.summaries[root] {
            return Ok(known);
        }
        let heap = self.heap;
        // Words are leaves: `parseChild` never looks below one.
        let below = |r: Ref| -> Vec<Option<Ref>> {
            if is_type(heap, Some(r), "word") {
                Vec::new()
            } else {
                children(heap, Some(r)).unwrap_or_default()
            }
        };
        self.enter(object)?;
        let mut stack: Vec<(Ref, Vec<Option<Ref>>, usize)> = vec![(root, below(root), 0)];
        while let Some((r, kids, at)) = stack.last_mut() {
            if let Some(Some(kid)) = kids.get(*at).copied() {
                *at += 1;
                if self.summaries[kid].is_none() {
                    self.enter(Some(kid))?;
                    stack.push((kid, below(kid), 0));
                }
                continue;
            }
            if *at < kids.len() {
                *at += 1;
                continue;
            }
            let r = *r;
            let summary = self.summarize(r);
            self.summaries[r] = Some(summary);
            stack.pop();
            self.leave(Some(r));
        }
        self.summaries[root].ok_or(Thrown)
    }

    /// The summary of `r` from the summaries of its children, which are all known.
    fn summarize(&self, r: Ref) -> Summary {
        let heap = self.heap;
        let object = Some(r);
        if is_type(heap, object, "word") {
            let text = heap.string_of(prop(heap, object, &Key::Text).as_ref());
            return Summary {
                throws: false,
                milestone_throws: false,
                milestone_empty: false,
                effect: [
                    if text.is_empty() {
                        Effect::Empty { spaced: true }
                    } else {
                        Effect::Grows
                    },
                    Effect::Grows,
                ],
            };
        }
        let kids = children(heap, object);
        let list = kids.clone().unwrap_or_default();
        let of = |kid: &Option<Ref>| kid.and_then(|k| self.summaries[k]).unwrap_or(NO_OBJECT);
        let kid_throws = list.iter().any(|kid| of(kid).throws);
        let milestone_throws = kids.is_none() || kid_throws;
        let own_text = prop(heap, object, &Key::Text);
        let mut text = if grows(heap, own_text.as_ref()) {
            Effect::Grows
        } else {
            Effect::Empty { spaced: false }
        };
        for kid in &list {
            text = text.then(of(kid).effect);
            if grows(heap, prop(heap, *kid, &Key::NextChar).as_ref()) {
                text = Effect::Grows;
            }
        }
        if grows(heap, prop(heap, object, &Key::NextChar).as_ref()) {
            text = Effect::Grows;
        }
        let milestone_empty = text != Effect::Grows;
        if is_type(heap, object, "milestone") {
            return Summary {
                throws: milestone_throws,
                milestone_throws,
                milestone_empty,
                effect: [
                    if milestone_empty {
                        Effect::Empty { spaced: true }
                    } else {
                        Effect::Grows
                    },
                    Effect::Grows,
                ],
            };
        }
        let truthy_children = has_truthy(heap, object, &Key::Children);
        let effect = [false, true].map(|spaced| {
            let mut state = Effect::Empty { spaced };
            if own_text.as_ref().is_some_and(truthy) {
                // An empty text does not end in a pause, so the spacing is dropped.
                state = if grows(heap, own_text.as_ref()) {
                    Effect::Grows
                } else {
                    Effect::Empty { spaced: false }
                };
            }
            if truthy_children {
                for kid in &list {
                    state = state.then(of(kid).effect);
                }
            }
            state
        });
        Summary {
            throws: truthy_children && kid_throws,
            milestone_throws,
            milestone_empty,
            effect,
        }
    }
}

/// Two verses whose objects are their own descendants, which no parse makes: one that
/// `parseMilestone` meets and one that only the nested walk meets, below a word.
#[cfg(test)]
pub(super) fn walks_of_cycles() -> [Result<(), Thrown>; 2] {
    let object = |heap: &mut Heap, ty: &str, kids: &[Ref]| {
        let r = heap.obj();
        heap.set_s(r, Key::Type, js(ty));
        heap.set_s(r, Key::Text, Vec::new());
        let array = heap.arr();
        for kid in kids {
            heap.push(array, *kid);
        }
        heap.set(r, Key::Children, Val::Ref(array));
        r
    };
    let tie = |heap: &mut Heap, from: Ref, to: Ref| {
        if let Some(Val::Ref(array)) = heap.get(from, &Key::Children).cloned() {
            heap.push(array, to);
        }
    };
    let mut first = Heap::default();
    let inner = object(&mut first, "x", &[]);
    let outer = object(&mut first, "x", &[inner]);
    tie(&mut first, inner, outer);
    let mut second = Heap::default();
    let below = object(&mut second, "x", &[]);
    let word = object(&mut second, "word", &[below]);
    let top = object(&mut second, "x", &[word]);
    tie(&mut second, below, below);
    [(first, outer), (second, top)].map(|(heap, root)| {
        Walk::new(&heap)
            .replace_words_and_milestones(root, false)
            .map(|_| ())
    })
}
