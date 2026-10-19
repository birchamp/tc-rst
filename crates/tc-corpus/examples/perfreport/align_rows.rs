//! The tc-align rows (PERFORMANCE.md "Aligning"): open the aligner on the longest verse of
//! Psalms, every edit of it, a reflow after a one-word edit, and suggestions from models
//! trained on the ULT v91 export. No project is open: the aligner takes one verse and
//! nothing of the project, which the count tests pin.
//!
//! The ULT verses and the UHB words are the ones `tc-align`'s own Psalms test reads
//! (`fixtures/scale/psa/`); the training verses come from `bundle/en/en_ult_v91.sb.zip`, as
//! the app reads the installed export.

use crate::common::{header, ms, on_pool};
use crate::table::Measured;
use std::io::Read;
use std::time::Instant;
use tc_align::suggest::{Model, TrainingVerse};
use tc_align::usfm::from_aligned;
use tc_align::{
    Aligner, AlignmentValue, Basis, SourceVerse, TargetVerse, TargetWord, bootstrap, reflow,
};
use tc_core::{BookId, Testament};
use tc_journal::ids::VerseNo;
use tc_usfm::Book;

const ULT_PSA: &str = "fixtures/scale/psa/en_ult/19-PSA.usfm";
const UHB_PSA: &str = "fixtures/scale/psa/hbo_uhb/19-PSA.usfm";

