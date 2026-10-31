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

use crate::inference::engine_residency::{EngineIncarnation, Liveness};
use crate::inference::lane_registry::LaneRecord;
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

/// PURE: the one engine incarnation that serves `root`, or why there is none. Only a lane this
/// node launched answers for an endpoint: its record names the root, and the process is that
/// incarnation (pid and OS start time) and is an engine now. A remote or secondary endpoint,
/// a replaced engine, a reused pid or two live claimants is refused, never sampled in its
/// place (Codex on #5528).
pub fn engine_of(
    root: &str,
    records: &[LaneRecord],
    liveness: impl Fn(&EngineIncarnation) -> Liveness,
    is_engine: impl Fn(u32) -> bool,
) -> Result<EngineIncarnation, String> {
    let root = root.trim_end_matches('/').trim_end_matches("/v1");
    let mut serving = records.iter().filter(|record| record.root_url() == root).peekable();
    if serving.peek().is_none() {
        return Err(format!("no lane this node launched serves {root}"));
    }
    let live: Vec<EngineIncarnation> = serving
        .map(LaneRecord::incarnation)
        .filter(|incarnation| incarnation.started_s != 0 && liveness(incarnation) == Liveness::Alive)
        .collect();
    match live.as_slice() {
        [one] if is_engine(one.pid) => Ok(*one),
        [one] => Err(format!("pid {} is not an engine", one.pid)),
        [] => Err(format!("the engine recorded for {root} is no longer running as recorded")),
        _ => Err(format!("{} live records claim {root}", live.len())),
    }
}

/// Capture what the engine at `root` was doing. Never fails: a sample that cannot be taken is
/// named in [`StallEvidence::sample`].
pub async fn capture(root: &str) -> StallEvidence {
    let engine = engine_of(
        root,
        &crate::inference::lane_registry::records(),
        EngineIncarnation::liveness,
        crate::inference::lane_process::is_llama_server,
    );
    let silent_ms = engine.as_ref().ok().and_then(|engine| crate::inference::child_log::silent_ms(engine.pid));
    let log = crate::inference::child_log::llama_server_log;
    let sample = match engine {
        Ok(engine) => match log(engine.port) {
            Some(log) => tokio::task::spawn_blocking(move || sample_incarnation(engine, &log))
                .await
                .unwrap_or_else(|join| Err(format!("the sampler task ended: {join}"))), // unwrap_or_else: a panicked or cancelled sampler is named, never a crash
            None => Err("no engine log path for this endpoint".into()),
        },
        Err(why) => Err(why),
    };
    StallEvidence { silent_ms, sample }
}

/// Sample `engine`, and keep the sample only if the same incarnation was running when it ended:
/// an engine replaced while it was sampled would otherwise lend its stack to the one that hung.
fn sample_incarnation(engine: EngineIncarnation, log: &Path) -> Result<PathBuf, String> {
    let out = sample(engine.pid, log)?;
    if engine.liveness() != Liveness::Alive {
        let _ = std::fs::remove_file(&out); // a discarded sample: its absence is the outcome wanted
        return Err(format!("pid {} was replaced while it was sampled", engine.pid));
    }
    Ok(out)
}

fn sample(pid: u32, log: &Path) -> Result<PathBuf, String> {
    let dir = log.parent().ok_or("the engine log has no directory")?;
    let out = dir.join(format!("engine-stall-{pid}-{}.txt", continuum_base::time::EpochMs::now().millis()));
    let sampler = continuum_host::stack_sample::sampler(pid, SAMPLE_SECONDS, &out)
        .ok_or("this host has no stack sampler")?;
    let args: Vec<&str> = sampler.args.iter().map(String::as_str).collect();
    let timeout = Duration::from_secs(u64::from(SAMPLE_SECONDS)) + SAMPLER_GRACE;
    let taken = match bounded_command::probe(sampler.program, &args, timeout) {
        Probed::Exited { success: true, stdout } if !sampler.writes_out => {
            std::fs::write(&out, stdout).map_err(|e| format!("writing {}: {e}", out.display()))
        }
        Probed::Exited { success: true, .. } => Ok(()),
        Probed::Exited { success: false, stdout } => Err(format!("{} failed: {}", sampler.program, stdout.trim())),
        Probed::Truncated => Err(format!("{}'s output passed the capture limit", sampler.program)),
        Probed::TimedOut => Err(format!("{} did not finish within {timeout:?}", sampler.program)),
        Probed::Unstartable { error } => Err(format!("{} could not start: {error}", sampler.program)),
    };
    settle(dir, out, taken)
}

