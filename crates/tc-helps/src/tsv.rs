//! The Door43 TSV tables: translation notes (the current seven-column table and the
//! legacy nine-column one), translation words links, and translation questions, for Bible
//! books and for Open Bible Stories.
//!
//! The header row is the version of a table. A header this module does not know is
//! refused, never guessed at. Rows are split on tabs only (the tables have no quoting). A
//! cell is read as tC4's `tsvRows` reads it: a CR at its end is not part of it.
//! A row that does not parse refuses the whole table, with the line and column named. The
//! `Reference` of a Bible table always parses: a form this crate gives no place to, such as
//! `1:1a`, is kept as written ([`BibleLocus::AsWritten`]), because tC4 reads every row
//! whatever its reference and one such row must not cost a book its helps.
//!
//! In the seven-column notes table, and in the question and response columns, a newline
//! is written `\n` and a backslash `\\`. The model holds the decoded markdown.
//!
//! # Writing
//!
//! A table holds one book, or Open Bible Stories; the writers refuse an item from
//! elsewhere, and any value the table has no column or form for, rather than drop it.
//! Each item read from a table keeps, in [`ReadCells`], the cells its row wrote in a form
//! the writers would not produce (`rc://es-419/…` links, bare slugs, `.md` suffixes, `a;b`
//! tags, `01` occurrences, `1:1-1:3` references, a lone backslash). A writer writes such a
//! cell back as it was read while the item still holds the value read from it, so a table
//! read and written unchanged keeps its bytes, and an edit changes only the cells it
//! touches. The writers write no byte order mark and LF line ends. A cell whose text ends
//! in a CR is written with one more, which reading drops.

use tc_core::BookId;

use crate::Error;
use crate::articles::without_bom;
use crate::error::{CellProblem, LinkProblem, TsvTable, WriteProblem};
use crate::ids::{
    AcademyArticleId, HelpItemId, Manual, Occurrence, Quote, RowId, Slug, WordArticleId,
};
use crate::links::{LinkBase, LinkTarget, resolve_link};
use crate::model::{Note, Question, WordLink};
use crate::refs::{BibleLocus, BibleRef, HelpRef, ObsRef, TsvLocus, parse_tsv_locus};

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

/// The cells of the row an item was read from that the writers would write differently,
/// by column, as they were written. See the module documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadCells(Vec<KeptCell>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct KeptCell {
    column: &'static str,
    text: String,
    cr: bool,
}

impl KeptCell {
    fn written(&self) -> String {
        let mut cell = self.text.clone();
        if self.cr {
            cell.push('\r');
        }
        cell
    }
}

impl ReadCells {
    /// The cell of `column` as tC4 reads it, without a CR at its end, when the writers
    /// would not write it that way.
    pub fn get(&self, column: &str) -> Option<&str> {
        self.kept(column).map(|c| c.text.as_str())
    }

    fn kept(&self, column: &str) -> Option<&KeptCell> {
        self.0.iter().find(|c| c.column == column)
    }

    /// The cells of `read` that differ from `written`, the writer's own form of the item;
    /// every cell when the writer refuses the item.
    fn of(
        header: &[&'static str],
        read: &[Cell<'_>],
        written: Result<Vec<String>, Error>,
    ) -> Option<Self> {
        let kept: Vec<KeptCell> = header
            .iter()
            .zip(read)
            .enumerate()
            .filter(|(i, (_, cell))| {
                !matches!(&written, Ok(cells) if cells.get(*i).is_some_and(|w| cell.is_written_as(w)))
            })
            .map(|(_, (column, cell))| KeptCell {
                column,
                text: cell.text.to_owned(),
                cr: cell.cr,
            })
            .collect();
        (!kept.is_empty()).then_some(Self(kept))
    }
}

/// One cell of a row. `text` is the cell as tC4 reads it: `tsvRows` (`derive.ts`) splits
/// the table on LF alone and drops one CR from the end of every cell, so `3:1` followed by
/// a CR and a tab is the reference `3:1`. `cr` tells that the file had that CR, so the cell
/// writes back as it was. The CR of a CRLF line end is the last cell's: that cell keeps
/// all the text before it.
#[derive(Debug, Clone, Copy)]
struct Cell<'a> {
    text: &'a str,
    cr: bool,
}

impl<'a> Cell<'a> {
    /// `raw` is the text between two tabs of a line whose CRLF end is removed.
    fn read(raw: &'a str, last: bool) -> Self {
        match raw.strip_suffix('\r') {
            Some(text) if !last => Self { text, cr: true },
            // A CR is left at the end of the last cell only when the line ended in two;
            // written back with LF line ends, the cell takes the one it lost.
            _ => Self {
                text: raw,
                cr: raw.ends_with('\r'),
            },
        }
    }

