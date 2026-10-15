//! Links inside helps: `rc://` links, relative paths between files of a resource, the
//! `aquifer:` links the Aquifer converter writes, and web and mail addresses. Each resolves
//! to a typed target the UI can open.
//!
//! The `aquifer:` scheme is this crate's own. The converter writes
//! `aquifer:resource/<resource_code>/<language>/<content_id>` for an Aquifer
//! `<data class="resource-ref">` tag and `aquifer:bible/<BBCCCVVV>[-<BBCCCVVV>]` for a
//! `<data class="bible-ref">` tag, so no information in the tag is lost.

use std::ops::Range;

use tc_core::BookId;

use crate::Error;
use crate::error::LinkProblem;
use crate::ids::{
    AcademyArticleId, AquiferContentId, Manual, RowId, Slug, WordArticleId, WordCategory,
};
use crate::refs::{BibleLocus, BibleRef, ChapterVerse, ObsRef, number, parse_aquifer_ref};

/// What a link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    Academy(AcademyArticleId),
    Word(WordArticleId),
    /// A passage of a book, or the notes on it.
    Bible(BibleRef),
    /// One note, named by its row id, such as `../01/09/pzi1`.
    Note {
        reference: BibleRef,
        id: RowId,
    },
    Obs(ObsRef),
    /// An Aquifer article of a resource this crate does not model, such as a study note.
    AquiferResource {
        resource_code: String,
        language: String,
        id: AquiferContentId,
    },
    Web(String),
    /// A `mailto:` link: the address and anything after it, as written.
    Mail(String),
}

/// The file a relative link is written in. Relative paths resolve against it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkBase {
    /// Only absolute links (`rc://`, `aquifer:`, `http`, `mailto:`) resolve.
    Absolute,
    /// The root of a DCS export, where TWL rows link `./payload/kt/god.md`.
    ExportRoot,
    /// A translation words article in this category.
    WordArticle(WordCategory),
    /// The `01.md` of a Translation Academy article.
    AcademyArticle { manual: Manual, slug: Slug },
    /// A translation note on `book`, in chapter `chapter`, or the book introduction when
    /// `chapter` is `None`. Notes link `../01/01.md` as if each lived in
    /// `<book>/<chapter>/<verse>.md`; Aquifer notes keep those links. A note gives its own
    /// base with [`crate::Note::link_base`].
    Note { book: BookId, chapter: Option<u16> },
    /// An Open Bible Stories note, which links `01/11` for story 1 frame 11.
    ObsNote,
}

/// A parsed `rc://` link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RcLink {
    /// The language segment; `None` for `*`, which means the language of the set that
    /// resolves the link.
    pub language: Option<String>,
    pub target: LinkTarget,
}

fn problem(href: &str, problem: LinkProblem) -> Error {
    Error::Link {
        href: href.to_owned(),
        problem,
    }
}

/// Resolves any link a help can carry. Relative paths need the right `base`.
pub fn resolve_link(href: &str, base: &LinkBase) -> Result<LinkTarget, Error> {
    let href = href.trim();
    if href.starts_with("rc://") {
        return parse_rc_link(href).map(|link| link.target);
    }
    if let Some(rest) = href.strip_prefix("aquifer:") {
        return parse_aquifer_link(href, rest);
    }
    if href.starts_with("http://") || href.starts_with("https://") {
        return Ok(LinkTarget::Web(href.to_owned()));
    }
    if let Some(address) = href.strip_prefix("mailto:") {
        return Ok(LinkTarget::Mail(address.to_owned()));
    }
    if href.contains(':') {
        return Err(problem(href, LinkProblem::Unsupported));
    }
    resolve_relative(href, base)
}

/// Parses `rc://<language>/<resource>/<type>/…`.
pub fn parse_rc_link(href: &str) -> Result<RcLink, Error> {
    let rest = href
        .strip_prefix("rc://")
        .ok_or_else(|| problem(href, LinkProblem::Unsupported))?;
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    let [language, resource, kind, path @ ..] = parts.as_slice() else {
        return Err(problem(href, LinkProblem::NoTarget));
    };
    let target = match (*resource, *kind, path) {
        ("ta", "man", [manual, slug, ..]) => academy(href, manual, slug)?,
        ("tw", "dict", ["bible", category, slug]) => word(href, category, slug)?,
        ("tn", "help", ["obs", story, frame]) => obs(href, story, frame)?,
        ("tn", "help", [book, chapter, verse]) | (_, "book", [book, chapter, verse]) => {
            LinkTarget::Bible(passage(href, book, chapter, verse)?)
        }
        _ => return Err(problem(href, LinkProblem::Unsupported)),
    };
    let language = (*language != "*").then(|| (*language).to_owned());
    Ok(RcLink { language, target })
}

