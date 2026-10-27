//! Community Checking's decisions (unit B8), the parts of tC4's `CommunityChecking.jsx`,
//! `ExportMenu.jsx` and `print/*` that are not drawing. Pure, so they are tested on the host.
//!
//! A preview page's flow is tC4's print DOM (`PrintParts`, `tC4:src/views/print/PrintBook.jsx`)
//! written as markup ([`flow_markup`]): the page measures its sheets with it, as tC4 renders
//! `PrintParts` to static markup in its measuring sheet, and the sheets show the same markup,
//! so what was measured is what shows. The parts come from `tc_api::model::print`.

use tc_api::ApiError;
use tc_api::export::{
    Exported, ExportedFile, PrintRequest, PrintWords, Producer, Run, RunExport, StoryRun, Subject,
    UtcOffset,
};
use tc_api::home::ProjectKind;
use tc_api::ids::{RequestId, Scope};
use tc_api::model::print::{FlowBlock, Opening, PAGE_MARGIN_MM, PageSetup, Pictures, VerseLine};
use tc_api::refusal::Message as _;
use tc_api::shown::Direction;

use crate::css::Css;
use crate::i18n::{Key, keys, refusal_text, t};
use crate::tokens::ToastTone;

/// React's escaping of text and attribute values.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

pub fn dir_attr(dir: Direction) -> &'static str {
    match dir {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    }
}

/// The line for a run `from`–`to`: `one` words a run of one, `many` a longer run.
fn run_text(one: Key, many: Key, from: u16, to: u16) -> String {
    if from == to {
        t(one, &[("n", &from.to_string())])
    } else {
        t(
            many,
            &[("from", &from.to_string()), ("to", &to.to_string())],
        )
    }
}

/// "[ chapter 3 not yet drafted ]", "[ chapters 5–7 not yet drafted ]" (`chapterGapText`).
pub fn chapter_gap_text(run: Run) -> String {
    run_text(
        keys::cc::CHAPTER_NOT_DRAFTED,
        keys::cc::CHAPTERS_NOT_DRAFTED,
        run.from,
        run.to,
    )
}

/// "[ frame 2 not yet drafted ]" (`frameGapText`).
pub fn frame_gap_text(run: Run) -> String {
    run_text(
        keys::cc::FRAME_NOT_DRAFTED,
        keys::cc::FRAMES_NOT_DRAFTED,
        run.from,
        run.to,
    )
}

/// "[ stories 2–3 not yet drafted ]" (`storyGapText`).
pub fn story_gap_text(run: StoryRun) -> String {
    run_text(
        keys::cc::STORY_NOT_DRAFTED,
        keys::cc::STORIES_NOT_DRAFTED,
        u16::from(run.from.get()),
        u16::from(run.to.get()),
    )
}

/// Which sheet a flow's markup is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    /// The measuring sheet: `renderToStaticMarkup`'s markup, as tC4 measures with.
    Measure,
    /// A page: the DOM React draws, with the preview's test handles, and a verse's text and
    /// the space after it as two text nodes, as React writes two children. Chromium sets a
    /// justified line by its text nodes, so one node would move glyphs by a fraction of a
    /// pixel.
    Page,
}

