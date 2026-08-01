//! moe-pager-driver — the RUN-2 adaptive control loop (#276).
//!
//! Runs NEXT TO the serve (windows-msvc clean by crate constraint):
//! tails `GGML_MOE_TRACE_FILE` as the C++ side appends routed-expert
//! records, segments decode tokens, folds each into the
//! [`BanditPlanController`], and every `--rewrite-every` tokens
//! atomically rewrites `GGML_MOE_PLAN_FILE` with a SMALL, ROLLING
//! top-N hot-routed pin set — the RUN-1 lesson encoded: pins never
//! exceed a modest cache fraction and never fossilize.
//!
//! Stdout is the live glass box: per-token serving hit, chosen decay,
//! and per-arm rewards — the `PagerCaptureEvent` policy fields.
//!
//! Usage:
//!   moe-pager-driver --trace <GGML_MOE_TRACE_FILE> \
//!                    --table <tkey-to-layer-matrix.json> \
//!                    --plan <GGML_MOE_PLAN_FILE> \
//!                    --budget-bytes <N> --window-k <N> \
//!                    [--pin-top 256] [--rewrite-every 8] [--poll-ms 50] \
//!                    [--pin-tier N] [--default-tier N]
//!
//! `--pin-tier` / `--default-tier` enable the rate-distortion split
//! (the beat-WASTE knobs): hot pins served from ladder bank
//! `--pin-tier`, the entire unpinned cold tail fetched from
//! `--default-tier`. Omit both for the plain v1 residency plan.

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use expert_pager_policy::segment::{
    parse_records, PrefillBoundaryDetector, TkeyTable, TokenSegmenter, RECORD_BYTES,
};
use expert_pager_policy::BanditPlanController;

struct Args {
    trace: PathBuf,
    table: PathBuf,
    plan: PathBuf,
    budget_bytes: u64,
    window_k: u32,
    pin_top: usize,
    rewrite_every: u64,
    poll_ms: u64,
    /// Precision-ladder index the hot pins are served from (the
    /// high-fidelity bank). None = container default.
    pin_tier: Option<u32>,
    /// Precision-ladder index the unpinned cold tail fetches from (the
    /// small-quant bank — the beat-WASTE knob). None = container default.
    default_tier: Option<u32>,
}