fn read(rel: &str) -> String {
    let path = tc_corpus::repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn basis() -> Basis {
    Basis::run(
        VerseNo::new(1).expect("verse 1"),
        VerseNo::new(1).expect("verse 1"),
        "dcs::unfoldingWord/hbo_uhb@v3.0.0".to_owned(),
        None,
    )
}

/// The ULT verse of Psalms with the most words, its reference, the Hebrew behind it, and its
/// stored value: made against the verse's text, with the bank and every card's words
/// reversed, as an old tC4 save or an import has them.
struct Longest {
    reference: String,
    words: usize,
    source: SourceVerse,
    target: TargetVerse,
    stored: AlignmentValue,
}

fn longest() -> Longest {
    let (ult, uhb) = (read(ULT_PSA), read(UHB_PSA));
    let (ult, uhb) = (Book::parse(&ult), Book::parse(&uhb));
    let mut best: Option<(usize, u16, tc_core::VerseSpan, String)> = None;
    for unit in ult.verses() {
        let (Some(chapter), Some(verses)) = (unit.chapter(), unit.verses()) else {
            continue;
        };
        let words = tc_usfm::parse_aligned(unit.body())
            .map(|a| a.words.len())
            .unwrap_or(0);
        if best.as_ref().is_none_or(|(n, ..)| words > *n) {
            best = Some((words, chapter, verses, unit.body().to_owned()));
        }
    }
    let (words, chapter, verses, body) = best.expect("Psalms has verses");
    let source = SourceVerse::from_book(&uhb, chapter, verses).expect("the Hebrew verse");
    let aligned = tc_usfm::parse_aligned(&body).expect("aligned USFM");
    let target = TargetVerse::new(&aligned.plain);
    let extracted = from_aligned(&aligned, Some(&source));
    let mut stored = bootstrap(&source, &target, basis());
    stored.alignments = extracted.alignments;
    stored.word_bank = extracted.word_bank;
    stored.word_bank.reverse();
    for group in &mut stored.alignments {
        group.bottom_words.reverse();
    }
    Longest {
        reference: format!("PSA {chapter}:{verses}"),
        words,
        source,
        target,
        stored,
    }
}

/// Open, edit and reflow the longest verse, 50 times each.
pub fn align_edit() -> Vec<Measured> {
    header("align-edit");
    let verse = longest();
    let open = || {
        Aligner::open(
            Some(&verse.stored),
            verse.target.clone(),
            &verse.source,
            basis(),
        )
        .expect("the verse opens")
    };
    let mut rows = Vec::new();

    let mut times = Vec::new();
    for _ in 0..51 {
        let started = Instant::now();
        let aligner = open();
        times.push(ms(started));
        std::hint::black_box(aligner);
    }
    let cards = verse.stored.alignments.len();
    rows.push(Measured::new(
        "align.open",
        times,
        format!(
            "{} ({} words, {cards} cards, {} source words)",
            verse.reference,
            verse.words,
            verse.source.words().len()
        ),
    ));

    // The edits a translator makes on the verse, each timed on its own: every word back to
    // the bank, then every word onto one card in one drop, then the verse cleared.
    let placed: Vec<TargetWord> = verse
        .stored
        .alignments
        .iter()
        .flat_map(|g| g.bottom_words.clone())
        .collect();
    let mut edits = Vec::new();
    for _ in 0..20 {
        let mut aligner = open();
        for (card, group) in verse.stored.alignments.iter().enumerate() {
            for word in &group.bottom_words {
                let started = Instant::now();
                aligner.unplace(card, word).expect("a placed word");
                edits.push(ms(started));
            }
        }
        let started = Instant::now();
        aligner.place_many(0, &placed).expect("all words");
        edits.push(ms(started));
        let started = Instant::now();
        aligner.clear();
        edits.push(ms(started));
        std::hint::black_box(aligner);
    }
    rows.push(Measured::new(
        "align.edits",
        edits,
        format!(
            "{} edits a round, 20 rounds, each timed alone",
            placed.len() + 2
        ),
    ));

    // A one-word edit of the verse text: the alignment carried over.
    let edited = TargetVerse::new(&verse.target.text().replacen("the", "a", 1));
    let mut saved = open();
    saved.mark_valid().expect("a complete alignment");
    let saved = saved.into_value();
    let mut times = Vec::new();
    for _ in 0..51 {
        let started = Instant::now();
        let carried = reflow(&saved, &edited);
        times.push(ms(started));
        std::hint::black_box(carried.is_ok());
    }
    rows.push(Measured::new(
        "align.reflow",
        times,
        "a one-word edit of the longest verse",
    ));
    rows
}

// ---- suggestions ------------------------------------------------------------------------

/// The aligned verses of the ULT v91 export for `books`, in canonical order.
fn training(books: &[BookId], held_out: &str) -> Vec<TrainingVerse> {
    let path = tc_corpus::repo_root().join("bundle/en/en_ult_v91.sb.zip");
    let file = std::fs::File::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut archive = zip::ZipArchive::new(file).expect("the en_ult export");
    let mut verses = Vec::new();
    for &book in books {
        let name = format!("en_ult/ingredients/{}.usfm", book.code());
        let mut text = String::new();
        match archive.by_name(&name) {
            Ok(mut entry) => entry.read_to_string(&mut text).expect("utf-8"),
            Err(_) => continue,
        };
        let parsed = Book::parse(&text);
        for unit in parsed.verses() {
            let (Some(c), Some(v)) = (unit.chapter(), unit.verses()) else {
                continue;
            };
            let reference = format!("{} {c}:{v}", book.code());
            if reference == held_out {
                continue;
            }
            if let Ok(Some(verse)) = TrainingVerse::from_aligned_usfm(reference, unit.body()) {
                verses.push(verse);
            }
        }
    }
    verses
}

/// One verse of the export as a question to a model: the verse being aligned has no
/// alignment yet.
fn question(book: BookId, reference: &str) -> Option<TrainingVerse> {
    let path = tc_corpus::repo_root().join("bundle/en/en_ult_v91.sb.zip");
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut text = String::new();
    archive
        .by_name(&format!("en_ult/ingredients/{}.usfm", book.code()))
        .ok()?
        .read_to_string(&mut text)
        .ok()?;
    let parsed = Book::parse(&text);
    parsed.verses().find_map(|unit| {
        let (c, v) = (unit.chapter()?, unit.verses()?);
        (format!("{} {c}:{v}", book.code()) == reference)
            .then(|| {
                TrainingVerse::from_aligned_usfm(reference, unit.body())
                    .ok()
                    .flatten()
            })
            .flatten()
    })
}

/// The 27 books of the New Testament.
fn nt() -> Vec<BookId> {
    (40..=66u8).filter_map(BookId::from_ordinal).collect()
}

/// Suggestions for one verse from the models of the NT and of Genesis and Psalms, and what
/// training them and updating them after a save costs.
pub fn align_suggest() -> Vec<Measured> {
    on_pool(align_suggest_in)
}

fn align_suggest_in() -> Vec<Measured> {
    header("align-suggest");
    let mut rows = Vec::new();
    let held = "PSA 42:4";

    // Genesis and Psalms without the verse being aligned.
    let started = Instant::now();
    let ot_verses = training(&[BookId::Genesis, BookId::Psalms], held);
    let ot_read = ms(started);
    let asked = question(BookId::Psalms, held).expect("PSA 42:4 in the export");
    let started = Instant::now();
    let ot = Model::train(Testament::Old, &ot_verses, 0);
    let ot_train = ms(started);
    rows.push(Measured::new(
        "align.train_ot",
        vec![ot_train],
        format!(
            "{} verses read in {ot_read:.0} ms; {} in the memory, booster on {} ({} trees)",
            ot_verses.len(),
            ot.verses(),
            ot.boosted(),
            ot.booster().map_or(0, tc_align::suggest::Booster::trees)
        ),
    ));

    let started = Instant::now();
    let nt_verses = training(&nt(), "");
    let nt_read = ms(started);
    let started = Instant::now();
    let nt = Model::train(Testament::New, &nt_verses, 0);
    let nt_train = ms(started);
    rows.push(Measured::new(
        "align.train_nt",
        vec![nt_train],
        format!(
            "{} verses read in {nt_read:.0} ms; {} in the memory, booster on {}",
            nt_verses.len(),
            nt.verses(),
            nt.boosted()
        ),
    ));

    // Suggestions: the verse asked about, 51 times.
    let mut times = Vec::new();
    let mut links = 0;
    for _ in 0..51 {
        let started = Instant::now();
        let raw = ot.predict(&asked.as_question());
        times.push(ms(started));
        links = raw.len();
    }
    // And a New Testament verse from the NT model: the longest by tokens.
    let nt_asked = nt_verses
        .iter()
        .max_by_key(|v| v.source().len() * v.target().len())
        .expect("NT verses");
    let mut nt_times = Vec::new();
    for _ in 0..51 {
        let started = Instant::now();
        let raw = nt.predict(&nt_asked.as_question());
        nt_times.push(ms(started));
        std::hint::black_box(raw);
    }
    rows.push(Measured::new(
        "align.suggest",
        times,
        format!("{held} from the Genesis and Psalms model ({links} links proposed), 51 times"),
    ));
    rows.push(Measured::new(
        "align.suggest_longest",
        nt_times,
        format!(
            "{}, the verse of the NT with the most candidates, from the NT model, 51 times",
            nt_asked.reference()
        ),
    ));

    // After a save: the model takes the verse into its memory.
    let mut appended = ot.clone();
    let mut times = Vec::new();
    for verse in ot_verses.iter().take(30) {
        let started = Instant::now();
        appended.append_verse(verse, false);
        times.push(ms(started));
    }
    rows.push(Measured::new(
        "align.append",
        times,
        "append_verse of verses already known, 30 times",
    ));
    rows
}
