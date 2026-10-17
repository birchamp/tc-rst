//! The real data of `fixtures/scale/` and of the bundled English suite in `bundle/en/` (see
//! their READMEs), read into plain values: aligned ULT books, translationNotes and
//! translationWords Links rows, the World English Bible Psalms, the Hebrew Bible and the
//! Greek New Testament.

use crate::CorpusError;
use crate::domain::{GroupInput, SourceWordInput, TargetWordInput};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use tc_core::BookId;
use tc_usfm::codec::{self, VersePart};

/// The Psalms files of `fixtures/scale/psa/`, relative to the repository root.
pub const PSA_ULT: &str = "fixtures/scale/psa/en_ult/19-PSA.usfm";
pub const PSA_TN: &str = "fixtures/scale/psa/en_tn/tn_PSA.tsv";
pub const PSA_TWL: &str = "fixtures/scale/psa/en_twl/twl_PSA.tsv";
pub const PSA_WEB: &str = "fixtures/scale/psa/web/web_psa.usx";
pub const PSA_UHB: &str = "fixtures/scale/psa/hbo_uhb/19-PSA.usfm";

/// The unfoldingWord releases of the bundled English suite, `bundle/en/`: the DCS Scripture
/// Burrito export of each pinned tag, the files the app ships. A book is
/// `<repo>/ingredients/<BOOK>.usfm` or `.tsv`.
pub const ULT_ZIP: &str = "bundle/en/en_ult_v91.sb.zip";
pub const TN_ZIP: &str = "bundle/en/en_tn_v91.sb.zip";
pub const TWL_ZIP: &str = "bundle/en/en_twl_v91.sb.zip";
pub const UHB_ZIP: &str = "bundle/en/hbo_uhb_v3.0.0.sb.zip";
pub const UGNT_ZIP: &str = "bundle/en/el-x-koine_ugnt_v0.34.sb.zip";

/// The commits of the bundled releases (`bundle/en/README.md`), which the corpus pins.
pub const TN_COMMIT: &str = "e586762e330f482a60c52aedd1c7b3a2f155df8a";
pub const TWL_COMMIT: &str = "7cbb3419f79780ce84b92ad7d42bd476cc5d8a64";
pub const UHB_COMMIT: &str = "74022f0fed012a3ef169886f595dd98e7b200543";
pub const UGNT_COMMIT: &str = "fc95b2b8aad08bb65ab54628ab685413a1139e97";

