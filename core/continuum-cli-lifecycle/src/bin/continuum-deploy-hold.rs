//! A measurer's lease on this node's deploy tracker (Codex on #6288): the one way a window script or an
//! agent holds the node while it measures a build, through `deploy_hold_file`'s owner, never a file
//! edit. It runs on the node whatever core build is deployed there, so a build older than the lease
//! still honors it (the tracker reads the typed hold).
//!
//!   continuum-deploy-hold acquire <who> <build> <ttl-secs>   prints the token; busy exits 2
//!   continuum-deploy-hold renew <token> <ttl-secs>           renew this acquisition; expired exits 4
//!   continuum-deploy-hold release <token>                    release this acquisition
//!
//! The TTL is the measurer's own bound for being lost; it renews through readiness and the window's end.

use std::process::ExitCode;

use continuum_cli_lifecycle::deploy_hold_file::{acquire, describe, release, renew, Acquired, ByToken, LeaseAsk};

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(why) => {
            eprintln!("continuum-deploy-hold: {why}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<ExitCode, String> {
    let state = continuum_base::home::continuum_home()?.join("state");
    let clock = || continuum_base::time::EpochMs::now().millis();
    let secs = |at: usize| -> Result<u64, String> {
        let text = args.get(at).ok_or("a TTL in seconds is required")?;
        text.parse::<u64>().map(|s| s.saturating_mul(1000)).map_err(|e| format!("the TTL '{text}' is not seconds: {e}"))
    };
    let token = |at: usize| -> Result<uuid::Uuid, String> {
        let text = args.get(at).ok_or("the token acquire printed is required")?;
        uuid::Uuid::parse_str(text).map_err(|e| format!("'{text}' is not a lease token: {e}"))
    };
    let done = |what: &str, outcome: ByToken| match outcome {
        ByToken::Done => {
            println!("{what}");
            ExitCode::SUCCESS
        }
        ByToken::Expired => {
            eprintln!("expired: this lease ended and the tracker may have moved on; acquire again");
            ExitCode::from(4)
        }
        ByToken::NotYours => {
            eprintln!("not this acquisition's: the hold is someone else's, or this lease already ended");
            ExitCode::from(3)
        }
    };
    match args.first().map(String::as_str) {
        Some("acquire") => {
            let who = args.get(1).ok_or("who is measuring is required")?;
            let build = args.get(2).ok_or("the build being measured is required")?;
            match acquire(&state, &LeaseAsk { who, build, ttl_ms: secs(3)? }, &clock).map_err(|e| e.to_string())? {
                Acquired::Leased { token } => {
                    println!("{token}");
                    Ok(ExitCode::SUCCESS)
                }
                Acquired::Busy(hold) => {
                    eprintln!("busy: {}", describe(&hold));
                    Ok(ExitCode::from(2))
                }
            }
        }
        Some("renew") => Ok(done("renewed", renew(&state, token(1)?, secs(2)?, &clock).map_err(|e| e.to_string())?)),
        Some("release") => Ok(done("released", release(&state, token(1)?).map_err(|e| e.to_string())?)),
        _ => Err("usage: continuum-deploy-hold acquire <who> <build> <ttl-secs> | renew <token> <ttl-secs> | release <token>".into()),
    }
}
