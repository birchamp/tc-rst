//! The suggestion model of one testament (tC4 `suggestEngine.ts`).

use super::boost::{self, Booster, Features, Mulberry32};
use super::wordmap::{self, Memory, Run, Token};
use super::{PositionLink, RawLink, SessionInput, TrainingVerse};
use std::collections::HashMap;
use tc_core::Testament;

/// Below this many aligned verses the booster is not fitted: plain wordMAP answers from
/// the memory, which works from the first verse.
pub const MIN_BOOST_VERSES: usize = 5;

const TRAIN_STEPS: usize = 1000;
const TREE_DEPTH: u32 = 5;
const LEARNING_RATE: f64 = 0.7;
/// The share of rejected candidates kept as negative examples.
const INCORRECT_KEPT: f64 = 0.1;
/// The most verses the booster collects candidates from; more are sampled at random.
const MAX_COLLECTED: usize = 1000;

/// The bound on the booster's training corpus, summed over verses as source words times
/// target words: 100 000 for the Greek model, 50 000 for the Hebrew one.
pub fn max_complexity(testament: Testament) -> usize {
    match testament {
        Testament::New => 100_000,
        Testament::Old => 50_000,
    }
}

fn complexity(verse: &TrainingVerse) -> usize {
    verse.source.len() * verse.target.len()
}

/// How many verses from the start of `verses` the booster fits on: verses are dropped
/// from the end until their summed complexity fits `cap`, keeping at least one.
pub fn bound_corpus(verses: &[TrainingVerse], cap: usize) -> usize {
    let mut total: usize = verses.iter().map(complexity).sum();
    let mut kept = verses.len();
    while total > cap && kept > 1 {
        kept -= 1;
        total -= complexity(&verses[kept]);
    }
    kept
}

/// A testament's suggestion model: the alignment memory of every confirmed verse, and a
/// booster when one is fitted.
#[derive(Debug, Clone)]
pub struct Model {
    testament: Testament,
    verses: usize,
    boosted: usize,
    too_few: bool,
    memory: Memory,
    booster: Option<Booster>,
}

/// A verse ready for the engine: its tokens and its links as token indices.
struct Prepared {
    source: Vec<Token>,
    target: Vec<Token>,
    links: Vec<PositionLink>,
}

impl Prepared {
    fn of(verse: &TrainingVerse) -> Self {
        Self {
            source: wordmap::tokens(&verse.source),
            target: wordmap::tokens(&verse.target),
            links: verse.links.clone(),
        }
    }

    fn remember(&self, memory: &mut Memory) {
        for link in &self.links {
            // A TrainingVerse's links name only words of the verse (its invariant).
            let source: Vec<&Token> = link.source.iter().map(|&i| &self.source[i]).collect();
            let target: Vec<&Token> = link.target.iter().map(|&i| &self.target[i]).collect();
            memory.append(&source, &target);
        }
    }
}

impl Model {
    /// A model that knows nothing yet.
    pub fn empty(testament: Testament) -> Self {
        Self {
            testament,
            verses: 0,
            boosted: 0,
            too_few: false,
            memory: Memory::default(),
            booster: None,
        }
    }

    pub fn testament(&self) -> Testament {
        self.testament
    }

    /// The verses in the memory, counted once each.
    pub fn verses(&self) -> usize {
        self.verses
    }

    /// The verses the booster was fitted on; 0 when plain wordMAP answers.
    pub fn boosted(&self) -> usize {
        self.boosted
    }

    /// The project had verses but the booster could not be fitted on them.
    pub fn too_few(&self) -> bool {
        self.too_few
    }

    pub fn booster(&self) -> Option<&Booster> {
        self.booster.as_ref()
    }