fn fixture_error(path: &str, reason: impl ToString) -> CorpusError {
    CorpusError::Fixture {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}

fn read_text(root: &Path, rel: &str) -> Result<String, CorpusError> {
    std::fs::read_to_string(root.join(rel)).map_err(|e| fixture_error(rel, e))
}

/// One verse of an aligned ULT book, with its alignment taken out.
#[derive(Debug, Clone)]
pub struct UltVerse {
    pub chapter: u16,
    pub verse: u16,
    pub last: u16,
    /// The words of the verse with the alignment markup removed (§2.4 `words`).
    pub words: String,
    /// The trailing paragraph-marker lines (§2.4 `layout`).
    pub layout: String,
    pub groups: Vec<GroupInput>,
}

/// An aligned ULT book taken apart.
#[derive(Debug, Clone)]
pub struct UltBook {
    /// Every byte before the first `\c`.
    pub header: String,
    /// The chapter fronts with their alignment removed, by chapter.
    pub fronts: BTreeMap<u16, String>,
    /// In book order.
    pub verses: Vec<UltVerse>,
}

/// Heading markers the ULT puts aligned words in.
const HEADING_MARKERS: [&str; 8] = [
    "\\d ", "\\qa ", "\\qd ", "\\s1 ", "\\s2 ", "\\s ", "\\ms ", "\\mr ",
];

/// Takes an aligned USFM book apart into verses without alignment markup and the groups.
pub fn parse_ult(text: &str, path: &str) -> Result<UltBook, CorpusError> {
    let parts = codec::decompose(text).map_err(|e| fixture_error(path, e))?;
    let mut fronts = BTreeMap::new();
    let mut verses = Vec::new();
    for (chapter, parts) in &parts.chapters {
        let front = parts
            .front
            .clone()
            .unwrap_or_else(|| format!("\\c {chapter}\n"));
        fronts.insert(*chapter, strip_front(&front));
        for (verse, part) in &parts.verses {
            let VersePart::Text(text) = part else {
                continue;
            };
            let (plain, groups) = match tc_usfm::parse_aligned(&text.words) {
                Ok(aligned) => (aligned.plain, aligned.groups),
                // tc-usfm refuses alignment inside a heading, but the ULT aligns the
                // superscriptions of the Psalms (`\\d`) and the acrostic letters of Psalm
                // 119 (`\\qa`). Their groups are read without the heading markers; the
                // text keeps them.
                Err(_) => {
                    let mut unheaded = text.words.clone();
                    for marker in HEADING_MARKERS {
                        unheaded = unheaded.replace(marker, "");
                    }
                    let groups = tc_usfm::parse_aligned(&unheaded)
                        .map(|aligned| aligned.groups)
                        .unwrap_or_default();
                    (strip_front(&text.words), groups)
                }
            };
            let content = format!("{plain}{}", text.layout);
            let (words, layout) = codec::split_verse(&content);
            verses.push(UltVerse {
                chapter: *chapter,
                verse: *verse,
                last: text.last,
                words: words.to_owned(),
                layout: layout.to_owned(),
                groups: groups.iter().map(group_input).collect(),
            });
        }
    }
    Ok(UltBook {
        header: strip_front(&parts.header),
        fronts,
        verses,
    })
}

fn group_input(group: &tc_usfm::AlignmentGroup) -> GroupInput {
    GroupInput {
        sources: group
            .sources
            .iter()
            .map(|s| SourceWordInput {
                word: s.content.clone(),
                strong: s.strong.clone(),
                lemma: s.lemma.clone(),
                morph: s.morph.clone(),
                occurrence: s.occurrence,
                occurrences: s.occurrences,
            })
            .collect(),
        targets: group
            .targets
            .iter()
            .map(|t| TargetWordInput {
                word: t.text.clone(),
                occurrence: t.occurrence,
                occurrences: t.occurrences,
            })
            .collect(),
    }
}

/// USFM outside verses without alignment markup: `\zaln` milestones and `\w` wrappers are
/// removed, and aligned words, which sit on lines of their own, are joined into one line.
fn strip_front(front: &str) -> String {
    if !front.contains("\\zaln") && !front.contains("\\w ") {
        return front.to_owned();
    }
    let mut out = String::with_capacity(front.len());
    let mut rest = front;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("\\zaln-s") {
            let end = after.find("\\*").map_or(after.len(), |at| at + 2);
            rest = &after[end..];
        } else if let Some(after) = rest.strip_prefix("\\zaln-e\\*") {
            rest = after;
        } else if let Some(after) = rest.strip_prefix("\\w ") {
            let bar = after.find('|').unwrap_or(after.len());
            let close = after.find("\\w*").unwrap_or(after.len());
            out.push_str(&after[..bar.min(close)]);
            rest = &after[(close + 3).min(after.len())..];
        } else {
            let mut chars = rest.chars();
            if let Some(c) = chars.next() {
                out.push(c);
            }
            rest = chars.as_str();
        }
    }
    let mut joined = String::with_capacity(out.len());
    for (i, line) in out.split('\n').enumerate() {
        if i > 0 {
            let starts_marker = line.starts_with('\\');
            if starts_marker || line.is_empty() || joined.ends_with('\n') {
                joined.push('\n');
            } else {
                joined.push(' ');
            }
        }
        joined.push_str(line);
    }
    joined
}

/// One translationNotes or translationWords Links row, as the TSV has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpsRow {
    /// `1`, `front`.
    pub chapter: String,
    /// `1`, `1-3`, `intro`.
    pub verse: String,
    pub id: String,
    /// The Translation Academy article (`figs-metaphor`) or the translationWords article
    /// (`counselor`).
    pub group: String,
    /// `translate` for notes; `kt`, `names` or `other` for words.
    pub category: String,
    pub quote: String,
    pub occurrence: u32,
    pub note: String,
}

