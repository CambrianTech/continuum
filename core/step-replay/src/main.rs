//! `step-replay [CAPTURES_DIR] [--gate]`: check the causal contract over a node's prompt captures and
//! print one line per mind and a total. `--gate` exits 1 when any step went unseen. The directory
//! defaults to `$CONTINUUM_HOME/fixtures/prompt-captures` (else `~/.continuum/...`).
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::path::PathBuf;

use step_replay::{capture, contract};

fn main() {
    let mut dir = None;
    let mut gate = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--gate" => gate = true,
            other => dir = Some(PathBuf::from(other)),
        }
    }
    let dir = match dir {
        Some(dir) => dir,
        None => match continuum_base::home::continuum_home() {
            Ok(home) => home.join("fixtures/prompt-captures"),
            Err(error) => {
                eprintln!("step-replay: {error}");
                std::process::exit(2);
            }
        },
    };
    let minds = match capture::load(&dir) {
        Ok(minds) => minds,
        Err(error) => {
            eprintln!("step-replay: {error}");
            std::process::exit(2);
        }
    };
    let mut total = contract::Tally::default();
    println!("mind acts seen call_missing yields_unseen outcome_missing dangling_above prose_calls repeat_runs longest_repeat");
    for (mind, acts) in &minds {
        let t = contract::check(acts);
        println!(
            "{mind} {} {} {} {} {} {} {} {} {}",
            t.acts, t.steps_seen, t.call_missing, t.yields_unseen, t.outcome_missing, t.dangling_above, t.prose_calls, t.repeat_runs, t.longest_repeat
        );
        total.add(&t);
    }
    println!(
        "total {} {} {} {} {} {} {} {} {}  (broken {})",
        total.acts,
        total.steps_seen,
        total.call_missing,
        total.yields_unseen,
        total.outcome_missing,
        total.dangling_above,
        total.prose_calls,
        total.repeat_runs,
        total.longest_repeat,
        total.broken()
    );
    if gate && total.broken() > 0 {
        std::process::exit(1);
    }
}