fn academy(href: &str, manual: &str, slug: &str) -> Result<LinkTarget, Error> {
    let manual = Manual::parse(manual)
        .ok_or_else(|| problem(href, LinkProblem::Manual(manual.to_owned())))?;
    let slug = Slug::parse(slug).map_err(|_| problem(href, LinkProblem::Slug(slug.to_owned())))?;
    Ok(LinkTarget::Academy(AcademyArticleId::Dcs { manual, slug }))
}

fn word(href: &str, category: &str, file: &str) -> Result<LinkTarget, Error> {
    let category = WordCategory::parse(category)
        .ok_or_else(|| problem(href, LinkProblem::Category(category.to_owned())))?;
    let name = file.strip_suffix(".md").unwrap_or(file);
    let slug = Slug::parse(name).map_err(|_| problem(href, LinkProblem::Slug(name.to_owned())))?;
    Ok(LinkTarget::Word(WordArticleId::Dcs { category, slug }))
}

fn obs(href: &str, story: &str, frame: &str) -> Result<LinkTarget, Error> {
    let frame = frame.strip_suffix(".md").unwrap_or(frame);
    let story_n =
        number(story).ok_or_else(|| problem(href, LinkProblem::Number(story.to_owned())))?;
    let frame_n =
        number(frame).ok_or_else(|| problem(href, LinkProblem::Number(frame.to_owned())))?;
    u8::try_from(story_n)
        .ok()
        .and_then(|s| ObsRef::new(s, frame_n))
        .map(LinkTarget::Obs)
        .ok_or_else(|| problem(href, LinkProblem::Number(story.to_owned())))
}

fn book(href: &str, code: &str) -> Result<BookId, Error> {
    code.parse()
        .map_err(|_| problem(href, LinkProblem::Book(code.to_owned())))
}

/// `<book>/<chapter>/<verse>`, where the verse may be `intro` and the chapter `front`.
fn passage(href: &str, code: &str, chapter: &str, verse: &str) -> Result<BibleRef, Error> {
    let book = book(href, code)?;
    let verse = verse.strip_suffix(".md").unwrap_or(verse);
    let num = |s: &str| number(s).ok_or_else(|| problem(href, LinkProblem::Number(s.to_owned())));
    let locus = match (chapter, verse) {
        ("front", "intro") => BibleLocus::BookIntro,
        (c, "intro") => {
            let c = num(c)?;
            if c == 0 {
                return Err(problem(href, LinkProblem::Number(chapter.to_owned())));
            }
            BibleLocus::ChapterIntro(c)
        }
        (c, v) => {
            let at = ChapterVerse::new(num(c)?, num(v)?)
                .ok_or_else(|| problem(href, LinkProblem::Number(format!("{c}:{v}"))))?;
            return Ok(BibleRef::verse(book, at));
        }
    };
    Ok(BibleRef { book, locus })
}

fn parse_aquifer_link(href: &str, rest: &str) -> Result<LinkTarget, Error> {
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        ["resource", code, language, id] => {
            let id = AquiferContentId::parse(id)
                .map_err(|_| problem(href, LinkProblem::ContentId((*id).to_owned())))?;
            Ok(match *code {
                "UWTranslationManual" => LinkTarget::Academy(AcademyArticleId::Aquifer(id)),
                "UWTranslationWords" => LinkTarget::Word(WordArticleId::Aquifer(id)),
                _ => LinkTarget::AquiferResource {
                    resource_code: (*code).to_owned(),
                    language: (*language).to_owned(),
                    id,
                },
            })
        }
        ["bible", range] => parse_aquifer_ref(range)
            .map(LinkTarget::Bible)
            .map_err(|why| problem(href, LinkProblem::Reference(why))),
        _ => Err(problem(href, LinkProblem::Unsupported)),
    }
}