/// A reference part as tC4 reads it (`refPart`, `src/data/derive.ts`): plain digits are a
/// number, so leading zeros go; anything else is kept as written, a list such as `1,3,8,12`
/// included. A part a check key cannot hold (empty, or holding `|` or `:`, STORAGE §2.2) is
/// no reference.
fn ref_part(text: &str) -> Option<String> {
    let part = if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        let trimmed = text.trim_start_matches('0');
        if trimmed.is_empty() { "0" } else { trimmed }
    } else {
        text
    };
    tc_journal::RefPart::parse(part)
        .ok()
        .map(|p| p.as_str().to_owned())
}

fn occurrence(text: &str) -> u32 {
    text.trim()
        .parse::<i64>()
        .map_or(1, |n| u32::try_from(n).unwrap_or(0))
}

fn split_reference(reference: &str) -> Option<(String, String)> {
    let (chapter, verse) = reference.split_once(':')?;
    Some((ref_part(chapter)?, ref_part(verse)?))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && !id.contains(['|', ':']) && !id.chars().any(char::is_control)
}

/// translationNotes v91 rows: `Reference ID Tags SupportReference Quote Occurrence Note`.
pub fn parse_tn(text: &str) -> Vec<HelpsRow> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let cells: Vec<&str> = line.split('\t').collect();
        let [reference, id, _tags, support, quote, occurrence_text, note] = cells[..] else {
            continue;
        };
        let Some((chapter, verse)) = split_reference(reference) else {
            continue;
        };
        if !valid_id(id) {
            continue;
        }
        let group = support
            .rsplit('/')
            .next()
            .filter(|g| !g.is_empty())
            .unwrap_or("translate-unknown")
            .to_owned();
        out.push(HelpsRow {
            chapter,
            verse,
            id: id.to_owned(),
            group,
            category: "translate".into(),
            quote: quote.to_owned(),
            occurrence: occurrence(occurrence_text),
            note: note.replace("\\n", "\n"),
        });
    }
    out
}

/// translationWords Links v91 rows: `Reference ID Tags OrigWords Occurrence TWLink`.
pub fn parse_twl(text: &str) -> Vec<HelpsRow> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let cells: Vec<&str> = line.split('\t').collect();
        let [reference, id, _tags, words, occurrence_text, link] = cells[..] else {
            continue;
        };
        let Some((chapter, verse)) = split_reference(reference) else {
            continue;
        };
        if !valid_id(id) {
            continue;
        }
        // `rc://*/tw/dict/bible/kt/god` in the repository, `./payload/kt/god.md` in the
        // burrito export (`bundle/en/README.md`): the same article.
        let mut segments = link.rsplit('/');
        let last = segments.next().unwrap_or("");
        let group = last.strip_suffix(".md").unwrap_or(last).to_owned();
        let category = segments.next().unwrap_or("other").to_owned();
        out.push(HelpsRow {
            chapter,
            verse,
            id: id.to_owned(),
            group,
            category,
            quote: words.to_owned(),
            occurrence: occurrence(occurrence_text),
            note: String::new(),
        });
    }
    out
}

