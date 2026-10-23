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

use super::wordmap::{Prediction, Run, Scores};
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
        let s: &Scores = &p.scores;
        let numeric = [
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
        ];
        let mut categorical: [String; CATEGORICAL] = Default::default();
        for token in run.source_tokens(p) {
            for (piece, slot) in token.morph.split(',').zip(categorical.iter_mut()) {
                slot.push_str(piece);
            }
        }
        Self {
            numeric,
            categorical,
        }
    }
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

/// One row of a prediction batch; all tree comparisons use the same normalized values.
struct PredictRow {
    numeric: [f64; NUMERIC],
    categorical: [u32; CATEGORICAL],
    output: usize,
}

/// A split outside the batch's range sends every row down the same branch. Morphology
/// is usually constant here: each source n-gram is paired with many target n-grams.
struct PredictBounds {
    numeric: [[f64; 2]; NUMERIC],
    categorical: [Option<u32>; CATEGORICAL],
}

impl PredictBounds {
    fn of(rows: &[PredictRow]) -> Self {
        let mut bounds = Self {
            numeric: [[f64::INFINITY, f64::NEG_INFINITY]; NUMERIC],
            categorical: rows
                .first()
                .map_or([None; CATEGORICAL], |row| row.categorical.map(Some)),
        };
        for row in rows {
            for (range, value) in bounds.numeric.iter_mut().zip(row.numeric) {
                range[0] = range[0].min(value);
                range[1] = range[1].max(value);
            }
            for (constant, value) in bounds.categorical.iter_mut().zip(row.categorical) {
                if *constant != Some(value) {
                    *constant = None;
                }
            }
        }
        bounds
    }
}

/// Partitions row indices in place without moving the feature arrays.
fn partition(rows: &mut [usize], mut left: impl FnMut(usize) -> bool) -> usize {
    let mut boundary = 0;
    for at in 0..rows.len() {
        if left(rows[at]) {
            rows.swap(boundary, at);
            boundary += 1;
        }
    }
    boundary
}

