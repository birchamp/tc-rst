//! A port of wordMAP 0.6.2 as tC4 runs it: alignment memory only, no corpus.
//!
//! tC4 never feeds wordMAP a corpus (`suggestEngine.ts` calls only
//! `appendAlignmentMemory`), so every corpus-based score is 0, as it is there: the
//! permutation ratios, the filtered corpus ratios, phrase plausibility (1 for a source
//! n-gram matched to nothing, else 0) and uniqueness. The other scores and the final
//! confidence follow wordMAP's code step by step, in its order of floating-point
//! operations, so the confidences equal tC4's to the last bit (the vector tests compare
//! them).
//!
//! Quirks kept because they decide which links are proposed:
//!
//! - An n-gram's key is `n:` and its token texts joined by `:`, lowercased; its lemma
//!   key is the same over lemmas, present only when every token has a lemma. An empty
//!   n-gram's key is `n:` but its lemma key is `n`, so an alignment to nothing always has
//!   a lemma key.
//! - An index counts an n-gram under its key and under its lemma key in one map, so a
//!   word whose lemma equals its text counts twice.
//! - Lengths in characters are UTF-16 code units, as JavaScript counts them.
//!
//! Ported from wordmap 0.6.2 and wordmap-lexer 0.3.6, © unfoldingWord, ISC licence; the
//! notice is in the crate's `NOTICE` file.

use super::TokenSeed;
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

/// A word of a sentence with wordMAP's positional metrics, built as tC4 builds it:
/// `new Token(seed)` and then uw-wordmapbooster's `updateTokenLocations`.
#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub text: String,
    pub position: usize,
    pub sentence_token_len: usize,
    pub sentence_char_len: usize,
    pub occurrence: u32,
    pub occurrences: u32,
    pub lemma: String,
    pub morph: String,
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Tokens for one sentence. `updateTokenLocations` renumbers positions and occurrences
/// from the order and the texts, whatever the seeds said. (It also sets a character
/// position, which no algorithm tC4 runs reads.)
///
/// Every sentence the engine learns or is asked reaches it here, so the text and lemma are
/// put in NFC here, once (tC4 #793): tC4 4.0 stores Hebrew source words in NFC and the pinned
/// UHB writes them otherwise, and a key must not tell the two apart.
pub(crate) fn tokens(seeds: &[TokenSeed]) -> Vec<Token> {
    let nfc = |text: &str| text.nfc().collect::<String>();
    let texts: Vec<String> = seeds.iter().map(|s| nfc(&s.text)).collect();
    let joined: usize =
        texts.iter().map(|t| utf16_len(t)).sum::<usize>() + seeds.len().saturating_sub(1);
    let mut seen: HashMap<&str, u32> = HashMap::new();
    let mut out: Vec<Token> = Vec::with_capacity(seeds.len());
    for (position, (seed, text)) in seeds.iter().zip(&texts).enumerate() {
        let n = seen.entry(text.as_str()).or_default();
        *n += 1;
        out.push(Token {
            text: text.clone(),
            position,
            sentence_token_len: seeds.len(),
            sentence_char_len: joined,
            occurrence: *n,
            occurrences: 0,
            lemma: seed.lemma.as_deref().map(nfc).unwrap_or_default(),
            morph: seed.morph.clone().unwrap_or_default(),
        });
    }
    for token in &mut out {
        token.occurrences = seen.get(token.text.as_str()).copied().unwrap_or(0);
    }
    out
}

/// wordMAP's `Ngram` keys for a run of tokens.
#[derive(Debug, Clone)]
pub(crate) struct NgramKeys {
    pub key: String,
    pub lemma_key: Option<String>,
}

pub(crate) fn ngram_keys<'a>(tokens: impl IntoIterator<Item = &'a Token>) -> NgramKeys {
    let mut key = String::from("n:");
    let mut lemma_key = String::from("n:");
    let mut missing_lemma = false;
    let mut count = 0;
    for token in tokens {
        count += 1;
        key.push_str(&token.text);
        key.push(':');
        if token.lemma.is_empty() {
            missing_lemma = true;
        } else {
            lemma_key.push_str(&token.lemma);
            lemma_key.push(':');
        }
    }
    let key = if count > 0 {
        key.pop();
        key.to_lowercase()
    } else {
        key
    };
    let lemma_key = (!missing_lemma).then(|| {
        lemma_key.pop();
        lemma_key.to_lowercase()
    });
    NgramKeys { key, lemma_key }
}

