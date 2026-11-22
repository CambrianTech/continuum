//! A corpus's themes in one pass (card 1d9a3512). A gene's subspaces are its largest themes: groups of
//! examples whose embeddings sit together. They were found by a pairwise similarity matrix over the
//! whole corpus, quadratic in it: on the 5090 (Codex, 2026-11-22) Kimi's 26,415-example bucket took
//! 27.3 s per signature, synchronously, at every seat. Here each embedding meets only the candidate
//! themes, a bounded set, so the pass is linear in the corpus.
//!
//! The bound is a heavy-hitter sketch (SpaceSaving): a new embedding near no candidate takes the
//! smallest candidate's place and inherits its count as error, so any theme holding more than
//! `n / CANDIDATES` of the corpus is never lost, and a candidate's true size is at least
//! `members - error`. A theme is admitted only when its own members beat chance: of `n` unrelated
//! examples, the space's significance owner says `n * chance_above(SIGNIFICANCE_SIGMA)` land above the
//! floor by luck, and a theme must exceed that count by the same σ (and hold more than one example).
//! Only the [`super::MAX_SUBSPACES`] largest admitted themes become subspaces.

/// How many themes the pass follows at once. A theme over 1/64 of the corpus is kept (SpaceSaving's
/// guarantee); a subspace is one of the four largest, far above that share in any corpus with themes.
const CANDIDATES: usize = 64;

struct Theme {
    sum: Vec<f32>,
    /// `sum`, L2-normalized: the theme's direction, compared by dot product (every input is unit).
    unit: Vec<f32>,
    members: usize,
    /// Members counted from an evicted theme whose place this one took: never its own.
    error: usize,
}

impl Theme {
    fn seeded(e: &[f32], error: usize) -> Self {
        Self { sum: e.to_vec(), unit: e.to_vec(), members: error + 1, error }
    }

    fn join(&mut self, e: &[f32]) {
        for (s, x) in self.sum.iter_mut().zip(e) {
            *s += x;
        }
        self.unit.clone_from(&self.sum);
        super::normalize(&mut self.unit);
        self.members += 1;
    }
}

use crate::ai::embedding::vector_math::unit_cosine as dot;

/// The count a theme's own members must exceed to be more than chance: of `n` unrelated examples,
/// `n p` land above the significance floor by luck (`p` = the normal tail at the bar), and a theme must
/// beat that by the bar's σ in the count's own spread.
fn beyond_chance(n: usize) -> f32 {
    use crate::ai::embedding::significance::{chance_above, SIGNIFICANCE_SIGMA};
    let p = chance_above(SIGNIFICANCE_SIGMA);
    let expected = n as f32 * p;
    expected + SIGNIFICANCE_SIGMA * (expected * (1.0 - p)).sqrt()
}

/// The largest themes of `embeddings` (each L2-normalized): an embedding joins the nearest candidate
/// at or above `floor` (the space's significance bar), else seeds one. Themes whose own members beat
/// chance, largest first, at most `seats`, each L2-normalized.
pub(super) fn themes(embeddings: &[Vec<f32>], floor: f32, seats: usize) -> Vec<Vec<f32>> {
    let mut candidates: Vec<Theme> = Vec::with_capacity(CANDIDATES);
    for e in embeddings {
        let nearest = candidates
            .iter()
            .enumerate()
            .map(|(i, theme)| (i, dot(&theme.unit, e)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match nearest {
            Some((i, near)) if near >= floor => candidates[i].join(e),
            _ if candidates.len() < CANDIDATES => candidates.push(Theme::seeded(e, 0)),
            _ => {
                // full: the smallest candidate gives up its place, and its count becomes the newcomer's error
                if let Some((smallest, theme)) = candidates.iter().enumerate().min_by_key(|(_, theme)| theme.members) {
                    let inherited = theme.members;
                    candidates[smallest] = Theme::seeded(e, inherited);
                }
            }
        }
    }
    let bar = beyond_chance(embeddings.len());
    // a theme groups examples: one example is not a theme, whatever chance allows on a tiny corpus
    let mut kept: Vec<Theme> = candidates.into_iter().filter(|theme| theme.members - theme.error >= 2 && (theme.members - theme.error) as f32 > bar).collect();
    kept.sort_by_key(|theme| std::cmp::Reverse(theme.members - theme.error));
    kept.into_iter().take(seats).map(|theme| theme.unit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 1d9a3512): the one-pass themes disagreeing with the pairwise clustering
    // they replace on a corpus with clear themes (a subspace lost, a noise pair promoted, the order of
    // the largest wrong), or a theme lost to eviction when noise fills every candidate first.
    #[test]
    fn one_pass_themes_match_the_pairwise_clusters_and_survive_noise() {
        let dim = 96;
        // `n` unit vectors near axis `axis`, each tilted a little toward a neighbouring axis
        let around = |axis: usize, n: usize| -> Vec<Vec<f32>> {
            (0..n)
                .map(|k| {
                    let mut v = vec![0.0f32; dim];
                    v[axis] = 1.0;
                    v[(axis + 1 + k % (dim - 1)) % dim] = 0.2;
                    super::super::normalize(&mut v);
                    v
                })
                .collect()
        };
        let mut corpus = Vec::new();
        corpus.extend(around(0, 30));
        corpus.extend(around(10, 20));
        corpus.extend(around(20, 12));
        // noise: one example per axis, near nothing else, more than the candidates can hold
        for axis in 30..dim {
            let mut v = vec![0.0f32; dim];
            v[axis] = 1.0;
            corpus.push(v);
        }
        corpus.extend(around(0, 10)); // the largest theme keeps growing after the noise
        let floor = 0.55; // the bar in this synthetic space: its themes sit near 0.98, its noise near 0
        let found = themes(&corpus, floor, 4);
        let mut pairwise = crate::ai::embedding::vector_math::detect_clusters(&corpus, floor, 3);
        pairwise.sort_by_key(|c| std::cmp::Reverse(c.indices.len()));
        assert_eq!(found.len(), pairwise.len().min(4), "the same number of themes");
        for (theme, cluster) in found.iter().zip(&pairwise) {
            let mut mean = vec![0.0f32; dim];
            for &i in &cluster.indices {
                for (m, x) in mean.iter_mut().zip(&corpus[i]) {
                    *m += x;
                }
            }
            super::super::normalize(&mut mean);
            assert!(dot(theme, &mean) > 0.999, "each theme is its pairwise cluster's direction, largest first");
        }
    }
}