/// The flow of a preview page (`PrintParts`).
pub fn flow_markup(blocks: &[FlowBlock<'_>], dir: Direction, sheet: Sheet) -> String {
    let d = dir_attr(dir);
    let test_ids = sheet == Sheet::Page;
    // A comment parts two text nodes and is no part of the layout.
    let space = if test_ids { "<!-- --> " } else { " " };
    let id = |name: &str| {
        if test_ids {
            format!(" data-testid=\"{name}\"")
        } else {
            String::new()
        }
    };
    let mut out = String::new();
    for block in blocks {
        match block {
            FlowBlock::Gap(run) => out.push_str(&format!(
                "<p class=\"print-chapter-gap\" dir=\"{d}\"{}>{}</p>",
                id("cc-chapter-gap"),
                escape(&chapter_gap_text(*run))
            )),
            FlowBlock::Chapter(chapter) => {
                let class = if chapter.continues {
                    "print-chapter print-chapter-continues"
                } else {
                    "print-chapter"
                };
                out.push_str(&format!(
                    "<section class=\"{class}\" dir=\"{d}\"{}>",
                    id("cc-chapter")
                ));
                match chapter.opening {
                    Some(Opening::DropCap) => out.push_str(&format!(
                        "<span class=\"print-dropcap\">{}</span>",
                        chapter.chapter
                    )),
                    Some(Opening::Heading) => out.push_str(&format!(
                        "<h2 class=\"print-chapter-heading\"{}>{}</h2>",
                        id("cc-chapter-heading"),
                        escape(&t(
                            keys::cc::CHAPTER_HEADING,
                            &[("n", &chapter.chapter.to_string())]
                        ))
                    )),
                    None => {}
                }
                for verse in &chapter.verses {
                    match verse {
                        VerseLine::Text { number, text } => {
                            out.push_str("<span>");
                            if let Some(n) = number {
                                out.push_str(&format!(
                                    "<sup class=\"print-verse-number\">{}</sup>",
                                    escape(n)
                                ));
                            }
                            out.push_str(&format!("{}{space}</span>", escape(text)));
                        }
                        VerseLine::NotDrafted { number } => out.push_str(&format!(
                            "<span class=\"print-undrafted\"><sup class=\"print-verse-number\">{}</sup>{}{space}</span>",
                            escape(number),
                            escape(&t(keys::cc::NOT_YET_DRAFTED, &[]))
                        )),
                    }
                }
                out.push_str("</section>");
            }
        }
    }
    out
}

/// Millimetres in CSS pixels.
pub fn px(mm: f64) -> f64 {
    mm * 96.0 / 25.4
}

/// A sheet of the preview (`Sheet`, `PrintPages.jsx:100-124`): the paper with the print's
/// margins, the page-setup variables print.css reads; `hidden` is the measuring sheet.
pub fn sheet_style(setup: &PageSetup, hidden: bool) -> Css {
    let (w, h) = setup.paper.size_mm();
    let (top, side, bottom) = PAGE_MARGIN_MM;
    let mut style = crate::css! {
        "--print-columns": f64::from(setup.columns.count()),
        "--print-leading": setup.leading(),
        "width": px(w),
        "height": px(h),
        "padding": format!(
            "{}px {}px {}px",
            crate::css::js_number(px(top)),
            crate::css::js_number(px(side)),
            crate::css::js_number(px(bottom))
        ),
        "box-sizing": "border-box",
        "display": "flex",
        "flex-direction": "column",
        "position": if hidden { "absolute" } else { "relative" },
        "background": "#fff",
    };
    if hidden {
        style.extend(&crate::css! {
            "visibility": "hidden",
            "pointer-events": "none",
            "inset-inline-start": 0,
            "top": 0,
        });
    } else {
        style.extend(&crate::css! {
            "box-shadow": "var(--shadow-page)",
            "border-radius": 4,
            "margin": "0 auto 24px",
        });
    }
    style
}

/// A sheet's flow (`FLOW`): it fills column by column; the last sheet balances its columns,
/// as the PDF's last page does.
pub fn flow_style(last: bool) -> Css {
    crate::css! {
        "flex": 1,
        "min-height": 0,
        "column-fill": if last { "balance" } else { "auto" },
        "overflow": "hidden",
    }
}

/// The page number in the sheet's bottom margin (`PAGE_NUMBER`).
pub fn page_number_style() -> Css {
    crate::css! {
        "position": "absolute",
        "inset-inline": 0,
        "bottom": px(PAGE_MARGIN_MM.2) / 2.0 - 7.0,
        "text-align": "center",
        "font-family": "\"Charis SIL\", \"PT Serif\", Georgia, \"Times New Roman\", serif",
        "font-size": 11,
        "color": "#626F78",
    }
}

/// An item of the export menu: a producer that applies to the open project, by its catalog
/// key (`tC4:src/data/export/producers.ts`).
pub fn export_items(kind: ProjectKind, pdf_bridge: bool) -> Vec<Key> {
    let mut items = Vec::new();
    // The PDF prints through the desktop shell; a browser has no PDF item (`pdf.ts:120`).
    if pdf_bridge {
        items.push(keys::cc::EXPORT_PDF);
    }
    match kind {
        ProjectKind::Bible => {
            items.push(keys::cc::EXPORT_USFM_ALIGNED);
            items.push(keys::cc::EXPORT_USFM_PLAIN);
        }
        ProjectKind::Stories => items.push(keys::cc::EXPORT_STORY_MARKDOWN),
    }
    items.push(keys::cc::EXPORT_BURRITO_ZIP);
    items
}

/// The producer behind an export menu item.
pub fn producer_of(item: Key) -> Option<Producer> {
    Some(match item {
        keys::cc::EXPORT_PDF => Producer::PrintHtml,
        keys::cc::EXPORT_USFM_ALIGNED => Producer::AlignedUsfm,
        keys::cc::EXPORT_USFM_PLAIN => Producer::PlainUsfm,
        keys::cc::EXPORT_STORY_MARKDOWN => Producer::StoryMarkdown,
        keys::cc::EXPORT_BURRITO_ZIP => Producer::BurritoZip,
        _ => return None,
    })
}

/// The print's lines in the shown language, each with its placeholders still in it: the
/// server fills them, so the PDF words what the preview words.
pub fn print_words() -> PrintWords {
    PrintWords {
        chapter_heading: t(keys::cc::CHAPTER_HEADING, &[]),
        chapter_not_drafted: t(keys::cc::CHAPTER_NOT_DRAFTED, &[]),
        chapters_not_drafted: t(keys::cc::CHAPTERS_NOT_DRAFTED, &[]),
        not_yet_drafted: t(keys::cc::NOT_YET_DRAFTED, &[]),
        frame_not_drafted: t(keys::cc::FRAME_NOT_DRAFTED, &[]),
        frames_not_drafted: t(keys::cc::FRAMES_NOT_DRAFTED, &[]),
        story_not_drafted: t(keys::cc::STORY_NOT_DRAFTED, &[]),
        stories_not_drafted: t(keys::cc::STORIES_NOT_DRAFTED, &[]),
        story_number: t(keys::story_draft::STORY_NUMBER, &[]),
        image_alt: t(keys::story_draft::IMAGE_ALT, &[]),
        nothing_drafted_obs: t(keys::cc::NOTHING_DRAFTED_OBS, &[]),
    }
}

/// What an export of the open screen names: the project, the book or the stories, and the
/// print's title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOf {
    pub at: Scope,
    pub subject: Subject,
    pub title: String,
}

