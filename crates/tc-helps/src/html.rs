//! A lenient HTML reader for Aquifer article content. It reads the small tag set Aquifer
//! uses and never fails: unknown tags pass their children through, stray end tags are
//! ignored, and unclosed tags close at the end.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn has_class(&self, class: &str) -> bool {
        self.attr("class")
            .is_some_and(|c| c.split_ascii_whitespace().any(|c| c == class))
    }

    /// All text inside, entities decoded, whitespace as written.
    pub fn text(&self) -> String {
        let mut out = String::new();
        collect_text(&self.children, &mut out);
        out
    }
}

pub(crate) fn collect_text(nodes: &[Node], out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(t),
            Node::Element(e) if e.name == "br" => out.push('\n'),
            Node::Element(e) => collect_text(&e.children, out),
        }
    }
}

const VOID: [&str; 8] = ["br", "img", "hr", "wbr", "input", "meta", "link", "col"];

enum Token {
    Start {
        name: String,
        attrs: Vec<(String, String)>,
        closed: bool,
    },
    End(String),
    Text(String),
}

fn tokenize(html: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut rest = html;
    let mut text = String::new();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        if rest.starts_with('<')
            && let Some((token, after)) = tag(rest)
        {
            if !text.is_empty() {
                tokens.push(Token::Text(decode_entities(&std::mem::take(&mut text))));
            }
            tokens.push(token);
            rest = after;
            continue;
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            text.push(c);
        }
        rest = chars.as_str();
    }
    if !text.is_empty() {
        tokens.push(Token::Text(decode_entities(&text)));
    }
    tokens
}

fn name_len(s: &str) -> usize {
    s.bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'-')
        .count()
}

/// Reads one tag at the start of `s`, which begins with `<`. `None` when it is not a tag,
/// so the `<` is text.
fn tag(s: &str) -> Option<(Token, &str)> {
    let body = &s[1..];
    if let Some(end_body) = body.strip_prefix('/') {
        let n = name_len(end_body);
        if n == 0 || !end_body.as_bytes()[0].is_ascii_alphabetic() {
            return None;
        }
        let close = end_body.find('>')?;
        return Some((
            Token::End(end_body[..n].to_ascii_lowercase()),
            &end_body[close + 1..],
        ));
    }
    let n = name_len(body);
    if n == 0 || !body.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    let name = body[..n].to_ascii_lowercase();
    let mut rest = &body[n..];
    let mut attrs = Vec::new();
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("/>") {
            return Some((
                Token::Start {
                    name,
                    attrs,
                    closed: true,
                },
                after,
            ));
        }
        if let Some(after) = rest.strip_prefix('>') {
            return Some((
                Token::Start {
                    name,
                    attrs,
                    closed: false,
                },
                after,
            ));
        }
        if rest.is_empty() {
            return None;
        }
        if let Some(after) = rest.strip_prefix('/') {
            rest = after;
            continue;
        }
        let an = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '=' | '>' | '/'))
            .unwrap_or(rest.len());
        if an == 0 {
            // A stray `=` or quote; skip one character.
            let mut chars = rest.chars();
            chars.next();
            rest = chars.as_str();
            continue;
        }
        let attr = rest[..an].to_ascii_lowercase();
        rest = rest[an..].trim_start();
        let value = if let Some(after_eq) = rest.strip_prefix('=') {
            let after_eq = after_eq.trim_start();
            let (value, after) = match after_eq.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let inner = &after_eq[1..];
                    let end = inner.find(q)?;
                    (&inner[..end], &inner[end + 1..])
                }
                _ => {
                    let end = after_eq
                        .find(|c: char| c.is_whitespace() || c == '>')
                        .unwrap_or(after_eq.len());
                    (&after_eq[..end], &after_eq[end..])
                }
            };
            rest = after;
            decode_entities(value)
        } else {
            String::new()
        };
        attrs.push((attr, value));
    }
}

/// Builds the node tree.
pub(crate) fn parse(html: &str) -> Vec<Node> {
    let mut stack: Vec<Element> = vec![Element {
        name: String::new(),
        attrs: Vec::new(),
        children: Vec::new(),
    }];
    for token in tokenize(html) {
        match token {
            Token::Text(t) => push(&mut stack, Node::Text(t)),
            Token::Start {
                name,
                attrs,
                closed,
            } => {
                let element = Element {
                    name,
                    attrs,
                    children: Vec::new(),
                };
                if closed || VOID.contains(&element.name.as_str()) {
                    push(&mut stack, Node::Element(element));
                } else {
                    stack.push(element);
                }
            }
            Token::End(name) => {
                let open = stack.iter().skip(1).rposition(|e| e.name == name);
                if let Some(at) = open {
                    while stack.len() > at + 1 {
                        close(&mut stack);
                    }
                }
            }
        }
    }
    while stack.len() > 1 {
        close(&mut stack);
    }
    stack.pop().map(|root| root.children).unwrap_or_default()
}

fn push(stack: &mut [Element], node: Node) {
    if let Some(top) = stack.last_mut() {
        top.children.push(node);
    }
}

fn close(stack: &mut Vec<Element>) {
    if let Some(done) = stack.pop() {
        push(stack, Node::Element(done));
    }
}

/// Decodes character references. An unknown name stays as written.
pub(crate) fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let decoded = rest[1..]
            .find(';')
            .filter(|&end| end <= 10)
            .and_then(|end| {
                let name = &rest[1..1 + end];
                entity(name).map(|c| (c, end + 2))
            });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code).filter(|&c| c != '\0');
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "copy" => '©',
        "reg" => '®',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_tags_attributes_and_entities() {
        let nodes = parse(
            "<p><strong><span style='direction: ltr;'>ἐφανέρωσεν &amp; τὸν</span></strong></p>\
             <p>a<br />b <data class=\"resource-ref\" data-content-id=\"33292\">Metaphor</data></p>",
        );
        assert_eq!(nodes.len(), 2);
        let Node::Element(p) = &nodes[0] else {
            panic!()
        };
        assert_eq!(p.text(), "ἐφανέρωσεν & τὸν");
        let Node::Element(p2) = &nodes[1] else {
            panic!()
        };
        assert_eq!(p2.text(), "a\nb Metaphor");
    }

    #[test]
    fn stray_and_unclosed_tags_do_not_lose_text() {
        let nodes = parse("x</em><b>y<i>z");
        let mut text = String::new();
        collect_text(&nodes, &mut text);
        assert_eq!(text, "xyz");
        assert_eq!(
            decode_entities("&#104;&#x65;&bogus; & &lt;"),
            "he&bogus; & <"
        );
    }
}