    /// Train a model on the project's confirmed alignments, as tC4's `trainModel` does.
    /// With fewer than [`MIN_BOOST_VERSES`] verses the model is memory only. From five
    /// up, the booster is fitted on the verses [`bound_corpus`] keeps: the even ones join
    /// the memory, the candidates of the odd ones against that memory are the training
    /// rows (every correct one, a random tenth of the rest), then the odd ones join the
    /// memory and the booster is fitted. Every verse past the bound joins the memory too.
    /// When the booster cannot be fitted, the model is memory only over every verse.
    ///
    /// The booster's random draws come from mulberry32 seeded with `seed`, so a training
    /// is reproducible; tC4's tests use seed 0. This can take minutes on a large project:
    /// run it off the thread that answers.
    pub fn train(testament: Testament, verses: &[TrainingVerse], seed: u32) -> Self {
        let mut model = Self::empty(testament);
        if verses.is_empty() {
            return model;
        }
        if verses.len() < MIN_BOOST_VERSES {
            return Self::memory_only(testament, verses, false);
        }
        let kept = bound_corpus(verses, max_complexity(testament));
        // tC4 keys the verses by reference: a repeated reference keeps its first place
        // and its last verse.
        let mut by_reference: Vec<&TrainingVerse> = Vec::new();
        let mut index: HashMap<&str, usize> = HashMap::new();
        for verse in &verses[..kept] {
            match index.get(verse.reference.as_str()) {
                Some(&at) => by_reference[at] = verse,
                None => {
                    index.insert(&verse.reference, by_reference.len());
                    by_reference.push(verse);
                }
            }
        }
        let prepared: Vec<Prepared> = by_reference.iter().map(|v| Prepared::of(v)).collect();
        let mut rng = Mulberry32::new(seed);
        let fitted = if kept >= 2 {
            // add_alignments_2: memory of the even verses, training rows from the odd.
            let (even, odd): (Vec<_>, Vec<_>) =
                prepared.iter().enumerate().partition(|(i, _)| i % 2 == 0);
            let even: Vec<&Prepared> = even.into_iter().map(|(_, p)| p).collect();
            let odd: Vec<&Prepared> = odd.into_iter().map(|(_, p)| p).collect();
            for p in &even {
                p.remember(&mut model.memory);
            }
            let (correct, incorrect) = collect(&model.memory, &odd, &mut rng);
            for p in &odd {
                p.remember(&mut model.memory);
            }
            boost::fit(
                correct,
                incorrect,
                TRAIN_STEPS,
                TREE_DEPTH,
                LEARNING_RATE,
                &mut rng,
            )
        } else {
            // add_alignments_3: every verse in memory, training rows from the same verses.
            let all: Vec<&Prepared> = prepared.iter().collect();
            for p in &all {
                p.remember(&mut model.memory);
            }
            let (correct, incorrect) = collect(&model.memory, &all, &mut rng);
            boost::fit(
                correct,
                incorrect,
                TRAIN_STEPS,
                TREE_DEPTH,
                LEARNING_RATE,
                &mut rng,
            )
        };
        let Ok(booster) = fitted else {
            return Self::memory_only(testament, verses, true);
        };
        for verse in &verses[kept..] {
            Prepared::of(verse).remember(&mut model.memory);
        }
        model.verses = verses.len();
        model.boosted = kept;
        model.booster = Some(booster);
        model
    }

    fn memory_only(testament: Testament, verses: &[TrainingVerse], too_few: bool) -> Self {
        let mut model = Self::empty(testament);
        for verse in verses {
            Prepared::of(verse).remember(&mut model.memory);
        }
        model.verses = verses.len();
        model.too_few = too_few;
        model
    }

    /// A confirmed save joins the memory at once and answers on the next prediction; the
    /// booster is untouched. `is_new` is false for a verse the memory already counts: its
    /// current links are added beside its old ones until the model is trained again, and
    /// it is not counted twice.
    pub fn append_verse(&mut self, verse: &TrainingVerse, is_new: bool) {
        Prepared::of(verse).remember(&mut self.memory);
        if is_new {
            self.verses += 1;
        }
    }

    /// Put a booster fitted elsewhere (a background training) on this model's memory,
    /// which stays as it is. tC4's `attachBooster`.
    pub fn attach_booster(&mut self, booster: Booster, boosted: usize) {
        self.booster = Some(booster);
        self.boosted = boosted;
    }

    /// One whole-verse suggestion for the open verse, as links in its positions. A
    /// boosted model scores through the booster with the placed links as context: a
    /// candidate that contradicts a placed link scores 0. A memory-only model answers
    /// with plain wordMAP. A model with an empty memory answers nothing.
    pub fn predict(&self, input: &SessionInput) -> Vec<RawLink> {
        self.predict_counted(input).0
    }

    /// [`Model::predict`] with what it did: the work of a suggestion is the verse's, not the
    /// project's (`docs/PERFORMANCE.md` rule 2).
    pub fn predict_counted(&self, input: &SessionInput) -> (Vec<RawLink>, Work) {
        if self.verses == 0 {
            return (Vec::new(), Work::default());
        }
        let source = wordmap::tokens(&input.source);
        let target = wordmap::tokens(&input.target);
        let mut run = wordmap::run(&self.memory, &source, &target);
        let mut work = Work {
            candidates: run.predictions.len(),
            memory_reads: run.memory_reads,
            tree_walks: 0,
        };
        match self.booster.as_ref().filter(|_| self.boosted > 0) {
            Some(booster) => {
                work.tree_walks = run.predictions.len() * booster.trees();
                score_with_context(&mut run, &input.manual, booster);
            }
            None => wordmap::score(&mut run.predictions),
        }
        let links = wordmap::suggest(&run)
            .into_iter()
            .map(|i| {
                let p = &run.predictions[i];
                RawLink {
                    source: run.source_tokens(p).iter().map(|t| t.position).collect(),
                    target: run.target_tokens(p).iter().map(|t| t.position).collect(),
                    confidence: p.scores.confidence,
                }
            })
            .collect();
        (links, work)
    }
}

/// What a prediction did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Work {
    /// Candidate alignments scored: source n-grams times target n-grams of the verse.
    pub candidates: usize,
    /// Reads of the alignment memory.
    pub memory_reads: usize,
    /// Trees walked, when a booster scored the candidates.
    pub tree_walks: usize,
}

