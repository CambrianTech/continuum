//! cooccur-ceiling — offline VDD measurement of the CROSS-LAYER PREFETCH
//! predictor's ceiling (the `CrossLayerExpertPredictor` lever) on a real
//! `GGML_MOE_TRACE_FILE`.
//!
//! The bandit EMA residency curve (moe-pager-driver) answers "which
//! experts to KEEP resident across tokens" — a recency-family signal. This
//! bin answers a DIFFERENT question: within ONE forward pass, given the
//! experts that fired in layer L, how predictable are layer L+1's experts
//! from learned co-occurrence? If high, prefetching L+1 RAM→VRAM on a copy
//! stream while L computes can HIDE the per-expert H2D latency the residency
//! curve can't avoid — the two levers compose.
//!
//! Method (honest held-out): segment the trace into per-token, per-layer
//! expert sets. Train an adjacent-layer noisy-OR co-occurrence model on the
//! first `--train-frac` of DECODE tokens; on the rest, for each layer step
//! L→L+1 predict L+1's top-|L+1| experts from L's fired experts and score
//! the hit fraction. Baseline = predict L+1 as the PREVIOUS token's L+1
//! experts (recency). The gap is the exploitable cross-layer structure.
//!
//! Usage:
//!   cooccur-ceiling --trace <file> --synth-layers <n> [--train-frac 0.6]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use expert_pager_policy::segment::{parse_records, TkeyTable, TokenSegmenter};
use expert_pager_policy::ExpertId;

struct Args {
    trace: PathBuf,
    synth_layers: u32,
    train_frac: f64,
}

fn parse_args() -> Result<Args, String> {
    let mut trace = None;
    let mut synth_layers = None;
    let mut train_frac = 0.6f64;
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        let value = argv
            .get(i + 1)
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag {
            "--trace" => trace = Some(PathBuf::from(value)),
            "--synth-layers" => {
                synth_layers = Some(value.parse().map_err(|e| format!("--synth-layers: {e}"))?)
            }
            "--train-frac" => {
                train_frac = value.parse().map_err(|e| format!("--train-frac: {e}"))?
            }
            other => return Err(format!("unknown flag {other}")),
        }
        i += 2;
    }
    Ok(Args {
        trace: trace.ok_or("--trace required")?,
        synth_layers: synth_layers.ok_or("--synth-layers required")?,
        train_frac,
    })
}

/// Group one token's flat expert set into per-layer expert sets.
fn by_layer(token: &HashSet<ExpertId>) -> HashMap<u32, HashSet<u32>> {
    let mut m: HashMap<u32, HashSet<u32>> = HashMap::new();
    for e in token {
        m.entry(e.layer).or_default().insert(e.expert);
    }
    m
}

/// Adjacent-layer co-occurrence tallies: cooccur[(L, p)][n] = times expert
/// `n` in layer L+1 fired in a pass where expert `p` fired in layer L. seen
/// counts the denominator P(n | L, p).
#[derive(Default)]
struct CoOccur {
    seen: HashMap<(u32, u32), u64>,
    cooccur: HashMap<(u32, u32), HashMap<u32, u64>>,
}

impl CoOccur {
    fn learn(&mut self, layers: &HashMap<u32, HashSet<u32>>, n_layers: u32) {
        for l in 0..n_layers.saturating_sub(1) {
            let (Some(prev), Some(next)) = (layers.get(&l), layers.get(&(l + 1))) else {
                continue;
            };
            for &p in prev {
                *self.seen.entry((l, p)).or_insert(0) += 1;
                let row = self.cooccur.entry((l, p)).or_default();
                for &n in next {
                    *row.entry(n).or_insert(0) += 1;
                }
            }
        }
    }

    /// Predict layer L+1's experts (noisy-OR over the fired predecessors in
    /// layer L), return the top-`budget` by confidence.
    fn predict(&self, l: u32, prev: &HashSet<u32>, budget: usize) -> HashSet<u32> {
        // noisy-OR: P(n) = 1 - Π_p (1 - P(n|p))
        let mut not_fire: HashMap<u32, f64> = HashMap::new();
        for &p in prev {
            let Some(seen) = self.seen.get(&(l, p)).copied().filter(|&s| s > 0) else {
                continue;
            };
            if let Some(row) = self.cooccur.get(&(l, p)) {
                for (&n, &c) in row {
                    let cond = c as f64 / seen as f64;
                    *not_fire.entry(n).or_insert(1.0) *= 1.0 - cond;
                }
            }
        }
        let mut scored: Vec<(f64, u32)> =
            not_fire.into_iter().map(|(n, nf)| (1.0 - nf, n)).collect();
        let b = budget.min(scored.len());
        if b == 0 {
            return HashSet::new();
        }
        let idx = (b - 1).min(scored.len() - 1);
        scored.select_nth_unstable_by(idx, |a, c| {
            c.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
        });
        scored[..b].iter().map(|x| x.1).collect()
    }
}