impl ExportOf {
    /// The request of one press; a print carries the preview's page setup and words.
    pub fn run(
        &self,
        request: RequestId,
        producer: Producer,
        setup: PageSetup,
        utc_offset: UtcOffset,
    ) -> RunExport {
        RunExport {
            utc_offset,
            request,
            at: self.at.clone(),
            producer,
            subject: self.subject,
            print: (producer == Producer::PrintHtml).then(|| PrintRequest {
                setup,
                title: self.title.clone(),
                words: print_words(),
            }),
        }
    }
}

/// "3 verses have two versions. The export uses the version shown. Review first?" (STORAGE
/// §8.5), or `None` when no verse the export reads is in conflict.
pub fn conflict_question(verses: u32) -> Option<String> {
    match verses {
        0 => None,
        1 => Some(t(keys::export::CONFLICTS_ONE, &[])),
        n => Some(t(keys::export::CONFLICTS_MANY, &[("n", &n)])),
    }
}

/// What a finished export says under the menu: tC4's "Export ready" toast, and, as tC4
/// e2bfd8c3 does, the verses whose alignment the file left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportNotice {
    pub ready: String,
    /// Info for "Export ready", success for "Saved", warn for an interrupted download (tC4).
    pub tone: ToastTone,
    pub skipped: Option<String>,
}

pub fn export_notice(exported: &Exported) -> ExportNotice {
    let (ExportedFile::Download { file_name, .. } | ExportedFile::Print { file_name }) =
        &exported.file;
    let facts = &exported.facts;
    let skipped = (!facts.skipped.is_empty()).then(|| {
        let named = facts
            .skipped
            .iter()
            .take(20)
            .map(|v| format!("{}:{}", v.chapter, v.verse))
            .collect::<Vec<_>>();
        let mut refs = named.join(", ");
        if facts.skipped_total as usize > named.len() {
            refs.push_str(", …");
        }
        // A plain export names only the verses it kept with their markup, which are in the
        // file: "not in the file" would be false of them.
        let key = if facts
            .skipped
            .iter()
            .all(|v| v.why == tc_api::export::SkipReason::MarkupKept)
        {
            keys::export::MARKUP_KEPT
        } else {
            keys::export::SKIPPED
        };
        t(key, &[("refs", &refs)])
    });
    ExportNotice {
        ready: t(keys::cc::EXPORT_READY, &[("filename", file_name)]),
        tone: ToastTone::Info,
        skipped,
    }
}

/// A failed export's words, as tC4's `ExportMenu` `failureText` puts them: the diagnosis,
/// then the recovery sentence of its code.
pub fn failure_text(error: &ApiError) -> String {
    let ApiError::Refused(refusal) = error else {
        return error.to_string();
    };
    let recovery = refusal_text(refusal);
    match refusal.detail() {
        Some(diagnosis) => format!("{diagnosis} {recovery}"),
        None => recovery,
    }
}

