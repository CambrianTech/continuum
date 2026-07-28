//! expert_observe — glass-box the LIVE MoE expert routing (#230 / #229).
//!
//! Runs a GGUF MoE through the core/llama FFI with a [`LiveExpertObserver`] attached, drives
//! REAL routing over a labelled multi-domain corpus, then reports the affinity that decides
//! the paging architecture (#180): PER-DOMAIN concentration + CROSS-DOMAIN hot-set overlap.
//!
//! ## Methodology (BigMama's three guardrails, 2026-07-27)
//! 1. **Sample size.** ~7 activations/slot is far too few to tell power-law from uniform —
//!    Poisson noise swamps the skew. We drive thousands of tokens so the head separates.
//! 2. **Clean samples, not a greedy loop.** A long greedy generation degenerates and
//!    OVER-routes to the same experts — a false positive for locality. We PREFILL diverse
//!    prompts (prefill routes every token on realistic input) with only short generation.
//! 3. **Diverse HOW — the knife-edge.** Pooling ACROSS domains measures the global
//!    mixed-workload histogram; MoE specialization routes domains to different experts, so
//!    pooling SMEARS toward uniform and answers the WRONG question. The paging architecture
//!    serves ONE persona doing ONE coherent thing, so we measure:
//!      (a) PER-DOMAIN concentration — one observer per domain = the coherent-session
//!          paging headroom (top-K% activation share WITHIN a domain).
//!      (b) CROSS-DOMAIN overlap — intersect each domain's hot set. Experts hot in EVERY
//!          domain = the always-resident shared base (never paged); experts hot in ONE
//!          domain = the pageable specialization tier. The money number: what fraction of a
//!          domain's activation MASS lands on experts cold for the other domains — the
//!          eviction win.
//!
//! Usage:
//!   cargo run --release -p continuum-core --features metal,accelerate --bin expert_observe -- <model.gguf> [n_gen_per_prompt]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use continuum_core::capacity::expert_observer::LiveExpertObserver;
use llama::{Batch, ContextParams, ExpertObserver, Model, ModelParams, Sampler};

