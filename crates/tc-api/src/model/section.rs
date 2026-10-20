//! Section drafting: tC4's Type and Place verse numbers flow (`sectionDraft.js`, #141), the
//! pure rules the section card and the server's section save both run (APP-DESIGN §3.8, AD24,
//! unit B4b).
//!
//! The card's one text is the Type-mode text. A line that starts with one of the section's pins
//! begins that verse; a line that starts with a run of pins (`9-10`) begins that span. Place
//! mode reads the same text as a flat list of words, the whitespace that followed each word
//! (`seps`, so an untouched verse writes back byte for byte), and one word index per pin
//! (`markers`). Its invariants:
//!
//! - words and their separators are never changed or reordered, only pins move;
//! - the section's first verse is fixed at word 0 while it has words;
//! - a pin can never pass a neighbouring pin ([`SectionBuffer::can_drop`] refuses it);
//! - pins that share a word are one span, the verses' text in order (#63);
//! - a pin with no word is a stub verse: it has no text.
//!
//! Where tC4 keeps the Place state beside the text, the card here keeps only the text and parses
//! it (a drop writes the text again), so the buffer, the mirror and the server's draft hold the
//! whole card in one string.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{js_space, js_trim};

/// A paragraph or poetry marker line between verses: tC4's `p`, `q1` and `q2` (#54).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ParaFormat {
    P,
    Q1,
    Q2,
}

impl ParaFormat {
    /// The marker's name, as USFM writes it after the backslash.
    pub fn name(self) -> &'static str {
        match self {
            Self::P => "p",
            Self::Q1 => "q1",
            Self::Q2 => "q2",
        }
    }

    /// The marker a layout line names, if it is exactly `\p`, `\q1` or `\q2` (tC4's
    /// `gapMarkerOf` matches a whole line).
    pub fn of_line(line: &str) -> Option<Self> {
        match line {
            "\\p" => Some(Self::P),
            "\\q1" => Some(Self::Q1),
            "\\q2" => Some(Self::Q2),
            _ => None,
        }
    }
}

/// The single verse numbers of a key list: `9-10` is 9 and 10 (tC4's `expandKeys`). A range
/// that runs backwards has none; one too long to be a verse range stays one key.
pub fn expand_keys(keys: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for key in keys {
        match digits_range(key) {
            Some((from, to)) if to.saturating_sub(from) <= u32::from(u16::MAX) => {
                out.extend((from..=to).map(|n| n.to_string()));
            }
            _ => out.push(key.clone()),
        }
    }
    out
}

/// `9-10` as its two numbers; none for anything else (`^(\d+)-(\d+)$`).
fn digits_range(text: &str) -> Option<(u32, u32)> {
    let (from, to) = text.split_once('-')?;
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(from) || !all_digits(to) {
        return None;
    }
    Some((from.parse().ok()?, to.parse().ok()?))
}

/// The marker token at the start of `text`, `\d+(?:-\d+)?`, and what follows it.
fn marker_token(text: &str) -> Option<(&str, &str)> {
    let digits = |s: &str| s.bytes().take_while(u8::is_ascii_digit).count();
    let first = digits(text);
    if first == 0 {
        return None;
    }
    let mut end = first;
    if text.as_bytes().get(first) == Some(&b'-') {
        let more = digits(text.get(first + 1..)?);
        if more > 0 {
            end = first + 1 + more;
        }
    }
    Some((text.get(..end)?, text.get(end..)?))
}

/// The pins a marker token names: one pin, or every pin of a run `a-b` whose two ends are pins
/// of this section, in order. Anything else is not a marker.
fn run_of<'a>(token: &str, pins: &'a [String]) -> Option<&'a [String]> {
    if let Some(at) = pins.iter().position(|p| p == token) {
        return pins.get(at..=at);
    }
    let (from, to) = token.split_once('-')?;
    let a = pins.iter().position(|p| p == from)?;
    let b = pins.iter().position(|p| p == to)?;
    (b > a).then(|| pins.get(a..=b)).flatten()
}

