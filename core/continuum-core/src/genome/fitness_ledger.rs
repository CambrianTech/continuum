//! `GeneFitnessIndex` — the resolver's FITNESS term, folded from the receipts
//! that already exist. READ, never parallel-store: `cognition/eval` has been
//! appending per-gene A/B rows to `~/.continuum/progress/<persona>.jsonl` since
//! the A/B lane landed (`geneId`, `lift`, `passRate`, `capturedAtMs`,
//! `meanDecodeTokensPerSecond` — 14 gene-labeled rows on disk the day this
//! module was written), and the L3 sentinel's adoption gate reads the same
//! `lift`. This module folds those rows into per-gene fitness the candidate
//! sources feed the ranking engine as `outcome_history_factor`.
//!
//! # The factor's shape (each decision deliberate)
//!
//! - **Neutral center 0.5** — the factor is 0..1; `lift` is a pass-rate DELTA
//!   in −1..1, so it maps around neutral: unmeasured = 0.5, positive lift
//!   above, negative below. (0.0 stays reserved for "no index was consulted",
//!   the sources' pre-index hardcode — 0.0-vs-0.5 distinguishes "unknown
//!   machinery" from "known machinery, unmeasured gene".)
//! - **Receipt-age decay** — an old triumph outranks nothing: each row's lift
//!   is weighted by exp(−age/half-life) before averaging, so fitness follows
//!   CURRENT worth ([[unknown-is-not-a-quantity]] applied to time).
//! - **UCB exploration bonus** — the diversity-retention obligation from the
//!   resolver spec (GENOME-REPOSITORY-ON-HF.md §2b): few trials → wide
//!   confidence → a small optimism bonus, `UCB_C·sqrt(ln(N+1)/n)`, so young
//!   forks get auditions and the commons never collapses to monoculture. This
//!   is the crate's first true UCB term; the house bandits
//!   (`expert-pager-policy`) use prior→EMA — same doctrine (optimism until
//!   measured), different horizon (they re-measure every serve; genes are
//!   measured per adoption eval).
//! - **The UCB rides INSIDE the factor**, not inside `recall_scoring::score` —
//!   the shared scorer is tested, weight-validated code shared by every
//!   source; exploration is a property of the FITNESS evidence, so it belongs
//!   with the evidence. Deliberate, per the slice-3 shape decision.

use std::collections::BTreeMap;
use std::path::Path;

/// Receipt half-life for the age decay. A month-old benchmark delta is weak
/// evidence about a gene serving TODAY (bases upgrade, corpora grow).
const RECEIPT_HALF_LIFE_MS: f64 = 30.0 * 24.0 * 3600.0 * 1000.0;

/// Exploration coefficient. Small on purpose: an audition nudges a tie, it
/// never outvotes a measured verdict.
const UCB_C: f64 = 0.1;

/// The factor for a gene the index knows machinery-wise but has no receipts
/// for: NEUTRAL. Auditions start from neutrality, never dominance.
const UNMEASURED_NEUTRAL: f32 = 0.5;

/// Folded fitness for one gene (keyed by gene NAME — the `geneId` the eval
/// ledger writes, which is the adapter alias / trait_kind the whole page-in
/// chain speaks).
#[derive(Debug, Clone, PartialEq)]
pub struct GeneFitnessRecord {
    /// Gene-labeled eval rows seen (the UCB `n`).
    pub trials: u32,
    /// Age-decayed mean lift (pass-rate delta, −1..1).
    pub decayed_mean_lift: f64,
    /// Newest receipt's timestamp.
    pub latest_ms: u64,
    /// Mean decode tok/s across this gene's rows — the SPEED term's local
    /// evidence, carried for the wiring slice (not yet consumed by scoring).
    pub mean_decode_tps: f64,
}

/// Per-gene fitness folded from the progress ledgers. Pure over parsed rows —
/// the fold is testable without a filesystem; [`GeneFitnessIndex::load`] is
/// the thin I/O wrapper.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GeneFitnessIndex {
    by_gene: BTreeMap<String, GeneFitnessRecord>,
    /// Total gene-labeled rows across ALL genes (the UCB `N`).
    total_trials: u32,
}