/// wordMAP's `Alignment` keys: `source->target`, and a lemma key when either side has
/// one, falling back to the plain key on the other side.
pub(crate) fn alignment_keys(source: &NgramKeys, target: &NgramKeys) -> (String, Option<String>) {
    let key = format!("{}->{}", source.key, target.key);
    let lemma = (source.lemma_key.is_some() || target.lemma_key.is_some()).then(|| {
        format!(
            "{}->{}",
            source.lemma_key.as_deref().unwrap_or(&source.key),
            target.lemma_key.as_deref().unwrap_or(&target.key)
        )
    });
    (key, lemma)
}

/// wordMAP's `FrequencyIndex`: counts by key. An n-gram or alignment increments its key
/// and then its lemma key, in the same map.
#[derive(Debug, Clone, Default)]
pub(crate) struct Frequencies(HashMap<String, u32>);

impl Frequencies {
    fn increment(&mut self, key: &str, lemma_key: Option<&str>) {
        *self.0.entry(key.to_owned()).or_default() += 1;
        if let Some(lemma) = lemma_key {
            *self.0.entry(lemma.to_owned()).or_default() += 1;
        }
    }

    pub fn read(&self, key: &str) -> f64 {
        f64::from(self.0.get(key).copied().unwrap_or(0))
    }
}

/// wordMAP's `AlignmentMemoryIndex`: how often each alignment, source n-gram and target
/// n-gram occurs in the confirmed alignments.
#[derive(Debug, Clone, Default)]
pub(crate) struct Memory {
    alignments: Frequencies,
    sources: Frequencies,
    targets: Frequencies,
}

impl Memory {
    /// Add one confirmed link: its source tokens and target tokens, in the link's order.
    pub fn append(&mut self, source: &[&Token], target: &[&Token]) {
        let s = ngram_keys(source.iter().copied());
        let t = ngram_keys(target.iter().copied());
        let (key, lemma) = alignment_keys(&s, &t);
        self.alignments.increment(&key, lemma.as_deref());
        self.sources.increment(&s.key, s.lemma_key.as_deref());
        self.targets.increment(&t.key, t.lemma_key.as_deref());
    }
}

/// An n-gram of the sentence being predicted: a run of tokens and its occurrence among
/// n-grams of the same key and size (wordMAP's `Parser.sizedNgrams`).
#[derive(Debug, Clone)]
pub(crate) struct Ngram {
    pub start: usize,
    pub len: usize,
    pub occurrence: u32,
    pub keys: NgramKeys,
}

/// wordMAP's `Parser.ngrams`: every contiguous n-gram up to `max` tokens, by size and
/// then position.
fn ngrams(sentence: &[Token], max: usize) -> Vec<Ngram> {
    let mut out = Vec::new();
    for len in 1..=max.min(sentence.len()) {
        let mut occurrences: HashMap<String, u32> = HashMap::new();
        for start in 0..=sentence.len() - len {
            let keys = ngram_keys(&sentence[start..start + len]);
            let n = occurrences.entry(keys.key.clone()).or_default();
            *n += 1;
            out.push(Ngram {
                start,
                len,
                occurrence: *n,
                keys,
            });
        }
    }
    out
}

/// The scores wordMAP gives one prediction. Corpus scores are always 0 (see the module
/// documentation) and are not stored.
#[derive(Debug, Clone, Default)]
pub(crate) struct Scores {
    pub source_memory_ratio: f64,
    pub target_memory_ratio: f64,
    pub memory_filtered: f64,
    pub source_memory_lemma_ratio: f64,
    pub target_memory_lemma_ratio: f64,
    pub memory_lemma_filtered: f64,
    pub relative_occurrence: Option<f64>,
    pub alignment_position: f64,
    pub phrase_plausibility: f64,
    pub lemma_phrase_plausibility: f64,
    pub ngram_length: f64,
    pub character_length: f64,
    pub alignment_occurrences: f64,
    pub lemma_alignment_occurrences: f64,
    pub uniqueness: f64,
    pub lemma_uniqueness: f64,
    pub confidence: f64,
}

/// One candidate alignment of a source n-gram to a target n-gram, or to nothing.
#[derive(Debug, Clone)]
pub(crate) struct Prediction {
    pub source: usize,
    pub target: Option<usize>,
    pub key: String,
    pub lemma_key: Option<String>,
    pub scores: Scores,
}

