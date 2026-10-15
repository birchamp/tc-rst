//! The Door43 TSV tables: translation notes (the current seven-column table and the
//! legacy nine-column one), translation words links, and translation questions, for Bible
//! books and for Open Bible Stories.
//!
//! The header row is the version of a table. A header this module does not know is
//! refused, never guessed at. Rows are split on tabs only (the tables have no quoting).
//! A row that does not parse refuses the whole table, with the line and column named.
//!
//! In the seven-column notes table, and in the question and response columns, a newline
//! is written `\n` and a backslash `\\`. The model holds the decoded markdown.

use tc_core::BookId;

use crate::Error;
use crate::error::{CellProblem, LinkProblem, TsvTable, WriteProblem};
use crate::ids::{
    AcademyArticleId, HelpItemId, Manual, Occurrence, Quote, RowId, Slug, WordArticleId,
};
use crate::links::{LinkBase, LinkTarget, resolve_link};
use crate::model::{Note, Question, WordLink};
use crate::refs::{BibleLocus, BibleRef, HelpRef, ObsRef, TsvLocus, number, parse_tsv_locus};

const NOTES: [&str; 7] = [
    "Reference",
    "ID",
    "Tags",
    "SupportReference",
    "Quote",
    "Occurrence",
    "Note",
];
const NOTES_LEGACY: [&str; 9] = [
    "Book",
    "Chapter",
    "Verse",
    "ID",
    "SupportReference",
    "OrigQuote",
    "Occurrence",
    "GLQuote",
    "OccurrenceNote",
];
const WORD_LINKS: [&str; 6] = [
    "Reference",
    "ID",
    "Tags",
    "OrigWords",
    "Occurrence",
    "TWLink",
];
const QUESTIONS: [&str; 7] = [
    "Reference",
    "ID",
    "Tags",
    "Quote",
    "Occurrence",
    "Question",
    "Response",
];

/// What a table is about: one Bible book, or all fifty Open Bible Stories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsvScope {
    Book(BookId),
    Obs,
}

/// The versions of the notes table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotesLayout {
    /// `Reference ID Tags SupportReference Quote Occurrence Note`, the current table.
    Seven,
    /// `Book Chapter Verse ID SupportReference OrigQuote Occurrence GLQuote OccurrenceNote`,
    /// the table before 2021. Bible books only.
    LegacyNine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedNotes {
    pub layout: NotesLayout,
    pub notes: Vec<Note>,
}

/// How a words-links table writes its links to articles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordLinkStyle {
    /// `rc://*/tw/dict/bible/kt/god`, as in RC repositories and the OBS export.
    Rc,
    /// `./payload/kt/god.md`, as in the DCS Scripture Burrito export of Bible books.
    ExportPath,
}

struct Row<'a> {
    table: TsvTable,
    header: &'static [&'static str],
    line: usize,
    cells: Vec<&'a str>,
}

impl<'a> Row<'a> {
    /// `rows` refuses a row whose cell count differs from the header's, so every column
    /// index of the header is in range.
    fn cell(&self, i: usize) -> &'a str {
        self.cells[i]
    }

    fn fail(&self, i: usize, problem: CellProblem) -> Error {
        Error::TsvCell {
            table: self.table,
            line: self.line,
            column: self.header[i],
            problem,
        }
    }

    fn reference(&self, i: usize, scope: TsvScope) -> Result<HelpRef, Error> {
        let text = self.cell(i);
        let parsed = match scope {
            TsvScope::Book(book) => {
                parse_tsv_locus(text).map(|locus| HelpRef::Bible(BibleRef { book, locus }))
            }
            TsvScope::Obs => ObsRef::parse(text).map(HelpRef::Obs),
        };
        parsed.ok_or_else(|| self.fail(i, CellProblem::Reference(text.to_owned())))
    }

    fn id(&self, i: usize) -> Result<HelpItemId, Error> {
        RowId::parse(self.cell(i))
            .map(HelpItemId::Row)
            .map_err(|_| self.fail(i, CellProblem::RowId(self.cell(i).to_owned())))
    }

    fn occurrence(&self, i: usize) -> Result<Occurrence, Error> {
        Occurrence::parse_cell(self.cell(i))
            .ok_or_else(|| self.fail(i, CellProblem::Occurrence(self.cell(i).to_owned())))
    }

    fn support_reference(&self, i: usize) -> Result<Option<AcademyArticleId>, Error> {
        let text = self.cell(i);
        if text.is_empty() {
            return Ok(None);
        }
        // The legacy table sometimes names the article by its bare slug; those articles are
        // all in the translate manual.
        if !text.contains('/') {
            return Slug::parse(text)
                .map(|slug| {
                    Some(AcademyArticleId::Dcs {
                        manual: Manual::Translate,
                        slug,
                    })
                })
                .map_err(|_| self.fail(i, CellProblem::Link(LinkProblem::Slug(text.to_owned()))));
        }
        match resolve_link(text, &LinkBase::Absolute) {
            Ok(LinkTarget::Academy(id)) => Ok(Some(id)),
            Ok(_) => Err(self.fail(i, CellProblem::Link(LinkProblem::NoTarget))),
            Err(Error::Link { problem, .. }) => Err(self.fail(i, CellProblem::Link(problem))),
            Err(other) => Err(other),
        }
    }
}

