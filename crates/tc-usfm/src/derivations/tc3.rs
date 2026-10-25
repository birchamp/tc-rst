//! A port of usfm-js 3.4.3 `toUSFM(json, { forcedNewLines: true })` (`lib/js/jsonToUsfm.js`)
//! for the object tC4's tC3 importer gives it: header objects, and chapters of verses that
//! each hold one text object. Function for function, quirks included.
//!
//! A verse whose text holds `\p`, `\m`, `\c`, `\n` or `\b` is parsed again with the chunk
//! parser of `usfmjs.rs` and written back with the library's other functions (words,
//! milestones, markers), so those are ported too. That inner `jsonToUSFM` call resets the
//! library's module-wide parameters, so `forcedNewLines` is off for the rest of the book.
//!
//! Where the library throws, there is no text: a verse whose rewrite holds the same verse
//! again recurses without end (`b \c x`), and the parser throws on a few nestings.
//! Not ported, as no input of this shape reaches them: `addPhrase` under `forcedNewLines`
//! (only words and milestones go through it, and they appear only after the reset), and
//! `makeSureParagraphsAtEndHaveLineFeeds`, which does nothing to a verse of one text object.

use super::js::{JsStr, concat, ends_with, eq, is_space, js, starts_with, substr, to_string, trim};
use super::usfmjs::{self, Heap, Key, Ref, Thrown, Val, array_index, props, truthy};
use super::usfmjs_tables::WORD_SPECIAL_ATTRIBUTES;

/// The tags `USFM_PROPERTIES` marks `milestone: true` (`markerIsMilestone`).
const MILESTONES: &[&str] = &[
    "qt", "qt-s", "qt1", "qt1-s", "qt2", "qt2-s", "qt3", "qt3-s", "qt4", "qt4-s", "qt5", "qt5-s",
    "ts", "ts-e", "ts-s",
];

/// shortcut: nesting (markers inside markers, rewrites inside rewrites) deeper than this
/// gives no text, where Node runs out of stack somewhere above it; Rust's stack would
/// overflow first without a limit. No tC3 project nests anywhere near it.
const MAX_DEPTH: usize = 256;

/// tC4's tC3 importer's book text (`bookUsfm` in `src/data/import/tc3.ts`): usfm-js's
/// `toUSFM({ headers, chapters }, { forcedNewLines: true })`, byte for byte.
///
/// `headers` are the objects of `<book>/headers.json` in order, each as its string-valued
/// fields (name, value); tC3 writes no other kind of value. As tC4 does, the `usfm` header
/// is set to `3.0`, or one is put in second place (last when there are fewer than two).
/// `chapters` are the chapter files in the order the caller lists them, each keyed by
/// `String(parseInt(file stem))`, with its verses as (key, text) in file order. Keys are
/// ordered as JavaScript orders an object's keys, and a key given twice keeps its first
/// place and its last value. Each verse text gets the `\n` tC4 appends.
///
/// `None` where the library throws, so tC4's import of the book fails.
pub fn tc3_book_usfm(
    headers: &[Vec<(String, String)>],
    chapters: &[(String, Vec<(String, String)>)],
) -> Option<String> {
    let mut heap = Heap::default();
    let mut cx = Cx {
        forced_new_lines: true,
        rewriting: Vec::new(),
        depth: 0,
    };
    let mut header_refs: Vec<Ref> = headers
        .iter()
        .map(|fields| {
            let r = heap.obj();
            for (name, value) in fields {
                heap.set_s(r, Key::named(&js(name)), js(value));
            }
            r
        })
        .collect();
    match header_refs
        .iter()
        .find(|r| heap.s(**r, &Key::Tag).is_some_and(|t| eq(t, "usfm")))
    {
        Some(r) => heap.set_s(*r, Key::Content, js("3.0")),
        None => {
            let r = heap.obj();
            heap.set_s(r, Key::Tag, js("usfm"));
            heap.set_s(r, Key::Content, js("3.0"));
            let at = if header_refs.len() < 2 {
                header_refs.len()
            } else {
                1
            };
            header_refs.insert(at, r);
        }
    }
    let mut output = Vec::new();
    for r in header_refs {
        output_header_object(&heap, &mut output, r);
    }
    let chapters: Vec<(JsStr, &[(String, String)])> = object_order(
        chapters
            .iter()
            .map(|(key, verses)| (js(key), verses.as_slice())),
    );
    for (number, verses) in chapters {
        let verses = object_order(verses.iter().map(|(key, text)| (js(key), text.as_str())));
        let lines = generate_chapter_lines(&mut cx, &mut heap, &number, verses).ok()?;
        add_chapter(&mut output, lines);
    }
    Some(to_string(&output.concat()))
}