/// Whether a failed print of `run` says to set Pictures off: only a stories print has
/// pictures; tC4's `pdf.ts` passes false for a book.
pub fn prints_pictures(run: &RunExport) -> bool {
    run.subject == Subject::Stories
        && run
            .print
            .as_ref()
            .is_some_and(|p| p.setup.pictures == Pictures::Shown)
}

/// The shell's answer to `printPdf` (G1's contract): `{ok: true, file}` is the file it saved;
/// `{ok: false, reason}` is a cancel, a failure or a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfAnswer {
    Saved(String),
    Cancelled,
    Failed,
}

/// What the menu shows once the shell answered a print: tC4's "Saved {filename}"
/// (`cc.exportSaved`), nothing after a cancel, the print-failed words otherwise.
pub fn pdf_panel(answer: PdfAnswer, exported: &Exported, pictures: bool) -> ExportPanel {
    match answer {
        PdfAnswer::Saved(file) => ExportPanel::Ready(ExportNotice {
            ready: t(keys::cc::EXPORT_SAVED, &[("filename", &file)]),
            tone: ToastTone::Success,
            ..export_notice(exported)
        }),
        PdfAnswer::Cancelled => ExportPanel::Idle,
        PdfAnswer::Failed if pictures => {
            ExportPanel::Failed(t(keys::refusal::export::PRINT_FAILED_PICTURES, &[]))
        }
        PdfAnswer::Failed => ExportPanel::Failed(t(keys::refusal::export::PRINT_FAILED, &[])),
    }
}

/// What the menu shows when an export's job ended with neither a file nor a refusal: for a
/// lost job (the server ran again), tC4's words for an export that made no file; nothing
/// after a cancel. `None` leaves the menu as it is: a job the page stopped following, and the
/// ends the caller words itself (a file, a refusal).
pub fn unfinished_panel<O>(end: &crate::app::jobs::JobEnd<O>) -> Option<ExportPanel> {
    use crate::app::jobs::JobEnd;
    match end {
        JobEnd::Lost => Some(ExportPanel::Failed(t(
            keys::refusal::export::READ_FAILED,
            &[],
        ))),
        JobEnd::Cancelled => Some(ExportPanel::Idle),
        JobEnd::Left | JobEnd::Done(_) | JobEnd::Failed(_) => None,
    }
}

/// The shell's `onDownloadDone({state, filename})` when its save dialog closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadDone {
    Completed(String),
    Interrupted(String),
    /// A cancelled save, or a state the page does not know.
    Other,
}

impl DownloadDone {
    pub fn of(state: Option<&str>, filename: String) -> Self {
        match state {
            Some("completed") => Self::Completed(filename),
            Some("interrupted") => Self::Interrupted(filename),
            _ => Self::Other,
        }
    }
}

/// What the menu shows once a file is handed to the browser. With a shell that reports the
/// save, tC4 says nothing until it does; the rebuild still names the skipped verses then.
/// Without one, tC4's "Export ready".
pub fn delivered_panel(exported: &Exported, shell_reports: bool) -> ExportPanel {
    let notice = export_notice(exported);
    if shell_reports {
        ExportPanel::Saving(notice.skipped)
    } else {
        ExportPanel::Ready(notice)
    }
}

/// What the menu shows when the shell reports a download (`ExportMenu.jsx`'s listener):
/// "Saved {filename}", "{filename} did not finish downloading…", or nothing after a cancel.
/// The skipped verses of the export on show stay with it.
pub fn download_done_panel(done: DownloadDone, shown: &ExportPanel) -> ExportPanel {
    let skipped = match shown {
        ExportPanel::Saving(skipped) => skipped.clone(),
        ExportPanel::Ready(notice) => notice.skipped.clone(),
        _ => None,
    };
    let (ready, tone) = match done {
        DownloadDone::Completed(filename) => (
            t(keys::cc::EXPORT_SAVED, &[("filename", &filename)]),
            ToastTone::Success,
        ),
        DownloadDone::Interrupted(filename) => (
            t(keys::cc::EXPORT_INTERRUPTED, &[("filename", &filename)]),
            ToastTone::Warn,
        ),
        DownloadDone::Other => return ExportPanel::Idle,
    };
    ExportPanel::Ready(ExportNotice {
        ready,
        tone,
        skipped,
    })
}

/// What the export menu shows under itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ExportPanel {
    #[default]
    Idle,
    Running,
    /// The conflicts question; "Export anyway" runs `run`, the press's own request.
    Ask {
        question: String,
        run: Box<RunExport>,
    },
    Ready(ExportNotice),
    /// A download handed to the shell, which reports the save; the skipped verses, if any.
    Saving(Option<String>),
    Failed(String),
}