/// Splits a table into rows, checking the header and the column count of every row.
/// Blank lines are skipped. Line numbers count from 1 at the header.
fn rows<'a>(
    text: &'a str,
    table: TsvTable,
    header: &'static [&'static str],
) -> Result<Vec<Row<'a>>, Error> {
    let mut lines = text.split('\n').enumerate();
    let first = lines.next().map(|(_, l)| l).unwrap_or("");
    check_header(first, table, header)?;
    let mut out = Vec::new();
    for (i, line) in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line
            .split('\t')
            .map(|c| c.strip_suffix('\r').unwrap_or(c))
            .collect();
        if cells.len() != header.len() {
            return Err(Error::TsvColumnCount {
                table,
                line: i + 1,
                found: cells.len(),
                expected: header.len(),
            });
        }
        out.push(Row {
            table,
            header,
            line: i + 1,
            cells,
        });
    }
    Ok(out)
}

fn header_text(first: &str) -> &str {
    let first = first.strip_prefix('\u{feff}').unwrap_or(first);
    first.strip_suffix('\r').unwrap_or(first)
}

fn check_header(first: &str, table: TsvTable, header: &[&str]) -> Result<(), Error> {
    let found = header_text(first);
    if found.split('\t').eq(header.iter().copied()) {
        Ok(())
    } else {
        Err(Error::TsvHeader {
            table,
            found: found.to_owned(),
        })
    }
}

fn tags(cell: &str) -> Vec<String> {
    cell.split(';')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `\n` is a newline and `\\` a backslash; any other backslash is kept as it is.
fn decode_escapes(cell: &str) -> String {
    let mut out = String::with_capacity(cell.len());
    let mut chars = cell.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('n') => {
                    out.push('\n');
                    chars.next();
                }
                Some('\\') => {
                    out.push('\\');
                    chars.next();
                }
                _ => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn encode_escapes(text: &str, table: TsvTable) -> Result<String, Error> {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' | '\r' => return Err(unwritable(table, text)),
            c => out.push(c),
        }
    }
    Ok(out)
}

fn unwritable(table: TsvTable, text: &str) -> Error {
    Error::TsvWrite {
        table,
        problem: WriteProblem::Unwritable(text.to_owned()),
    }
}

/// A cell that is written as it is: it must not hold a tab or a line break.
fn plain(text: &str, table: TsvTable) -> Result<&str, Error> {
    if text.contains(['\t', '\n', '\r']) {
        Err(unwritable(table, text))
    } else {
        Ok(text)
    }
}

/// Parses a translation notes table, current or legacy, chosen by its header.
pub fn parse_notes(text: &str, scope: TsvScope) -> Result<ParsedNotes, Error> {
    let first = header_text(text.split('\n').next().unwrap_or(""));
    let legacy = first.split('\t').eq(NOTES_LEGACY.iter().copied());
    match (legacy, scope) {
        (true, TsvScope::Book(book)) => {
            let notes = rows(text, TsvTable::Notes, &NOTES_LEGACY)?
                .iter()
                .map(|row| legacy_note(row, book))
                .collect::<Result<_, _>>()?;
            Ok(ParsedNotes {
                layout: NotesLayout::LegacyNine,
                notes,
            })
        }
        _ => {
            let notes = rows(text, TsvTable::Notes, &NOTES)?
                .iter()
                .map(|row| note(row, scope))
                .collect::<Result<_, _>>()?;
            Ok(ParsedNotes {
                layout: NotesLayout::Seven,
                notes,
            })
        }
    }
}

fn note(row: &Row, scope: TsvScope) -> Result<Note, Error> {
    Ok(Note {
        reference: row.reference(0, scope)?,
        id: row.id(1)?,
        tags: tags(row.cell(2)),
        support_reference: row.support_reference(3)?,
        quote: Quote::new(row.cell(4)),
        occurrence: row.occurrence(5)?,
        gl_quote: None,
        text: decode_escapes(row.cell(6)),
    })
}