/// How many candidates one task of the worker pool scores: a suggestion for a short verse
/// has hundreds and stays on the calling thread.
const SCORE_CHUNK: usize = 512;

/// uw-wordmapbooster's `score_with_context`: a candidate that would break a placed link
/// (a token of it placed with a token the candidate does not hold) scores 0; the rest are
/// scored by the booster.
fn score_with_context(run: &mut Run<'_>, manual: &[PositionLink], booster: &Booster) {
    let mut source_to_target: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut target_to_source: HashMap<usize, Vec<usize>> = HashMap::new();
    for link in manual {
        for &s in &link.source {
            for &t in &link.target {
                source_to_target.entry(s).or_default().push(t);
                target_to_source.entry(t).or_default().push(s);
            }
        }
    }
    // The morphology a candidate is scored on is its source words': read once for each
    // source n-gram of the run, not for each of the thousands of candidates.
    let categorical: Vec<[u32; boost::CATEGORICAL]> = run
        .source_ngrams
        .iter()
        .map(|n| booster.categorical_of_tokens(&run.source[n.start..n.start + n.len]))
        .collect();
    // The candidates are independent of one another, so they are scored on the worker pool
    // in chunks; the scores come back in the candidates' order.
    use rayon::prelude::*;
    let run_ref: &Run<'_> = run;
    let scores: Vec<f64> = run_ref
        .predictions
        .par_iter()
        .with_min_len(SCORE_CHUNK)
        .map(|p| {
            // The positions of a candidate's words are the runs of tokens it covers.
            let source = &run_ref.source_ngrams[p.source];
            let source = source.start..source.start + source.len;
            let target = p.target.map(|t| {
                let n = &run_ref.target_ngrams[t];
                n.start..n.start + n.len
            });
            // A candidate matched to nothing has no connections to compare.
            let breaks = target.as_ref().is_some_and(|target| {
                !target.is_empty()
                    && (source.clone().any(|s| {
                        source_to_target
                            .get(&s)
                            .is_some_and(|placed| placed.iter().any(|t| !target.contains(t)))
                    }) || target.clone().any(|t| {
                        target_to_source
                            .get(&t)
                            .is_some_and(|placed| placed.iter().any(|s| !source.contains(s)))
                    }))
            });
            if breaks {
                0.0
            } else {
                booster.score_row(&boost::numeric_of(&p.scores), &categorical[p.source])
            }
        })
        .collect();
    for (p, score) in run.predictions.iter_mut().zip(scores) {
        p.scores.confidence = score;
    }
}

/// uw-wordmapbooster's `collect_boost_training_data`: every candidate of each verse
/// against the memory, kept when it matches a confirmed link, else with probability
/// [`INCORRECT_KEPT`]. Over [`MAX_COLLECTED`] verses, a random sample of that many.
fn collect(
    memory: &Memory,
    verses: &[&Prepared],
    rng: &mut Mulberry32,
) -> (Vec<Features>, Vec<Features>) {
    let mut verses: Vec<&Prepared> = verses.to_vec();
    if verses.len() > MAX_COLLECTED {
        for i in (1..verses.len()).rev() {
            let j = rng.below(i + 1);
            verses.swap(i, j);
        }
        verses.truncate(MAX_COLLECTED);
    }
    let mut correct = Vec::new();
    let mut incorrect = Vec::new();
    for verse in verses {
        let gold: Vec<(Vec<TokenId<'_>>, Vec<TokenId<'_>>)> = verse
            .links
            .iter()
            .map(|link| {
                (
                    sorted_ids(link.source.iter().map(|&i| &verse.source[i])),
                    sorted_ids(link.target.iter().map(|&i| &verse.target[i])),
                )
            })
            .collect();
        let run = wordmap::run(memory, &verse.source, &verse.target);
        for p in &run.predictions {
            let source = sorted_ids(run.source_tokens(p).iter());
            let target = sorted_ids(run.target_tokens(p).iter());
            if gold.iter().any(|(s, t)| *s == source && *t == target) {
                correct.push(Features::of(&run, p));
            } else if rng.next() < INCORRECT_KEPT {
                incorrect.push(Features::of(&run, p));
            }
        }
    }
    (correct, incorrect)
}

/// What uw-wordmapbooster's `is_correct_prediction` compares of a token.
type TokenId<'a> = (&'a str, u32, u32);

/// The tokens of one side of a link as `is_correct_prediction` compares them: sorted by
/// text with JavaScript's default sort (stable, by UTF-16 code units), then compared by
/// text, occurrence and occurrences.
fn sorted_ids<'a>(tokens: impl Iterator<Item = &'a Token>) -> Vec<TokenId<'a>> {
    let mut tokens: Vec<&Token> = tokens.collect();
    tokens.sort_by(|a, b| a.text.encode_utf16().cmp(b.text.encode_utf16()));
    tokens
        .into_iter()
        .map(|t| (t.text.as_str(), t.occurrence, t.occurrences))
        .collect()
}