/// The text of each verse of a USX book: character data between a verse's start and end
/// milestones, outside notes, with runs of whitespace made one space.
pub fn parse_usx(text: &str) -> BTreeMap<(u16, u16), String> {
    let mut out = BTreeMap::new();
    let mut current: Option<((u16, u16), String)> = None;
    let mut note_depth = 0usize;
    let mut rest = text;
    while let Some(lt) = rest.find('<') {
        let data = &rest[..lt];
        if note_depth == 0
            && let Some((_, buf)) = current.as_mut()
        {
            buf.push_str(&unescape(data));
        }
        let Some(gt) = rest[lt..].find('>') else {
            break;
        };
        let tag = &rest[lt + 1..lt + gt];
        rest = &rest[lt + gt + 1..];
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let closing = tag.starts_with('/');
        let self_closing = tag.ends_with('/');
        match name {
            "note" if !closing && !self_closing => note_depth += 1,
            "note" if closing => note_depth = note_depth.saturating_sub(1),
            "verse" => {
                if let Some(sid) = attribute(tag, "sid") {
                    current = usx_reference(sid).map(|r| (r, String::new()));
                } else if attribute(tag, "eid").is_some()
                    && let Some((r, buf)) = current.take()
                {
                    let clean = buf.split_whitespace().collect::<Vec<_>>().join(" ");
                    out.insert(r, clean);
                }
            }
            "para" if note_depth == 0 => {
                if let Some((_, buf)) = current.as_mut() {
                    buf.push(' ');
                }
            }
            _ => {}
        }
    }
    out
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let len = tag[start..].find('"')?;
    Some(&tag[start..start + len])
}

/// `PSA 1:2` as (1, 2).
fn usx_reference(sid: &str) -> Option<(u16, u16)> {
    let (_, cv) = sid.split_once(' ')?;
    let (c, v) = cv.split_once(':')?;
    Some((c.parse().ok()?, v.parse().ok()?))
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The Psalms data of `fixtures/scale/psa/`.
#[derive(Debug, Clone)]
pub struct Psalms {
    pub ult: UltBook,
    pub notes: Vec<HelpsRow>,
    pub words: Vec<HelpsRow>,
    /// Plain WEB text by (chapter, verse).
    pub web: BTreeMap<(u16, u16), String>,
    /// The UHB 3.0.0 Psalms.
    pub hebrew: String,
}

impl Psalms {
    pub fn load(root: &Path) -> Result<Self, CorpusError> {
        Ok(Self {
            ult: parse_ult(&read_text(root, PSA_ULT)?, PSA_ULT)?,
            notes: parse_tn(&read_text(root, PSA_TN)?),
            words: parse_twl(&read_text(root, PSA_TWL)?),
            web: parse_usx(&read_text(root, PSA_WEB)?),
            hebrew: read_text(root, PSA_UHB)?,
        })
    }
}

/// One book of the unfoldingWord ULT, tN and TWL releases.
#[derive(Debug, Clone)]
pub struct ReleaseBook {
    pub ult: UltBook,
    pub notes: Vec<HelpsRow>,
    pub words: Vec<HelpsRow>,
}

/// The unfoldingWord releases of the bundled English suite.
#[derive(Debug, Clone)]
pub struct Releases {
    /// The books of the ULT release, with their notes and words links.
    pub books: BTreeMap<BookId, ReleaseBook>,
    /// Every book of the UHB and UGNT releases.
    pub originals: BTreeMap<BookId, String>,
}

/// Reads every book file of a burrito export, `<repo>/ingredients/<BOOK>.<extension>`, by
/// book. Other entries are left alone: the export's front matter (`FRT.usfm`, no book of the
/// canon) and the copy of the other resource that the TWL and tW exports carry under
/// `ingredients/payload/` (`bundle/en/README.md`).
fn zip_books(
    root: &Path,
    rel: &str,
    extension: &str,
) -> Result<BTreeMap<BookId, String>, CorpusError> {
    let file = std::fs::File::open(root.join(rel)).map_err(|e| fixture_error(rel, e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| fixture_error(rel, e))?;
    let mut out = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| fixture_error(rel, e))?;
        let name = entry.name().to_owned();
        let Some(book) = export_book(&name, extension) else {
            continue;
        };
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|e| fixture_error(rel, format!("{name}: {e}")))?;
        out.insert(book, text);
    }
    Ok(out)
}

/// `en_ult/ingredients/PSA.usfm` → Psalms.
fn export_book(name: &str, extension: &str) -> Option<BookId> {
    let mut parts = name.split('/');
    let (_repo, folder, file) = (parts.next()?, parts.next()?, parts.next()?);
    if folder != "ingredients" || parts.next().is_some() {
        return None;
    }
    file.strip_suffix(extension)?.parse().ok()
}