    fn is_written_as(&self, cell: &str) -> bool {
        match cell.strip_suffix('\r') {
            Some(text) if self.cr => text == self.text,
            _ => !self.cr && cell == self.text,
        }
    }
}

/// The cell that tC4 and [`rows`] read back as `text`. A cell that ends in a CR takes one
/// more, which reading drops; a CR anywhere else is cell text.
fn with_cr(mut text: String) -> String {
    if text.ends_with('\r') {
        text.push('\r');
    }
    text
}

struct Row<'a> {
    table: TsvTable,
    header: &'static [&'static str],
    line: usize,
    cells: Vec<Cell<'a>>,
}

impl<'a> Row<'a> {
    /// `rows` refuses a row whose cell count differs from the header's, so every column
    /// index of the header is in range.
    fn cell(&self, i: usize) -> &'a str {
        self.cells[i].text
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
        parse_reference(text, scope)
            .ok_or_else(|| self.fail(i, CellProblem::Reference(text.to_owned())))
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
        parse_support(self.cell(i)).map_err(|p| self.fail(i, CellProblem::Link(p)))
    }

    fn read_cells(&self, written: Result<Vec<String>, Error>) -> Option<ReadCells> {
        ReadCells::of(self.header, &self.cells, written)
    }
}

fn parse_reference(text: &str, scope: TsvScope) -> Option<HelpRef> {
    match scope {
        TsvScope::Book(book) => {
            parse_tsv_locus(text).map(|locus| HelpRef::Bible(BibleRef { book, locus }))
        }
        TsvScope::Obs => ObsRef::parse(text).map(HelpRef::Obs),
    }
}

fn parse_support(text: &str) -> Result<Option<AcademyArticleId>, LinkProblem> {
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
            .map_err(|_| LinkProblem::Slug(text.to_owned()));
    }
    match resolve_link(text, &LinkBase::Absolute) {
        Ok(LinkTarget::Academy(id)) => Ok(Some(id)),
        Ok(_) => Err(LinkProblem::NoTarget),
        Err(e) => Err(link_problem(e)),
    }
}

fn parse_word_link(text: &str) -> Result<WordArticleId, LinkProblem> {
    match resolve_link(text, &LinkBase::ExportRoot) {
        Ok(LinkTarget::Word(id)) => Ok(id),
        Ok(_) => Err(LinkProblem::NoTarget),
        Err(e) => Err(link_problem(e)),
    }
}

fn link_problem(e: Error) -> LinkProblem {
    match e {
        Error::Link { problem, .. } => problem,
        other => LinkProblem::Reference(other.to_string()),
    }
}

/// The `Chapter` and `Verse` cells of the legacy table, read as one place.
fn parse_legacy_place(chapter: &str, verse: &str) -> Option<BibleLocus> {
    parse_tsv_locus(&format!("{chapter}:{verse}"))
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
        // The CR of a CRLF line end; see `Cell`.
        let line = line.strip_suffix('\r').unwrap_or(line);
        let raw: Vec<&str> = line.split('\t').collect();
        if raw.len() != header.len() {
            return Err(Error::TsvColumnCount {
                table,
                line: i + 1,
                found: raw.len(),
                expected: header.len(),
            });
        }
        let last = raw.len() - 1;
        out.push(Row {
            table,
            header,
            line: i + 1,
            cells: raw
                .iter()
                .enumerate()
                .map(|(i, raw)| Cell::read(raw, i == last))
                .collect(),
        });
    }
    Ok(out)
}