/// What the library's module-wide state and the call stack hold.
struct Cx {
    /// `params_.forcedNewLines`.
    forced_new_lines: bool,
    /// The texts being rewritten, outermost first: meeting one again recurses forever.
    rewriting: Vec<JsStr>,
    depth: usize,
}

/// A JavaScript object built by assigning `entries` in order: integer keys ascending, then
/// the others in the order first assigned, each with its last value.
fn object_order<T>(entries: impl Iterator<Item = (JsStr, T)>) -> Vec<(JsStr, T)> {
    let mut out: Vec<(JsStr, T)> = Vec::new();
    for (key, value) in entries {
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => out.push((key, value)),
        }
    }
    out.sort_by_key(|(key, _)| array_index(key).map_or((1, 0), |n| (0, n)));
    out
}

fn text_object(heap: &mut Heap, text: &str) -> Val {
    let r = heap.obj();
    heap.set_s(r, Key::Text, js(&format!("{text}\n")));
    heap.set_s(r, Key::Type, js("text"));
    Val::Ref(r)
}

/// `String(value)` of a property that is set and truthy (`object.key || …`).
fn truthy_str(heap: &Heap, r: Ref, key: &Key) -> Option<JsStr> {
    heap.get(r, key)
        .filter(|v| truthy(v))
        .map(|v| heap.string_of(Some(v)))
}

fn is_str(heap: &Heap, r: Ref, key: &Key, ascii: &str) -> bool {
    heap.s(r, key).is_some_and(|s| eq(s, ascii))
}

fn trim_left(text: &[u16]) -> &[u16] {
    let start = text
        .iter()
        .position(|u| !is_space(*u))
        .unwrap_or(text.len());
    &text[start..]
}

fn trim_right(text: &[u16]) -> &[u16] {
    let end = text
        .iter()
        .rposition(|u| !is_space(*u))
        .map_or(0, |at| at + 1);
    &text[..end]
}

fn key_name(key: &Key) -> JsStr {
    js(match key {
        Key::Open => "open",
        Key::Tag => "tag",
        Key::Number => "number",
        Key::Content => "content",
        Key::Text => "text",
        Key::NextChar => "nextChar",
        Key::Type => "type",
        Key::Children => "children",
        Key::Attrib => "attrib",
        Key::EndTag => "endTag",
        Key::Nesting => "nesting",
        Key::Usfm3Milestone => "usfm3Milestone",
        Key::EndMarkerChar => "endMarkerChar",
        Key::Other(name) => return name.clone(),
    })
}

/// `needsNewLine`.
fn needs_new_line(heap: &Heap, next: Option<Ref>) -> JsStr {
    match next {
        Some(next) if is_str(heap, next, &Key::Tag, "zaln") => js("\n"),
        _ => Vec::new(),
    }
}

/// `generateWord`, with the default parameters (`strongs` written as `strong`).
fn generate_word(heap: &Heap, word: Ref) -> JsStr {
    let mut attributes: Vec<JsStr> = Vec::new();
    for key in heap.keys(word) {
        if matches!(key, Key::Text | Key::Tag | Key::Type) {
            continue;
        }
        let value = heap.get(word, &key);
        let mut name = key_name(&key);
        if eq(&name, "strongs") {
            name = js("strong");
        }
        let mut attribute = if WORD_SPECIAL_ATTRIBUTES.iter().any(|a| eq(&name, a)) {
            concat(&[&js("x-"), &name])
        } else {
            name
        };
        if let Some(value) = value.filter(|v| truthy(v)) {
            attribute = concat(&[
                &attribute,
                &js("=\""),
                &heap.string_of(Some(value)),
                &js("\""),
            ]);
        }
        attributes.push(attribute);
    }
    let mut attr_out = attributes.join(&u16::from(b' '));
    if !attr_out.is_empty() {
        attr_out = concat(&[&js("|"), trim_left(&attr_out)]);
    }
    let text = heap.string_of(heap.get(word, &Key::Text));
    concat(&[&js("\\w "), &text, &attr_out, &js("\\w*")])
}