fn legacy_note(row: &Row, book: BookId) -> Result<Note, Error> {
    let code = row.cell(0);
    let found: BookId = code
        .parse()
        .map_err(|_| row.fail(0, CellProblem::Book(code.to_owned())))?;
    if found != book {
        return Err(row.fail(
            0,
            CellProblem::WrongBook {
                found: found.code().to_owned(),
                expected: book.code().to_owned(),
            },
        ));
    }
    let (chapter, verse) = (row.cell(1), row.cell(2));
    let chapter_ok = chapter == "front" || number(chapter).is_some();
    let locus = parse_tsv_locus(&format!("{chapter}:{verse}")).ok_or_else(|| {
        let (i, text) = if chapter_ok { (2, verse) } else { (1, chapter) };
        row.fail(i, CellProblem::Reference(text.to_owned()))
    })?;
    let gl = row.cell(7);
    Ok(Note {
        reference: HelpRef::Bible(BibleRef { book, locus }),
        id: row.id(3)?,
        tags: Vec::new(),
        support_reference: row.support_reference(4)?,
        quote: Quote::new(row.cell(5)),
        occurrence: row.occurrence(6)?,
        gl_quote: (!gl.is_empty()).then(|| gl.to_owned()),
        text: row.cell(8).replace("<br>", "\n"),
    })
}

/// Parses a translation words links table. `TWLink` may be an `rc://` link or an export
/// path; both name the same article.
pub fn parse_word_links(text: &str, scope: TsvScope) -> Result<Vec<WordLink>, Error> {
    rows(text, TsvTable::WordLinks, &WORD_LINKS)?
        .iter()
        .map(|row| {
            let link = row.cell(5);
            let article = match resolve_link(link, &LinkBase::ExportRoot) {
                Ok(LinkTarget::Word(id)) => id,
                Ok(_) => {
                    return Err(row.fail(5, CellProblem::Link(LinkProblem::NoTarget)));
                }
                Err(Error::Link { problem, .. }) => {
                    return Err(row.fail(5, CellProblem::Link(problem)));
                }
                Err(other) => return Err(other),
            };
            Ok(WordLink {
                reference: row.reference(0, scope)?,
                id: row.id(1)?,
                tags: tags(row.cell(2)),
                orig_words: Quote::new(row.cell(3)),
                occurrence: row.occurrence(4)?,
                article,
            })
        })
        .collect()
}

/// Parses a translation questions table.
pub fn parse_questions(text: &str, scope: TsvScope) -> Result<Vec<Question>, Error> {
    rows(text, TsvTable::Questions, &QUESTIONS)?
        .iter()
        .map(|row| {
            Ok(Question {
                reference: row.reference(0, scope)?,
                id: row.id(1)?,
                tags: tags(row.cell(2)),
                quote: Quote::new(row.cell(3)),
                occurrence: row.occurrence(4)?,
                question: decode_escapes(row.cell(5)),
                response: decode_escapes(row.cell(6)),
            })
        })
        .collect()
}

fn row_id(id: &HelpItemId, table: TsvTable, what: &'static str) -> Result<String, Error> {
    match id {
        HelpItemId::Row(id) => Ok(id.as_str().to_owned()),
        HelpItemId::Aquifer(_) => Err(Error::TsvWrite {
            table,
            problem: WriteProblem::AquiferItem { what },
        }),
    }
}

fn support_cell(id: &Option<AcademyArticleId>) -> Result<String, Error> {
    match id {
        None => Ok(String::new()),
        Some(AcademyArticleId::Dcs { manual, slug }) => {
            Ok(format!("rc://*/ta/man/{}/{}", manual.folder(), slug))
        }
        Some(AcademyArticleId::Aquifer(_)) => Err(Error::TsvWrite {
            table: TsvTable::Notes,
            problem: WriteProblem::AquiferItem {
                what: "Translation Academy link",
            },
        }),
    }
}

fn table_text(header: &[&str], rows: Vec<Vec<String>>) -> String {
    let mut out = header.join("\t");
    out.push('\n');
    for row in rows {
        out.push_str(&row.join("\t"));
        out.push('\n');
    }
    out
}