/// One engine run over a sentence pair: its n-grams and every candidate with its scores.
#[derive(Debug, Clone)]
pub(crate) struct Run<'a> {
    pub source: &'a [Token],
    pub target: &'a [Token],
    pub source_ngrams: Vec<Ngram>,
    pub target_ngrams: Vec<Ngram>,
    pub predictions: Vec<Prediction>,
}

impl Run<'_> {
    pub fn source_tokens(&self, p: &Prediction) -> &[Token] {
        let n = &self.source_ngrams[p.source];
        &self.source[n.start..n.start + n.len]
    }

    pub fn target_tokens(&self, p: &Prediction) -> &[Token] {
        match p.target {
            Some(t) => {
                let n = &self.target_ngrams[t];
                &self.target[n.start..n.start + n.len]
            }
            None => &[],
        }
    }
}

const SOURCE_NGRAM_LENGTH: usize = 3;
const TARGET_NGRAM_LENGTH: usize = 5;

fn divide_safe(dividend: f64, divisor: f64) -> f64 {
    if divisor == 0.0 {
        0.0
    } else {
        dividend / divisor
    }
}

/// `Math.min(a, b) / Math.max(a, b)`, as wordMAP's similarity scores compute it.
fn min_over_max(a: f64, b: f64) -> f64 {
    a.min(b) / a.max(b)
}

fn similarity(source: f64, target: f64) -> f64 {
    if source == 0.0 || target == 0.0 {
        0.0
    } else {
        min_over_max(source, target)
    }
}