/// `generatePhrase`.
fn generate_phrase(
    cx: &mut Cx,
    heap: &mut Heap,
    phrase: Ref,
    next: Option<Ref>,
) -> Result<JsStr, Thrown> {
    let tag = truthy_str(heap, phrase, &Key::Tag).unwrap_or_else(|| js("zaln"));
    let mut next_char = truthy_str(heap, phrase, &Key::NextChar);
    let termination = match heap.get(phrase, &Key::EndTag) {
        Some(Val::Str(end)) => end.clone(),
        _ => concat(&[&tag, &js("-e\\*")]),
    };
    let mut content: JsStr = Vec::new();
    if is_str(heap, phrase, &Key::Type, "milestone") {
        let mut attributes: Vec<JsStr> = Vec::new();
        for key in heap.keys(phrase) {
            if matches!(key, Key::Children | Key::Tag | Key::Type | Key::EndTag) {
                continue;
            }
            let value = heap.string_of(heap.get(phrase, &key));
            let mut name = key_name(&key);
            if eq(&name, "strongs") {
                name = js("strong");
            }
            attributes.push(concat(&[&js("x-"), &name, &js("=\""), &value, &js("\"")]));
        }
        let joined = attributes.join(&u16::from(b' '));
        content = concat(&[&js("-s |"), trim_left(&joined), &js("\\*")]);
    } else {
        if MILESTONES.iter().any(|m| eq(&tag, m)) {
            if let Some(attrib) = truthy_str(heap, phrase, &Key::Attrib) {
                content = attrib;
            }
            content.extend(js("\\*"));
        }
        if let Some(text) = truthy_str(heap, phrase, &Key::Text) {
            if !(starts_with(&text, " ") || starts_with(&text, "\n")) {
                content.push(u16::from(b' '));
            }
            content.extend(text);
        }
        if let Some(inner) = truthy_str(heap, phrase, &Key::Content) {
            content.push(u16::from(b' '));
            content.extend(inner);
        }
        if content.is_empty()
            && let Some(next_char) = next_char.take()
        {
            content = next_char;
        }
    }
    let line = concat(&[&js("\\"), &tag, &content]);
    let children = heap.get(phrase, &Key::Children).cloned();
    let mut line = object_to_string(cx, heap, children, line, None)?;
    if !termination.is_empty() {
        let after = next_char.unwrap_or_else(|| needs_new_line(heap, next));
        line = concat(&[&line, &js("\\"), &termination, &after]);
    }
    Ok(line)
}

/// `usfmMarkerToString`.
fn usfm_marker_to_string(
    heap: &Heap,
    object: Ref,
    next: Option<Ref>,
    no_space_after_tag: bool,
    no_termination: bool,
) -> JsStr {
    let mut output: JsStr = Vec::new();
    let mut content = truthy_str(heap, object, &Key::Text)
        .or_else(|| truthy_str(heap, object, &Key::Content))
        .unwrap_or_default();
    let tag = heap.get(object, &Key::Tag).filter(|v| truthy(v));
    let termination: Option<JsStr> = match heap.get(object, &Key::EndTag) {
        Some(Val::Str(end)) => Some(end.clone()),
        _ => {
            let tag_text = tag.map(|t| heap.string_of(Some(t)));
            let terminated = props(tag_text.as_deref())
                .is_some_and(|p| !matches!(p.end_tag, usfmjs::EndTag::None));
            match tag_text {
                Some(tag) if terminated && !no_termination => Some(concat(&[&tag, &js("*")])),
                _ => None,
            }
        }
    };
    let terminated = termination.as_ref().is_some_and(|t| !t.is_empty());
    let next_char = truthy_str(heap, object, &Key::NextChar);
    if let Some(tag) = tag {
        let tag = heap.string_of(Some(tag));
        output = concat(&[&js("\\"), &tag]);
        if let Some(number) = truthy_str(heap, object, &Key::Number) {
            output = concat(&[&output, &js(" "), &number]);
        }
        let first_char = substr(&content, 0, Some(1)).to_vec();
        if no_space_after_tag {
        } else if let Some(attrib) = truthy_str(heap, object, &Key::Attrib) {
            if !content.is_empty() {
                output = concat(&[&output, &js(" "), &content]);
            }
            if ends_with(&tag, "\\*") {
                let len = output.len() as isize;
                output = concat(&[
                    substr(&output, 0, Some(len - 2)),
                    &attrib,
                    substr(&output, -2, None),
                ]);
            } else {
                output.extend(attrib);
            }
            content.clear();
        } else if !terminated {
            if !first_char.is_empty() && !eq(&first_char, "\n") && !eq(&content, " \n") {
                output.push(u16::from(b' '));
            } else if let Some(next) = next
                && content.is_empty()
                && next_char.is_none()
                && !["w", "k", "zaln"]
                    .iter()
                    .any(|t| is_str(heap, next, &Key::Tag, t))
            {
                output.push(u16::from(b' '));
            }
        } else if !eq(&first_char, " ") {
            output.push(u16::from(b' '));
        }
    }
    output.extend(content);
    if let Some(termination) = termination.filter(|t| !t.is_empty()) {
        output = concat(&[&output, &js("\\"), &termination]);
    }
    if let Some(next_char) = next_char {
        output.extend(next_char);
    }
    output
}

