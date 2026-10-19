//! A port of uw-wordmapbooster 1.0.5's `JLBoost`, as `MorphJLBoostWordMap` uses it: a
//! gradient-boosted ensemble of random trees that scores wordMAP's candidates from their
//! wordMAP scores and the morphology of their source words.
//!
//! The trees are random. tC4 draws from `Math.random`; this draws from the same
//! generator tC4's tests install (mulberry32), seeded, in the same order of draws, so a
//! given seed grows the same trees that tC4 grows under that seed, and every training is
//! reproducible. The arithmetic follows the library step by step, including its quirks:
//! a leaf under a split averages over the whole node's row count, not its own.
//!
//! One difference: when every training row has the same answer (all candidates correct
//! or all wrong), the library drops the answer column as a constant feature and trains on
//! nothing, which scores every candidate as NaN. This refuses to fit instead, and the
//! caller answers from the alignment memory alone.
//!
//! Ported from uw-wordmapbooster 1.0.5, © Joshua Lansford, ISC licence; the notice is in
//! the crate's `NOTICE` file.

use super::wordmap::{Prediction, Run, Scores, Token};
use std::collections::HashMap;

/// The generator tC4's suggestion tests install as `Math.random` (and the one
/// uw-wordmapbooster's `JLBoost.js` ships): mulberry32.
#[derive(Debug, Clone)]
pub(crate) struct Mulberry32(u32);

impl Mulberry32 {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }

    /// The next number in `[0, 1)`.
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    /// `Math.floor(Math.random() * n)`.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() * n as f64).floor() as usize
    }
}

/// The numeric features, in uw-wordmapbooster's `catboost_feature_order`.
pub(crate) const NUMERIC: usize = 23;
/// The categorical features `src_morph_0` to `src_morph_7`.
pub(crate) const CATEGORICAL: usize = 8;

/// A candidate's features, as `morph_code_prediction_to_feature_dict` makes them.
#[derive(Debug, Clone)]
pub(crate) struct Features {
    pub numeric: [f64; NUMERIC],
    pub categorical: [String; CATEGORICAL],
}

impl Features {
    pub fn of(run: &Run<'_>, p: &Prediction) -> Self {
        Self {
            numeric: numeric_of(&p.scores),
            categorical: morph_pieces(run.source_tokens(p)),
        }
    }
}

/// The numeric features of a candidate's scores.
pub(crate) fn numeric_of(s: &Scores) -> [f64; NUMERIC] {
    [
        0.0, // sourceCorpusPermutationsFrequencyRatio
        0.0, // targetCorpusPermutationsFrequencyRatio
        s.source_memory_ratio,
        s.target_memory_ratio,
        0.0, // frequencyRatioCorpusFiltered
        s.memory_filtered,
        0.0, // sourceCorpusLemmaPermutationsFrequencyRatio
        0.0, // targetCorpusLemmaPermutationsFrequencyRatio
        s.source_memory_lemma_ratio,
        s.target_memory_lemma_ratio,
        0.0, // lemmaFrequencyRatioCorpusFiltered
        s.memory_lemma_filtered,
        0.0, // ngramRelativeTokenDistance: no algorithm sets it
        s.relative_occurrence.unwrap_or(0.0),
        s.alignment_position,
        s.phrase_plausibility,
        s.lemma_phrase_plausibility,
        s.ngram_length,
        s.character_length,
        s.alignment_occurrences,
        s.lemma_alignment_occurrences,
        s.uniqueness,
        s.lemma_uniqueness,
    ]
}

/// The categorical features of source words: the morphology pieces `src_morph_0` to
/// `src_morph_7`, each the pieces of that position joined over the words.
fn morph_pieces(tokens: &[Token]) -> [String; CATEGORICAL] {
    let mut categorical: [String; CATEGORICAL] = Default::default();
    for token in tokens {
        for (piece, slot) in token.morph.split(',').zip(categorical.iter_mut()) {
            slot.push_str(piece);
        }
    }
    categorical
}

/// Why a booster could not be fitted. tC4 then answers from the alignment memory alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoFit {
    /// No candidate to learn from.
    NoRows,
    /// Every candidate has the same answer.
    ConstantAnswer,
    /// A node had no feature left to split on and no rows to grow from (the library
    /// throws a TypeError here).
    NothingToSplit,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Leaf(f64),
    Numeric {
        feature: usize,
        split: f64,
        left: Box<Node>,
        right: Box<Node>,
    },
    Categorical {
        feature: usize,
        split: u32,
        left: Box<Node>,
        right: Box<Node>,
    },
}

/// The value JLBoost compares: a NaN reads as -1.
fn numeric_value(v: f64) -> f64 {
    if v.is_nan() { -1.0 } else { v }
}

