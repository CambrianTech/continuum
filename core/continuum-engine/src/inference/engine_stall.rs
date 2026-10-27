//! Evidence from an engine that stopped answering, taken before it is condemned (card 3913621a).
//!
//! On enrico, 2026-10-27, a KV page restore got no answer for 90 s twice (08:27Z on 7d8bff696,
//! 11:25Z on 741b0175b). Each time the endpoint was quarantined and the engine replaced, a cold
//! start for every mind, and nothing said why. The 11:25Z engine's own log went silent about ten
//! minutes before the restore timed out. A stack sample of the engine and the time since its
//! last stderr line tell the causes apart: an engine blocked writing its log, waiting on a lock,
//! or busy computing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::system_resources::bounded_command::{self, Probed};

/// How long the sampler watches: two seconds of stacks show a thread stuck in one frame apart
/// from one that is only slow.
// derived-or-floor: a FLOOR on evidence taken from an engine already condemned; it bounds no serving, prefill or decode, and nothing measured decides how long a stuck stack must be watched.
const SAMPLE_SECONDS: u32 = 2;

/// Symbolicating a sample on the slowest node takes longer than the sample itself; the bound
/// only keeps a hung sampler from holding the quarantine forever.
const SAMPLER_GRACE: Duration = Duration::from_secs(30);

/// Stall samples kept beside the engine's log. A sample is a few hundred KB; older ones are
/// deleted when a new one is written, so the directory stays bounded.
const KEEP: usize = 4;

/// What was captured from the engine before it was condemned.
#[derive(Debug)]
pub struct StallEvidence {
    /// How long the engine's stderr had been silent, when this process drains it.
    pub silent_ms: Option<u64>,
    /// Where the stack sample was written, or why there is none.
    pub sample: Result<PathBuf, String>,
}

/// Capture what the engine at `root` was doing. Never fails: a sample that cannot be taken is
/// named in [`StallEvidence::sample`].
pub async fn capture(root: &str) -> StallEvidence {
    let log = crate::inference::llama_server::port_of_base_url(root)
        .and_then(crate::inference::child_log::llama_server_log);
    let silent_ms = log.as_deref().and_then(crate::inference::child_log::silent_ms);
    let sample = match (crate::inference::lane_pidfile::read(), log) {
        // The recorded pid may be dead or reused: sample only a process that is the engine.
        (Some(pid), Some(log)) if crate::inference::lane_process::is_llama_server(pid) => {
            tokio::task::spawn_blocking(move || sample(pid, &log))
                .await
                .unwrap_or_else(|join| Err(format!("the sampler task ended: {join}"))) // unwrap_or_else: a panicked or cancelled sampler is named, never a crash
        }
        (Some(pid), Some(_)) => Err(format!("pid {pid} is no longer the engine")),
        (None, _) => Err("no live lane pid is recorded".into()),
        (_, None) => Err("no engine log path for this endpoint".into()),
    };
    StallEvidence { silent_ms, sample }
}

fn sample(pid: u32, log: &Path) -> Result<PathBuf, String> {
    let dir = log.parent().ok_or("the engine log has no directory")?;
    let out = dir.join(format!("engine-stall-{pid}-{}.txt", continuum_base::time::EpochMs::now().millis()));
    let sampler = continuum_host::stack_sample::sampler(pid, SAMPLE_SECONDS, &out)
        .ok_or("this host has no stack sampler")?;
    let args: Vec<&str> = sampler.args.iter().map(String::as_str).collect();
    let timeout = Duration::from_secs(u64::from(SAMPLE_SECONDS)) + SAMPLER_GRACE;
    match bounded_command::probe(sampler.program, &args, timeout) {
        Probed::Exited { success: true, stdout } => {
            if !sampler.writes_out {
                std::fs::write(&out, stdout).map_err(|e| format!("writing {}: {e}", out.display()))?;
            }
        }
        Probed::Exited { success: false, stdout } => return Err(format!("{} failed: {}", sampler.program, stdout.trim())),
        Probed::Truncated => return Err(format!("{}'s output passed the capture limit", sampler.program)),
        Probed::TimedOut => return Err(format!("{} did not finish within {timeout:?}", sampler.program)),
        Probed::Unstartable { error } => return Err(format!("{} could not start: {error}", sampler.program)),
    }
    prune(dir, KEEP);
    Ok(out)
}

/// Keep the newest `keep` stall samples in `dir`.
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut samples: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("engine-stall-"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    samples.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, old) in samples.into_iter().skip(keep) {
        let _ = std::fs::remove_file(old); // a sample already gone is the outcome wanted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: stall samples growing without bound in the logs directory; only the
    // newest are kept, and nothing else in the directory is touched.
    #[test]
    fn only_the_newest_samples_are_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        for n in 0..6 {
            std::fs::write(dir.path().join(format!("engine-stall-1-{n}.txt")), "stack").expect("write");
            std::thread::sleep(Duration::from_millis(20));
        }
        std::fs::write(dir.path().join("llama-server-58057.log"), "log").expect("write");
        prune(dir.path(), 4);
        let mut left: Vec<String> = std::fs::read_dir(dir.path()).expect("read").flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["engine-stall-1-2.txt", "engine-stall-1-3.txt", "engine-stall-1-4.txt", "engine-stall-1-5.txt", "llama-server-58057.log"]);
    }

    // what this catches: sampling a pid that is not the engine (dead or reused) instead of
    // naming why there is no sample.
    #[tokio::test]
    async fn an_endpoint_with_no_engine_is_named_not_sampled() {
        let evidence = capture("not a url").await;
        assert!(evidence.sample.is_err(), "{evidence:?}");
    }
}
