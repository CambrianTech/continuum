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

/// Why a hold command did not run, by its facts.
#[derive(Debug)]
enum HoldCommandError {
    /// The continuum home could not be resolved (`continuum_base::home`'s own reason).
    Home(String),
    /// A required argument is absent: which one.
    Missing(&'static str),
    TtlNotSeconds { text: String, cause: std::num::ParseIntError },
    NotAToken { text: String, cause: uuid::Error },
    File(std::io::Error),
    Usage,
}

impl std::fmt::Display for HoldCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HoldCommandError::Home(why) => write!(f, "the continuum home: {why}"),
            HoldCommandError::Missing(what) => write!(f, "{what} is required"),
            HoldCommandError::TtlNotSeconds { text, cause } => write!(f, "the TTL '{text}' is not seconds: {cause}"),
            HoldCommandError::NotAToken { text, cause } => write!(f, "'{text}' is not a lease token: {cause}"),
            HoldCommandError::File(cause) => write!(f, "the hold file: {cause}"),
            HoldCommandError::Usage => write!(
                f,
                "usage: continuum-deploy-hold acquire <who> <build> <ttl-secs> | renew <token> <ttl-secs> | release <token>"
            ),
        }
    }
}

impl From<std::io::Error> for HoldCommandError {
    fn from(cause: std::io::Error) -> Self {
        HoldCommandError::File(cause)
    }
}

fn run(args: Vec<String>) -> Result<ExitCode, HoldCommandError> {
    let state = continuum_base::home::continuum_home().map_err(HoldCommandError::Home)?.join("state");
    let clock = || continuum_base::time::EpochMs::now().millis();
    let arg = |at: usize, what: &'static str| args.get(at).ok_or(HoldCommandError::Missing(what));
    let secs = |at: usize| -> Result<u64, HoldCommandError> {
        let text = arg(at, "a TTL in seconds")?;
        text.parse::<u64>()
            .map(|s| s.saturating_mul(1000))
            .map_err(|cause| HoldCommandError::TtlNotSeconds { text: text.clone(), cause })
    };
    let token = |at: usize| -> Result<uuid::Uuid, HoldCommandError> {
        let text = arg(at, "the token acquire printed")?;
        uuid::Uuid::parse_str(text).map_err(|cause| HoldCommandError::NotAToken { text: text.clone(), cause })
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
            let who = arg(1, "who is measuring")?;
            let build = arg(2, "the build being measured")?;
            match acquire(&state, &LeaseAsk { who, build, ttl_ms: secs(3)? }, &clock)? {
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
        Some("renew") => Ok(done("renewed", renew(&state, token(1)?, secs(2)?, &clock)?)),
        Some("release") => Ok(done("released", release(&state, token(1)?)?)),
        _ => Err(HoldCommandError::Usage),
    }
}