/// `hasParagraph`.
fn has_paragraph(text: &[u16]) -> bool {
    let has = |mark: &str| {
        let mark = js(mark);
        text.windows(mark.len()).any(|w| w == mark.as_slice())
    };
    has("\\") && ["\\p", "\\m", "\\c", "\\n", "\\b"].iter().any(|m| has(m))
}

/// The text branch of `objectToString` for a text with a paragraph marker:
/// `jsonToUSFM(usfmToJSON('\\v 1 ' + text, { chunk: true })).substr(5)`.
fn rewrite(cx: &mut Cx, text: &[u16]) -> Result<JsStr, Thrown> {
    if cx.rewriting.iter().any(|t| t.as_slice() == text) || cx.depth >= MAX_DEPTH {
        return Err(Thrown);
    }
    let mut parsed = usfmjs::to_json_chunk(&concat(&[&js("\\v 1 "), text]));
    if parsed.thrown {
        return Err(Thrown);
    }
    cx.forced_new_lines = false;
    cx.rewriting.push(text.to_vec());
    cx.depth += 1;
    let written = chunk_to_usfm(cx, &mut parsed);
    cx.depth -= 1;
    cx.rewriting.pop();
    Ok(substr(&written?, 5, None).to_vec())
}

/// `jsonToUSFM` of a chunk parse, with no parameters.
fn chunk_to_usfm(cx: &mut Cx, parsed: &mut usfmjs::Parsed) -> Result<JsStr, Thrown> {
    let heap = &mut parsed.heap;
    let headers = cleanup_header_new_lines(heap, parsed.headers);
    let mut output = Vec::new();
    for header in headers {
        output_header_object(heap, &mut output, header);
    }
    for chapter in &parsed.chapters {
        add_chapter(
            &mut output,
            vec![concat(&[&js("\\c "), chapter, &js("\n")])],
        );
    }
    let keys = sort_verses(parsed.verses.iter().map(|(k, _)| k.clone()).collect());
    for key in keys {
        let verse = parsed
            .verses
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, r)| Val::Ref(*r));
        let line = generate_verse(cx, heap, &key, verse)?;
        add_verse(cx, &mut output, line);
    }
    Ok(output.concat())
}

/// `cleanupHeaderNewLines`, which the parser runs at its end: the headers left after it.
fn cleanup_header_new_lines(heap: &mut Heap, headers: Ref) -> Vec<Ref> {
    let mut headers = heap.elems(headers).to_vec();
    let mut i = 0;
    while i < headers.len() {
        let header = headers[i];
        if is_str(heap, header, &Key::Type, "text") {
            let mut text = heap.string_of(heap.get(header, &Key::Text));
            text.extend(truthy_str(heap, header, &Key::NextChar).unwrap_or_default());
            while let Some(next) = headers.get(i + 1).copied() {
                if !is_str(heap, next, &Key::Type, "text") {
                    break;
                }
                text.extend(heap.string_of(heap.get(next, &Key::Text)));
                text.extend(truthy_str(heap, next, &Key::NextChar).unwrap_or_default());
                headers.remove(i + 1);
            }
            if ends_with(&text, "\n") {
                text.pop();
            }
            heap.set_s(header, Key::Text, text);
        }
        if is_str(heap, header, &Key::NextChar, "\n") {
            heap.del(header, &Key::NextChar);
        }
        i += 1;
    }
    headers
}

/// `objectToString`. `object` is `undefined` when `None`.
fn object_to_string(
    cx: &mut Cx,
    heap: &mut Heap,
    object: Option<Val>,
    output: JsStr,
    next: Option<Ref>,
) -> Result<JsStr, Thrown> {
    // Only an object with keys reaches a branch: a string's indices name none of them.
    let r = match object {
        Some(Val::Ref(r)) if heap.key_count(r) > 0 => r,
        _ => return Ok(output),
    };
    if cx.depth >= MAX_DEPTH {
        return Err(Thrown);
    }
    cx.depth += 1;
    let result = object_to_string_at(cx, heap, r, output, next);
    cx.depth -= 1;
    result
}