/// Every attempt ends here: a failed one leaves no partial sample (the macOS sampler writes its
/// file as it goes), and the directory is pruned either way, so the bound holds however the
/// sampler ends (Codex on #5528).
fn settle(dir: &Path, out: PathBuf, taken: Result<(), String>) -> Result<PathBuf, String> {
    if taken.is_err() {
        let _ = std::fs::remove_file(&out); // a partial sample is removed; one never written is the outcome wanted
    }
    prune(dir, KEEP);
    taken.map(|()| out)
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

    fn record(pid: u32, port: u16, started_s: u64) -> LaneRecord {
        serde_json::from_value(serde_json::json!({
            "pid": pid, "port": port, "role": "live", "started_s": started_s
        }))
        .expect("a lane record fixture")
    }

    // what this catches (Codex on #5528): a sampler that fails part-way leaving its partial file,
    // and failures skipping the prune, so failed captures pile up past the bound.
    #[test]
    fn a_failed_sample_leaves_nothing_and_every_attempt_prunes() {
        let dir = tempfile::tempdir().expect("tempdir");
        for n in 0..5 {
            std::fs::write(dir.path().join(format!("engine-stall-1-{n}.txt")), "stack").expect("write");
            std::thread::sleep(Duration::from_millis(20));
        }
        let partial = dir.path().join("engine-stall-1-9.txt");
        std::fs::write(&partial, "half a stack").expect("write");
        assert!(settle(dir.path(), partial.clone(), Err("sample did not finish".into())).is_err());
        assert!(!partial.exists(), "the partial sample is removed");
        let left = std::fs::read_dir(dir.path()).expect("read").count();
        assert_eq!(left, KEEP, "a failed attempt still prunes to the bound");
        let whole = dir.path().join("engine-stall-1-10.txt");
        std::fs::write(&whole, "stack").expect("write");
        assert_eq!(settle(dir.path(), whole.clone(), Ok(())), Ok(whole.clone()));
        assert!(whole.exists());
    }

    // what this catches (Codex on #5528): sampling some other engine in the failed endpoint's
    // name. Only this node's lane for that exact root, alive as the same incarnation and still
    // an engine, is sampled; every other case is named and refused.
    #[test]
    fn only_the_engine_that_serves_the_endpoint_is_sampled() {
        let alive = |_: &EngineIncarnation| Liveness::Alive;
        let engine = |_: u32| true;
        let local = [record(41, 58057, 100)];
        assert_eq!(engine_of("http://127.0.0.1:58057/", &local, alive, engine).map(|e| e.pid), Ok(41));
        assert_eq!(engine_of("http://127.0.0.1:58057/v1", &local, alive, engine).map(|e| e.pid), Ok(41));
        // A remote or secondary endpoint: no lane this node launched serves it.
        assert!(engine_of("http://10.0.0.7:58057", &local, alive, engine).is_err());
        assert!(engine_of("http://127.0.0.1:58058", &local, alive, engine).is_err());
        // Replaced: the record's incarnation is no longer the process.
        assert!(engine_of("http://127.0.0.1:58057", &local, |_: &EngineIncarnation| Liveness::Dead, engine).is_err());
        // A reused pid that is not an engine; a legacy record with no start time.
        assert!(engine_of("http://127.0.0.1:58057", &local, alive, |_: u32| false).is_err());
        assert!(engine_of("http://127.0.0.1:58057", &[record(41, 58057, 0)], alive, engine).is_err());
        // A stale record beside the live one: the live incarnation answers, never the stale one.
        let both = [record(40, 58057, 90), record(41, 58057, 100)];
        let only_41 = |i: &EngineIncarnation| if i.pid == 41 { Liveness::Alive } else { Liveness::Dead };
        assert_eq!(engine_of("http://127.0.0.1:58057", &both, only_41, engine).map(|e| e.pid), Ok(41));
        assert!(engine_of("http://127.0.0.1:58057", &both, alive, engine).is_err(), "two live claimants");
    }
}
