//! The aligner's view, a pure function of the `Aligner` and the facts of its verse (APP-DESIGN
//! AD18, A-14). It depends on tc-align, tc-helps' highlight and tc-api only, so the aligner
//! could run in the page if a round trip ever missed its budget; G0 found it does not.

use tc_align::{Aligner, Alignment, Basis, SourceVerse, VerseState, verse_status};
use tc_api::align::{
    AlignPlace, AlignSave, AlignerView, BankSelection, Card, DroppedLinkView, Freshness,
    GatewayLens, RailState, RailStatus, SuggestStatus, SuggestView, Testament, Word,
};
use tc_api::ids::AlignerId;
use tc_api::shown::Direction;
use tc_helps::highlight::{AlignedVerse, TokenKind, normalize_word};
use tc_journal::record::{SourceWord, TargetWord};

/// The gateway text of the open verse with its own alignment: what the gateway lens reads.
#[derive(Debug, Clone)]
pub(super) struct Gateway {
    pub verse: AlignedVerse,
}

impl Gateway {
    /// Whether any word of the gateway text is tied to an original word (tC4's `gwAvailable`).
    pub(super) fn has_alignment(&self) -> bool {
        self.verse
            .tokens()
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Word(Some(_))))
    }

    /// The text with its white space collapsed, as the reference band shows it.
    fn text(&self) -> String {
        self.verse
            .text()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The gateway words that render a card's original words (tC4's `gwLabelFor`), compared
    /// in NFC on both sides (tC4 #768): a record imported from tC4 holds NFC words, and the
    /// resource's own text may not.
    fn label(&self, top: &[SourceWord]) -> Option<String> {
        let wanted: Vec<(String, u32)> = top
            .iter()
            .map(|w| (normalize_word(&w.word), w.occurrence))
            .collect();
        let words: Vec<&str> = self
            .verse
            .tokens()
            .iter()
            .filter_map(|token| {
                let TokenKind::Word(Some(id)) = &token.kind else {
                    return None;
                };
                let renders = self.verse.sources(*id)?.iter().any(|source| {
                    let key = normalize_word(&source.content);
                    wanted
                        .iter()
                        .any(|(word, occurrence)| *word == key && *occurrence == source.occurrence)
                });
                renders.then(|| token.text.trim())
            })
            .collect();
        (!words.is_empty()).then(|| words.join(" "))
    }
}

/// What does not change while a verse is open.
#[derive(Debug, Clone)]
pub(super) struct Facts {
    pub id: AlignerId,
    pub place: AlignPlace,
    pub testament: Testament,
    pub target_dir: Direction,
    pub orig_dir: Direction,
    pub original: String,
    pub gateway: Option<Gateway>,
    /// What the stored alignment is checked against.
    pub basis: Basis,
    pub source: SourceVerse,
}

/// What the session knows beside the aligner.
#[derive(Debug, Clone)]
pub(super) struct Moment {
    pub seq: u64,
    pub save: AlignSave,
    pub mark_refused: bool,
    pub suggest_on: bool,
    pub suggest: SuggestStatus,
    /// A Suggest was answered for this verse.
    pub asked: bool,
    pub suggesting: bool,
}

pub(super) fn word(w: &TargetWord) -> Word {
    Word {
        word: w.word.clone(),
        occurrence: w.occurrence,
        occurrences: w.occurrences,
    }
}

pub(super) fn source_word(w: &SourceWord) -> Word {
    Word {
        word: w.word.clone(),
        occurrence: w.occurrence,
        occurrences: w.occurrences,
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The open verse's row on the rail. A value that needed review when the verse opened stays
/// "Invalid" until the translator edits it, marks it valid or keeps the carry-over, though the
/// aligner's own value is already carried over to the verse's text.
pub(super) fn rail_status(aligner: &Aligner, facts: &Facts) -> RailStatus {
    let value = aligner.value();
    let status = verse_status(
        Some(value),
        Some((&facts.basis.aligned_run(aligner.verse()), aligner.verse())),
        Some(&facts.source),
    );
    let state = if aligner.is_stale() {
        RailState::Invalid
    } else {
        rail_state(status.state)
    };
    RailStatus {
        state,
        placed: count(status.placed),
        total: count(status.total),
    }
}

pub(super) fn rail_state(state: VerseState) -> RailState {
    match state {
        VerseState::Undrafted => RailState::Undrafted,
        VerseState::Todo => RailState::Todo,
        VerseState::Valid => RailState::Valid,
        VerseState::Invalid => RailState::Invalid,
    }
}

pub(super) fn build(aligner: &Aligner, facts: &Facts, now: &Moment) -> AlignerView {
    let value = aligner.value();
    let shown = aligner.shown_bank();
    let selected = aligner.selection();
    let armed: Vec<u32> = shown
        .iter()
        .enumerate()
        .filter(|(_, w)| {
            selected
                .iter()
                .any(|s| s.word == w.word && s.occurrence == w.occurrence)
        })
        .map(|(i, _)| count(i))
        .collect();
    let lens = facts.gateway.as_ref().filter(|g| g.has_alignment());
    let cards = value
        .alignments
        .iter()
        .enumerate()
        .map(|(index, group)| Card {
            index: count(index),
            top: group.top_words.iter().map(source_word).collect(),
            placed: group.bottom_words.iter().map(word).collect(),
            suggested: aligner
                .proposals()
                .iter()
                .filter(|p| p.card == index)
                .map(|p| word(&p.word))
                .collect(),
            gateway: lens.and_then(|g| g.label(&group.top_words)),
        })
        .collect();
    AlignerView {
        aligner: facts.id,
        seq: now.seq,
        place: facts.place,
        testament: facts.testament,
        target_dir: facts.target_dir,
        orig_dir: facts.orig_dir,
        original: facts.original.clone(),
        gateway: lens.map(|g| GatewayLens { text: g.text() }),
        bank: shown.iter().map(word).collect(),
        selection: if armed.is_empty() {
            BankSelection::None
        } else {
            BankSelection::Armed(armed)
        },
        cards,
        placed: count(value.placed()),
        total: count(value.total()),
        done: value.done && !aligner.is_stale() && !value.marked_invalid,
        mark_refused: now.mark_refused,
        suggest: SuggestView {
            on: now.suggest_on,
            status: if now.suggest_on {
                now.suggest.clone()
            } else {
                SuggestStatus::Off
            },
            asked: now.asked,
            standing: count(aligner.proposals().len()),
            suggesting: now.suggesting,
        },
        freshness: if aligner.is_stale() {
            Freshness::Stale
        } else {
            Freshness::Current
        },
        dropped: aligner
            .dropped()
            .iter()
            .map(|d| DroppedLinkView {
                card: count(d.card),
                top: d.top_words.iter().map(source_word).collect(),
                word: word(&d.word),
            })
            .collect(),
        status: rail_status(aligner, facts),
        save: now.save.clone(),
    }
}