/// Labelled corpus: each domain gets its OWN observer, fed several diverse prompts so the
/// per-domain sample is dense. The point is a COHERENT workload per observer (what a persona
/// actually does in a session), not a global mixture.
const DOMAINS: &[(&str, &[&str])] = &[
    (
        "code",
        &[
            "Implement a lock-free single-producer single-consumer ring buffer in Rust using \
             atomics with Release/Acquire ordering; explain why a SeqCst fence is unnecessary \
             and how the head/tail indices wrap a power-of-two capacity without a modulo in the \
             hot path.\n\nuse std::sync::atomic::{AtomicUsize, Ordering};\n",
            "Write a React hook useDebouncedResource taking an async fetcher and debounce \
             interval that cancels in-flight requests on key change, dedupes concurrent callers, \
             and surfaces loading/error/stale-while-revalidate state without tearing under \
             concurrent mode.\n\nimport { useEffect, useRef, useState } from 'react';\n",
            "Given orders(id, customer_id, placed_at, total) and refunds(order_id, amount, \
             refunded_at), write SQL returning each customer's net revenue by month, excluding \
             months with over 40% refunded, ranking customers within each month by a window \
             function.",
        ],
    ),
    (
        "prose",
        &[
            "The lighthouse keeper had not spoken to another person in forty-one days when the \
             boat appeared on the horizon. He noticed first the wrongness in the grey, and only \
             afterward resolved the shape into a hull riding low, and set down the brass polish \
             and went to the door, the cold coming up through the stone under his socks.",
            "She had rehearsed the apology on the train, each version softer than the last, until \
             the words lost their edges and became a kind of weather she carried into the room. \
             Her mother was at the sink with her back turned, and for a moment neither of them \
             moved, and the tap ran over a single white plate.",
            "Write the opening of a short story about a cartographer who discovers that a river \
             on his oldest map no longer exists, and who sets out on foot to find where it went, \
             narrated in close third person with attention to the texture of the walking.",
        ],
    ),
    (
        "math",
        &[
            "Prove that every finite integral domain is a field. Fix a nonzero a and consider \
             x -> a x; show injectivity, conclude surjectivity from finiteness, hence an inverse \
             of a exists. Then exhibit an infinite integral domain that is not a field to show \
             finiteness is essential.",
            "Derive the closed form for the variance of a sum of two correlated random variables \
             in terms of their individual variances and covariance, then generalize to n \
             variables and interpret the cross terms as the reason diversification reduces \
             portfolio variance.",
            "State and prove the pigeonhole principle, then use it to show that among any 51 \
             integers chosen from 1 to 100 there must be two that are coprime, and separately two \
             whose difference is exactly 10.",
        ],
    ),
    (
        "science",
        &[
            "Explain how the sodium-potassium pump maintains a neuron's resting potential: three \
             sodium out for two potassium in, the ATP-driven conformational change, and how the \
             electrogenic imbalance plus leak channels and the Nernst equilibria set the roughly \
             -70 mV resting potential.",
            "Describe why the sky is blue in terms of Rayleigh scattering: the inverse fourth \
             power wavelength dependence, why shorter wavelengths scatter more strongly, and why \
             sunsets are red because of the longer atmospheric path length near the horizon.",
            "Explain the greenhouse effect at the level of molecular physics: which atmospheric \
             gases absorb in the infrared, why their vibrational modes couple to outgoing \
             longwave radiation while nitrogen and oxygen do not, and how re-emission warms the \
             surface.",
        ],
    ),
    (
        "planning",
        &[
            "Sketch the migration plan to decompose a Rust monolith into services over a \
             command-and-event bus: identify the seams where synchronous calls become async, how \
             you preserve transactional guarantees that relied on one process, and how you roll \
             out incrementally behind a facade without a big-bang cutover.",
            "Write a JSON schema for a distributed-cache eviction policy supporting LRU, LFU, and \
             TTL tiers with per-key overrides, budgets in both bytes and percentage-of-pool, and \
             validation that every declared cache class has at least one eviction dimension so \
             none grows unbounded.",
            "Draft a rollout plan for a feature flag that changes a checkout flow: staged \
             percentage ramp, the metrics that gate each stage, the automatic rollback trigger, \
             and how you keep the two code paths from diverging while the flag is live.",
        ],
    ),
];

fn drive_prompt(model: &Model, observer: &Arc<LiveExpertObserver>, prompt: &str, n_gen: usize) {
    let mut ctx = model
        .new_context(ContextParams {
            n_ctx: 4096,
            n_batch: 512,
            n_seq_max: 1,
            expert_observer: Some(observer.clone() as Arc<dyn ExpertObserver>),
            ..Default::default()
        })
        .expect("context");

    let tokens = model.tokenize(prompt, true, false).expect("tokenize");
    let mut n_cur: i32 = 0;
    for chunk in tokens.chunks(512) {
        let mut batch = Batch::allocated(512, 1);
        let last_global = n_cur as usize + chunk.len() - 1;
        for (i, tok) in chunk.iter().enumerate() {
            let pos = n_cur + i as i32;
            batch.push(*tok, pos, &[0], (n_cur as usize + i) == last_global);
        }
        ctx.decode(&batch).expect("prefill decode");
        n_cur += chunk.len() as i32;
    }

    let mut sampler = Sampler::greedy();
    for _ in 0..n_gen {
        let token = sampler.sample(&ctx, -1);
        if model.is_eog_token(token) {
            break;
        }
        let mut batch = Batch::allocated(1, 1);
        batch.push(token, n_cur, &[0], true);
        ctx.decode(&batch).expect("gen decode");
        n_cur += 1;
    }
}