fn object_to_string_at(
    cx: &mut Cx,
    heap: &mut Heap,
    object: Ref,
    mut output: JsStr,
    mut next: Option<Ref>,
) -> Result<JsStr, Thrown> {
    if heap.is_array(object) {
        let elements = heap.elems(object).to_vec();
        // The library passes a copy of the next element as `nextObject`, edits it, and then
        // writes that copy in the element's place.
        let mut next_copy: Option<Ref> = None;
        for i in 0..elements.len() {
            let current = next_copy.unwrap_or(elements[i]);
            next_copy = elements.get(i + 1).map(|e| heap.copy(*e));
            output = object_to_string(cx, heap, Some(Val::Ref(current)), output, next_copy)?;
        }
        return Ok(output);
    }
    if is_str(heap, object, &Key::Type, "text") {
        let mut text = truthy_str(heap, object, &Key::Text).unwrap_or_default();
        if has_paragraph(&text) {
            text = rewrite(cx, &text)?;
        }
        output.extend(text);
        return Ok(output);
    }
    if is_str(heap, object, &Key::Type, "word") {
        output.extend(generate_word(heap, object));
        return Ok(output);
    }
    let tag = heap.string_of(heap.get(object, &Key::Tag));
    let closed_as_tag = matches!(heap.get(object, &Key::EndTag),
        Some(Val::Str(end)) if *end == concat(&[&tag, &js("*")]));
    let has_children = match heap.get(object, &Key::Children) {
        Some(Val::Ref(children)) => !heap.elems(*children).is_empty(),
        Some(Val::Str(children)) => !children.is_empty(),
        _ => false,
    };
    if is_str(heap, object, &Key::Type, "milestone") && !closed_as_tag || has_children {
        output.extend(generate_phrase(cx, heap, object, next)?);
        return Ok(output);
    }
    if is_str(heap, object, &Key::Type, "paragraph") {
        let mut check_ahead = false;
        let next_char_space = is_str(heap, object, &Key::NextChar, " ");
        if let Some(text) = truthy_str(heap, object, &Key::Text) {
            if ends_with(&text, "\n") {
                let kept = trim_right(&text[..text.len() - 1]);
                heap.set_s(object, Key::Text, concat(&[kept, &js("\n")]));
            } else if trim(&text).is_empty() {
                heap.set_s(object, Key::Text, Vec::new());
                check_ahead = next_char_space;
            }
        } else {
            check_ahead = next_char_space;
        }
        if check_ahead {
            match next {
                Some(following) => {
                    if is_str(heap, following, &Key::Type, "text") {
                        let text = truthy_str(heap, following, &Key::Text).unwrap_or_default();
                        let text = trim_left(&text).to_vec();
                        if text.is_empty() {
                            heap.del(following, &Key::Text);
                            heap.del(following, &Key::Type);
                            next = None;
                            heap.del(object, &Key::NextChar);
                        } else {
                            heap.set_s(following, Key::Text, text);
                        }
                    }
                }
                None => heap.del(object, &Key::NextChar),
            }
        }
    }
    if heap.get(object, &Key::Tag).is_some_and(truthy) {
        output.extend(usfm_marker_to_string(heap, object, next, false, false));
    }
    Ok(output)
}

/// `generateVerse`.
fn generate_verse(
    cx: &mut Cx,
    heap: &mut Heap,
    number: &[u16],
    verse: Option<Val>,
) -> Result<JsStr, Thrown> {
    let text = object_to_string(cx, heap, verse, Vec::new(), None)?;
    let object = heap.obj();
    heap.set_s(object, Key::Tag, js("v"));
    heap.set_s(object, Key::Number, number.to_vec());
    heap.set_s(object, Key::Text, text);
    Ok(usfm_marker_to_string(heap, object, None, false, false))
}

/// `addVerse`.
fn add_verse(cx: &Cx, lines: &mut Vec<JsStr>, mut verse: JsStr) {
    if cx.forced_new_lines
        && let Some(last) = lines.last()
        && !ends_with(last, "\n")
        && !last.windows(3).any(|w| w == js("\n\\q").as_slice())
    {
        verse.insert(0, u16::from(b'\n'));
    }
    lines.push(verse);
}

