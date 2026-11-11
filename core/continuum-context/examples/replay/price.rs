//! `replay --act-price`: what holding context costs an act on this node, per model, measured from
//! the residents' prompt captures through the core's own fit (`continuum_context::price`).
//!
//! ```text
//! cargo run -p continuum-context --example replay -- --act-price --since 2026-11-06T07:00Z \
//!     [--until <UTC>] [<capture.jsonl>...]
//! ```
//!
//! With no capture files it reads every `*.jsonl` (and `.prev.jsonl`) under
//! `~/.continuum/fixtures/prompt-captures/` written since `--since`. Each completed request inside
//! the window is one act (its terminal row: `response.usage.inputTokens` and `response.timing`).
//! It prints, per model: each fitted line with its standard error, the act's seconds and the
//! marginal seconds one held token costs one act across the measured range, and a table by band of
//! context of what was measured beside what the composed curve and the direct line predict there.

use std::collections::BTreeMap;

use continuum_context::price::{ActSample, ActTimeFit, ContextPrice, Line};
use serde_json::Value;

use crate::acts::{default_paths, utc};
use crate::captures::{self, Window};

/// One act's sample from its terminal row; `None` for a row that is not a completed act with the
/// engine's timing. The allocation dyno reads its acts through this too.
pub fn sample(row: &Value) -> Option<ActSample> {
    if row["event"].as_str() != Some("completed") {
        return None;
    }
    let response = &row["response"];
    let timing = &response["timing"];
    let count = |value: &Value| value.as_u64().and_then(|n| u32::try_from(n).ok());
    Some(ActSample {
        input: count(&response["usage"]["inputTokens"])?,
        cached: count(&timing["cachedTokens"])?,
        fresh: count(&timing["prefillTokens"])?,
        prefill_ms: timing["prefillMs"].as_f64()?,
        output: count(&timing["decodeTokens"])?,
        decode_ms: timing["decodeMs"].as_f64()?,
    })
}

fn line(name: &str, unit: &str, line: &Line) {
    println!(
        "  {name}: {:.5} (+-{:.5}) + {:.4e} (+-{:.1e}) x context {unit}  [{} acts, context {}..{}]",
        line.intercept, line.intercept_se, line.slope, line.slope_se, line.acts, line.context.0, line.context.1
    );
}

/// Measured sums in one band of context.
#[derive(Default)]
struct Band {
    acts: u32,
    input: f64,
    fresh: f64,
    seconds: f64,
}

pub fn main(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut window = Window::default();
    let mut paths = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--since" => window.since_ms = Some(utc("--since", args.next())?),
            "--until" => window.until_ms = Some(utc("--until", args.next())?),
            flag if flag.starts_with("--") => return Err(format!("--act-price does not take {flag}").into()),
            _ => paths.push(arg),
        }
    }
    let paths = if paths.is_empty() { default_paths(window.since_ms)? } else { paths };
    let rows = captures::load(&paths, window)?;
    // Every completed act inside the window, by model; one terminal row per request (the newest).
    let mut by_model: BTreeMap<String, Vec<ActSample>> = BTreeMap::new();
    for row in rows.terminal.values() {
        if !row["captured_at_ms"].as_u64().is_some_and(|at_ms| window.holds(at_ms)) {
            continue;
        }
        if let (Some(act), Some(model)) = (sample(row), row["response"]["model"].as_str()) {
            by_model.entry(model.to_owned()).or_default().push(act);
        }
    }
    println!("act price: {} capture files, {} models", paths.len(), by_model.len());
    for (model, acts) in &by_model {
        let mut fit = ActTimeFit::default();
        acts.iter().for_each(|act| fit.observe(act));
        println!("\n{model}: {} completed acts, {:.0} output tokens per act", fit.acts(), acts.iter().map(|a| f64::from(a.output)).sum::<f64>() / f64::from(fit.acts().max(1)));
        let ContextPrice::Measured(time) = fit.price() else {
            println!("  unmeasured: too few acts, or no spread in context, to fit every line");
            continue;
        };
        line("decode s/output token", "(input + output/2)", &time.decode);
        line("prefill s/fresh token", "(cached + fresh/2)", &time.prefill);
        line("fresh tokens per act ", "(input)", &time.fresh);
        line("engine s/act (direct)", "(input)", &time.observed);
        println!("  context | act s composed | direct | marginal s per 1k held (+-1 se)");
        let (lo, hi) = time.observed.context;
        let step = ((hi - lo) / 8).max(1);
        for context in (lo..=hi).step_by(step as usize) {
            let at = f64::from(context);
            println!(
                "  {:>7} | {:>14.1} | {:>6.1} | {:.2} (+-{:.2})",
                context,
                time.seconds(at),
                time.observed.at(at),
                time.seconds_per_token(at) * 1_000.0,
                time.seconds_per_token_se(at) * 1_000.0
            );
        }
        // Measured beside fitted, by band of 10k tokens of prompt.
        let mut bands: BTreeMap<u32, Band> = BTreeMap::new();
        for act in acts {
            let band = bands.entry(act.input / 10_000).or_default();
            band.acts += 1;
            band.input += f64::from(act.input);
            band.fresh += f64::from(act.fresh);
            band.seconds += (act.decode_ms + act.prefill_ms) / 1_000.0;
        }
        println!("  band       | acts | fresh/act meas, fit | engine s/act meas, composed, direct");
        for (band, b) in &bands {
            let n = f64::from(b.acts);
            let mean_input = b.input / n;
            println!(
                "  {:>3}k-{:>3}k | {:>4} | {:>6.0}, {:>6.0}      | {:>6.1}, {:>6.1}, {:>6.1}",
                band * 10,
                band * 10 + 10,
                b.acts,
                b.fresh / n,
                time.fresh_at(mean_input),
                b.seconds / n,
                time.seconds(mean_input),
                time.observed.at(mean_input)
            );
        }
    }
    Ok(())
}