/// Reads the five release exports.
pub fn load_releases(root: &Path) -> Result<Releases, CorpusError> {
    let ult = zip_books(root, ULT_ZIP, ".usfm")?;
    let tn = zip_books(root, TN_ZIP, ".tsv")?;
    let twl = zip_books(root, TWL_ZIP, ".tsv")?;
    let mut originals = zip_books(root, UHB_ZIP, ".usfm")?;
    originals.extend(zip_books(root, UGNT_ZIP, ".usfm")?);
    let mut books = BTreeMap::new();
    for (book, text) in ult {
        let path = format!("{ULT_ZIP}#{}", book.code());
        books.insert(
            book,
            ReleaseBook {
                ult: parse_ult(&text, &path)?,
                notes: tn.get(&book).map(|t| parse_tn(t)).unwrap_or_default(),
                words: twl.get(&book).map(|t| parse_twl(t)).unwrap_or_default(),
            },
        );
    }
    Ok(Releases { books, originals })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helps_rows_keep_their_reference_strings() {
        let tn = "Reference\tID\tTags\tSupportReference\tQuote\tOccurrence\tNote\n\
                  front:intro\trx9u\t\t\t\t0\t# Intro\\n\n\
                  1:1-3\tab12\t\trc://*/ta/man/translate/figs-metaphor\tא ב\t1\tA note\n\
                  1:4,6\tcd34\t\t\tג\t-1\tB\n\
                  bad\tef56\t\t\t\t1\tC\n";
        let rows = parse_tn(tn);
        assert_eq!(rows.len(), 3);
        assert_eq!(
            (rows[0].chapter.as_str(), rows[0].verse.as_str()),
            ("front", "intro")
        );
        assert_eq!(rows[0].occurrence, 0);
        assert_eq!(rows[0].note, "# Intro\n");
        assert_eq!(rows[1].verse, "1-3");
        assert_eq!(rows[1].group, "figs-metaphor");
        assert_eq!(rows[2].verse, "4,6");
        assert_eq!(rows[2].occurrence, 0);
        let twl = "Reference\tID\tTags\tOrigWords\tOccurrence\tTWLink\n\
                   1:1\txlen\t\tעֵצָה\t1\trc://*/tw/dict/bible/other/counselor\n";
        let words = parse_twl(twl);
        assert_eq!(words[0].group, "counselor");
        assert_eq!(words[0].category, "other");
        // The burrito export links the same article by its file in `payload/`.
        let exported = twl.replace(
            "rc://*/tw/dict/bible/other/counselor",
            "./payload/other/counselor.md",
        );
        let from_export = parse_twl(&exported);
        assert_eq!(
            (
                from_export[0].group.as_str(),
                from_export[0].category.as_str()
            ),
            ("counselor", "other")
        );
    }

    #[test]
    fn only_the_book_files_of_an_export_are_books() {
        assert_eq!(
            export_book("en_ult/ingredients/PSA.usfm", ".usfm"),
            Some(BookId::Psalms)
        );
        assert_eq!(
            export_book("en_twl/ingredients/TIT.tsv", ".tsv"),
            Some(BookId::Titus)
        );
        for (other, extension) in [
            ("en_ult/ingredients/FRT.usfm", ".usfm"),
            ("en_ult/ingredients/PSA.usfm", ".tsv"),
            ("en_twl/ingredients/payload/kt/god.md", ".md"),
            ("en_ult/PSA.usfm", ".usfm"),
            ("en_ult/metadata.json", ".json"),
        ] {
            assert_eq!(export_book(other, extension), None, "{other}");
        }
    }

    #[test]
    fn a_usx_verse_is_its_text_outside_notes() {
        let usx = r#"<para style="p"><verse number="1" style="v" sid="PSA 1:1"/>Blessed <note caller="+">x</note>is the man<verse eid="PSA 1:1"/></para>"#;
        let verses = parse_usx(usx);
        assert_eq!(
            verses.get(&(1, 1)).map(String::as_str),
            Some("Blessed is the man")
        );
    }
}
