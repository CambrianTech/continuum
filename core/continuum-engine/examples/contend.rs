//! Replay the minds a node served under each admission policy: the contention dyno.
//!
//! `cargo run -p continuum-engine --example contend -- --prefill <tok/s> --decode <n:tok/s,...> [--from <epoch s>] [--to <epoch s>] [--hours <h>] <probes.jsonl>...`
//!
//! The rates are REQUIRED and measured: the engine's prefill tok/s and its decode tok/s by
//! streams in flight (its own `print_timing` lines, or `state/serving-rates.json`). There are
//! no defaults: the dyno never runs on a guessed rate (Cormac on #5435). Each mind's loop is
//! read from `delib.generate.cache` probes inside `--from`..`--to`: her acts' fresh and output
//! tokens and the median gap between her acts. A row missing any field it needs is skipped and
//! counted, never read as zero work.

use continuum_engine::inference::planning::contention::{replay, MindLoop, Policy};
use continuum_engine::inference::planning::service_rate::{ModelRates, Workload};
use std::collections::BTreeMap;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(f64::NAN) // unwrap_or: no acts reads NaN, never a number
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (mut prefill, mut decode, mut hours, mut from, mut to, mut paths) =
        (None::<f64>, None::<String>, 1.0, f64::NEG_INFINITY, f64::INFINITY, Vec::new());
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--prefill" => prefill = Some(args.next().ok_or("--prefill needs tok/s")?.parse()?),
            "--decode" => decode = Some(args.next().ok_or("--decode needs n:tok/s,...")?),
            "--hours" => hours = args.next().ok_or("--hours needs a number")?.parse()?,
            "--from" => from = args.next().ok_or("--from needs epoch seconds")?.parse()?,
            "--to" => to = args.next().ok_or("--to needs epoch seconds")?.parse()?,
            _ => paths.push(arg),
        }
    }
    let prefill = prefill.ok_or("--prefill is required: the engine's measured prefill tok/s")?;
    let decode = decode.ok_or("--decode is required: the engine's measured decode tok/s by streams, n:tok/s,...")?;
    let mut decode_per_stream = BTreeMap::new();
    for point in decode.split(',') {
        let (n, tps) = point.split_once(':').ok_or("--decode points are n:tok/s")?;
        decode_per_stream.insert(n.parse()?, tps.parse()?);
    }
    let rates = ModelRates { prefill_tps: Some(prefill), decode_per_stream };
    let mut acts: BTreeMap<String, Vec<(f64, f64, Workload)>> = BTreeMap::new();
    let (mut read, mut skipped) = (0usize, 0usize);
    for path in &paths {
        for line in std::fs::read_to_string(path)?.lines() {
            if !line.contains("delib.generate.cache") {
                continue;
            }
            let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            if row["class"] != "delib.generate.cache" {
                continue;
            }
            let field = |k: &str| row["fields"][k].as_str().and_then(|v| v.parse::<f64>().ok());
            let (Some(end_ms), Some(latency), Some(fresh), Some(out), Some(persona)) = (
                row["captured_at_ms"].as_f64(),
                field("latency_ms"),
                field("prefill_tokens"),
                field("output_tokens"),
                row["fields"]["persona"].as_str(),
            ) else {
                skipped += 1;
                continue;
            };
            let end = end_ms / 1000.0;
            if end < from || end > to {
                continue;
            }
            read += 1;
            acts.entry(persona.to_owned()).or_default().push((end - latency / 1000.0, end, Workload { prefill_tokens: fresh, decode_tokens: out }));
        }
    }
    let mut observed = Vec::new();
    let mut minds: Vec<(String, MindLoop)> = Vec::new();
    for (persona, mut list) in acts {
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        observed.extend(list.iter().map(|(s, e, _)| e - s));
        // A gap past ten minutes is a rest or a restart, not her working rhythm.
        let gaps: Vec<f64> = list.windows(2).map(|w| w[1].0 - w[0].1).filter(|g| *g >= 0.0 && *g < 600.0).collect();
        let gap_secs = if gaps.is_empty() { 0.0 } else { median(gaps) };
        minds.push((persona, MindLoop { acts: list.into_iter().map(|(_, _, w)| w).collect(), gap_secs }));
    }
    minds.sort_by_key(|(_, m)| std::cmp::Reverse(m.acts.len()));
    let names: Vec<&str> = minds.iter().map(|(n, _)| n.as_str()).collect();
    let loops: Vec<MindLoop> = minds.iter().map(|(_, m)| m.clone()).collect();
    println!(
        "{} minds ({}), {read} acts read, {skipped} rows skipped for a missing field; observed median act {:.0} s; prefill {prefill} tok/s, decode {decode}",
        loops.len(),
        names.join(", "),
        median(observed)
    );
    let horizon = hours * 3600.0;
    let policies = [
        ("all at once", Policy::AllAtOnce),
        ("cap 4", Policy::Cap { active: 4 }),
        ("cap 2", Policy::Cap { active: 2 }),
        ("foreground + all", Policy::Foreground { mind: 0, background: usize::MAX }),
    ];
    println!("{:16} {:>12} {:>12} {:>16} {:>10}  {}", "policy", "fg med s", "bg med s", "worst mind med s", "acts/hour", "minds with no act completed");
    for (name, policy) in policies {
        let r = replay(&loops, &rates, policy, horizon).ok_or("the replay refused its inputs (an unmeasured rate or a broken workload)")?;
        let worst = r.per_mind.iter().skip(1).filter(|v| !v.is_empty()).map(|v| median(v.clone())).fold(f64::NAN, f64::max);
        let total = (r.foreground.len() + r.background.len()) as f64 / hours;
        // A mind that completed nothing is not in the median above: name it, with the horizon.
        let idle = r.without_completions();
        let idle = if idle.is_empty() { "none".to_owned() } else { format!("{} of {} in {hours} h: {idle:?}", idle.len(), loops.len()) };
        println!("{:16} {:>12.0} {:>12.0} {:>16.0} {:>10.0}  {}", name, median(r.foreground.clone()), median(r.background.clone()), worst, total, idle);
    }
    Ok(())
}
