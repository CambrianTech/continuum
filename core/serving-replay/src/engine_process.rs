//! A llama-server the replay launched itself: its command line, its log, readiness, and
//! replacement when it dies. Dropping it stops the engine, so a replay never leaves an
//! engine holding unified memory behind it.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::http::Endpoint;
use crate::target::TargetError;

pub struct EngineProcess {
    argv: Vec<String>,
    log: PathBuf,
    child: Child,
    /// How long the last launch took to answer `/health` with 200 (model loaded).
    pub ready_ms: f64,
}

/// How often readiness is polled while the model loads: a load is seconds, so a quarter
/// second costs at most that much of the measured `ready_ms`.
const READY_POLL: Duration = Duration::from_millis(250);

impl EngineProcess {
    /// Launch `argv` (program first), logging to `log`, and wait until `endpoint` is ready.
    pub fn launch(argv: Vec<String>, log: PathBuf, endpoint: &Endpoint) -> Result<Self, TargetError> {
        if endpoint.get("/health").is_ok() {
            return Err(TargetError(format!("something already answers on {}: pick another --port", endpoint.0)));
        }
        let child = spawn(&argv, &log)?;
        let mut process = Self { argv, log, child, ready_ms: 0.0 };
        process.wait_ready(endpoint)?;
        Ok(process)
    }

    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    /// `Ok(true)` when the engine had exited and was launched again.
    pub fn revive(&mut self, endpoint: &Endpoint) -> Result<bool, TargetError> {
        match self.child.try_wait() {
            Ok(None) => Ok(false),
            Ok(Some(status)) => {
                eprintln!("serving-replay: engine exited ({status}); relaunching, its KV is gone (log {})", self.log.display());
                self.child = spawn(&self.argv, &self.log)?;
                self.wait_ready(endpoint)?;
                Ok(true)
            }
            Err(e) => Err(TargetError(format!("cannot poll the engine process: {e}"))),
        }
    }

    fn wait_ready(&mut self, endpoint: &Endpoint) -> Result<(), TargetError> {
        let started = Instant::now();
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Err(TargetError(format!("engine exited during load ({status}); see {}", self.log.display())));
            }
            if endpoint.get("/health").is_ok_and(|r| r.status == 200) {
                self.ready_ms = started.elapsed().as_secs_f64() * 1e3;
                return Ok(());
            }
            std::thread::sleep(READY_POLL);
        }
    }
}

fn spawn(argv: &[String], log: &Path) -> Result<Child, TargetError> {
    let (program, args) = argv.split_first().ok_or_else(|| TargetError("empty engine command".into()))?;
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|e| TargetError(format!("cannot open engine log {}: {e}", log.display())))?;
    let err = out.try_clone().map_err(|e| TargetError(format!("cannot share engine log: {e}")))?;
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| TargetError(format!("cannot launch {program}: {e}")))
}

impl Drop for EngineProcess {
    fn drop(&mut self) {
        let _ = self.child.kill(); // already exited is fine: the goal is that it is not running
        let _ = self.child.wait(); // reap, so no zombie outlives the replay
    }
}