/// `addChapter`.
fn add_chapter(lines: &mut Vec<JsStr>, mut chapter: Vec<JsStr>) {
    if lines.last().is_some_and(|last| !ends_with(last, "\n"))
        && let Some(first) = chapter.first_mut()
    {
        first.insert(0, u16::from(b'\n'));
    }
    lines.extend(chapter);
}

/// `generateChapterLines` for a chapter of text verses.
fn generate_chapter_lines(
    cx: &mut Cx,
    heap: &mut Heap,
    number: &[u16],
    mut verses: Vec<(JsStr, &str)>,
) -> Result<Vec<JsStr>, Thrown> {
    let mut lines = vec![concat(&[&js("\\c "), number, &js("\n")])];
    if let Some(at) = verses.iter().position(|(key, _)| eq(key, "front")) {
        let (_, text) = verses.remove(at);
        let front = text_object(heap, text);
        lines.push(object_to_string(cx, heap, Some(front), Vec::new(), None)?);
    }
    let keys = sort_verses(verses.iter().map(|(key, _)| key.clone()).collect());
    for key in keys {
        if let Some(last) = lines.last_mut()
            && last
                .last()
                .is_some_and(|c| *c != u16::from(b'\n') && *c != u16::from(b' '))
        {
            last.push(u16::from(b' '));
        }
        let text = verses
            .iter()
            .find(|(k, _)| *k == key)
            .map_or("", |(_, t)| *t);
        let verse = text_object(heap, text);
        let line = generate_verse(cx, heap, &key, Some(verse))?;
        add_verse(cx, &mut lines, line);
    }
    Ok(lines)
}

/// `outputHeaderObject`.
fn output_header_object(heap: &Heap, output: &mut Vec<JsStr>, header: Ref) {
    let no_space = truthy_str(heap, header, &Key::Content).is_some_and(|content| {
        starts_with(&content, "-") || starts_with(&content, "*") || starts_with(&content, "\\*")
    });
    let mut text = usfm_marker_to_string(heap, header, None, no_space, true);
    let text_object = is_str(heap, header, &Key::Type, "text")
        && matches!(heap.get(header, &Key::Text), Some(Val::Str(_)));
    if text_object || heap.get(header, &Key::Tag).is_some_and(truthy) {
        text.push(u16::from(b'\n'));
    }
    output.push(text);
}

/// `parseInt(text, 10)`.
fn parse_int(text: &[u16]) -> f64 {
    let text = trim_left(text);
    let (negative, digits) = match text.first() {
        Some(&u) if u == u16::from(b'-') => (true, &text[1..]),
        Some(&u) if u == u16::from(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let len = digits
        .iter()
        .take_while(|u| (0x30..=0x39).contains(*u))
        .count();
    if len == 0 {
        return f64::NAN;
    }
    let value = to_string(&digits[..len]).parse::<f64>().unwrap_or(f64::NAN);
    if negative { -value } else { value }
}

/// `sortVerses`: `Array.prototype.sort` with the library's comparator.
fn sort_verses(mut keys: Vec<JsStr>) -> Vec<JsStr> {
    let numbers: Vec<f64> = keys.iter().map(|k| parse_int(k)).collect();
    if numbers.iter().all(|n| n.is_finite()) {
        // Then the comparator is a total order (number, then text), which every correct
        // sort agrees on.
        let mut pairs: Vec<(f64, JsStr)> = numbers.into_iter().zip(keys).collect();
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        return pairs.into_iter().map(|(_, k)| k).collect();
    }
    let compare = |a: &[u16], b: &[u16]| -> f64 {
        let delta = parse_int(a) - parse_int(b);
        if delta == 0.0 {
            if a > b { 1.0 } else { -1.0 }
        } else {
            delta
        }
    };
    // V8's TimSort: below 64 elements, one run extended by binary insertion.
    // shortcut: 64 keys or more, some not numbers, are sorted as below too, where V8 merges
    // runs; tC3 writes only numbers and `front`.
    let n = keys.len();
    if n < 2 {
        return keys;
    }
    let mut run = 2;
    let descending = compare(&keys[1], &keys[0]) < 0.0;
    while run < n {
        let order = compare(&keys[run], &keys[run - 1]);
        if (descending && order >= 0.0) || (!descending && order < 0.0) {
            break;
        }
        run += 1;
    }
    if descending {
        keys[..run].reverse();
    }
    for start in run..n {
        let (mut left, mut right) = (0, start);
        while left < right {
            let mid = left + ((right - left) >> 1);
            if compare(&keys[start], &keys[mid]) < 0.0 {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        let pivot = keys.remove(start);
        keys.insert(left, pivot);
    }
    keys
}
