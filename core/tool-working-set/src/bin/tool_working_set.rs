//! Score every working-set policy on the residents' real tool-call history.
//!
//! ```text
//! tool-working-set --registry <commands-list.json> --engrams <dir> --captures <dir>
//!                  --epoch-minutes <m[,m...]> --budget-tokens <n>
//!                  [--static <names.txt>] [--pin <verb>] [--train-fraction <f>]
//!                  [--cost-window-days <d>] [--population-half-life-days <d>]
//!                  [--online-rate <r>] [--minds <label,...>]
//! ```
//!
//! - `--registry`: `continuum commands/list` saved to a file (every verb, priced by its schema).
//! - `--engrams`: a directory of `<label>.json`, each `continuum cognition/recall-engrams
//!   --personaId=<id> --kind=by_origin --origin=tool --limit=<all>` for one mind.
//! - `--captures`: the prompt-capture directory; its completed rows give the engine's rates.
//! - `--epoch-minutes`: the measured epoch span (`delib.view.turnover` intervals); several
//!   give a sensitivity table each.
//! - `--budget-tokens`: the most schema the head may hold: the core's tool share of the served
//!   window (`ContextBudget::tool_surface_tokens`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tool_working_set::evaluate::{replay, walk, Event, Mind, Score};
use tool_working_set::features::{Signals, DAY_MS};
use tool_working_set::logistic::{Example, Logistic};
use tool_working_set::policy::blend::{examples, Blend};
use tool_working_set::policy::cold_start::{Calibration, ColdStart};
use tool_working_set::policy::frequency::{Frequency, Population};
use tool_working_set::policy::static_set::StaticSet;
use tool_working_set::policy::transition::Transition;
use tool_working_set::policy::Pricing;
use tool_working_set::relevance::Lexical;
use tool_working_set::{epoch, load, report, CostModel, HeadSet, Vocabulary, WorkingSetPolicy};

/// Today's M5 surface, the baseline (see the file's own receipt).
const M5_HANDS: &str = include_str!("../../data/m5-hands.txt");

struct Args {
    registry: PathBuf,
    engrams: PathBuf,
    captures: PathBuf,
    epoch_minutes: Vec<f64>,
    budget_tokens: u32,
    static_set: Option<PathBuf>,
    pin: String,
    train_fraction: f64,
    cost_window_days: u64,
    population_half_life_days: u64,
    online_rate: f64,
    minds: Option<Vec<String>>,
}