/// Sorted-desc counts → activation share captured by the top `frac` of FIRED experts.
fn top_share(counts_desc: &[u64], total: u64, frac: f64) -> f64 {
    if counts_desc.is_empty() || total == 0 {
        return 0.0;
    }
    let k = ((counts_desc.len() as f64 * frac).ceil() as usize).max(1).min(counts_desc.len());
    let s: u64 = counts_desc.iter().take(k).sum();
    100.0 * s as f64 / total as f64
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let model_path = args
        .get(1)
        .expect("usage: expert_observe <model.gguf> [n_gen_per_prompt]");
    let n_gen: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(48);

    let model = Model::load(
        PathBuf::from(model_path),
        ModelParams { n_gpu_layers: -1, use_mmap: true },
    )
    .expect("load model");
    println!("Loaded {model_path} (vocab={})", model.n_vocab());

    // Drive each domain into its OWN observer → per-domain hit maps.
    let mut per_domain = Vec::new();
    for (label, prompts) in DOMAINS {
        let obs = LiveExpertObserver::new();
        for prompt in *prompts {
            drive_prompt(&model, &obs, prompt, n_gen);
        }
        let hits = obs.snapshot_hits();
        println!(
            "  domain {label:<9} : {} activations across {} experts",
            obs.total_hits(),
            hits.len()
        );
        per_domain.push((label.to_string(), hits));
    }

    // ---- PER-DOMAIN concentration (the coherent-session paging headroom) ----
    println!("\n=== PER-DOMAIN CONCENTRATION (top-K% share vs uniform null) ===");
    println!("{:<10} {:>8} {:>8} {:>8}  {:>7} {:>7} {:>7}", "domain", "total", "fired", "mean", "top1%", "top10%", "top25%");
    for (label, hits) in &per_domain {
        let total: u64 = hits.values().sum();
        let mut counts: Vec<u64> = hits.values().copied().collect();
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let mean = if counts.is_empty() { 0.0 } else { total as f64 / counts.len() as f64 };
        println!(
            "{label:<10} {total:>8} {:>8} {mean:>8.1}  {:>6.1}% {:>6.1}% {:>6.1}%",
            counts.len(),
            top_share(&counts, total, 0.01),
            top_share(&counts, total, 0.10),
            top_share(&counts, total, 0.25),
        );
    }
    println!("(uniform null → top1%≈1, top10%≈10, top25%≈25; materially above = per-session paging headroom)");

    // ---- WORKING-SET SIZE: how many experts resident to capture X% of a domain's mass ----
    // THE engineering number for #180: the resident working set you must keep hot; the rest
    // pages to CPU/disk (misses on the cold tail are low-mass = infrequent).
    let experts_for_mass = |counts_desc: &[u64], total: u64, frac: f64| -> usize {
        if total == 0 { return 0; }
        let target = frac * total as f64;
        let mut acc = 0u64;
        for (i, c) in counts_desc.iter().enumerate() {
            acc += c;
            if acc as f64 >= target { return i + 1; }
        }
        counts_desc.len()
    };
    println!("\n=== WORKING-SET SIZE (experts resident for X% of a domain's mass; % of fired) ===");
    println!("{:<10} {:>10} {:>12} {:>12} {:>12}", "domain", "50%mass", "80%mass", "90%mass", "95%mass");
    for (label, hits) in &per_domain {
        let total: u64 = hits.values().sum();
        let mut counts: Vec<u64> = hits.values().copied().collect();
        counts.sort_unstable_by(|a, b| b.cmp(a));
        let n = counts.len().max(1);
        let cell = |f: f64| { let k = experts_for_mass(&counts, total, f); format!("{k} ({:.0}%)", 100.0 * k as f64 / n as f64) };
        println!("{label:<10} {:>10} {:>12} {:>12} {:>12}", cell(0.50), cell(0.80), cell(0.90), cell(0.95));
    }
    println!("(the 50%-mass column = the tight resident set; 95%-mass = keep-everything-hot floor; the GAP is the pageable tail)");

    // ---- CROSS-DOMAIN overlap: shared base vs pageable specialization ----
    // A domain's HOT SET = the smallest set of experts capturing 50% of that domain's mass.
    let hot_set = |hits: &HashMap<_, u64>| -> HashSet<_> {
        let total: u64 = hits.values().sum();
        let mut ranked: Vec<(_, u64)> = hits.iter().map(|(k, v)| (*k, *v)).collect();
        ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        let mut acc = 0u64;
        let mut set = HashSet::new();
        for (k, v) in ranked {
            if (acc as f64) >= 0.5 * total as f64 {
                break;
            }
            acc += v;
            set.insert(k);
        }
        set
    };
    let hot_sets: Vec<(String, HashSet<_>)> =
        per_domain.iter().map(|(l, h)| (l.clone(), hot_set(h))).collect();

    // Jaccard overlap matrix of hot sets.
    println!("\n=== CROSS-DOMAIN HOT-SET JACCARD (top-experts-to-50%-mass) ===");
    print!("{:<10}", "");
    for (l, _) in &hot_sets {
        print!("{l:>9}");
    }
    println!();
    for (la, sa) in &hot_sets {
        print!("{la:<10}");
        for (_lb, sb) in &hot_sets {
            let inter = sa.intersection(sb).count();
            let uni = sa.union(sb).count();
            let j = if uni > 0 { inter as f64 / uni as f64 } else { 0.0 };
            print!("{:>8.2} ", j);
        }
        println!();
    }

    // Shared base = experts in the hot set of ALL domains. Specialization = hot in exactly one.
    let n_domains = hot_sets.len();
    let mut membership: HashMap<_, usize> = HashMap::new();
    for (_l, s) in &hot_sets {
        for k in s {
            *membership.entry(*k).or_insert(0) += 1;
        }
    }
    let shared_base: HashSet<_> = membership
        .iter()
        .filter(|(_, &c)| c == n_domains)
        .map(|(k, _)| *k)
        .collect();
    let specialized: HashSet<_> = membership
        .iter()
        .filter(|(_, &c)| c == 1)
        .map(|(k, _)| *k)
        .collect();
    println!(
        "\nshared base (hot in ALL {n_domains} domains): {} experts",
        shared_base.len()
    );
    println!(
        "domain-specialized (hot in exactly 1)      : {} experts",
        specialized.len()
    );

    // The money number: per domain, what fraction of activation MASS lands on the shared base
    // vs on experts NOT in any OTHER domain's hot set (the pageable eviction win).
    println!("\n=== ACTIVATION MASS: shared-base vs pageable-specialized (the eviction win) ===");
    println!("{:<10} {:>14} {:>22}", "domain", "on shared base", "on own-only specialists");
    for (li, (label, hits)) in per_domain.iter().enumerate() {
        let total: u64 = hits.values().sum();
        // experts that are in THIS domain's hot set and NO other domain's hot set
        let own_only: HashSet<_> = hot_sets[li]
            .1
            .iter()
            .filter(|k| membership.get(*k).copied().unwrap_or(0) == 1)
            .copied()
            .collect();
        let mut base_mass = 0u64;
        let mut own_mass = 0u64;
        for (k, v) in hits {
            if shared_base.contains(k) {
                base_mass += v;
            }
            if own_only.contains(k) {
                own_mass += v;
            }
        }
        let pct = |m: u64| if total > 0 { 100.0 * m as f64 / total as f64 } else { 0.0 };
        println!(
            "{label:<10} {:>13.1}% {:>21.1}%",
            pct(base_mass),
            pct(own_mass)
        );
    }
    println!(
        "\nRead: high own-only mass = a coherent session concentrates on experts the OTHER\n\
         domains never touch → evict them when the workload isn't that domain. High shared-base\n\
         mass = a resident core you never page. That split IS the #180 paging architecture."
    );
}