fn hit(pred: &HashSet<u32>, actual: &HashSet<u32>) -> f64 {
    if actual.is_empty() {
        return 0.0;
    }
    actual.iter().filter(|e| pred.contains(e)).count() as f64 / actual.len() as f64
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("cooccur-ceiling: {e}");
            eprintln!("usage: cooccur-ceiling --trace <file> --synth-layers <n> [--train-frac 0.6]");
            std::process::exit(2);
        }
    };
    let bytes = match std::fs::read(&args.trace) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cooccur-ceiling: read {}: {e}", args.trace.display());
            std::process::exit(1);
        }
    };
    let table = TkeyTable::for_layers(args.synth_layers);
    let (records, _consumed) = parse_records(&bytes);

    // Segment into completed token expert-sets, grouped per layer.
    let mut segmenter = TokenSegmenter::new();
    let mut tokens: Vec<HashMap<u32, HashSet<u32>>> = Vec::new();
    for rec in records {
        if let Some(token) = segmenter.push(rec, &table) {
            if !token.is_empty() {
                tokens.push(by_layer(&token));
            }
        }
    }
    // The big prefill token skews sizes; drop the first (prefill) token so
    // the measurement is on decode-shaped passes.
    if !tokens.is_empty() {
        tokens.remove(0);
    }
    let n = tokens.len();
    if n < 4 {
        eprintln!("cooccur-ceiling: only {n} tokens — need more trace");
        std::process::exit(1);
    }
    let split = ((n as f64) * args.train_frac) as usize;
    let split = split.clamp(1, n - 1);

    // Train.
    let mut model = CoOccur::default();
    for tok in &tokens[..split] {
        model.learn(tok, args.synth_layers);
    }

    // Test each L→L+1 step on held-out tokens: co-occurrence vs recency.
    let mut co_sum = 0.0f64;
    let mut rec_sum = 0.0f64;
    let mut steps = 0u64;
    // The decision-relevant metric: of the experts RECENCY MISSES (the ones
    // a prefetch predictor would exist to catch), what fraction does
    // co-occurrence recover? High = prefetch has a real niche on top of
    // residency; low = co-occurrence is redundant-or-worse than recency.
    let mut miss_total = 0u64;
    let mut miss_caught = 0u64;
    for t in split..n {
        let cur = &tokens[t];
        let prevtok = &tokens[t - 1];
        for l in 0..args.synth_layers.saturating_sub(1) {
            let (Some(prev_experts), Some(actual)) = (cur.get(&l), cur.get(&(l + 1))) else {
                continue;
            };
            let budget = actual.len(); // predict exactly as many as fire
            let co_pred = model.predict(l, prev_experts, budget);
            co_sum += hit(&co_pred, actual);
            // Recency baseline: last token's SAME layer L+1 experts.
            let empty = HashSet::new();
            let rec_pred = prevtok.get(&(l + 1)).unwrap_or(&empty);
            rec_sum += hit(rec_pred, actual);
            // Co-occurrence recall on the recency-miss subset.
            for e in actual {
                if !rec_pred.contains(e) {
                    miss_total += 1;
                    if co_pred.contains(e) {
                        miss_caught += 1;
                    }
                }
            }
            steps += 1;
        }
    }

    let scale = if steps > 0 { 1.0 / steps as f64 } else { 0.0 };
    println!(
        "# cooccur-ceiling: {n} tokens ({split} train / {} test), {steps} layer-steps scored",
        n - split
    );
    println!("cross_layer_cooccur_hit {:.4}", co_sum * scale);
    println!("recency_same_layer_hit  {:.4}", rec_sum * scale);
    println!(
        "# structure beyond recency: {:+.4} (positive = co-occurrence beats recency overall)",
        (co_sum - rec_sum) * scale
    );
    let miss_recall = if miss_total > 0 {
        miss_caught as f64 / miss_total as f64
    } else {
        0.0
    };
    println!(
        "cooccur_recall_on_recency_misses {:.4} ({miss_caught}/{miss_total}) \
         — the prefetch niche: co-occurrence's recall on the experts recency misses",
        miss_recall
    );
}