fn header_text(first: &str) -> &str {
    let first = without_bom(first);
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
    write_error(table, WriteProblem::Unwritable(text.to_owned()))
}

fn write_error(table: TsvTable, problem: WriteProblem) -> Error {
    Error::TsvWrite { table, problem }
}

/// A cell that is written as it is: it must not hold a tab or a line break.
fn plain(text: &str, table: TsvTable) -> Result<String, Error> {
    if text.contains(['\t', '\n', '\r']) {
        Err(unwritable(table, text))
    } else {
        Ok(text.to_owned())
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
    let mut note = Note {
        reference: row.reference(0, scope)?,
        id: row.id(1)?,
        tags: tags(row.cell(2)),
        support_reference: row.support_reference(3)?,
        quote: Quote::new(row.cell(4)),
        occurrence: row.occurrence(5)?,
        gl_quote: None,
        text: decode_escapes(row.cell(6)),
        read_from: None,
    };
    note.read_from = row.read_cells(note_row(&note, scope));
    Ok(note)
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
    let locus = parse_legacy_place(chapter, verse)
        .ok_or_else(|| row.fail(1, CellProblem::Reference(format!("{chapter}:{verse}"))))?;
    let gl = row.cell(7);
    let mut note = Note {
        reference: HelpRef::Bible(BibleRef { book, locus }),
        id: row.id(3)?,
        tags: Vec::new(),
        support_reference: row.support_reference(4)?,
        quote: Quote::new(row.cell(5)),
        occurrence: row.occurrence(6)?,
        gl_quote: (!gl.is_empty()).then(|| gl.to_owned()),
        text: row.cell(8).replace("<br>", "\n"),
        read_from: None,
    };
    note.read_from = row.read_cells(legacy_row(&note, TsvScope::Book(book)));
    Ok(note)
}

/// Parses a translation words links table. `TWLink` may be an `rc://` link or an export
/// path; both name the same article.
pub fn parse_word_links(text: &str, scope: TsvScope) -> Result<Vec<WordLink>, Error> {
    rows(text, TsvTable::WordLinks, &WORD_LINKS)?
        .iter()
        .map(|row| {
            let article =
                parse_word_link(row.cell(5)).map_err(|p| row.fail(5, CellProblem::Link(p)))?;
            let mut link = WordLink {
                reference: row.reference(0, scope)?,
                id: row.id(1)?,
                tags: tags(row.cell(2)),
                orig_words: Quote::new(row.cell(3)),
                occurrence: row.occurrence(4)?,
                article,
                read_from: None,
            };
            let style = if row.cell(5).starts_with("rc://") {
                WordLinkStyle::Rc
            } else {
                WordLinkStyle::ExportPath
            };
            link.read_from = row.read_cells(word_link_row(&link, scope, style));
            Ok(link)
        })
        .collect()
}

/// Parses a translation questions table.
pub fn parse_questions(text: &str, scope: TsvScope) -> Result<Vec<Question>, Error> {
    rows(text, TsvTable::Questions, &QUESTIONS)?
        .iter()
        .map(|row| {
            let mut question = Question {
                reference: row.reference(0, scope)?,
                id: row.id(1)?,
                tags: tags(row.cell(2)),
                quote: Quote::new(row.cell(3)),
                occurrence: row.occurrence(4)?,
                question: decode_escapes(row.cell(5)),
                response: decode_escapes(row.cell(6)),
                read_from: None,
            };
            question.read_from = row.read_cells(question_row(&question, scope));
            Ok(question)
        })
        .collect()
}

/// Builds one row: each cell as it was read when it still holds the item's value, else in
/// the writer's own form.
struct RowWriter<'a> {
    table: TsvTable,
    read: Option<&'a ReadCells>,
    cells: Vec<String>,
}