/// wordMAP's `Engine.run`: generate every candidate and run its algorithms, in order:
/// n-gram frequency and lemma n-gram frequency over all candidates, then for each one
/// relative occurrence, position, phrase plausibility, n-gram length, character length,
/// occurrences and uniqueness. `confidence` is left at 0 for [`score`] or the booster.
pub(crate) fn run<'a>(memory: &Memory, source: &'a [Token], target: &'a [Token]) -> Run<'a> {
    let source_ngrams = ngrams(source, SOURCE_NGRAM_LENGTH);
    let target_ngrams = ngrams(target, TARGET_NGRAM_LENGTH);
    let empty = ngram_keys(std::iter::empty());

    // The unaligned sentence index: how often each n-gram occurs in this sentence pair.
    let mut sentence_sources = Frequencies::default();
    for n in &source_ngrams {
        sentence_sources.increment(&n.keys.key, n.keys.lemma_key.as_deref());
    }
    let mut sentence_targets = Frequencies::default();
    for n in &target_ngrams {
        sentence_targets.increment(&n.keys.key, n.keys.lemma_key.as_deref());
    }

    let mut predictions = Vec::with_capacity(source_ngrams.len() * (target_ngrams.len() + 1));
    for (s, source_ngram) in source_ngrams.iter().enumerate() {
        let targets = target_ngrams
            .iter()
            .enumerate()
            .map(|(t, n)| (Some(t), &n.keys))
            .chain(std::iter::once((None, &empty)));
        for (t, target_keys) in targets {
            let (key, lemma_key) = alignment_keys(&source_ngram.keys, target_keys);
            predictions.push(Prediction {
                source: s,
                target: t,
                key,
                lemma_key,
                scores: Scores::default(),
            });
        }
    }

    let target_keys =
        |p: &Prediction| -> &NgramKeys { p.target.map_or(&empty, |t| &target_ngrams[t].keys) };

    // NgramFrequency.
    let mut sums: HashMap<&str, f64> = HashMap::new();
    let mut frequencies = Vec::with_capacity(predictions.len());
    for p in &predictions {
        let alignment = memory.alignments.read(&p.key);
        let source_frequency = memory.sources.read(&source_ngrams[p.source].keys.key);
        let target_frequency = memory.targets.read(&target_keys(p).key);
        frequencies.push((alignment, source_frequency, target_frequency));
        *sums.entry(p.key.as_str()).or_default() += alignment;
    }
    let filtered: Vec<f64> = predictions
        .iter()
        .zip(&frequencies)
        .map(|(p, (alignment, _, _))| divide_safe(*alignment, sums[p.key.as_str()]))
        .collect();
    for ((p, (alignment, source_frequency, target_frequency)), filtered) in
        predictions.iter_mut().zip(frequencies).zip(filtered)
    {
        p.scores.source_memory_ratio = divide_safe(alignment, source_frequency);
        p.scores.target_memory_ratio = divide_safe(alignment, target_frequency);
        p.scores.memory_filtered = filtered;
    }

    // LemmaNgramFrequency.
    let mut lemma_sums: HashMap<&str, f64> = HashMap::new();
    let mut lemma_frequencies = Vec::with_capacity(predictions.len());
    for p in &predictions {
        let Some(lemma) = &p.lemma_key else {
            lemma_frequencies.push(None);
            continue;
        };
        let alignment = memory.alignments.read(lemma);
        let s = &source_ngrams[p.source].keys;
        let t = target_keys(p);
        let source_frequency = memory
            .sources
            .read(s.lemma_key.as_deref().unwrap_or(&s.key));
        let target_frequency = memory
            .targets
            .read(t.lemma_key.as_deref().unwrap_or(&t.key));
        lemma_frequencies.push(Some((alignment, source_frequency, target_frequency)));
        *lemma_sums.entry(p.key.as_str()).or_default() += alignment;
    }
    let lemma_filtered: Vec<f64> = predictions
        .iter()
        .zip(&lemma_frequencies)
        .map(|(p, f)| match f {
            Some((alignment, _, _)) => divide_safe(*alignment, lemma_sums[p.key.as_str()]),
            None => 0.0,
        })
        .collect();
    for ((p, f), filtered) in predictions
        .iter_mut()
        .zip(lemma_frequencies)
        .zip(lemma_filtered)
    {
        if let Some((alignment, source_frequency, target_frequency)) = f {
            p.scores.source_memory_lemma_ratio = divide_safe(alignment, source_frequency);
            p.scores.target_memory_lemma_ratio = divide_safe(alignment, target_frequency);
        }
        p.scores.memory_lemma_filtered = filtered;
    }

    // The per-prediction algorithms.
    for p in &mut predictions {
        let s = &source_ngrams[p.source];
        let s_tokens = &source[s.start..s.start + s.len];
        let t = p.target.map(|t| &target_ngrams[t]);
        let t_keys = t.map_or(&empty, |t| &t.keys);
        let t_tokens: &[Token] = t.map_or(&[], |t| &target[t.start..t.start + t.len]);

        // AlignmentRelativeOccurrence.
        if let Some(t) = t {
            let y_range = sentence_sources.read(s.keys.lemma_key.as_deref().unwrap_or(&s.keys.key));
            let x_range = sentence_targets.read(t.keys.lemma_key.as_deref().unwrap_or(&t.keys.key));
            let y = f64::from(s.occurrence);
            let x = f64::from(t.occurrence);
            let x_prime =
                (x * y_range - 1.0 * x - 1.0 * y_range + 1.0 * 1.0) / (x_range - 1.0) + 1.0;
            let ny = (y - 1.0) / (y_range - 1.0);
            let nx_prime = (x_prime - 1.0) / (y_range - 1.0);
            let weight = 1.0 - (ny - nx_prime).abs();
            if !weight.is_nan() {
                p.scores.relative_occurrence = Some(weight);
            }
        }

        // AlignmentPosition.
        let mut position = 0.0;
        if !t_tokens.is_empty() {
            let source_position = 1.0 + s_tokens[0].position as f64;
            let target_position = 1.0 + t_tokens[0].position as f64;
            let source_relative = source_position / s_tokens[0].sentence_token_len as f64;
            let target_relative = target_position / t_tokens[0].sentence_token_len as f64;
            position = 1.0 - (source_relative - target_relative).abs();
        }
        if let Some(relative) = p.scores.relative_occurrence {
            position *= relative;
        }
        p.scores.alignment_position = position;

        // PhrasePlausibility, against the empty corpus.
        p.scores.phrase_plausibility = if t_tokens.is_empty() { 1.0 } else { 0.0 };
        p.scores.lemma_phrase_plausibility =
            if s.keys.lemma_key.is_some() && t_keys.lemma_key.is_some() {
                p.scores.phrase_plausibility
            } else {
                0.0
            };

        // NgramLength and CharacterLength.
        if t_tokens.is_empty() {
            p.scores.ngram_length = 0.0;
            p.scores.character_length = 0.0;
        } else {
            let primary = s_tokens.len() as f64 / s_tokens[0].sentence_token_len as f64;
            let secondary = t_tokens.len() as f64 / t_tokens[0].sentence_token_len as f64;
            p.scores.ngram_length = (1.0 - (primary - secondary).abs()).powf(5.0);
            let chars =
                |tokens: &[Token]| tokens.iter().map(|t| utf16_len(&t.text)).sum::<usize>() as f64;
            let primary = chars(s_tokens) / s_tokens[0].sentence_char_len as f64;
            let secondary = chars(t_tokens) / t_tokens[0].sentence_char_len as f64;
            p.scores.character_length = (1.0 - (primary - secondary).abs()).powf(5.0);
        }

        // AlignmentOccurrences, against the sentence index.
        p.scores.alignment_occurrences = similarity(
            sentence_sources.read(&s.keys.key),
            sentence_targets.read(&t_keys.key),
        );
        p.scores.lemma_alignment_occurrences = match (&s.keys.lemma_key, &t_keys.lemma_key) {
            (Some(sl), Some(tl)) => {
                similarity(sentence_sources.read(sl), sentence_targets.read(tl))
            }
            _ => 0.0,
        };

        // Uniqueness, against the empty corpus.
        p.scores.uniqueness = 0.0;
        p.scores.lemma_uniqueness = 0.0;
    }

    Run {
        source,
        target,
        source_ngrams,
        target_ngrams,
        predictions,
    }
}

