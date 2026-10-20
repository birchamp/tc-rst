//! The decisions of the open overlay and the Understand screen (unit B3), the parts of tC4's
//! `OpenProgress.jsx`, `Understand.jsx`, `sections.js`, `SourceVerse.jsx`, `helpsFocus.js` and
//! `HelpsPanel.jsx` that are not drawing. Pure, so they are tested on the host.

use tc_api::editor::{CommentView, FrameRow, PaneChapter, PaneVerse, RowSource};
use tc_api::helps::{PaneFocus, Slot};
use tc_api::jobs::{OpenPhase, Phase, Progress};
use tc_api::model::{ArticleBlock, ArticleSpan, js_len, take_spans};
use tc_core::BookId;

use crate::i18n::{Key, keys, t};

// ---------------------------------------------------------------------------------------
// The open overlay (S14)
// ---------------------------------------------------------------------------------------

/// How long an open may run before the overlay shows (`SHOW_THRESHOLD_MS`,
/// `tC4:src/views/OpenProgress.jsx:19`), counted from the person's action, whatever the
/// server has reported by then (A-22).
pub const SHOW_THRESHOLD_MS: f64 = 300.0;

/// tC4's three stages of an open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenStage {
    /// Reading the saved changes: tC4's journal.
    Journal,
    /// Checking the project's files.
    State,
    /// Preparing the screen: writing files, and reading the book after the open.
    Prepare,
}

impl OpenStage {
    /// tC4's `data-stage`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Journal => "journal",
            Self::State => "state",
            Self::Prepare => "prepare",
        }
    }

    pub fn caption_key(self) -> Key {
        match self {
            Self::Journal => keys::open::READING_JOURNAL,
            Self::State => keys::open::CHECKING_STATE,
            Self::Prepare => keys::open::PREPARING,
        }
    }

    /// STORAGE's five phases on tC4's three stages (Q-7): reading and folding the changes are
    /// tC4's journal, finishing earlier work and checking the files its state check, writing
    /// the files its preparing.
    pub fn of(phase: OpenPhase) -> Self {
        match phase {
            OpenPhase::ReadingChanges | OpenPhase::Folding => Self::Journal,
            OpenPhase::FinishingWork | OpenPhase::CheckingFiles => Self::State,
            OpenPhase::WritingFiles => Self::Prepare,
        }
    }
}

/// What the overlay shows of an open in flight: tC4's `s.opening`.
#[derive(Debug, Clone, PartialEq)]
pub struct Opening {
    /// When the person asked, in the page's milliseconds.
    pub started_at: f64,
    pub stage: OpenStage,
    pub done: u64,
    pub total: Option<u64>,
}

impl Opening {
    /// An open that has just been asked for: tC4 starts it in the journal stage.
    pub fn new(started_at: f64) -> Self {
        Self {
            started_at,
            stage: OpenStage::Journal,
            done: 0,
            total: None,
        }
    }

    /// The open job's report. Only the reading of the changes has a count tC4 draws.
    pub fn report(&mut self, progress: &Progress) {
        if let Phase::Open(phase) = progress.phase {
            self.stage = OpenStage::of(phase);
            let counted = phase == OpenPhase::ReadingChanges;
            self.done = if counted { progress.done } else { 0 };
            self.total = if counted { progress.total } else { None };
        }
    }

    /// The open ended and the screen reads its book.
    pub fn preparing(&mut self) {
        self.stage = OpenStage::Prepare;
        self.done = 0;
        self.total = None;
    }

    /// The wait left before the overlay shows, at `now`; 0 when it shows already.
    pub fn wait(&self, now: f64) -> f64 {
        (SHOW_THRESHOLD_MS - (now - self.started_at)).max(0.0)
    }

    /// The bar's percentage: floored, so 100 means every file is read; none (tC4's
    /// indeterminate bar) until the stage has a count.
    pub fn percent(&self) -> Option<u8> {
        let total = self
            .total
            .filter(|t| *t > 0 && self.stage == OpenStage::Journal)?;
        let pct = self.done.min(total).saturating_mul(100) / total;
        u8::try_from(pct).ok()
    }