/// A line that begins with one of the section's pins (or a run of them) begins that verse. The
/// token must sit at column 0: a verse body may wrap onto a line that starts with a number of
/// its own, and reading that as a marker would move the words and rewrite a verse nobody edited.
fn marker_line<'a>(line: &'a str, pins: &'a [String]) -> Option<(&'a [String], &'a str)> {
    let (token, after) = marker_token(line)?;
    let rest = if after.is_empty() {
        after
    } else if after.starts_with(js_space) {
        after.trim_start_matches(js_space)
    } else {
        return None;
    };
    Some((run_of(token, pins)?, rest))
}

/// A body line that needs the escape: a section pin (or run) under any number of leading
/// spaces. Counting the already indented forms too is what makes the escape reversible: one
/// space is added, one space is taken off, whatever the line started with.
fn escapable(line: &str, pins: &[String]) -> bool {
    let Some((token, after)) = marker_token(line.trim_start_matches(' ')) else {
        return false;
    };
    (after.is_empty() || after.starts_with(js_space)) && run_of(token, pins).is_some()
}

/// Indent every continuation line of a body that would read as a marker.
fn escape_body(body: &str, pins: &[String]) -> String {
    body.split('\n')
        .enumerate()
        .map(|(i, line)| {
            if i > 0 && escapable(line, pins) {
                format!(" {line}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Add (`delta` > 0, at most two) or remove (`delta` < 0) one leading tab at the caret's line
/// (#54). The caret is a UTF-16 offset, as a text field reports it; the new one is too.
pub fn indent_line(text: &str, caret: usize, delta: i32) -> (String, usize) {
    let units: Vec<u16> = text.encode_utf16().collect();
    let caret = caret.min(units.len());
    let tab = u16::from(b'\t');
    // `lastIndexOf('\n', caret - 1)`: a negative start counts as 0.
    let from = caret.saturating_sub(1);
    let line_start = units
        .iter()
        .take(from + 1)
        .rposition(|&u| u == u16::from(b'\n'))
        .map_or(0, |at| at + 1);
    let tabs = units
        .iter()
        .skip(line_start)
        .take_while(|&&u| u == tab)
        .count();
    let rebuilt = |units: Vec<u16>, caret: usize| (String::from_utf16_lossy(&units), caret);
    if delta > 0 {
        if tabs >= 2 {
            return (text.to_owned(), caret);
        }
        let mut out = units;
        out.insert(line_start.min(out.len()), tab);
        return rebuilt(out, caret + 1);
    }
    if delta < 0 {
        if tabs == 0 {
            return (text.to_owned(), caret);
        }
        let mut out = units;
        out.remove(line_start);
        return rebuilt(out, line_start.max(caret.saturating_sub(1)));
    }
    (text.to_owned(), caret)
}

/// One verse of a section as the card opens it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionVerse {
    /// The verse's key: `9`, or `9-10` for a span.
    pub key: String,
    /// Its words hold text: neither empty nor the stub.
    pub drafted: bool,
    /// The words, trimmed; empty when not drafted.
    pub body: String,
    /// The paragraph or poetry marker before the verse (#54).
    pub format: Option<ParaFormat>,
}

fn indented(format: Option<ParaFormat>, line: String) -> String {
    match format {
        Some(ParaFormat::Q1) => format!("\t{line}"),
        Some(ParaFormat::Q2) => format!("\t\t{line}"),
        Some(ParaFormat::P) => format!("\n{line}"),
        None => line,
    }
}

/// The card's opening text: one `key body` line per verse, a stub verse an empty one, and
/// nothing at all for a section with no draft yet. Formats (`\p`, `\q1`, `\q2`) are blank lines
/// and leading tabs (#54).
pub fn initial_text(verses: &[SectionVerse], pins: &[String]) -> String {
    if !verses.iter().any(|v| v.drafted) {
        return String::new();
    }
    verses
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let body = if v.drafted {
                escape_body(&v.body, pins)
            } else {
                String::new()
            };
            let line = format!("{} {body}", v.key);
            if i == 0 {
                line
            } else {
                indented(v.format, line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A verse of the section as a save will write it: placed pins that share a word are one span
/// (`9-10`), an unplaced pin is a stub, and a span whose pins are all unplaced stays the stub
/// span it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub key: String,
    pub members: Vec<String>,
    /// The word the verse begins at; none for a stub.
    pub at: Option<usize>,
}

/// The typed section read into words, separators, pins and blocks (tC4's `parseDraft`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionBuffer {
    keys: Vec<String>,
    pins: Vec<String>,
    words: Vec<String>,
    seps: Vec<String>,
    markers: Vec<Option<usize>>,
    blocks: BTreeMap<usize, ParaFormat>,
}

fn tokenize_line(rest: &str, words: &mut Vec<String>, seps: &mut Vec<String>, is_last: bool) {
    let mut current = String::new();
    let mut space = String::new();
    let flush_word = |current: &mut String, words: &mut Vec<String>, seps: &mut Vec<String>| {
        if !current.is_empty() {
            words.push(std::mem::take(current));
            seps.push(String::new());
        }
    };
    for c in rest.chars() {
        if js_space(c) {
            flush_word(&mut current, words, seps);
            space.push(c);
        } else {
            if !space.is_empty() {
                if let Some(last) = seps.last_mut() {
                    last.push_str(&space);
                }
                space.clear();
            }
            current.push(c);
        }
    }
    flush_word(&mut current, words, seps);
    if !space.is_empty()
        && let Some(last) = seps.last_mut()
    {
        last.push_str(&space);
    }
    if !is_last && let Some(last) = seps.last_mut() {
        last.push('\n');
    }
}

fn line_format(tabs: usize, preceded_by_blank: bool, is_marker: bool) -> Option<ParaFormat> {
    match tabs {
        1 => Some(ParaFormat::Q1),
        0 if !(preceded_by_blank || !is_marker) => None,
        0 => Some(ParaFormat::P),
        _ => Some(ParaFormat::Q2),
    }
}

impl SectionBuffer {
    /// Reads the typed `text` over the section's verse `keys` (as the book has them: `9`,
    /// `10-11`).
    pub fn parse(text: &str, keys: &[String]) -> Self {
        let pins = expand_keys(keys);
        let mut words: Vec<String> = Vec::new();
        let mut seps: Vec<String> = Vec::new();
        let mut found: Vec<Option<(usize, usize)>> = vec![None; pins.len()];
        let mut blocks: BTreeMap<usize, ParaFormat> = BTreeMap::new();
        let lines: Vec<&str> = text.split('\n').collect();
        let last_line = lines.len().saturating_sub(1);
        let mut preceded_by_blank = false;
        for (li, line) in lines.iter().enumerate() {
            if js_trim(line).is_empty() {
                preceded_by_blank = true;
                if li < last_line
                    && !words.is_empty()
                    && let Some(last) = seps.last_mut()
                {
                    last.push('\n');
                }
                continue;
            }
            let tabs = line.bytes().take_while(|&b| b == b'\t').count();
            let mut rest = line.get(tabs..).unwrap_or_default();
            let marker = marker_line(rest, &pins);
            if let Some(format) = line_format(tabs, preceded_by_blank, marker.is_some()) {
                blocks.insert(words.len(), format);
            }
            preceded_by_blank = false;
            if let Some((run, after)) = marker {
                for pin in run {
                    if let Some(at) = pins.iter().position(|p| p == pin)
                        && let Some(slot) = found.get_mut(at)
                    {
                        *slot = Some((words.len(), li));
                    }
                }
                rest = after;
            } else if let Some(stripped) = rest.strip_prefix(' ')
                && escapable(stripped, &pins)
            {
                rest = stripped;
            }
            tokenize_line(rest, &mut words, &mut seps, li == last_line);
        }
        if words.is_empty() {
            return Self {
                keys: keys.to_vec(),
                markers: vec![None; pins.len()],
                pins,
                words,
                seps,
                blocks: BTreeMap::new(),
            };
        }
        let markers = resolve_pins(&found, words.len());
        Self {
            keys: keys.to_vec(),
            pins,
            words,
            seps,
            markers,
            blocks,
        }
    }

    /// The section's verse keys as the book had them.
    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    /// The section's single verse numbers, in order.
    pub fn pins(&self) -> &[String] {
        &self.pins
    }

    /// The words of the section, in order; a pin only moves where a verse begins among them.
    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// The whitespace after each word, as typed, a line break where a line ended.
    pub fn seps(&self) -> &[String] {
        &self.seps
    }

    /// The paragraph or poetry marker typed before the verse that begins at word `index`.
    pub fn block_at(&self, index: usize) -> Option<ParaFormat> {
        self.blocks.get(&index).copied()
    }

    /// Every typed block, by the word it precedes.
    pub fn blocks(&self) -> &BTreeMap<usize, ParaFormat> {
        &self.blocks
    }

    /// No word is typed: a save would empty every verse of the section (#600).
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// The word index `pin` begins at; none for a pin in the bank.
    pub fn marker(&self, pin: &str) -> Option<usize> {
        let at = self.pins.iter().position(|p| p == pin)?;
        self.markers.get(at).copied().flatten()
    }

    /// The pins nothing begins at, except the first, which is fixed: the "To place" bank.
    pub fn bank(&self) -> Vec<&str> {
        self.pins
            .iter()
            .zip(&self.markers)
            .skip(1)
            .filter(|(_, at)| at.is_none())
            .map(|(pin, _)| pin.as_str())
            .collect()
    }

    /// The pins that begin at word `index`, in order.
    pub fn pins_at(&self, index: usize) -> Vec<&str> {
        self.pins
            .iter()
            .zip(&self.markers)
            .filter(|(_, at)| **at == Some(index))
            .map(|(pin, _)| pin.as_str())
            .collect()
    }

    fn keyed(&self, pin: &str) -> Option<&String> {
        self.keys.iter().find(|k| {
            expand_keys(std::slice::from_ref(*k))
                .iter()
                .any(|p| p == pin)
        })
    }

    /// The section's verses as the save will write them, in order (tC4's `sectionGroups`).
    pub fn groups(&self) -> Vec<Group> {
        let mut groups = Vec::new();
        let mut i = 0;
        while i < self.pins.len() {
            let Some(pin) = self.pins.get(i) else { break };
            match self.markers.get(i).copied().flatten() {
                None => {
                    let orig = self.keyed(pin).cloned().unwrap_or_else(|| pin.clone());
                    let members = expand_keys(std::slice::from_ref(&orig));
                    let whole = members.first() == Some(pin)
                        && members.iter().all(|m| self.marker(m).is_none());
                    let run = if whole { members } else { vec![pin.clone()] };
                    i += run.len().max(1);
                    groups.push(Group {
                        key: if whole { orig } else { pin.clone() },
                        members: run,
                        at: None,
                    });
                }
                Some(at) => {
                    let mut j = i;
                    while self.markers.get(j + 1).copied().flatten() == Some(at) {
                        j += 1;
                    }
                    let members: Vec<String> = self.pins.get(i..=j).unwrap_or_default().to_vec();
                    let key = match (members.first(), members.last()) {
                        (Some(a), Some(b)) if members.len() > 1 => format!("{a}-{b}"),
                        (Some(a), _) => a.clone(),
                        _ => break,
                    };
                    groups.push(Group {
                        key,
                        members,
                        at: Some(at),
                    });
                    i = j + 1;
                }
            }
        }
        groups
    }

    /// The section's verse keys after the edit, stubs included.
    pub fn new_keys(&self) -> Vec<String> {
        self.groups().into_iter().map(|g| g.key).collect()
    }

    /// The text of each verse from the pinned word list: a verse (or span) runs from its pin to
    /// the next pin, words joined by their own separators. Only verses with text are returned,
    /// keyed as the save writes them (tC4's `sectionVerses`). Verses joined into a span are one
    /// paragraph: a blank line inside one becomes a space (#575).
    pub fn verses(&self) -> Vec<(String, String)> {
        let groups = self.groups();
        if groups.is_empty() {
            return Vec::new();
        }
        let begins: BTreeMap<usize, usize> = groups
            .iter()
            .enumerate()
            .filter_map(|(g, group)| group.at.map(|at| (at, g)))
            .collect();
        let mut text = vec![String::new(); groups.len()];
        // Words before any pin belong to the first verse.
        let mut current = 0;
        for (i, word) in self.words.iter().enumerate() {
            if let Some(&g) = begins.get(&i) {
                current = g;
            }
            if let Some(buf) = text.get_mut(current) {
                buf.push_str(word);
                buf.push_str(self.seps.get(i).map_or("", String::as_str));
            }
        }
        groups
            .into_iter()
            .zip(text)
            .filter_map(|(group, text)| {
                let mut text = js_trim(&text).to_owned();
                if group.members.len() > 1 {
                    text = collapse_blank_lines(&text);
                }
                (!text.is_empty()).then_some((group.key, text))
            })
            .collect()
    }

    /// The marker before each verse after the first, as the typed lines ask (tC4's
    /// `groupFormats`): `formats[i]` is for the verse whose key is `.0`, the one that comes
    /// before the verse the format was typed on.
    pub fn formats(&self) -> Vec<(String, Option<ParaFormat>)> {
        let groups = self.groups();
        groups
            .windows(2)
            .map(|pair| {
                let format = pair
                    .get(1)
                    .and_then(|next| next.at)
                    .and_then(|at| self.blocks.get(&at).copied());
                (
                    pair.first().map(|g| g.key.clone()).unwrap_or_default(),
                    format,
                )
            })
            .collect()
    }

    /// The pinned word list written back as Type-mode text, one verse per line (#54).
    pub fn serialize(&self) -> String {
        let verses = self.verses();
        let groups = self.groups();
        groups
            .iter()
            .filter_map(|g| {
                verses
                    .iter()
                    .find(|(key, _)| *key == g.key)
                    .map(|(_, text)| (g, text))
            })
            .enumerate()
            .map(|(i, (g, text))| {
                let line = format!("{} {}", g.key, escape_body(text, &self.pins));
                if i == 0 {
                    line
                } else {
                    indented(g.at.and_then(|at| self.blocks.get(&at).copied()), line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// May `pin` begin at word `index`? Never the first verse (fixed), never past a later
    /// verse's pin, never before an earlier verse's pin. On another pin is allowed: the two form
    /// a span (#63), unless an unplaced pin would then sit inside that span. The pin's own
    /// current place is ignored: it is in hand.
    pub fn can_drop(&self, pin: &str, index: usize) -> bool {
        let Some(pos) = self.pins.iter().position(|p| p == pin) else {
            return false;
        };
        if pos == 0 {
            return false;
        }
        for (k, (other, at)) in self.pins.iter().zip(&self.markers).enumerate() {
            let Some(at) = at else { continue };
            if other == pin {
                continue;
            }
            if if k < pos { *at > index } else { *at < index } {
                return false;
            }
        }
        let stacked: Vec<usize> = self
            .pins
            .iter()
            .zip(&self.markers)
            .enumerate()
            .filter(|(_, (other, at))| *other != pin && **at == Some(index))
            .map(|(k, _)| k)
            .collect();
        let (Some(&lo), Some(&hi)) = (stacked.iter().min(), stacked.iter().max()) else {
            return true;
        };
        let (lo, hi) = (lo.min(pos), hi.max(pos));
        (lo..=hi).all(|k| {
            self.pins.get(k).is_some_and(|p| p == pin)
                || self.markers.get(k).copied().flatten().is_some()
        })
    }

    /// The buffer with `pin` placed at word `index`, when [`can_drop`](Self::can_drop) allows
    /// it. The first verse takes the leading words back when no pin begins at word 0 any more
    /// (a stub first verse whose neighbour moved on).
    pub fn drop_pin(&self, pin: &str, index: usize) -> Option<Self> {
        if !self.can_drop(pin, index) {
            return None;
        }
        let at = self.pins.iter().position(|p| p == pin)?;
        let mut next = self.clone();
        *next.markers.get_mut(at)? = Some(index);
        if next.markers.first().copied().flatten().is_none() && !next.markers.contains(&Some(0)) {
            *next.markers.first_mut()? = Some(0);
        }
        Some(next)
    }

    /// Where a verse beginning at `index` would end: the next pin after it, the held pin's old
    /// place ignored.
    pub fn span_end(&self, index: usize, held: Option<&str>) -> usize {
        self.pins
            .iter()
            .zip(&self.markers)
            .filter(|(pin, _)| Some(pin.as_str()) != held)
            .filter_map(|(_, at)| *at)
            .filter(|&at| at > index)
            .fold(self.words.len(), usize::min)
    }
}

/// tC4's `resolvePins`: the first pin is fixed at word 0; a later pin keeps the word it was
/// typed at unless that is before an earlier pin or past the last word, and a pin typed on a
/// later line than another at the same word takes the word from it.
fn resolve_pins(found: &[Option<(usize, usize)>], words: usize) -> Vec<Option<usize>> {
    let mut markers: Vec<Option<usize>> = vec![None; found.len()];
    let Some(first) = markers.first_mut() else {
        return markers;
    };
    *first = Some(0);
    let mut last = (0usize, found.first().copied().flatten().map(|f| f.1));
    for (k, f) in found.iter().enumerate().skip(1) {
        let Some((at, line)) = *f else { continue };
        if at >= words || at < last.0 {
            continue;
        }
        if at == last.0 {
            // `last.1` is the line of the previous pin; the first pin typed nowhere is line -1.
            let before = last.1;
            if before.is_some_and(|before| line < before) {
                continue;
            }
            if before.is_none_or(|before| line > before) {
                for marker in &mut markers {
                    if *marker == Some(at) {
                        *marker = None;
                    }
                }
            }
        }
        if let Some(slot) = markers.get_mut(k) {
            *slot = Some(at);
        }
        last = (at, Some(line));
    }
    markers
}

/// `t.replace(/[ \t]*\n(?:[ \t]*\n)+[ \t]*/g, ' ')`: a run of two or more line breaks, with
/// only spaces and tabs between them, becomes one space. A single line break stays.
fn collapse_blank_lines(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let blank = |c: Option<&char>| matches!(c, Some(' ' | '\t'));
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let mut j = i;
        while blank(chars.get(j)) {
            j += 1;
        }
        if chars.get(j) == Some(&'\n') {
            j += 1;
            let mut breaks = 1;
            loop {
                let mut k = j;
                while blank(chars.get(k)) {
                    k += 1;
                }
                if chars.get(k) == Some(&'\n') {
                    j = k + 1;
                    breaks += 1;
                } else {
                    break;
                }
            }
            if breaks >= 2 {
                while blank(chars.get(j)) {
                    j += 1;
                }
                out.push(' ');
                i = j;
                continue;
            }
        }
        if let Some(c) = chars.get(i) {
            out.push(*c);
        }
        i += 1;
    }
    out
}