fn resolve_relative(href: &str, base: &LinkBase) -> Result<LinkTarget, Error> {
    let mut path: Vec<String> = match base {
        LinkBase::Absolute => return Err(problem(href, LinkProblem::Unsupported)),
        LinkBase::ExportRoot => vec!["export".into()],
        LinkBase::WordArticle(category) => vec!["tw".into(), category.folder().into()],
        LinkBase::AcademyArticle { manual, slug } => {
            vec!["ta".into(), manual.folder().into(), slug.as_str().into()]
        }
        LinkBase::Note { book, chapter } => vec![
            "tn".into(),
            book.code().to_ascii_lowercase(),
            chapter.map_or_else(|| "front".to_owned(), |c| format!("{c:02}")),
        ],
        LinkBase::ObsNote => vec!["obs".into()],
    };
    for segment in href.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if path.pop().is_none() {
                    return Err(problem(href, LinkProblem::OutsideResource));
                }
            }
            s => path.push(s.to_owned()),
        }
    }
    let parts: Vec<&str> = path.iter().map(String::as_str).collect();
    match parts.as_slice() {
        ["export", "payload", category, file] | ["tw", category, file] => {
            word(href, category, file)
        }
        [.., "bible", category, file] if file.ends_with(".md") => word(href, category, file),
        ["ta", manual, slug] | ["ta", manual, slug, "01.md" | "title.md" | "sub-title.md"] => {
            academy(href, manual, slug)
        }
        ["tn", code, chapter, verse] => passage(href, code, chapter, verse).map(LinkTarget::Bible),
        ["tn", code, chapter, verse, id] => {
            let reference = passage(href, code, chapter, verse)?;
            let id = RowId::parse(id).map_err(|_| problem(href, LinkProblem::NoTarget))?;
            Ok(LinkTarget::Note { reference, id })
        }
        ["obs", story, frame] => obs(href, story, frame),
        _ => Err(problem(href, LinkProblem::NoTarget)),
    }
}

/// A link found in markdown text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownLink {
    /// Byte range of the whole link in the text, brackets included.
    pub span: Range<usize>,
    /// The bracketed text of `[label](href)`; `None` for `[[href]]`, which shows the
    /// target's title.
    pub label: Option<String>,
    pub href: String,
}

/// Byte ranges of code: fenced blocks and backtick spans, in text order. Brackets there are
/// not links.
fn code_ranges(markdown: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut fence: Option<(usize, &str)> = None;
    let mut at = 0;
    for line in markdown.split_inclusive('\n') {
        let bare = line.trim_start_matches([' ', '>']);
        let opens = ["```", "~~~"].into_iter().find(|f| bare.starts_with(f));
        match (fence, opens) {
            (Some((start, open)), Some(f)) if f == open => {
                ranges.push(start..at + line.len());
                fence = None;
            }
            (Some(_), _) => {}
            (None, Some(f)) => fence = Some((at, f)),
            (None, None) => {
                let mut rest = 0;
                while let Some(open) = line[rest..].find('`') {
                    let start = rest + open;
                    let ticks = line[start..].bytes().take_while(|b| *b == b'`').count();
                    let marker = &line[start..start + ticks];
                    match line[start + ticks..].find(marker) {
                        Some(end) => {
                            let stop = start + ticks + end + ticks;
                            ranges.push(at + start..at + stop);
                            rest = stop;
                        }
                        None => break,
                    }
                }
            }
        }
        at += line.len();
    }
    if let Some((start, _)) = fence {
        ranges.push(start..markdown.len());
    }
    ranges
}

/// True when an odd run of backslashes ends just before byte `at`, so the character there
/// is escaped (`\[` is a bracket, `\\[` a backslash and then a link).
fn escaped(bytes: &[u8], at: usize) -> bool {
    bytes[..at]
        .iter()
        .rev()
        .take_while(|b| **b == b'\\')
        .count()
        % 2
        == 1
}

/// A link or an image found by [`scan`].
pub(crate) struct Scanned {
    pub link: MarkdownLink,
    /// `![alt](src)`: the span starts at the `!` and the label is the alt text.
    pub image: bool,
}

/// Finds links and images in one pass. Each search for a closing bracket stops at the next
/// opening bracket or line end, so the scan takes time in proportion to the text.
pub(crate) fn scan(markdown: &str) -> Vec<Scanned> {
    let bytes = markdown.as_bytes();
    let code = code_ranges(markdown);
    let mut next_code = 0;
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while next_code < code.len() && code[next_code].end <= i {
            next_code += 1;
        }
        let in_code = code.get(next_code).is_some_and(|r| r.start <= i);
        if bytes[i] != b'[' || in_code || escaped(bytes, i) {
            i += 1;
            continue;
        }
        let image = i > 0 && bytes[i - 1] == b'!' && !escaped(bytes, i - 1);
        match link_at(markdown, i) {
            Some(mut link) => {
                i = link.span.end;
                if image {
                    link.span.start -= 1;
                }
                found.push(Scanned { link, image });
            }
            None => i += 1,
        }
    }
    found
}

/// Finds `[label](href)` and `[[href]]` links, in order. Images (`![alt](src)`),
/// backslash-escaped brackets and brackets inside code are not links.
pub fn find_links(markdown: &str) -> Vec<MarkdownLink> {
    scan(markdown)
        .into_iter()
        .filter(|f| !f.image)
        .map(|f| f.link)
        .collect()
}

