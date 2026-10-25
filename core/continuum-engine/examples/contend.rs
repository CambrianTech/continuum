//! Replay the minds a node served under each admission policy: the contention dyno.
//!
//! `cargo run -p continuum-engine --example contend -- [--prefill <tok/s>] [--decode <n:tok/s,...>] [--hours <h>] <probes.jsonl>...`
//!
//! Each mind's loop is read from `delib.generate.cache` probes: her acts' fresh (prefilled)
//! tokens and output tokens, in order, and the median gap from the end of one act to the start
//! of her next. The rates are the engine's ALONE (a quiet engine's prefill and decode by
//! streams), measured, passed in; the replay shares them the way contention does.

use continuum_engine::inference::planning::contention::{replay, MindLoop, Policy};
use continuum_engine::inference::planning::service_rate::{ModelRates, Workload};
use std::collections::BTreeMap;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0) // unwrap_or: no acts, nothing to report
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (mut prefill, mut decode, mut hours, mut paths) = (800.0, String::from("1:50,11:11"), 1.0, Vec::new());
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--prefill" => prefill = args.next().ok_or("--prefill needs tok/s")?.parse()?,
            "--decode" => decode = args.next().ok_or("--decode needs n:tok/s,...")?,
            "--hours" => hours = args.next().ok_or("--hours needs a number")?.parse()?,
            _ => paths.push(arg),
        }
    }
    let mut decode_per_stream = BTreeMap::new();
    for point in decode.split(',') {
        let (n, tps) = point.split_once(':').ok_or("--decode points are n:tok/s")?;
        decode_per_stream.insert(n.parse()?, tps.parse()?);
    }
    let rates = ModelRates { prefill_tps: Some(prefill), decode_per_stream };
    // Per mind: (start, end, workload), from each generation's receipt.
    let mut acts: BTreeMap<String, Vec<(f64, f64, Workload)>> = BTreeMap::new();
    for path in &paths {
        for line in std::fs::read_to_string(path)?.lines() {
            if !line.contains("delib.generate.cache") {
                continue;
            }
            let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            if row["class"] != "delib.generate.cache" {
                continue;
            }
            let field = |k: &str| row["fields"][k].as_str().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0); // unwrap_or: an absent field is no work
            let end = row["captured_at_ms"].as_f64().unwrap_or(0.0) / 1000.0; // unwrap_or: a row without its time sorts first
            let start = end - field("latency_ms") / 1000.0;
            let persona = row["fields"]["persona"].as_str().unwrap_or("?").to_owned(); // unwrap_or: an unnamed mind is still a mind
            acts.entry(persona).or_default().push((start, end, Workload { prefill_tokens: field("prefill_tokens"), decode_tokens: field("output_tokens") }));
        }
    }
    let mut minds: Vec<(String, MindLoop)> = Vec::new();
    for (persona, mut list) in acts {
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        // A gap past ten minutes is a rest or a restart, not her working rhythm.
        let gaps: Vec<f64> = list.windows(2).map(|w| w[1].0 - w[0].1).filter(|g| *g >= 0.0 && *g < 600.0).collect();
        let gap_secs = median(gaps);
        minds.push((persona, MindLoop { acts: list.into_iter().map(|(_, _, w)| w).collect(), gap_secs }));
    }
    // The busiest mind is the one a person works with in the foreground policies.
    minds.sort_by_key(|(_, m)| std::cmp::Reverse(m.acts.len()));
    let names: Vec<&str> = minds.iter().map(|(n, _)| n.as_str()).collect();
    let loops: Vec<MindLoop> = minds.iter().map(|(_, m)| m.clone()).collect();
    println!("{} minds ({}), {} acts; foreground = {}; prefill {prefill} tok/s, decode {decode}", loops.len(), names.join(", "), loops.iter().map(|m| m.acts.len()).sum::<usize>(), names.first().unwrap_or(&"-")); // unwrap_or: no minds read
    let horizon = hours * 3600.0;
    let policies = [
        ("all at once", Policy::AllAtOnce),
        ("cap 4", Policy::Cap { active: 4 }),
        ("cap 2", Policy::Cap { active: 2 }),
        ("foreground + 3", Policy::Foreground { mind: 0, background: 3 }),
        ("foreground + 2", Policy::Foreground { mind: 0, background: 2 }),
        ("foreground + 1", Policy::Foreground { mind: 0, background: 1 }),
        ("settle (60 s)", Policy::Settle { mind: 0, budget_secs: 60 }),
        ("settle (30 s)", Policy::Settle { mind: 0, budget_secs: 30 }),
    ];
    println!("{:16} {:>14} {:>14} {:>14}", "policy", "fg act med s", "bg act med s", "acts/hour");
    for (name, policy) in policies {
        let Some(r) = replay(&loops, &rates, policy, horizon) else { return Err("unmeasured rate".into()) };
        let total = (r.foreground.len() + r.background.len()) as f64 / hours;
        let settled = if r.grants.is_empty() { String::new() } else { format!("   grant settles near {} (median), range {}-{}", median(r.grants.iter().map(|g| *g as f64).collect()), r.grants.iter().min().unwrap_or(&0), r.grants.iter().max().unwrap_or(&0)) }; // unwrap_or: grants checked non-empty
        println!("{:16} {:>14.0} {:>14.0} {:>14.0}{settled}", name, median(r.foreground.clone()), median(r.background.clone()), total);
    }
    Ok(())
}