impl GeneFitnessIndex {
    /// Fold ledger rows (any personas', any order). Rows without a `geneId`
    /// are the A/B BASE lane and carry no gene evidence — skipped. Malformed
    /// rows are skipped silently: the ledger is append-only telemetry, and a
    /// torn tail line must never zero the whole index.
    pub fn fold_rows<'a>(rows: impl Iterator<Item = &'a serde_json::Value>, now_ms: u64) -> Self {
        let mut acc: BTreeMap<String, (f64, f64, u32, u64, f64)> = BTreeMap::new();
        // (weighted_lift_sum, weight_sum, trials, latest_ms, tps_sum)
        let mut total = 0u32;
        for row in rows {
            let Some(gene) = row.get("geneId").and_then(|v| v.as_str()) else {
                continue;
            };
            total += 1;
            let ts = row.get("capturedAtMs").and_then(|v| v.as_u64()).unwrap_or(0); // undated receipt: decays as maximally old rather than being dropped
            let age = now_ms.saturating_sub(ts) as f64;
            let weight = (-age / RECEIPT_HALF_LIFE_MS * std::f64::consts::LN_2).exp();
            let lift = row.get("lift").and_then(|v| v.as_f64()).unwrap_or(0.0); // a gene row without a lift is an aborted A/B: neutral evidence, still a trial
            let tps = row
                .get("meanDecodeTokensPerSecond")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0); // absent tps: contributes 0 to the mean rather than poisoning it
            let e = acc.entry(gene.to_string()).or_insert((0.0, 0.0, 0, 0, 0.0));
            e.0 += lift * weight;
            e.1 += weight;
            e.2 += 1;
            e.3 = e.3.max(ts);
            e.4 += tps;
        }
        let by_gene = acc
            .into_iter()
            .map(|(gene, (wsum, w, trials, latest, tps_sum))| {
                (
                    gene,
                    GeneFitnessRecord {
                        trials,
                        decayed_mean_lift: if w > f64::EPSILON { wsum / w } else { 0.0 },
                        latest_ms: latest,
                        mean_decode_tps: tps_sum / trials.max(1) as f64,
                    },
                )
            })
            .collect();
        Self { by_gene, total_trials: total }
    }

    /// Load every persona's progress ledger under `progress_dir`. A missing
    /// dir is the legitimate no-evals-yet state (empty index); unreadable
    /// files or lines are skipped — see [`Self::fold_rows`].
    pub fn load(progress_dir: &Path, now_ms: u64) -> Self {
        let mut rows: Vec<serde_json::Value> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(progress_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    rows.extend(text.lines().filter_map(|l| serde_json::from_str(l).ok()));
                }
            }
        }
        Self::fold_rows(rows.iter(), now_ms)
    }

    /// The canonical progress-ledger location (`cognition/eval` writes here).
    pub fn default_dir() -> Option<std::path::PathBuf> {
        dirs::home_dir().map(|h| h.join(".continuum/progress"))
    }

    pub fn record(&self, gene: &str) -> Option<&GeneFitnessRecord> {
        self.by_gene.get(gene)
    }

    /// The 0..1 `outcome_history_factor` for a gene: neutral 0.5 shifted by
    /// the decayed mean lift, plus the UCB audition bonus. See the module
    /// docs for every term's rationale.
    pub fn outcome_factor(&self, gene: &str) -> f32 {
        let Some(rec) = self.by_gene.get(gene) else {
            return UNMEASURED_NEUTRAL;
        };
        let base = 0.5 + (rec.decayed_mean_lift / 2.0).clamp(-0.5, 0.5);
        let ucb = UCB_C
            * ((self.total_trials as f64 + 1.0).ln() / rec.trials.max(1) as f64).sqrt();
        (base + ucb).clamp(0.0, 1.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DAY_MS: u64 = 24 * 3600 * 1000;

    fn row(gene: Option<&str>, lift: f64, ts: u64) -> serde_json::Value {
        match gene {
            Some(g) => json!({"geneId": g, "lift": lift, "capturedAtMs": ts,
                              "meanDecodeTokensPerSecond": 20.0}),
            None => json!({"geneId": null, "lift": null, "capturedAtMs": ts}),
        }
    }

    // what this catches: the fold's evidence rules — base-lane rows (no geneId)
    // carry no gene evidence, gene rows count as trials, and RECEIPT AGE decays
    // worth: an old triumph must lose to a recent modest win, or the index
    // routes on stale glory forever.
    #[test]
    fn base_rows_are_skipped_and_old_glory_decays_below_recent_modesty() {
        let now = 100 * DAY_MS;
        let rows = vec![
            row(None, 0.9, now),                        // base lane — no evidence
            row(Some("old-hero"), 0.8, now - 90 * DAY_MS), // 3 half-lives ago
            row(Some("recent-solid"), 0.2, now - DAY_MS),
        ];
        let idx = GeneFitnessIndex::fold_rows(rows.iter(), now);
        assert!(idx.record("old-hero").is_some());
        assert_eq!(idx.total_trials, 2, "base row is not a trial");
        // Decay weights WITHIN a gene's mean; with one row each the means stay
        // 0.8 vs 0.2 — so pin the decay by mixing: old 0.8 + recent -0.4.
        let mixed = vec![
            row(Some("g"), 0.8, now - 90 * DAY_MS),
            row(Some("g"), -0.4, now - DAY_MS),
        ];
        let m = GeneFitnessIndex::fold_rows(mixed.iter(), now);
        let rec = m.record("g").expect("folded");
        assert!(
            rec.decayed_mean_lift < 0.0,
            "the recent −0.4 must outweigh the 3-half-life-old +0.8, got {}",
            rec.decayed_mean_lift
        );
    }

    // what this catches: the factor's contract — unmeasured genes sit at the
    // NEUTRAL 0.5 (auditions from neutrality, never dominance), positive lift
    // rises above, negative sinks below, and the UCB bonus gives a low-trial
    // gene the audition edge over an identically-scored high-trial one
    // (diversity retention — the anti-monoculture obligation).
    #[test]
    fn outcome_factor_is_neutral_lifted_sunk_and_ucb_favors_the_unproven() {
        let now = 10 * DAY_MS;
        let rows = vec![
            row(Some("proven"), 0.3, now),
            row(Some("proven"), 0.3, now),
            row(Some("proven"), 0.3, now),
            row(Some("proven"), 0.3, now),
            row(Some("young"), 0.3, now),
            row(Some("harmful"), -0.4, now),
        ];
        let idx = GeneFitnessIndex::fold_rows(rows.iter(), now);
        assert_eq!(idx.outcome_factor("never-seen"), 0.5, "unmeasured = neutral");
        assert!(idx.outcome_factor("proven") > 0.5);
        assert!(idx.outcome_factor("harmful") < 0.5, "negative lift sinks");
        assert!(
            idx.outcome_factor("young") > idx.outcome_factor("proven"),
            "same mean, fewer trials → the audition bonus: young {} vs proven {}",
            idx.outcome_factor("young"),
            idx.outcome_factor("proven")
        );
        // The bonus nudges; it never outvotes a measured verdict.
        assert!(idx.outcome_factor("harmful") < idx.outcome_factor("proven"));
    }

    // what this catches: torn/malformed ledger tails zeroing the index. The
    // ledger is append-only telemetry; a half-written last line must cost one
    // row, never the fold.
    #[test]
    fn malformed_rows_cost_themselves_not_the_index() {
        let good = row(Some("g"), 0.2, DAY_MS);
        let junk = json!("not an object");
        let idx = GeneFitnessIndex::fold_rows([&good, &junk].into_iter(), 2 * DAY_MS);
        assert_eq!(idx.record("g").map(|r| r.trials), Some(1));
    }
}