/// Reads a link at the `[` at byte `start`. A label holds no unescaped `[` and no line
/// break, and ends at the first unescaped `]`; an href holds no whitespace, and the href of
/// a `[[href]]` link no bracket.
fn link_at(text: &str, start: usize) -> Option<MarkdownLink> {
    let bytes = text.as_bytes();
    if bytes.get(start + 1) == Some(&b'[') {
        let from = start + 2;
        let mut j = from;
        loop {
            match bytes.get(j)? {
                b']' if bytes.get(j + 1) == Some(&b']') => break,
                b'[' | b']' => return None,
                b if b.is_ascii_whitespace() => return None,
                _ => j += 1,
            }
        }
        if j == from {
            return None;
        }
        return Some(MarkdownLink {
            span: start..j + 2,
            label: None,
            href: text[from..j].to_owned(),
        });
    }
    let mut close = start + 1;
    loop {
        match bytes.get(close)? {
            b'\n' => return None,
            b'[' if !escaped(bytes, close) => return None,
            b']' if !escaped(bytes, close) => break,
            _ => close += 1,
        }
    }
    if bytes.get(close + 1) != Some(&b'(') {
        return None;
    }
    let from = close + 2;
    let mut end = from;
    for c in text[from..].chars() {
        if c == ')' {
            break;
        }
        if c.is_whitespace() {
            return None;
        }
        end += c.len_utf8();
    }
    if end == from || bytes.get(end) != Some(&b')') {
        return None;
    }
    Some(MarkdownLink {
        span: start..end + 1,
        label: Some(text[start + 1..close].to_owned()),
        href: text[from..end].to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tit(chapter: u16, verse: u16) -> BibleRef {
        BibleRef::verse(BookId::Titus, ChapterVerse::new(chapter, verse).unwrap())
    }

    #[test]
    fn relative_note_links_resolve_from_the_note_that_holds_them() {
        let intro = LinkBase::Note {
            book: BookId::Titus,
            chapter: None,
        };
        assert_eq!(
            resolve_link("../01/01.md", &intro).unwrap(),
            LinkTarget::Bible(tit(1, 1))
        );
        let ch1 = LinkBase::Note {
            book: BookId::Titus,
            chapter: Some(1),
        };
        assert_eq!(
            resolve_link("../01/09/pzi1", &ch1).unwrap(),
            LinkTarget::Note {
                reference: tit(1, 9),
                id: RowId::parse("pzi1").unwrap()
            }
        );
        let jonah = LinkBase::Note {
            book: BookId::Jonah,
            chapter: Some(1),
        };
        assert_eq!(
            resolve_link("../../gen/30/08.md", &jonah).unwrap(),
            LinkTarget::Bible(BibleRef::verse(
                BookId::Genesis,
                ChapterVerse::new(30, 8).unwrap()
            ))
        );
        assert!(resolve_link("../../../../x", &jonah).is_err());
    }

    #[test]
    fn markdown_links_are_found_with_their_spans() {
        let text = "See [[rc://*/ta/man/translate/figs-metaphor]] and [1:1](../01/01.md), \
                    not ![img](x.jpg) or \\[x](y) or [x\\](y) or `[x](y)`.\n```\n[x](y)\n```\n";
        let links = find_links(text);
        assert_eq!(links.len(), 2);
        assert_eq!(
            &text[links[0].span.clone()],
            "[[rc://*/ta/man/translate/figs-metaphor]]"
        );
        assert_eq!(links[1].label.as_deref(), Some("1:1"));
        assert_eq!(&text[links[1].span.clone()], "[1:1](../01/01.md)");
    }

    #[test]
    fn escapes_count_backslashes_and_double_brackets_hold_one_href() {
        let hrefs =
            |text: &str| -> Vec<String> { find_links(text).into_iter().map(|l| l.href).collect() };
        // An even run of backslashes is escaped backslashes; the bracket opens a link.
        assert_eq!(hrefs(r"\\[a](x) \\\[b](y)"), ["x"]);
        assert_eq!(hrefs(r"[a\]b](x)"), ["x"]);
        assert_eq!(hrefs(r"\![a](x) ![b](y)"), ["x"]);
        // A stray `[` before a link does not swallow it into a `[[href]]`.
        assert_eq!(hrefs("[[a](x) and y]]"), ["x"]);
        assert_eq!(
            hrefs("[[rc://*/tw/dict/bible/kt/god]]"),
            ["rc://*/tw/dict/bible/kt/god"]
        );
    }
}