fn parse_args() -> Result<Args, String> {
    let mut trace = None;
    let mut table = None;
    let mut plan = None;
    let mut budget_bytes = None;
    let mut window_k = None;
    let mut pin_top = 256usize;
    let mut rewrite_every = 8u64;
    let mut poll_ms = 50u64;
    let mut pin_tier = None;
    let mut default_tier = None;
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        let value = argv
            .get(i + 1)
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag {
            "--trace" => trace = Some(PathBuf::from(value)),
            "--table" => table = Some(PathBuf::from(value)),
            "--plan" => plan = Some(PathBuf::from(value)),
            "--budget-bytes" => {
                budget_bytes = Some(value.parse().map_err(|e| format!("--budget-bytes: {e}"))?)
            }
            "--window-k" => window_k = Some(value.parse().map_err(|e| format!("--window-k: {e}"))?),
            "--pin-top" => pin_top = value.parse().map_err(|e| format!("--pin-top: {e}"))?,
            "--rewrite-every" => {
                rewrite_every = value.parse().map_err(|e| format!("--rewrite-every: {e}"))?
            }
            "--poll-ms" => poll_ms = value.parse().map_err(|e| format!("--poll-ms: {e}"))?,
            "--pin-tier" => {
                pin_tier = Some(value.parse().map_err(|e| format!("--pin-tier: {e}"))?)
            }
            "--default-tier" => {
                default_tier = Some(value.parse().map_err(|e| format!("--default-tier: {e}"))?)
            }
            other => return Err(format!("unknown flag {other}")),
        }
        i += 2;
    }
    Ok(Args {
        trace: trace.ok_or("--trace required")?,
        table: table.ok_or("--table required")?,
        plan: plan.ok_or("--plan required")?,
        budget_bytes: budget_bytes.ok_or("--budget-bytes required")?,
        window_k: window_k.ok_or("--window-k required")?,
        pin_top,
        rewrite_every,
        poll_ms,
        pin_tier,
        default_tier,
    })
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("moe-pager-driver: {e}");
            eprintln!(
                "usage: moe-pager-driver --trace <file> --table <json> --plan <file> \
                 --budget-bytes <n> --window-k <n> [--pin-top 256] [--rewrite-every 8] [--poll-ms 50]"
            );
            std::process::exit(2);
        }
    };

    let table_json = match std::fs::read_to_string(&args.table) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("moe-pager-driver: read table {}: {e}", args.table.display());
            std::process::exit(1);
        }
    };
    let table = match TkeyTable::from_json(&table_json) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("moe-pager-driver: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "# driver up: table {} tkeys, trace {}, plan {}, pin_top {}, rewrite_every {}",
        table.len(),
        args.trace.display(),
        args.plan.display(),
        args.pin_top,
        args.rewrite_every
    );

    // The bandit's prediction budget in EXPERT slots: adaptive — set
    // from the first observed token (×1.5, the prototypes' heuristic).
    let mut controller: Option<BanditPlanController> = None;
    let mut segmenter = TokenSegmenter::new();
    let mut boundary = PrefillBoundaryDetector::new();
    let mut offset: u64 = 0;
    let mut carry: Vec<u8> = Vec::new();
    let mut token_idx: u64 = 0;

    loop {
        let mut file = match std::fs::File::open(&args.trace) {
            Ok(f) => f,
            Err(_) => {
                // Serve not started yet — wait for the trace to appear.
                std::thread::sleep(std::time::Duration::from_millis(args.poll_ms));
                continue;
            }
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len < offset {
            // Truncated: a NEW serve started. Reset the whole loop
            // state — stale scores belong to the previous generation.
            println!("# trace truncated (new serve) — resetting state");
            offset = 0;
            carry.clear();
            segmenter = TokenSegmenter::new();
            boundary = PrefillBoundaryDetector::new();
            controller = None;
            token_idx = 0;
        }
        if len > offset && file.seek(SeekFrom::Start(offset)).is_ok() {
            {
                let mut fresh = Vec::with_capacity((len - offset) as usize);
                if file.read_to_end(&mut fresh).is_ok() {
                    offset = len;
                    carry.extend_from_slice(&fresh);
                    let (records, consumed) = parse_records(&carry);
                    carry.drain(..consumed);
                    debug_assert!(carry.len() < RECORD_BYTES);
                    for rec in records {
                        if let Some(token) = segmenter.push(rec, &table) {
                            if token.is_empty() {
                                continue;
                            }
                            let experts: Vec<_> = token.into_iter().collect();
                            // Prefill→decode boundary: the bandit is already
                            // seeded by the prefill batches — publish the
                            // warm-start plan NOW instead of waiting
                            // rewrite_every decode tokens (prefill's tail
                            // predicts ~47-66% of decode experts; the big
                            // prefill batches churn the cache, so the
                            // boundary is when the hint matters most).
                            if boundary.observe(experts.len()) {
                                if let Some(ctl) = controller.as_ref() {
                                    match ctl.write_tiered_plan(
                                        &args.plan,
                                        args.budget_bytes,
                                        args.window_k,
                                        args.pin_top,
                                        args.pin_tier,
                                        args.default_tier,
                                    ) {
                                        Ok(()) => println!(
                                            "# prefill->decode boundary — warm-start plan published (top {} prefill-seeded pins)",
                                            args.pin_top
                                        ),
                                        Err(e) => eprintln!(
                                            "moe-pager-driver: BOUNDARY PLAN WRITE FAILED: {e}"
                                        ),
                                    }
                                }
                            }
                            let ctl = controller.get_or_insert_with(|| {
                                let budget = experts.len() * 3 / 2;
                                println!(
                                    "# first token: {} experts -> budget {budget} slots",
                                    experts.len()
                                );
                                BanditPlanController::new(budget)
                            });
                            let hit = ctl.observe_token(&experts);
                            token_idx += 1;
                            let arms: Vec<String> = ctl
                                .per_arm_reward()
                                .iter()
                                .map(|r| format!("{r:.3}"))
                                .collect();
                            println!(
                                "tok {token_idx} size {} hit {hit:.3} decay {:.2} arms [{}] unknown_tkeys {}",
                                experts.len(),
                                ctl.chosen_decay(),
                                arms.join(","),
                                segmenter.unknown_tkeys,
                            );
                            if token_idx.is_multiple_of(args.rewrite_every) {
                                match ctl.write_tiered_plan(
                                    &args.plan,
                                    args.budget_bytes,
                                    args.window_k,
                                    args.pin_top,
                                    args.pin_tier,
                                    args.default_tier,
                                ) {
                                    Ok(()) => println!(
                                        "# plan rewritten @tok {token_idx} (top {} pins)",
                                        args.pin_top
                                    ),
                                    Err(e) => eprintln!(
                                        "moe-pager-driver: PLAN WRITE FAILED @tok {token_idx}: {e}"
                                    ),
                                }
                            }
                        }
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(args.poll_ms));
    }
}