impl<'a> RowWriter<'a> {
    fn new(table: TsvTable, read: Option<&'a ReadCells>) -> Self {
        Self {
            table,
            read,
            cells: Vec::new(),
        }
    }

    /// `holds` tells whether a cell as read still holds the item's value; `write` gives
    /// the writer's own form of the value.
    fn cell(
        &mut self,
        column: &str,
        holds: impl Fn(&str) -> bool,
        write: impl FnOnce() -> Result<String, Error>,
    ) -> Result<(), Error> {
        let kept = self
            .read
            .and_then(|r| r.kept(column))
            .filter(|kept| holds(&kept.text));
        let cell = match kept {
            Some(kept) => kept.written(),
            None => with_cr(write()?),
        };
        self.cells.push(cell);
        Ok(())
    }

    fn reference(&mut self, reference: &HelpRef, scope: TsvScope) -> Result<(), Error> {
        in_scope(reference, scope, self.table)?;
        self.cell(
            "Reference",
            |raw| parse_reference(raw, scope).as_ref() == Some(reference),
            || Ok(reference.tsv_text()),
        )
    }

    fn id(&mut self, id: &HelpItemId, what: &'static str) -> Result<(), Error> {
        let table = self.table;
        self.cell(
            "ID",
            |raw| matches!(id, HelpItemId::Row(row) if row.as_str() == raw),
            || row_id(id, table, what),
        )
    }

    fn tags(&mut self, values: &[String]) -> Result<(), Error> {
        let table = self.table;
        self.cell(
            "Tags",
            |raw| tags(raw) == values,
            || tags_cell(values, table),
        )
    }

    fn occurrence(&mut self, occurrence: Occurrence) -> Result<(), Error> {
        self.cell(
            "Occurrence",
            |raw| Occurrence::parse_cell(raw) == Some(occurrence),
            || Ok(occurrence.cell()),
        )
    }

    fn support_reference(&mut self, id: &Option<AcademyArticleId>) -> Result<(), Error> {
        self.cell(
            "SupportReference",
            |raw| parse_support(raw).as_ref() == Ok(id),
            || support_cell(id),
        )
    }

    fn plain(&mut self, column: &str, text: &str) -> Result<(), Error> {
        let table = self.table;
        self.cell(column, |raw| raw == text, || plain(text, table))
    }

    fn markdown(&mut self, column: &str, text: &str) -> Result<(), Error> {
        let table = self.table;
        self.cell(
            column,
            |raw| decode_escapes(raw) == text,
            || encode_escapes(text, table),
        )
    }
}

/// Refuses an item that belongs to another book, or to Open Bible Stories in a Bible table
/// and the reverse: its row would read back as belonging to the table's book.
fn in_scope(reference: &HelpRef, scope: TsvScope, table: TsvTable) -> Result<(), Error> {
    match (reference, scope) {
        (HelpRef::Bible(r), TsvScope::Book(book)) if r.book == book => Ok(()),
        (HelpRef::Bible(r), TsvScope::Book(book)) => Err(write_error(
            table,
            WriteProblem::WrongBook {
                expected: book.code().to_owned(),
                found: r.book.code().to_owned(),
            },
        )),
        (HelpRef::Obs(_), TsvScope::Obs) => Ok(()),
        _ => Err(write_error(
            table,
            WriteProblem::Reference(reference.to_string()),
        )),
    }
}

fn row_id(id: &HelpItemId, table: TsvTable, what: &'static str) -> Result<String, Error> {
    match id {
        HelpItemId::Row(id) => Ok(id.as_str().to_owned()),
        HelpItemId::Aquifer(_) => Err(write_error(table, WriteProblem::AquiferItem { what })),
    }
}

/// Tags are written `a; b`. A tag that is empty, holds `;` or a line break, or has spaces
/// at either end would read back as other tags.
fn tags_cell(values: &[String], table: TsvTable) -> Result<String, Error> {
    for tag in values {
        if tag.is_empty() || tag.contains(';') || tag.trim() != tag {
            return Err(write_error(table, WriteProblem::Tag(tag.clone())));
        }
    }
    plain(&values.join("; "), table)
}

