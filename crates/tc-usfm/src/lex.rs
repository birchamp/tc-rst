//! The lossless lexer. Every byte of the input lands in exactly one token, in order.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// U+FEFF at the very start of the input.
    Bom,
    Text,
    /// `\name`, `\+name`, `\name*` or `\+name*`, with the separator that follows an opening
    /// marker, and for `\c` and `\v` also the number.
    Marker,
    /// `\*`, the end of a milestone such as `\zaln-e\*` or `\ts\*`.
    MilestoneEnd,
    /// `|…` from the first bar of a text run up to a closing marker or `\*`.
    Attributes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub range: Range<usize>,
}

const BOM: &str = "\u{FEFF}";

pub(crate) fn lex(src: &str) -> Vec<Token> {
    let bytes = src.as_bytes();
    // About one token per 16 bytes in aligned text; reserving avoids regrowth copies.
    let mut tokens = Vec::with_capacity(src.len() / 16 + 8);
    let mut pos = 0;
    if src.starts_with(BOM) {
        tokens.push(Token {
            kind: TokenKind::Bom,
            range: 0..BOM.len(),
        });
        pos = BOM.len();
    }
    let mut text_start = pos;
    while let Some(offset) = src[pos..].find('\\') {
        pos += offset;
        let Some(marker) = marker_at(bytes, pos) else {
            pos += 1;
            continue;
        };
        push_text(src, &mut tokens, text_start..pos, marker.ends_attributes);
        tokens.push(Token {
            kind: marker.kind,
            range: pos..marker.end,
        });
        pos = marker.end;
        text_start = pos;
    }
    push_text(src, &mut tokens, text_start..bytes.len(), false);
    tokens
}

/// A text run before a closing marker or `\*` carries attributes from its first `|` on.
fn push_text(src: &str, tokens: &mut Vec<Token>, range: Range<usize>, attributes_allowed: bool) {
    if range.is_empty() {
        return;
    }
    let bar = attributes_allowed
        .then(|| src[range.clone()].find('|'))
        .flatten();
    match bar {
        Some(offset) => {
            let bar = range.start + offset;
            if bar > range.start {
                tokens.push(Token {
                    kind: TokenKind::Text,
                    range: range.start..bar,
                });
            }
            tokens.push(Token {
                kind: TokenKind::Attributes,
                range: bar..range.end,
            });
        }
        None => tokens.push(Token {
            kind: TokenKind::Text,
            range,
        }),
    }
}

struct MarkerEnd {
    kind: TokenKind,
    end: usize,
    /// A closing marker or `\*` ends the attribute list of the text before it.
    ends_attributes: bool,
}

/// Recognise a marker starting at the backslash at `at`. A backslash that does not start a
/// marker name (or `\*`) is ordinary text.
fn marker_at(bytes: &[u8], at: usize) -> Option<MarkerEnd> {
    let mut p = at + 1;
    let nested = bytes.get(p) == Some(&b'+');
    if nested {
        p += 1;
    }
    let name_start = p;
    if bytes.get(p).is_some_and(u8::is_ascii_alphabetic) {
        while bytes
            .get(p)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
        {
            p += 1;
        }
    }
    let name = &bytes[name_start..p];
    if bytes.get(p) == Some(&b'*') {
        let kind = if name.is_empty() {
            if nested {
                return None;
            }
            TokenKind::MilestoneEnd
        } else {
            TokenKind::Marker
        };
        return Some(MarkerEnd {
            kind,
            end: p + 1,
            ends_attributes: true,
        });
    }
    if name.is_empty() {
        return None;
    }
    let mut end = p;
    if !nested
        && (name == b"c" || name == b"v")
        && let Some(after_label) = number_label_end(bytes, p)
    {
        end = after_label;
    }
    end += separator_len(&bytes[end..]);
    Some(MarkerEnd {
        kind: TokenKind::Marker,
        end,
        ends_attributes: false,
    })
}

/// After `\c` or `\v`: spaces or tabs, then the number (any run without whitespace or a
/// backslash). Returns the end of the number.
fn number_label_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut p = from;
    while matches!(bytes.get(p), Some(b' ' | b'\t')) {
        p += 1;
    }
    if p == from {
        return None;
    }
    let label_start = p;
    while bytes
        .get(p)
        .is_some_and(|b| !b.is_ascii_whitespace() && *b != b'\\')
    {
        p += 1;
    }
    (p > label_start).then_some(p)
}

/// One space, tab or line break after an opening marker belongs to the marker.
fn separator_len(rest: &[u8]) -> usize {
    match rest {
        [b'\r', b'\n', ..] => 2,
        [b' ' | b'\t' | b'\n' | b'\r', ..] => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(TokenKind, &str)> {
        lex(src)
            .into_iter()
            .map(|t| (t.kind, &src[t.range]))
            .collect()
    }

    #[test]
    fn markers_own_their_separator_and_verse_numbers() {
        use TokenKind::*;
        assert_eq!(
            kinds("\u{FEFF}\\c 1\r\n\\p\n\\v 9-10 Text \\w a|x=\"1\"\\w*\\zaln-e\\*"),
            vec![
                (Bom, "\u{FEFF}"),
                (Marker, "\\c 1\r\n"),
                (Marker, "\\p\n"),
                (Marker, "\\v 9-10 "),
                (Text, "Text "),
                (Marker, "\\w "),
                (Text, "a"),
                (Attributes, "|x=\"1\""),
                (Marker, "\\w*"),
                (Marker, "\\zaln-e"),
                (MilestoneEnd, "\\*"),
            ]
        );
    }

    #[test]
    fn stray_backslashes_are_text() {
        use TokenKind::*;
        assert_eq!(
            kinds("a \\ b \\+ \\9 c\\"),
            vec![(Text, "a \\ b \\+ \\9 c\\")]
        );
        assert_eq!(kinds("\\v\n1"), vec![(Marker, "\\v\n"), (Text, "1")]);
    }
}