impl Node {
    fn predict(&self, numeric: &[f64; NUMERIC], categorical: &[u32; CATEGORICAL]) -> f64 {
        match self {
            Node::Leaf(v) => *v,
            Node::Numeric {
                feature,
                split,
                left,
                right,
            } => {
                if numeric_value(numeric[*feature]) <= *split {
                    left.predict(numeric, categorical)
                } else {
                    right.predict(numeric, categorical)
                }
            }
            Node::Categorical {
                feature,
                split,
                left,
                right,
            } => {
                if categorical[*feature] == *split {
                    left.predict(numeric, categorical)
                } else {
                    right.predict(numeric, categorical)
                }
            }
        }
    }
}

/// A fitted booster: its trees and the categorical values it has seen.
#[derive(Debug, Clone, PartialEq)]
pub struct Booster {
    trees: Vec<Node>,
    learning_rate: f64,
    values: HashMap<String, u32>,
    /// The trees laid out in one array, which is what scoring walks.
    compiled: Compiled,
}

/// A value the booster never saw: equal to no split.
const UNSEEN: u32 = u32::MAX;

/// What a node of the laid-out trees does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Holds a value, and its branches are itself, so a walk that goes on stays.
    Leaf,
    /// Goes left when the feature is at most `split`.
    AtMost,
    /// Goes left when the feature is exactly `split` (a category's id).
    Is,
}

/// One node of the laid-out trees. A leaf holds its value in `split`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Flat {
    kind: Kind,
    /// An index into a [`Row`].
    feature: u32,
    split: f64,
    left: u32,
    right: u32,
}

/// What the trees read of a candidate: the numeric features, a NaN read as -1, then the
/// ids of the categories as numbers, so a node is one comparison of one value.
type Row = [f64; NUMERIC + CATEGORICAL];

/// How many trees are walked together. The walk of one tree is a chain of dependent loads;
/// eight chains in step keep the processor busy while each waits.
const LANES: usize = 8;

/// Every tree in one array, the nodes of a tree after its root in the order a walk meets
/// them, so scoring a candidate follows indices through one allocation and not a chain of
/// boxes.
#[derive(Debug, Clone, PartialEq)]
struct Compiled {
    nodes: Vec<Flat>,
    roots: Vec<u32>,
    /// The steps that take a walk from a root to a leaf of the deepest tree.
    depth: usize,
}

impl Compiled {
    fn of(trees: &[Node]) -> Self {
        fn push(nodes: &mut Vec<Flat>, node: &Node, depth: usize, deepest: &mut usize) -> u32 {
            *deepest = (*deepest).max(depth);
            let at = nodes.len() as u32;
            nodes.push(Flat {
                kind: Kind::Leaf,
                feature: 0,
                split: 0.0,
                left: at,
                right: at,
            });
            let flat = match node {
                Node::Leaf(v) => Flat {
                    kind: Kind::Leaf,
                    feature: 0,
                    split: *v,
                    left: at,
                    right: at,
                },
                Node::Numeric {
                    feature,
                    split,
                    left,
                    right,
                } => Flat {
                    kind: Kind::AtMost,
                    feature: *feature as u32,
                    split: *split,
                    left: push(nodes, left, depth + 1, deepest),
                    right: push(nodes, right, depth + 1, deepest),
                },
                Node::Categorical {
                    feature,
                    split,
                    left,
                    right,
                } => Flat {
                    kind: Kind::Is,
                    feature: (NUMERIC + *feature) as u32,
                    split: f64::from(*split),
                    left: push(nodes, left, depth + 1, deepest),
                    right: push(nodes, right, depth + 1, deepest),
                },
            };
            nodes[at as usize] = flat;
            at
        }
        let mut nodes = Vec::new();
        let mut depth = 0;
        let roots = trees
            .iter()
            .map(|t| push(&mut nodes, t, 0, &mut depth))
            .collect();
        Self {
            nodes,
            roots,
            depth,
        }
    }

    /// The walk of one node: where it goes from `at` on `row`.
    #[inline(always)]
    fn step(&self, at: u32, row: &Row) -> u32 {
        let node = &self.nodes[at as usize];
        let value = row[node.feature as usize];
        let left = match node.kind {
            Kind::Leaf => true,
            Kind::AtMost => value <= node.split,
            Kind::Is => value == node.split,
        };
        if left { node.left } else { node.right }
    }

    /// The trees' outputs added in the order of the trees.
    fn sum(&self, row: &Row) -> f64 {
        let mut output = 0.0;
        for group in self.roots.chunks(LANES) {
            let mut at = [group[0]; LANES];
            at[..group.len()].copy_from_slice(group);
            for _ in 0..self.depth {
                for lane in &mut at {
                    *lane = self.step(*lane, row);
                }
            }
            for lane in &at[..group.len()] {
                output += self.nodes[*lane as usize].split;
            }
        }
        output
    }
}