/// Writes notes as a table of the given layout. Values the layout has no column for are
/// refused rather than dropped.
pub fn write_notes(notes: &[Note], layout: NotesLayout) -> Result<String, Error> {
    let t = TsvTable::Notes;
    let mut rows = Vec::with_capacity(notes.len());
    match layout {
        NotesLayout::Seven => {
            for n in notes {
                if n.gl_quote.is_some() {
                    return Err(Error::TsvWrite {
                        table: t,
                        problem: WriteProblem::NoColumn { column: "GLQuote" },
                    });
                }
                rows.push(vec![
                    n.reference.tsv_text(),
                    row_id(&n.id, t, "note")?,
                    plain(&n.tags.join("; "), t)?.to_owned(),
                    support_cell(&n.support_reference)?,
                    plain(n.quote.as_str(), t)?.to_owned(),
                    n.occurrence.cell(),
                    encode_escapes(&n.text, t)?,
                ]);
            }
            Ok(table_text(&NOTES, rows))
        }
        NotesLayout::LegacyNine => {
            let mut book: Option<BookId> = None;
            for n in notes {
                let HelpRef::Bible(r) = &n.reference else {
                    return Err(Error::TsvWrite {
                        table: t,
                        problem: WriteProblem::Reference(n.reference.tsv_text()),
                    });
                };
                match book {
                    Some(b) if b != r.book => {
                        return Err(Error::TsvWrite {
                            table: t,
                            problem: WriteProblem::MixedBooks(
                                b.code().to_owned(),
                                r.book.code().to_owned(),
                            ),
                        });
                    }
                    _ => book = Some(r.book),
                }
                if !n.tags.is_empty() {
                    return Err(Error::TsvWrite {
                        table: t,
                        problem: WriteProblem::NoColumn { column: "Tags" },
                    });
                }
                let (chapter, verse) = legacy_place(&r.locus).ok_or_else(|| Error::TsvWrite {
                    table: t,
                    problem: WriteProblem::Reference(r.to_string()),
                })?;
                if n.text.contains("<br>") {
                    return Err(unwritable(t, &n.text));
                }
                rows.push(vec![
                    r.book.code().to_owned(),
                    chapter,
                    verse,
                    row_id(&n.id, t, "note")?,
                    support_cell(&n.support_reference)?,
                    plain(n.quote.as_str(), t)?.to_owned(),
                    n.occurrence.cell(),
                    plain(n.gl_quote.as_deref().unwrap_or(""), t)?.to_owned(),
                    plain(&n.text.replace('\n', "<br>"), t)?.to_owned(),
                ]);
            }
            Ok(table_text(&NOTES_LEGACY, rows))
        }
    }
}

/// The `Chapter` and `Verse` cells of the legacy table: one chapter per row.
fn legacy_place(locus: &BibleLocus) -> Option<(String, String)> {
    match locus {
        BibleLocus::BookIntro => Some(("front".into(), "intro".into())),
        BibleLocus::ChapterIntro(c) => Some((c.to_string(), "intro".into())),
        BibleLocus::Verses(list) => {
            let [range] = list.ranges() else { return None };
            if range.start().chapter != range.end().chapter {
                return None;
            }
            let text = TsvLocus(locus).to_string();
            let (chapter, verse) = text.split_once(':')?;
            Some((chapter.to_owned(), verse.to_owned()))
        }
    }
}

/// Writes a words links table.
pub fn write_word_links(links: &[WordLink], style: WordLinkStyle) -> Result<String, Error> {
    let t = TsvTable::WordLinks;
    let mut rows = Vec::with_capacity(links.len());
    for l in links {
        let link = match (&l.article, style) {
            (WordArticleId::Dcs { category, slug }, WordLinkStyle::Rc) => {
                format!("rc://*/tw/dict/bible/{}/{}", category.folder(), slug)
            }
            (WordArticleId::Dcs { category, slug }, WordLinkStyle::ExportPath) => {
                format!("./payload/{}/{}.md", category.folder(), slug)
            }
            (WordArticleId::Aquifer(_), _) => {
                return Err(Error::TsvWrite {
                    table: t,
                    problem: WriteProblem::AquiferItem {
                        what: "translation words article",
                    },
                });
            }
        };
        rows.push(vec![
            l.reference.tsv_text(),
            row_id(&l.id, t, "word link")?,
            plain(&l.tags.join("; "), t)?.to_owned(),
            plain(l.orig_words.as_str(), t)?.to_owned(),
            l.occurrence.cell(),
            link,
        ]);
    }
    Ok(table_text(&WORD_LINKS, rows))
}

/// Writes a questions table.
pub fn write_questions(questions: &[Question]) -> Result<String, Error> {
    let t = TsvTable::Questions;
    let mut rows = Vec::with_capacity(questions.len());
    for q in questions {
        rows.push(vec![
            q.reference.tsv_text(),
            row_id(&q.id, t, "question")?,
            plain(&q.tags.join("; "), t)?.to_owned(),
            plain(q.quote.as_str(), t)?.to_owned(),
            q.occurrence.cell(),
            encode_escapes(&q.question, t)?,
            encode_escapes(&q.response, t)?,
        ]);
    }
    Ok(table_text(&QUESTIONS, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_decode_and_encode() {
        let cell = r"# Title\n\nSee \\*\\*And\\*\\*.";
        let text = decode_escapes(cell);
        assert_eq!(text, "# Title\n\nSee \\*\\*And\\*\\*.");
        assert_eq!(encode_escapes(&text, TsvTable::Notes).unwrap(), cell);
        // A backslash before any other letter is kept as it is.
        assert_eq!(decode_escapes(r"\v 1"), r"\v 1");
        assert!(encode_escapes("a\tb", TsvTable::Notes).is_err());
    }
}