    /// The caption: the stage, and while the changes are read, the count of files.
    pub fn caption(&self) -> String {
        let stage = t(self.stage.caption_key(), &[]);
        match (self.percent(), self.total) {
            (Some(_), Some(total)) => format!(
                "{stage} · {}",
                t(
                    keys::open::CHANGE_FILES,
                    &[
                        ("done", &self.done.to_string()),
                        ("total", &total.to_string())
                    ]
                )
            ),
            _ => stage,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Verse keys and focus
// ---------------------------------------------------------------------------------------

/// `4` → (4, 4), `4-5` → (4, 5); anything else is none.
fn range(text: &str) -> Option<(u32, u32)> {
    let (from, to) = text.split_once('-').unwrap_or((text, text));
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(from) || !digits(to) {
        return None;
    }
    Some((from.parse().ok()?, to.parse().ok()?))
}

/// Whether a verse key carries the verse a help names: both read as ranges, and they overlap
/// (tC4's `keyCarries`).
pub fn key_carries(key: &str, verse: &str) -> bool {
    match (range(key), range(verse)) {
        (Some((k_from, k_to)), Some((v_from, v_to))) => v_from <= k_to && k_from <= v_to,
        _ => false,
    }
}

/// The leading verse number of a key (`17-18` → 17), as JavaScript's `Number` reads it: a
/// key that is not a number is none.
pub fn leading_num(key: &str) -> Option<u32> {
    key.split('-').next()?.parse().ok()
}

fn trailing_num(key: &str) -> Option<u32> {
    key.rsplit('-').next()?.parse().ok()
}

/// Whether a help at `verse` is in focus (tC4's `inFocus`): every help with no focus; a
/// chapter's introduction when verse 1 is.
pub fn in_focus(verse: &str, focus: Option<&[String]>) -> bool {
    let Some(focus) = focus else {
        return true;
    };
    if verse == "intro" {
        return focus.iter().any(|k| key_carries(k, "1"));
    }
    focus.iter().any(|k| key_carries(k, verse))
}

/// The card in focus: its id, its verse, and where its quote is in each pane.
#[derive(Debug, Clone, PartialEq)]
pub struct HelpFocus {
    pub id: String,
    pub verse: String,
    pub panes: Vec<PaneFocus>,
}

impl HelpFocus {
    /// The tokens of `pane`'s verse `key` that render the quote; empty when the verse does not
    /// carry the help.
    pub fn lit(&self, pane: &str, key: &str) -> Vec<usize> {
        if !key_carries(key, &self.verse) {
            return Vec::new();
        }
        self.panes
            .iter()
            .find(|p| p.pane == pane)
            .and_then(|p| p.lit.iter().find(|l| l.verse.as_str() == key))
            .map(|l| {
                l.tokens
                    .iter()
                    .filter_map(|t| usize::try_from(*t).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The card's title in `pane`'s words.
    pub fn title(panes: &[PaneFocus], pane: &str) -> Option<String> {
        panes.iter().find(|p| p.pane == pane)?.title.clone()
    }
}

// ---------------------------------------------------------------------------------------
// Units, sections and paragraphs
// ---------------------------------------------------------------------------------------

/// Section or Verse: how Understand groups a chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitMode {
    Section,
    Verse,
}

/// One unit of the passage: a section, a verse, or the whole chapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// `s4`, `v4` or `whole`; in another numbering `m2:1`, `x2:1` or `u2:1`.
    pub key: String,
    /// Its first verse key.
    pub head: String,
    /// "Titus 1:1–3"; in another numbering, the source verse's place.
    pub label: String,
    pub verses: Vec<String>,
    /// In another numbering, the source verse's number, which the passage shows for it.
    pub number: Option<String>,
    /// In another numbering, why the unit has no passage.
    pub notice: Option<UnitNotice>,
}

/// Why a unit of another numbering shows no passage (tC4's `unitNotice`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitNotice {
    /// The project verse (`2:1`) has no place in the sources' numbering.
    Unmapped { at: String },
    /// The sources have it in another book (`NEH 1:1`), whose text this book never shows.
    OtherBook { at: String, to: String },
}

/// The design's unit label (tC4's `refLabel`): "John 1:1–2".
pub fn ref_label(book: BookId, chapter: u16, keys: &[String]) -> String {
    let (Some(from), Some(to)) = (keys.first(), keys.last()) else {
        return format!("{} {chapter}", book.english_name());
    };
    let span = if keys.len() > 1 || from.contains('-') {
        let end = match to.split_once('-') {
            Some((_, end)) => end.to_owned(),
            None => leading_num(to).map_or_else(String::new, |n| n.to_string()),
        };
        format!(
            "{}–{end}",
            leading_num(from).map_or_else(String::new, |n| n.to_string())
        )
    } else {
        from.clone()
    };
    format!("{} {chapter}:{span}", book.english_name())
}

/// The chapter's verse keys in order of their first verse (tC4 sorts `Object.keys`).
fn verse_keys(pane: &PaneChapter) -> Vec<String> {
    let mut keys: Vec<&PaneVerse> = pane
        .verses
        .iter()
        .filter(|v| range(v.key.as_str()).is_some())
        .collect();
    keys.sort_by_key(|v| leading_num(v.key.as_str()));
    keys.into_iter()
        .map(|v| v.key.as_str().to_owned())
        .collect()
}

/// tC4's `understandUnits` for a project in the source's numbering: one unit per section in
/// Section view (the whole chapter when the source marks no section), one per verse in Verse
/// view. No pane, or a pane with no chapter: no unit.
pub fn units(mode: UnitMode, book: BookId, chapter: u16, pane: Option<&PaneChapter>) -> Vec<Unit> {
    let Some(pane) = pane else {
        return Vec::new();
    };
    let keys = verse_keys(pane);
    match mode {
        UnitMode::Verse => keys
            .iter()
            .map(|k| Unit {
                key: format!("v{k}"),
                head: k.clone(),
                label: ref_label(book, chapter, std::slice::from_ref(k)),
                verses: vec![k.clone()],
                number: None,
                notice: None,
            })
            .collect(),
        UnitMode::Section if pane.sections.is_empty() => keys
            .first()
            .map(|head| Unit {
                key: "whole".to_owned(),
                head: head.clone(),
                label: format!("{} {chapter}", book.english_name()),
                verses: keys.clone(),
                number: None,
                notice: None,
            })
            .into_iter()
            .collect(),
        UnitMode::Section => {
            let starts = &pane.sections;
            starts
                .iter()
                .enumerate()
                .filter_map(|(i, from)| {
                    let from = u32::from(*from);
                    let to = starts.get(i + 1).map_or(u32::MAX, |n| u32::from(*n) - 1);
                    let within: Vec<String> = keys
                        .iter()
                        .filter(|k| leading_num(k).is_some_and(|n| n >= from && n <= to))
                        .cloned()
                        .collect();
                    let head = within.first()?.clone();
                    Some(Unit {
                        key: format!("s{from}"),
                        head,
                        label: ref_label(book, chapter, &within),
                        verses: within,
                        number: None,
                        notice: None,
                    })
                })
                .collect()
        }
    }
}

/// tC4's `crossFrameUnits`: in another numbering, one unit per project verse, labelled with
/// the source verse it shows, or stating why it shows none. The comment box keeps the project's
/// verse (#121).
pub fn frame_units(book: BookId, chapter: u16, rows: &[FrameRow]) -> Vec<Unit> {
    rows.iter()
        .map(|row| {
            let verse = row.verse.as_str().to_owned();
            let at = format!("{chapter}:{verse}");
            let (key, label, number, notice) = match &row.source {
                RowSource::Verse {
                    chapter: c,
                    verse: v,
                } => (
                    format!("m{at}"),
                    format!("{} {c}:{v}", book.english_name()),
                    Some(v.as_str().to_owned()),
                    None,
                ),
                RowSource::OtherBook { to } => (
                    format!("x{at}"),
                    String::new(),
                    None,
                    Some(UnitNotice::OtherBook {
                        at: at.clone(),
                        to: to.clone(),
                    }),
                ),
                RowSource::Unmapped => (
                    format!("u{at}"),
                    String::new(),
                    None,
                    Some(UnitNotice::Unmapped { at: at.clone() }),
                ),
            };
            Unit {
                key,
                head: verse.clone(),
                label,
                verses: vec![verse],
                number,
                notice,
            }
        })
        .collect()
}

/// A unit's display paragraphs (tC4's `paragraphsOf` and `paragraphLevel`): a verse opens a
/// paragraph when it is the unit's first, or a paragraph marker ends the verse before it or
/// opens its own text. Each paragraph is its verses with its poetry level.
pub fn paragraphs<'a>(unit: &Unit, pane: &'a PaneChapter) -> Vec<(Vec<&'a PaneVerse>, u8)> {
    let verses: Vec<&PaneVerse> = unit
        .verses
        .iter()
        .filter_map(|k| pane.verses.iter().find(|v| v.key.as_str() == k))
        .collect();
    let mut out: Vec<(Vec<&PaneVerse>, u8)> = Vec::new();
    for (i, verse) in verses.iter().enumerate() {
        let opens = i == 0 || verse.before.is_some() || verse.lead.is_some();
        if opens || out.is_empty() {
            // The previous paragraph's last verse is in the unit only after the first.
            let before = if out.is_empty() { None } else { verse.before };
            let level = verse.lead.or(before).unwrap_or(0);
            out.push((vec![verse], level));
        } else if let Some((para, _)) = out.last_mut() {
            para.push(verse);
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// The comment box (read)
// ---------------------------------------------------------------------------------------

/// What a unit's comment box shows (tC4's `displayedUnitNote`, A-18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownComment {
    pub text: String,
    /// The verse the box's comment is on (the unit's head when none is shown).
    pub target: String,
    /// Some verse of the unit has a comment: the unit's dot.
    pub any: bool,
    /// Comments on other verses of the unit that the box does not show.
    pub hidden: usize,
}

/// The newest comment of each verse of `chapter`, as (verse, text), in verse order.
fn newest_by_verse(comments: &[CommentView], chapter: u16) -> Vec<(u16, &str)> {
    let mut out: Vec<(u16, &CommentView)> = Vec::new();
    for comment in comments.iter().filter(|c| c.chapter == chapter) {
        match out.iter_mut().find(|(v, _)| *v == comment.verse) {
            Some((_, kept)) if kept.at <= comment.at => *kept = comment,
            Some(_) => {}
            None => out.push((comment.verse, comment)),
        }
    }
    out.sort_by_key(|(v, _)| *v);
    out.into_iter().map(|(v, c)| (v, c.text.as_str())).collect()
}

/// The comment a unit's box shows: the one at its head verse, else its only one, else none
/// when several verses have one; the newest when a verse has several.
pub fn shown_comment(comments: &[CommentView], chapter: u16, unit: &Unit) -> ShownComment {
    let lo = unit.verses.first().and_then(|k| leading_num(k));
    let hi = unit.verses.last().and_then(|k| trailing_num(k));
    let entries: Vec<(u16, &str)> = match (lo, hi) {
        (Some(lo), Some(hi)) => newest_by_verse(comments, chapter)
            .into_iter()
            .filter(|(v, _)| (lo..=hi).contains(&u32::from(*v)))
            .collect(),
        _ => Vec::new(),
    };
    let exact = entries.iter().find(|(v, _)| v.to_string() == unit.head);
    let shown = exact.or(if entries.len() == 1 {
        entries.first()
    } else {
        None
    });
    ShownComment {
        text: shown.map_or_else(String::new, |(_, text)| (*text).to_owned()),
        target: shown.map_or_else(|| unit.head.clone(), |(v, _)| v.to_string()),
        any: !entries.is_empty(),
        hidden: entries.len() - usize::from(shown.is_some()),
    }
}

// ---------------------------------------------------------------------------------------
// The helps panel
// ---------------------------------------------------------------------------------------

/// How much of a long note shows before "Show more" (`PREVIEW_CHARS`).
pub const PREVIEW_CHARS: usize = 400;

/// JavaScript's `\s`.
fn js_space(c: char) -> bool {
    crate::app::shell::js_space(c)
}

/// The first `units` UTF-16 code units of `text`, never half a character.
fn js_slice(text: &str, units: usize) -> &str {
    let mut used = 0;
    for (at, c) in text.char_indices() {
        if used + c.len_utf16() > units {
            return text.get(..at).unwrap_or(text);
        }
        used += c.len_utf16();
    }
    text
}

fn ellipsis() -> ArticleSpan {
    ArticleSpan {
        text: "…".to_owned(),
        bold: false,
        italic: false,
    }
}

/// A long note's preview, cut between words of the rendered blocks (tC4's `previewBlocks`),
/// with "…" where more follows.
pub fn preview_blocks(blocks: &[ArticleBlock], limit: usize) -> Vec<ArticleBlock> {
    let mut out: Vec<ArticleBlock> = Vec::new();
    let mut used = 0;
    for block in blocks {
        let room = limit.saturating_sub(used);
        if room == 0 {
            break;
        }
        let text = block.text();
        let len = js_len(&text);
        if len <= room {
            out.push(block.clone());
            used += len;
            continue;
        }
        let slice = js_slice(&text, room);
        // A budget that cannot hold the block's first whole word stops before the block.
        if !slice.chars().any(js_space) {
            break;
        }
        // `slice.replace(/\s+\S*$/, '')`: the last space run and the part word after it go.
        let trimmed = slice.trim_end_matches(|c| !js_space(c));
        let cut = trimmed.trim_end_matches(js_space);
        let mut spans = take_spans(&block.spans, js_len(cut));
        spans.push(ellipsis());
        out.push(ArticleBlock {
            spans,
            ..block.clone()
        });
        break;
    }
    let cut_short = out.len() < blocks.len();
    if let Some(last) = out.last_mut()
        && cut_short
        && !last.text().ends_with('…')
    {
        last.spans.push(ellipsis());
    }
    out
}

/// The total length of a note's blocks, as tC4 counts it.
pub fn blocks_len(blocks: &[ArticleBlock]) -> usize {
    blocks.iter().map(|b| js_len(&b.text())).sum()
}

/// The simplified tab's name: the pinned text's abbreviation (`en_ust` → UST), else "UST"
/// (tC4's `simplifiedLabel`).
pub fn simplified_label(pin: Option<&str>) -> String {
    pin.and_then(|repo| {
        let (_, abbreviation) = repo.rsplit_once('_')?;
        (!abbreviation.is_empty() && abbreviation.chars().all(|c| c.is_ascii_alphanumeric()))
            .then(|| abbreviation.to_uppercase())
    })
    .unwrap_or_else(|| t(keys::source::UST, &[]))
}

/// A slot's state for tC4's `data-testid` (`helps-state-<state>`) and its sentence.
pub fn slot_state<T>(slot: &Slot<T>) -> Option<(&'static str, Key)> {
    match slot {
        Slot::Ready(_) | Slot::Error { .. } => None,
        Slot::None => Some(("none", keys::understand::HELP_NONE)),
        Slot::Fetch => Some(("fetch", keys::understand::HELP_FETCH)),
        Slot::Missing => Some(("missing", keys::understand::HELP_MISSING)),
        Slot::OldTwl => Some(("old-twl", keys::check::OLD_TWL)),
    }
}

/// The pane a project's passage shows when the chosen tab is not one of its panes: its
/// literal Bible (`ult`, else `glt`), else its first pane (tC4's `literalPaneId`).
pub fn literal_pane(ids: &[String]) -> Option<String> {
    ["ult", "glt"]
        .into_iter()
        .find(|id| ids.iter().any(|i| i == id))
        .map(str::to_owned)
        .or_else(|| ids.first().cloned())
}

/// The helps panel's tabs (tC4's `helpsTab`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpsTab {
    Notes,
    Words,
    Questions,
    Simplified,
    /// Translate's own tab (#252): the user comments of the chapter, or of the story.
    Comments,
}

impl HelpsTab {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Notes => "notes",
            Self::Words => "words",
            Self::Questions => "questions",
            Self::Simplified => "simplified",
            Self::Comments => "comments",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        [
            Self::Notes,
            Self::Words,
            Self::Questions,
            Self::Simplified,
            Self::Comments,
        ]
        .into_iter()
        .find(|tab| tab.as_str() == text)
    }

    /// The tab a panel shows when the one chosen is not among its own (tC4's `shownTab`):
    /// Comments is Translate's only, so a panel with no comments shows Notes in its place, and a
    /// story has no simplified text.
    pub fn shown(self, comments: bool, story: bool) -> Self {
        match self {
            Self::Comments if !comments => Self::Notes,
            Self::Simplified if story => Self::Notes,
            tab => tab,
        }
    }
}

/// tC4's helps-panel width limits, in px (#602).
pub const HELPS_MIN: f64 = 280.0;
pub const HELPS_MAX: f64 = 720.0;

/// The dragged width, clamped and rounded (tC4's `clampHelps`).
pub fn clamp_helps(width: f64) -> f64 {
    width.clamp(HELPS_MIN, HELPS_MAX).round()
}

/// The panel's width while dragging (tC4's `widthAt`): it grows toward the editing pane.
pub fn drag_width(start_x: f64, start_width: f64, rtl: bool, x: f64) -> f64 {
    clamp_helps(start_width + if rtl { x - start_x } else { start_x - x })
}