fn support_cell(id: &Option<AcademyArticleId>) -> Result<String, Error> {
    match id {
        None => Ok(String::new()),
        Some(AcademyArticleId::Dcs { manual, slug }) => {
            Ok(format!("rc://*/ta/man/{}/{}", manual.folder(), slug))
        }
        Some(AcademyArticleId::Aquifer(_)) => Err(write_error(
            TsvTable::Notes,
            WriteProblem::AquiferItem {
                what: "Translation Academy link",
            },
        )),
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

fn note_row(n: &Note, scope: TsvScope) -> Result<Vec<String>, Error> {
    let mut w = RowWriter::new(TsvTable::Notes, n.read_from.as_ref());
    if n.gl_quote.is_some() {
        return Err(write_error(
            TsvTable::Notes,
            WriteProblem::NoColumn { column: "GLQuote" },
        ));
    }
    w.reference(&n.reference, scope)?;
    w.id(&n.id, "note")?;
    w.tags(&n.tags)?;
    w.support_reference(&n.support_reference)?;
    w.plain("Quote", n.quote.as_str())?;
    w.occurrence(n.occurrence)?;
    w.markdown("Note", &n.text)?;
    Ok(w.cells)
}

fn legacy_row(n: &Note, scope: TsvScope) -> Result<Vec<String>, Error> {
    let t = TsvTable::Notes;
    let TsvScope::Book(book) = scope else {
        return Err(write_error(t, WriteProblem::LegacyObs));
    };
    in_scope(&n.reference, scope, t)?;
    let HelpRef::Bible(r) = &n.reference else {
        return Err(write_error(t, WriteProblem::LegacyObs));
    };
    if !n.tags.is_empty() {
        return Err(write_error(t, WriteProblem::NoColumn { column: "Tags" }));
    }
    let mut w = RowWriter::new(t, n.read_from.as_ref());
    w.cell(
        "Book",
        |raw| raw.parse::<BookId>() == Ok(book),
        || Ok(book.code().to_owned()),
    )?;
    // Chapter and Verse name one place together: a kept cell is written with the other
    // as read or canonical, when the pair still names the item's place. Each is a pair of
    // the text read and the cell written.
    let canonical = legacy_place(&r.locus).map(|(c, v)| {
        let cell = |text: String| (text.clone(), with_cr(text));
        (cell(c), cell(v))
    });
    let read = |column: &str, canonical: Option<&(String, String)>| {
        n.read_from
            .as_ref()
            .and_then(|cells| cells.kept(column))
            .map(|kept| (kept.text.clone(), kept.written()))
            .or_else(|| canonical.cloned())
    };
    let pair = (
        read("Chapter", canonical.as_ref().map(|p| &p.0)),
        read("Verse", canonical.as_ref().map(|p| &p.1)),
    );
    let (chapter, verse) = match pair {
        (Some(c), Some(v)) if parse_legacy_place(&c.0, &v.0).as_ref() == Some(&r.locus) => (c, v),
        _ => canonical.ok_or_else(|| write_error(t, WriteProblem::Reference(r.to_string())))?,
    };
    w.cells.push(chapter.1);
    w.cells.push(verse.1);
    w.id(&n.id, "note")?;
    w.support_reference(&n.support_reference)?;
    w.plain("OrigQuote", n.quote.as_str())?;
    w.occurrence(n.occurrence)?;
    w.plain("GLQuote", n.gl_quote.as_deref().unwrap_or(""))?;
    w.cell(
        "OccurrenceNote",
        |raw| raw.replace("<br>", "\n") == n.text,
        || {
            if n.text.contains("<br>") {
                return Err(unwritable(t, &n.text));
            }
            plain(&n.text.replace('\n', "<br>"), t)
        },
    )?;
    Ok(w.cells)
}

/// The `Chapter` and `Verse` cells of the legacy table: one chapter per row.
fn legacy_place(locus: &BibleLocus) -> Option<(String, String)> {
    match locus {
        BibleLocus::BookIntro => Some(("front".into(), "intro".into())),
        BibleLocus::ChapterIntro(c) => Some((c.to_string(), "intro".into())),
        BibleLocus::ChapterFront(c) => Some((c.to_string(), "front".into())),
        // The chapter cell ends at the first `:`, so the pair reads back as the same text.
        BibleLocus::AsWritten(text) => text
            .as_str()
            .split_once(':')
            .map(|(c, v)| (c.to_owned(), v.to_owned())),
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

/// Writes notes as a table of the given layout for `scope`. Items of another scope, and
/// values the layout has no column for, are refused rather than dropped.
pub fn write_notes(notes: &[Note], scope: TsvScope, layout: NotesLayout) -> Result<String, Error> {
    match layout {
        NotesLayout::Seven => {
            let rows = notes
                .iter()
                .map(|n| note_row(n, scope))
                .collect::<Result<_, _>>()?;
            Ok(table_text(&NOTES, rows))
        }
        NotesLayout::LegacyNine => {
            if scope == TsvScope::Obs {
                return Err(write_error(TsvTable::Notes, WriteProblem::LegacyObs));
            }
            let rows = notes
                .iter()
                .map(|n| legacy_row(n, scope))
                .collect::<Result<_, _>>()?;
            Ok(table_text(&NOTES_LEGACY, rows))
        }
    }
}

fn word_link_row(
    l: &WordLink,
    scope: TsvScope,
    style: WordLinkStyle,
) -> Result<Vec<String>, Error> {
    let t = TsvTable::WordLinks;
    let mut w = RowWriter::new(t, l.read_from.as_ref());
    w.reference(&l.reference, scope)?;
    w.id(&l.id, "word link")?;
    w.tags(&l.tags)?;
    w.plain("OrigWords", l.orig_words.as_str())?;
    w.occurrence(l.occurrence)?;
    w.cell(
        "TWLink",
        |raw| {
            raw.starts_with("rc://") == (style == WordLinkStyle::Rc)
                && parse_word_link(raw).as_ref() == Ok(&l.article)
        },
        || match (&l.article, style) {
            (WordArticleId::Dcs { category, slug }, WordLinkStyle::Rc) => Ok(format!(
                "rc://*/tw/dict/bible/{}/{}",
                category.folder(),
                slug
            )),
            (WordArticleId::Dcs { category, slug }, WordLinkStyle::ExportPath) => {
                Ok(format!("./payload/{}/{}.md", category.folder(), slug))
            }
            (WordArticleId::Aquifer(_), _) => Err(write_error(
                t,
                WriteProblem::AquiferItem {
                    what: "translation words article",
                },
            )),
        },
    )?;
    Ok(w.cells)
}

/// Writes a words links table for `scope`.
pub fn write_word_links(
    links: &[WordLink],
    scope: TsvScope,
    style: WordLinkStyle,
) -> Result<String, Error> {
    let rows = links
        .iter()
        .map(|l| word_link_row(l, scope, style))
        .collect::<Result<_, _>>()?;
    Ok(table_text(&WORD_LINKS, rows))
}

fn question_row(q: &Question, scope: TsvScope) -> Result<Vec<String>, Error> {
    let mut w = RowWriter::new(TsvTable::Questions, q.read_from.as_ref());
    w.reference(&q.reference, scope)?;
    w.id(&q.id, "question")?;
    w.tags(&q.tags)?;
    w.plain("Quote", q.quote.as_str())?;
    w.occurrence(q.occurrence)?;
    w.markdown("Question", &q.question)?;
    w.markdown("Response", &q.response)?;
    Ok(w.cells)
}

/// Writes a questions table for `scope`.
pub fn write_questions(questions: &[Question], scope: TsvScope) -> Result<String, Error> {
    let rows = questions
        .iter()
        .map(|q| question_row(q, scope))
        .collect::<Result<_, _>>()?;
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