impl Booster {
    fn new(trees: Vec<Node>, learning_rate: f64, values: HashMap<String, u32>) -> Self {
        let compiled = Compiled::of(&trees);
        Self {
            trees,
            learning_rate,
            values,
            compiled,
        }
    }

    /// The ids of the categorical values the booster has seen; a value it has not is equal
    /// to no split.
    pub(crate) fn categorical_of(&self, pieces: &[String; CATEGORICAL]) -> [u32; CATEGORICAL] {
        pieces
            .each_ref()
            .map(|v| self.values.get(v).copied().unwrap_or(UNSEEN))
    }

    /// [`Booster::categorical_of`] for the morphology of source words: it depends on the
    /// source n-gram alone, so a run reads it once for each n-gram, not for each candidate.
    pub(crate) fn categorical_of_tokens(&self, tokens: &[Token]) -> [u32; CATEGORICAL] {
        self.categorical_of(&morph_pieces(tokens))
    }

    /// The trees' outputs summed in order, times the learning rate.
    pub(crate) fn score_row(
        &self,
        numeric: &[f64; NUMERIC],
        categorical: &[u32; CATEGORICAL],
    ) -> f64 {
        let mut row: Row = [0.0; NUMERIC + CATEGORICAL];
        for (slot, v) in row.iter_mut().zip(numeric) {
            *slot = numeric_value(*v);
        }
        for (slot, id) in row[NUMERIC..].iter_mut().zip(categorical) {
            *slot = f64::from(*id);
        }
        self.compiled.sum(&row) * self.learning_rate
    }

    /// The number of trees accepted by training.
    pub fn trees(&self) -> usize {
        self.trees.len()
    }
}

/// Training rows: features by row, categorical values interned.
struct Rows {
    numeric: Vec<[f64; NUMERIC]>,
    categorical: Vec<[u32; CATEGORICAL]>,
    answer: Vec<f64>,
}

/// A feature index: below [`NUMERIC`] a numeric feature, else a categorical one.
type Feature = usize;

struct Grower<'a> {
    rows: &'a Rows,
    features: Vec<Feature>,
    /// The answers the next tree fits: the residuals of the trees so far.
    target: Vec<f64>,
    rng: &'a mut Mulberry32,
}

struct Grown {
    node: Node,
    /// The feature the top node chose, which the next tree may not start with.
    feature: Option<Feature>,
}