/// wordMAP's `Engine.calculateWeightedConfidence` over its eleven score keys, with the
/// four corpus or memory ratios in front. The sums run in wordMAP's key order.
fn weighted(ratios: [f64; 4], s: &Scores) -> f64 {
    let terms = [
        (ratios[0], 0.7),
        (ratios[1], 0.7),
        (ratios[2], 0.7),
        (ratios[3], 0.7),
        (s.alignment_position, 0.7),
        (s.ngram_length, 1.0),
        (s.character_length, 0.3),
        (s.alignment_occurrences, 0.4),
        (s.lemma_alignment_occurrences, 0.4),
        (s.uniqueness, 0.5),
        (s.lemma_uniqueness, 0.5),
    ];
    let mut score_sum = 0.0;
    let mut weight_sum = 0.0;
    for (score, weight) in terms {
        score_sum += score * weight;
        weight_sum += weight;
    }
    score_sum / weight_sum
}

/// wordMAP's `Engine.calculateConfidence`: a candidate found in the alignment memory (by
/// key, else by lemma key) scores 1 plus its memory-weighted confidence; any other scores
/// its corpus-weighted confidence times its phrase plausibility.
pub(crate) fn score(memory: &Memory, predictions: &mut [Prediction]) {
    for p in predictions {
        let mut in_memory = memory.alignments.read(&p.key) != 0.0;
        if !in_memory && let Some(lemma) = &p.lemma_key {
            in_memory = memory.alignments.read(lemma) != 0.0;
        }
        let s = &p.scores;
        let confidence = if in_memory {
            weighted(
                [
                    s.source_memory_ratio,
                    s.source_memory_lemma_ratio,
                    s.target_memory_ratio,
                    s.target_memory_lemma_ratio,
                ],
                s,
            ) + 1.0
        } else {
            weighted([0.0; 4], s) * s.phrase_plausibility
        };
        p.scores.confidence = confidence;
    }
}

/// wordMAP's `Engine.sortPredictions` and `Engine.suggest` for one suggestion: the
/// candidates by confidence, highest first and otherwise in their order, taken greedily
/// while they share no token with one already taken; the result in source order.
pub(crate) fn suggest(run: &Run<'_>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..run.predictions.len()).collect();
    // JavaScript's comparator finds -0 and 0 equal, and confidences are finite; a NaN
    // would sort last rather than break the order.
    let key = |i: usize| {
        let c = run.predictions[i].scores.confidence;
        if c.is_nan() {
            f64::NEG_INFINITY
        } else {
            c + 0.0
        }
    };
    order.sort_by(|&a, &b| key(b).total_cmp(&key(a)));
    let mut source_used = vec![false; run.source.len()];
    let mut target_used = vec![false; run.target.len()];
    let mut chosen = Vec::new();
    for i in order {
        let p = &run.predictions[i];
        let source = run.source_tokens(p);
        let target = run.target_tokens(p);
        let clash = source.iter().any(|t| source_used[t.position])
            || target.iter().any(|t| target_used[t.position]);
        if clash {
            continue;
        }
        for t in source {
            source_used[t.position] = true;
        }
        for t in target {
            target_used[t.position] = true;
        }
        chosen.push(i);
    }
    chosen.sort_by_key(|&i| run.source_tokens(&run.predictions[i])[0].position);
    chosen
}