fn main() -> ExitCode {
    match parse(std::env::args().skip(1).collect()).and_then(|args| run(&args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tool-working-set: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse(argv: Vec<String>) -> Result<Args, String> {
    let mut values = std::collections::HashMap::new();
    let mut argv = argv.into_iter();
    while let Some(flag) = argv.next() {
        let key = flag.strip_prefix("--").ok_or_else(|| format!("unexpected argument {flag}"))?.to_owned();
        let value = argv.next().ok_or_else(|| format!("{flag} needs a value"))?;
        values.insert(key, value);
    }
    let required = |key: &str| values.get(key).cloned().ok_or_else(|| format!("--{key} is required"));
    let number = |key: &str, default: &str| -> Result<f64, String> {
        let text = values.get(key).map_or(default, String::as_str); // map_or: an omitted option takes its documented default
        text.parse::<f64>().map_err(|e| format!("--{key} {text}: {e}"))
    };
    let epoch_minutes = required("epoch-minutes")?
        .split(',')
        .map(|minutes| minutes.trim().parse::<f64>().map_err(|e| format!("--epoch-minutes {minutes}: {e}")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Args {
        registry: required("registry")?.into(),
        engrams: required("engrams")?.into(),
        captures: required("captures")?.into(),
        epoch_minutes,
        budget_tokens: required("budget-tokens")?.parse().map_err(|e| format!("--budget-tokens: {e}"))?,
        static_set: values.get("static").map(PathBuf::from),
        pin: values.get("pin").cloned().unwrap_or_else(|| "commands/help".to_owned()), // unwrap_or_else: the verb that gives any other verb's call, until tools/find exists
        train_fraction: number("train-fraction", "0.7")?,
        cost_window_days: number("cost-window-days", "7")? as u64,
        population_half_life_days: number("population-half-life-days", "7")? as u64,
        online_rate: number("online-rate", "0.05")?,
        minds: values.get("minds").map(|list| list.split(',').map(|label| label.trim().to_owned()).collect()),
    })
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn run(args: &Args) -> Result<(), String> {
    let vocab = load::registry(&read(&args.registry)?)?;
    let records = load_minds(&args.engrams, &vocab)?;
    let samples = load::act_samples(&args.captures)?;
    let cost = CostModel::from_samples(&samples, args.cost_window_days * DAY_MS)
        .ok_or_else(|| format!("no completed capture with engine rates under {}", args.captures.display()))?;
    println!(
        "# cost: {} completed calls in the {} days to the newest; prefill {:.1} tok/s, decode {:.2} tok/s, act decodes {:.0} tokens, overhead {:.0} ms",
        cost.samples, args.cost_window_days, cost.prefill_tokens_per_s, cost.decode_tokens_per_s, cost.act_decode_tokens, cost.act_overhead_ms
    );
    println!(
        "# a miss (one extra act) = {:.0} prompt-token equivalents = {:.1} s; a head schema token = 1 (prefilled once per epoch) = {:.4} s",
        cost.miss_tokens(),
        cost.seconds(cost.miss_tokens()),
        cost.seconds(1.0)
    );
    let static_text = match &args.static_set {
        Some(path) => read(path)?,
        None => M5_HANDS.to_owned(),
    };
    let static_names = load::names(&static_text);
    let missing: Vec<&str> = static_names.iter().copied().filter(|name| vocab.id(name).is_none()).collect();
    if !missing.is_empty() {
        return Err(format!("the static set names verbs the registry lacks: {missing:?}"));
    }
    let static_head = HeadSet::of(&vocab, static_names.iter().filter_map(|name| vocab.id(name)).collect());
    let pin = vocab.id(&args.pin).ok_or_else(|| format!("--pin {}: not a verb of the registry", args.pin))?;
    println!(
        "# static set: {} verbs, {} tokens at registry pricing; head budget {} tokens; pinned {}",
        static_head.tools().len(),
        static_head.tokens(),
        args.budget_tokens,
        args.pin
    );
    println!("# registry: {} verbs", vocab.len());
    let pricing = Pricing { cost, pinned: vec![pin] };
    let lexical = Lexical::new(&vocab);
    for (index, minutes) in args.epoch_minutes.iter().enumerate() {
        let span_ms = (minutes * 60_000.0) as u64;
        let minds: Vec<Mind> = records
            .iter()
            .map(|(label, record)| Mind { label: label.clone(), epochs: epoch::segment(&record.calls, span_ms), calls: record.calls.clone() })
            .collect();
        if index == 0 {
            summarize(&records, &minds);
        }
        let population_half_life_ms = args.population_half_life_days * DAY_MS;
        let (model, calibration) = fit(&vocab, &minds, &lexical, args.train_fraction, population_half_life_ms);
        println!("\n# epoch span {minutes} min: blend weights on standardized inputs [short 3d, long 30d, activity 7d, transition, society, relevance, bias] = {:?}", rounded(&model.weights()));
        let mut policies: Vec<Box<dyn WorkingSetPolicy>> = vec![Box::new(StaticSet::new(format!("static ({} verbs)", static_head.tools().len()), static_head.clone()))];
        for days in [1, 3, 7, 30] {
            policies.push(Box::new(Frequency::new(Some(days * DAY_MS), pricing.clone())));
        }
        policies.push(Box::new(Frequency::new(None, pricing.clone())));
        policies.push(Box::new(Transition::new(pricing.clone())));
        policies.push(Box::new(Population::new(pricing.clone())));
        policies.push(Box::new(ColdStart::new(lexical.clone(), calibration, pricing.clone())));
        policies.push(Box::new(Blend::fitted(model, lexical.clone(), pricing.clone())));
        policies.push(Box::new(Blend::online(args.online_rate, lexical.clone(), pricing.clone())));
        let names: Vec<String> = policies.iter().map(|policy| policy.name()).chain(["oracle (hindsight)".to_owned()]).collect();
        let scores = replay(&vocab, &minds, &mut policies, args.budget_tokens, args.train_fraction, population_half_life_ms);
        let all: Vec<(String, [Score; 2])> = names
            .iter()
            .zip(&scores)
            .map(|(name, per_mind)| {
                let mut halves = [Score::default(); 2];
                for mind in per_mind {
                    halves[0].add(&mind[0]);
                    halves[1].add(&mind[1]);
                }
                (name.clone(), halves)
            })
            .collect();
        print!("{}", report::table(&format!("all minds, epoch {minutes} min"), &all, &cost));
        if index > 0 {
            continue;
        }
        for (mind_index, mind) in minds.iter().enumerate() {
            if args.minds.as_ref().is_some_and(|wanted| !wanted.contains(&mind.label)) {
                continue;
            }
            let rows: Vec<(String, [Score; 2])> = names.iter().zip(&scores).map(|(name, per_mind)| (name.clone(), per_mind[mind_index])).collect();
            print!("{}", report::table(&format!("{}, epoch {minutes} min", mind.label), &rows, &cost));
        }
    }
    Ok(())
}

/// Every `<label>.json` under `dir`, in label order.
fn load_minds(dir: &Path, vocab: &Vocabulary) -> Result<Vec<(String, load::MindRecord)>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let label = path.file_stem().and_then(|stem| stem.to_str()).map_or_else(|| path.display().to_string(), str::to_owned); // map_or_else: a non-UTF-8 name is labelled by its whole path
            let record = load::engrams(&read(&path)?, vocab).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok((label, record))
        })
        .collect()
}

/// The data each mind brings: calls, epochs, activities, span, what was left out.
fn summarize(records: &[(String, load::MindRecord)], minds: &[Mind]) {
    println!("\n# data: mind, calls, epochs, activities, first, last, calls per epoch, left out (not verbs / duplicates)");
    let (mut calls, mut epochs, mut left_out) = (0usize, 0usize, 0u64);
    for ((label, record), mind) in records.iter().zip(minds) {
        let mut activities: Vec<_> = record.calls.iter().filter_map(|call| call.activity).collect();
        activities.sort_by_key(|activity| activity.0);
        activities.dedup();
        let day = |index: usize| record.calls.get(index).map_or_else(|| "-".to_owned(), |call| call.at.rfc3339()[..10].to_owned()); // map_or_else: a mind with no calls has no span; head-cut-ok: a date label for the report, not prompt text
        let not_verbs: u64 = record.not_verbs.values().sum();
        println!(
            "{label:<10} {:>6} {:>5} {:>4} {} {} {:>5.2} {not_verbs}/{}",
            record.calls.len(),
            mind.epochs.len(),
            activities.len(),
            day(0),
            day(record.calls.len().saturating_sub(1)),
            record.calls.len() as f64 / mind.epochs.len().max(1) as f64,
            record.duplicates
        );
        calls += record.calls.len();
        epochs += mind.epochs.len();
        left_out += not_verbs;
    }
    println!("{:<10} {calls:>6} {epochs:>5}   (left out as not verbs: {left_out})", "total");
}

/// Fit the blend and the relevance calibration on every mind's training epochs, each example
/// read as it stood at its turnover.
fn fit(vocab: &Vocabulary, minds: &[Mind], lexical: &Lexical, train_fraction: f64, population_half_life_ms: u64) -> (Logistic, Calibration) {
    let mut blend_examples: Vec<Example> = Vec::new();
    let mut relevance_examples: Vec<(f32, bool)> = Vec::new();
    walk(vocab, minds, train_fraction, population_half_life_ms, |event, turn| {
        if event != Event::Ended || turn.test {
            return;
        }
        let signals = Signals::of(&turn.history, lexical);
        blend_examples.extend(examples(&signals, turn.used));
        if !turn.history.calls.is_empty() {
            relevance_examples.extend(
                signals.relevance.iter().enumerate().map(|(tool, relevance)| (*relevance, turn.used.iter().any(|used| used.index() == tool))),
            );
        }
    });
    // A batch fit on a convex loss over seven standardized inputs: 300 full-gradient steps at
    // 0.5 (the fixture test converges in that budget; the run prints the fitted weights).
    let model = Logistic::fit(&blend_examples, 300, 0.5);
    // Ten equal-count bins: a decile calibration, coarse enough that every bin holds thousands.
    (model, Calibration::fit(&relevance_examples, 10))
}

fn rounded(weights: &[f64]) -> Vec<f64> {
    weights.iter().map(|weight| (weight * 1_000.0).round() / 1_000.0).collect()
}
