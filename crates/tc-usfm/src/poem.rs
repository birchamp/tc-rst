//! A verse's words as the verse and section editors show them (tC4 #786, #810, #812;
//! `tC4:src/data/usfm/poemLines.ts`): an inner `\q1`, `\q2` or `\q3` line with one to three
//! leading tabs, a break (`\p`, `\b`, `\ts\*` …) or a blank line as an empty line, any other line
//! as it is. Saving turns the editor text back into the markers.
//!
//! Where tC4 would write back other bytes than it read (a bare `\q`, an upper-case `\Q2`, a line
//! that starts with a tab, mixed line ends), the verse keeps its markers in the editor instead,
//! so opening and saving a verse never changes a byte the translator did not change.

use crate::codec::is_layout_only_line;

/// What one inner line of a verse's words is, for the editors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line<'a> {
    /// Whitespace only.
    Blank,
    /// A paragraph-level marker with nothing after it: shown as an empty line.
    Break,
    /// `\q1`, `\q2` or `\q3`, one space, then text: shown after that many tabs.
    Poem { level: usize, text: &'a str },
    /// Anything else: shown as it is.
    Other,
}

fn line_kind(line: &str) -> Line<'_> {
    if line.trim().is_empty() {
        return Line::Blank;
    }
    if is_layout_only_line(line) {
        return Line::Break;
    }
    for level in 1..=3 {
        let marker = ["\\q1 ", "\\q2 ", "\\q3 "][level - 1];
        if let Some(text) = line.strip_prefix(marker)
            && text.starts_with(|c: char| !c.is_whitespace())
        {
            return Line::Poem { level, text };
        }
    }
    Line::Other
}

/// The line end of `words`: CRLF when its first line end is one.
fn line_end(words: &str) -> &'static str {
    match words.find('\n') {
        Some(at) if words[..at].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

fn tabbed_text(words: &str) -> String {
    let nl = line_end(words);
    words
        .split(nl)
        .enumerate()
        .map(|(i, line)| match (i, line_kind(line)) {
            (0, _) | (_, Line::Other) => line.to_owned(),
            (_, Line::Blank | Line::Break) => String::new(),
            (_, Line::Poem { level, text }) => format!("{}{text}", "\t".repeat(level)),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tabbed_words(text: &str, opened: &str) -> String {
    let nl = line_end(opened);
    let mut breaks = opened
        .split(nl)
        .skip(1)
        .filter(|line| matches!(line_kind(line), Line::Blank | Line::Break));
    let mut lines: Vec<&str> = text.split('\n').collect();
    // A blank line after the last words keeps a break it had; a new one is nothing (tC4).
    let mut tail = 0;
    while lines.len() > 1 && lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
        tail += 1;
    }
    let mut out: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                return line.trim_start_matches('\t').to_owned();
            }
            if line.trim().is_empty() {
                return breaks.next().unwrap_or("\\p").to_owned();
            }
            let tabs = line.len() - line.trim_start_matches('\t').len();
            match tabs {
                1..=3 => format!("\\q{tabs} {}", line[tabs..].trim_start_matches([' ', '\t'])),
                _ => (*line).to_owned(),
            }
        })
        .collect();
    out.extend(breaks.take(tail).map(str::to_owned));
    out.join(nl)
}

/// True when `words` shows with tabs: turning its editor text back gives its exact bytes.
pub fn shows_tabs(words: &str) -> bool {
    let one_line_end = if line_end(words) == "\r\n" {
        words.matches('\n').count() == words.matches("\r\n").count()
    } else {
        !words.contains('\r')
    };
    one_line_end && tabbed_words(&tabbed_text(words), words) == words
}

/// The editor text of a verse's words.
pub fn editor_text(words: &str) -> String {
    if shows_tabs(words) {
        tabbed_text(words)
    } else {
        words.to_owned()
    }
}

/// The words to save for an editor `text` typed over `opened`, the words the editor showed.
pub fn words_from_editor_text(text: &str, opened: &str) -> String {
    if shows_tabs(opened) {
        tabbed_words(text, opened)
    } else {
        text.to_owned()
    }
}