impl Grower<'_> {
    fn value(&self, row: usize, feature: Feature) -> f64 {
        numeric_value(self.rows.numeric[row][feature])
    }

    fn average(&self, rows: &[usize], over: usize) -> f64 {
        rows.iter().fold(0.0, |sum, &r| sum + self.target[r]) / over as f64
    }

    /// JLBoost's `TreeBranch.random_tree`.
    fn grow(
        &mut self,
        rows: &[usize],
        levels: u32,
        ignored: Option<Feature>,
    ) -> Result<Grown, NoFit> {
        if rows.is_empty() {
            return Err(NoFit::NothingToSplit);
        }
        let categories: Vec<Feature> = self
            .features
            .iter()
            .copied()
            .filter(|f| Some(*f) != ignored)
            .collect();
        let length = rows.len();
        let mut sorted: Vec<usize> = Vec::new();
        let mut first = 0;
        let mut chosen = None;
        let mut split = None;
        if !categories.is_empty() {
            let feature = categories[self.rng.below(categories.len())];
            chosen = Some(feature);
            if feature >= NUMERIC {
                let c = feature - NUMERIC;
                let mut options: Vec<u32> = Vec::new();
                for &r in rows {
                    let v = self.rows.categorical[r][c];
                    if !options.contains(&v) {
                        options.push(v);
                    }
                }
                let value = options[self.rng.below(options.len())];
                let (haves, have_nots): (Vec<usize>, Vec<usize>) = rows
                    .iter()
                    .partition(|&&r| self.rows.categorical[r][c] == value);
                first = haves.len();
                sorted = haves;
                sorted.extend(have_nots);
                split = Some(SplitAt::Categorical(c, value));
            } else {
                first = self.rng.below(length).max(1).min(length - 1);
                sorted = rows.to_vec();
                sorted.sort_by(|&a, &b| {
                    self.value(a, feature)
                        .partial_cmp(&self.value(b, feature))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                if first == 0 {
                    // The library reads the row before the first and throws.
                    return Err(NoFit::NothingToSplit);
                }
                let at = 0.5
                    * (self.value(sorted[first - 1], feature) + self.value(sorted[first], feature));
                split = Some(SplitAt::Numeric(feature, at));
            }
        }
        if first == 0 || first == length {
            let node = if levels > 1 {
                self.grow(&sorted, levels - 1, None)?.node
            } else {
                Node::Leaf(self.average(&sorted, length))
            };
            return Ok(Grown {
                node,
                feature: chosen,
            });
        }
        let (left, right) = if levels > 1 {
            let left = if first > 1 {
                self.grow(&sorted[..first], levels - 1, None)?.node
            } else {
                Node::Leaf(self.average(&sorted[..first], length))
            };
            let right = if length - first > 1 {
                self.grow(&sorted[first..], levels - 1, None)?.node
            } else {
                Node::Leaf(self.average(&sorted[first..], length))
            };
            (left, right)
        } else {
            (
                Node::Leaf(self.average(&sorted[..first], length)),
                Node::Leaf(self.average(&sorted[first..], length)),
            )
        };
        let (left, right) = (Box::new(left), Box::new(right));
        let node = match split {
            Some(SplitAt::Numeric(feature, split)) => Node::Numeric {
                feature,
                split,
                left,
                right,
            },
            Some(SplitAt::Categorical(feature, split)) => Node::Categorical {
                feature,
                split,
                left,
                right,
            },
            // A split row exists only when a feature was chosen; without one the node is
            // degenerate and returned above.
            None => return Err(NoFit::NothingToSplit),
        };
        Ok(Grown {
            node,
            feature: chosen,
        })
    }
}

enum SplitAt {
    Numeric(Feature, f64),
    Categorical(usize, u32),
}

/// JLBoost's `train`: `steps` random trees of `depth` levels, each kept only if it
/// lowers the standard deviation of the error. `correct` and `incorrect` are the
/// candidates `collect_boost_training_data` kept, which become rows answering 1 and 0.
pub(crate) fn fit(
    correct: Vec<Features>,
    incorrect: Vec<Features>,
    steps: usize,
    depth: u32,
    learning_rate: f64,
    rng: &mut Mulberry32,
) -> Result<Booster, NoFit> {
    let mut values: HashMap<String, u32> = HashMap::new();
    let mut rows = Rows {
        numeric: Vec::new(),
        categorical: Vec::new(),
        answer: Vec::new(),
    };
    let n_correct = correct.len();
    for (i, features) in correct.into_iter().chain(incorrect).enumerate() {
        let categorical = features.categorical.map(|v| {
            let next = values.len() as u32;
            *values.entry(v).or_insert(next)
        });
        rows.numeric.push(features.numeric);
        rows.categorical.push(categorical);
        rows.answer.push(if i < n_correct { 1.0 } else { 0.0 });
    }
    let n = rows.answer.len();
    if n == 0 {
        return Err(NoFit::NoRows);
    }

    // Drop every feature that has one value across the rows, the answer included.
    let constant = |values: &mut dyn Iterator<Item = f64>| {
        let (mut max, mut min) = (-9_007_199_254_740_991.0_f64, 9_007_199_254_740_991.0_f64);
        for v in values {
            if v > max {
                max = v;
            }
            if v < min {
                min = v;
            }
        }
        max == min
    };
    let mut features = Vec::new();
    for f in 0..NUMERIC {
        if !constant(&mut rows.numeric.iter().map(|r| r[f])) {
            features.push(f);
        }
    }
    for c in 0..CATEGORICAL {
        let first = rows.categorical[0][c];
        if rows.categorical.iter().any(|r| r[c] != first) {
            features.push(NUMERIC + c);
        }
    }
    if constant(&mut rows.answer.iter().copied()) {
        return Err(NoFit::ConstantAnswer);
    }

    let all: Vec<usize> = (0..n).collect();
    let mut current = vec![0.0; n];
    let mut trees = Vec::new();
    let mut ignored: Option<Feature> = None;
    let mut last_loss: Option<f64> = None;
    for _ in 0..steps {
        let target: Vec<f64> = rows
            .answer
            .iter()
            .zip(&current)
            .map(|(y, c)| y - c)
            .collect();
        let mut grower = Grower {
            rows: &rows,
            features: features.clone(),
            target,
            rng: &mut *rng,
        };
        let grown = grower.grow(&all, depth, ignored)?;
        let new_output: Vec<f64> = (0..n)
            .map(|i| {
                current[i]
                    + grown.node.predict(&rows.numeric[i], &rows.categorical[i]) * learning_rate
            })
            .collect();
        let error: Vec<f64> = new_output
            .iter()
            .zip(&rows.answer)
            .map(|(o, y)| o - y)
            .collect();
        let mean = error.iter().fold(0.0, |s, e| s + e) / n as f64;
        let spread = error.iter().fold(0.0, |s, e| {
            let d = e - mean;
            s + d * d
        });
        let loss = (spread / n as f64).sqrt();
        ignored = grown.feature;
        if last_loss.is_none_or(|last| loss < last) {
            trees.push(grown.node);
            last_loss = Some(loss);
            current = new_output;
        }
    }
    Ok(Booster::new(trees, learning_rate, values))
}
