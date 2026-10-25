//! A new book's stubs (`docs/STORAGE.md` §5.7 Add): tC4's `seedBookFromSource`
//! (`src/data/seed.ts` at d310bf3), ported. The stub book has the source book's chapters and
//! verse keys (spans kept), its paragraph-level markers and its `\d` superscription slots,
//! and every verse body is the stub `___`: no source word, no `\zaln` or `\w`, no `\ts`.
//!
//! The walk is a marker stream over the raw text, not a line walk: aligned sources put
//! `\v N` mid-line after a paragraph marker (`\q1 \v 3 \zaln-s …`).

use tc_core::BookId;

/// tC4's `PARA_TAGS`.
const PARA_TAGS: &[&str] = &[
    "p", "pi", "pi1", "pi2", "pi3", "q", "q1", "q2", "q3", "q4", "qr", "qc", "qm", "qm1", "qm2",
    "qm3", "qa", "m", "mi", "b", "nb", "li", "li1", "li2", "li3", "li4", "lh", "lf", "lim", "lim1",
    "lim2", "pc", "ph", "ph1", "ph2", "po", "pr", "cls", "pmo", "pm", "pmc", "pmr",
];

/// The blank book's header lines (#574, D94): the book name in `\h`, `\toc1`, `\toc2` and
/// `\mt`, `\toc3` the code.
fn header_lines(code: &str, name: &str) -> Vec<String> {
    vec![
        "\\usfm 3.0".to_owned(),
        "\\ide UTF-8".to_owned(),
        format!("\\h {name}"),
        format!("\\toc1 {name}"),
        format!("\\toc2 {name}"),
        format!("\\toc3 {code}"),
        format!("\\mt {name}"),
    ]
}

/// The name a source book carries in its `\h` line, before its first `\c`.
pub fn source_book_name(source: &str) -> Option<String> {
    let head = match source.find("\n\\c ").or_else(|| source.find("\\c ")) {
        Some(at) => &source[..at],
        None => source,
    };
    head.lines().find_map(|line| {
        let rest = line.strip_prefix("\\h")?;
        let rest = rest.strip_prefix([' ', '\t'])?;
        let name = rest.trim_matches([' ', '\t']);
        (!name.is_empty()).then(|| name.to_owned())
    })
}

/// Every backslash marker in document order, with the number or span after `\c` and `\v`
/// (tC4's `MARKER_RE`: `\\([a-z0-9]+)(?:\s+(\d+(?:-\d+)?))?`).
fn markers(source: &str) -> Vec<(&str, Option<&str>)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_lowercase() || bytes[end].is_ascii_digit())
        {
            end += 1;
        }
        if end == start {
            i += 1;
            continue;
        }
        let tag = &source[start..end];
        let mut j = end;
        while j < bytes.len() && (bytes[j] as char).is_ascii_whitespace() {
            j += 1;
        }
        let number = if j > end {
            let num_start = j;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > num_start {
                if j + 1 < bytes.len() && bytes[j] == b'-' && bytes[j + 1].is_ascii_digit() {
                    j += 1;
                    while j < bytes.len() && bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                }
                Some(&source[num_start..j])
            } else {
                None
            }
        } else {
            None
        };
        out.push((tag, number));
        i = end;
    }
    out
}

/// The stub book of `code` seeded from `source`, or `None` when the source has no `\c` or
/// `\v` structure (tC4: "source unusable for seeding").
pub fn seed_book(
    source: &str,
    code: &str,
    english_name: &str,
    project_name: &str,
) -> Option<String> {
    let name = source_book_name(source).unwrap_or_else(|| english_name.to_owned());
    let mut out = vec![format!("\\id {code} {project_name}")];
    out.extend(header_lines(code, &name));
    let mut pending_para: Option<String> = None;
    let mut saw_content = false;
    for (tag, number) in markers(source) {
        match (tag, number) {
            ("c", Some(n)) => {
                out.push(format!("\\c {n}"));
                pending_para = None;
                saw_content = true;
            }
            ("v", Some(n)) => {
                if let Some(para) = pending_para.take() {
                    out.push(para);
                }
                out.push(format!("\\v {n} ___"));
                saw_content = true;
            }
            ("d", _) => {
                if let Some(para) = pending_para.take() {
                    out.push(para);
                }
                out.push("\\d ___".to_owned());
            }
            (tag, _) if PARA_TAGS.contains(&tag) => pending_para = Some(format!("\\{tag}")),
            _ => {}
        }
    }
    saw_content.then(|| out.join("\n") + "\n")
}

/// A blank book with no text to take its shape from: every chapter of `verse_counts` and
/// every verse of the chapter as the stub `___`, which is what tC4's platform wrote for a
/// book (`add_cv`).
pub fn skeleton(book: BookId, verse_counts: &[u16]) -> String {
    let mut out = format!("\\id {}\n", book.code());
    for (index, verses) in verse_counts.iter().enumerate() {
        out.push_str(&format!("\\c {}\n\\p\n", index + 1));
        for verse in 1..=*verses {
            out.push_str(&format!("\\v {verse} ___\n"));
        }
    }
    out
}

/// The book a USFM file's `\id` line names (tC4's parser reads the code of the line).
pub fn id_book(header: &str) -> Option<BookId> {
    header.lines().find_map(|line| {
        let rest = line
            .trim_start_matches('\u{feff}')
            .trim_start()
            .strip_prefix("\\id")?;
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        rest.split_whitespace().next()?.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_keeps_structure_and_drops_words() {
        let source = "\\id TIT EN_ULT\n\\h Titus\n\\c 1\n\\p\n\\v 1 Paul, a servant\n\\q1 \\v 2-3 \\zaln-s |x\\*\\w in|x\\w*\\zaln-e\\*\n\\ts\\*\n\\c 2\n\\d For the director\n\\v 1 Text\n";
        let seeded = seed_book(source, "TIT", "Titus", "My Titus").unwrap();
        assert_eq!(
            seeded,
            "\\id TIT My Titus\n\\usfm 3.0\n\\ide UTF-8\n\\h Titus\n\\toc1 Titus\n\\toc2 Titus\n\\toc3 TIT\n\\mt Titus\n\\c 1\n\\p\n\\v 1 ___\n\\q1\n\\v 2-3 ___\n\\c 2\n\\d ___\n\\v 1 ___\n"
        );
        assert_eq!(
            seed_book("\\id TIT\n\\h Titus\n", "TIT", "Titus", "x"),
            None
        );
        assert_eq!(
            source_book_name("\\id X\n\\h   Tito  \n\\c 1"),
            Some("Tito".into())
        );
    }
}