/// Preorder nodes keep a fitted tree contiguous. A branch's left child is the next
/// node; its right child follows the complete left subtree.
#[derive(Debug, Clone, Copy, PartialEq)]
enum CompiledNode {
    Leaf(f64),
    Numeric {
        feature: usize,
        split: f64,
        right: usize,
    },
    Categorical {
        feature: usize,
        split: u32,
        right: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct CompiledTree {
    nodes: Vec<CompiledNode>,
}

impl CompiledTree {
    fn of(tree: &Node) -> Self {
        fn append(tree: &Node, nodes: &mut Vec<CompiledNode>) {
            let at = nodes.len();
            nodes.push(CompiledNode::Leaf(0.0));
            nodes[at] = match tree {
                Node::Leaf(value) => CompiledNode::Leaf(*value),
                Node::Numeric {
                    feature,
                    split,
                    left,
                    right,
                } => {
                    append(left, nodes);
                    let next = nodes.len();
                    append(right, nodes);
                    CompiledNode::Numeric {
                        feature: *feature,
                        split: *split,
                        right: next,
                    }
                }
                Node::Categorical {
                    feature,
                    split,
                    left,
                    right,
                } => {
                    append(left, nodes);
                    let next = nodes.len();
                    append(right, nodes);
                    CompiledNode::Categorical {
                        feature: *feature,
                        split: *split,
                        right: next,
                    }
                }
            };
        }
        let mut nodes = Vec::new();
        append(tree, &mut nodes);
        Self { nodes }
    }

    fn accumulate(
        &self,
        at: usize,
        rows: &[PredictRow],
        bounds: &PredictBounds,
        order: &mut [usize],
        output: &mut [f64],
    ) {
        if order.is_empty() {
            return;
        }
        match self.nodes[at] {
            CompiledNode::Leaf(value) => {
                // No partition changed a complete batch's membership. With no refused
                // candidates its outputs are contiguous and need no indirect indexing.
                if order.len() == rows.len() {
                    let first = rows[0].output;
                    let end = rows[rows.len() - 1].output + 1;
                    if end - first == rows.len() {
                        for score in &mut output[first..end] {
                            *score += value;
                        }
                        return;
                    }
                }
                for &row in order.iter() {
                    output[rows[row].output] += value;
                }
            }
            CompiledNode::Numeric {
                feature,
                split,
                right,
            } => {
                if bounds.numeric[feature][1] <= split {
                    return self.accumulate(at + 1, rows, bounds, order, output);
                }
                if bounds.numeric[feature][0] > split {
                    return self.accumulate(right, rows, bounds, order, output);
                }
                let boundary = partition(order, |row| rows[row].numeric[feature] <= split);
                let (yes, no) = order.split_at_mut(boundary);
                self.accumulate(at + 1, rows, bounds, yes, output);
                self.accumulate(right, rows, bounds, no, output);
            }
            CompiledNode::Categorical {
                feature,
                split,
                right,
            } => {
                if let Some(value) = bounds.categorical[feature] {
                    return self.accumulate(
                        if value == split { at + 1 } else { right },
                        rows,
                        bounds,
                        order,
                        output,
                    );
                }
                let boundary = partition(order, |row| rows[row].categorical[feature] == split);
                let (yes, no) = order.split_at_mut(boundary);
                self.accumulate(at + 1, rows, bounds, yes, output);
                self.accumulate(right, rows, bounds, no, output);
            }
        }
    }
}

/// A fitted booster: its trees and the categorical values it has seen.
#[derive(Debug, Clone, PartialEq)]
pub struct Booster {
    trees: Vec<CompiledTree>,
    learning_rate: f64,
    values: HashMap<String, u32>,
}

/// A value the booster never saw: equal to no split.
const UNSEEN: u32 = u32::MAX;

impl Booster {
    /// Each tree visits a small batch while its nodes and the feature rows stay hot. A
    /// row receives exactly one leaf per tree, in the original tree order: batching does
    /// not reorder its floating-point sum. `None` is a candidate refused by placed links.
    pub(crate) fn scores(&self, features: impl IntoIterator<Item = Option<Features>>) -> Vec<f64> {
        const BATCH: usize = 64;
        let mut features = features.into_iter();
        let mut output = Vec::with_capacity(features.size_hint().0);
        let mut rows = Vec::with_capacity(BATCH);
        let mut order = Vec::with_capacity(BATCH);
        loop {
            rows.clear();
            let first = output.len();
            for features in features.by_ref().take(BATCH) {
                let at = output.len();
                output.push(0.0);
                if let Some(features) = features {
                    rows.push(PredictRow {
                        numeric: features.numeric.map(numeric_value),
                        categorical: features
                            .categorical
                            .each_ref()
                            .map(|v| self.values.get(v).copied().unwrap_or(UNSEEN)),
                        output: at,
                    });
                }
            }
            if first == output.len() {
                return output;
            }
            let bounds = PredictBounds::of(&rows);
            for tree in &self.trees {
                order.clear();
                order.extend(0..rows.len());
                tree.accumulate(0, &rows, &bounds, &mut order, &mut output);
            }
            for row in &rows {
                output[row.output] *= self.learning_rate;
            }
        }
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
    Ok(Booster {
        trees: trees.iter().map(CompiledTree::of).collect(),
        learning_rate,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batching_keeps_every_confidence_bit_across_splits_and_chunk_boundaries() {
        let trees = vec![
            Node::Leaf(1e16),
            Node::Numeric {
                feature: 14,
                split: 0.5,
                left: Box::new(Node::Categorical {
                    feature: 0,
                    split: 0,
                    left: Box::new(Node::Leaf(-1e16)),
                    right: Box::new(Node::Leaf(1.0)),
                }),
                right: Box::new(Node::Leaf(3.0)),
            },
            Node::Numeric {
                feature: 13,
                split: -0.0,
                left: Box::new(Node::Leaf(-0.25)),
                right: Box::new(Node::Leaf(0.125)),
            },
            Node::Categorical {
                feature: 7,
                split: 1,
                left: Box::new(Node::Leaf(0.0)),
                right: Box::new(Node::Leaf(-1e16)),
            },
        ];
        let booster = Booster {
            trees: trees.iter().map(CompiledTree::of).collect(),
            learning_rate: 0.7,
            values: HashMap::from([("known".into(), 0), ("other".into(), 1)]),
        };
        for omitted in [false, true] {
            for count in [0, 1, 63, 64, 65, 129] {
                let features: Vec<_> = (0..count)
                    .map(|at| {
                        if omitted && at % 7 == 0 {
                            return None;
                        }
                        let mut row = Features {
                            numeric: [0.0; NUMERIC],
                            categorical: Default::default(),
                        };
                        row.numeric[13] =
                            [f64::NAN, f64::NEG_INFINITY, -0.0, f64::INFINITY][at % 4];
                        row.numeric[14] = [-1.0, 0.5, 0.75][at % 3];
                        row.categorical[0] = ["known", "unseen", "other"][at % 3].into();
                        row.categorical[7] = ["known", "other"][at % 2].into();
                        Some(row)
                    })
                    .collect();
                let expected: Vec<_> = features
                    .iter()
                    .map(|row| {
                        row.as_ref()
                            .map_or(0.0, |row| {
                                let categorical = row.categorical.each_ref().map(|value| {
                                    booster.values.get(value).copied().unwrap_or(UNSEEN)
                                });
                                let mut score = 0.0;
                                for tree in &trees {
                                    score += tree.predict(&row.numeric, &categorical);
                                }
                                score * booster.learning_rate
                            })
                            .to_bits()
                    })
                    .collect();
                let actual: Vec<_> = booster
                    .scores(features)
                    .into_iter()
                    .map(f64::to_bits)
                    .collect();
                assert_eq!(actual, expected, "{count} rows, omitted={omitted}");
            }
        }
    }
}
