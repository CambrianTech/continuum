//! Where a spawned engine's stderr goes (#175, card 1d7d6d4c): to a relay in the engine's own
//! process group that outlives this core, which this core then follows like any later core
//! would; or, on a host with no handoff, to this core's own capped pump.

use crate::inference::child_log::{self, LineWatch};
use crate::inference::engine_residency::EngineIncarnation;
use crate::inference::stderr_relay::{follow, process};
use continuum_host::output_handoff::Handoff;

use super::LlamaServerProcess;

impl LlamaServerProcess {
    /// The watchers on every line the engine writes, and the flag the debug-build watcher
    /// raises. The sink reads every line to keep the file capped, so it is also the cheapest
    /// place to notice the engine reporting an IMPOSSIBLE state. A slot printing
    /// `progress = 1.10` is wedged; the decode heartbeat can't see it (the OTHER slots still
    /// decode, so the lane reads healthy), which is how one slot burned four hours on
    /// 2026-08-05. The watcher only RAISES; the serving daemon reaps. The same lines carry the
    /// engine's OFFLOAD BANNER, the readback half of the placement contract (#441): the
    /// governor planned a placement, the banner is what the engine actually allocated, and
    /// serve() compares them after readiness.
    pub(super) fn stderr_watch(&self) -> (Box<dyn LineWatch>, crate::inference::debug_build_watch::DebugBuildFlag) {
        let offload_watch = Box::new(crate::inference::placement_watch::OffloadWatch::new(self.offload.clone()));
        let debug_flag = crate::inference::debug_build_watch::DebugBuildFlag::default();
        let watch: Box<dyn LineWatch> = match self.wedge.clone() {
            Some(flag) => Box::new(crate::inference::placement_watch::ChainWatch(
                Box::new(crate::inference::wedge::WedgeWatch::new(flag)),
                offload_watch,
            )),
            None => offload_watch,
        };
        let watch = Box::new(crate::inference::placement_watch::ChainWatch(
            Box::new(crate::inference::debug_build_watch::DebugBuildWatch::new(debug_flag.clone())),
            watch,
        ));
        (watch, debug_flag)
    }

    /// Follow the relay of the engine serving this endpoint, when this core did not spawn it
    /// (an adopted engine): its lines reach the same watchers a spawned one's do. A no-op when
    /// it is already followed or the host has no relay.
    pub(super) fn follow_served_engine(&self) {
        if let Some(engine) = super::LlamaServerControl::served_incarnation(self) {
            follow::follow(engine, self.stderr_watch().0);
        }
    }
}

// derived-or-floor: a CEILING on one local process (this core's own binary) starting and binding a socket; past it the relay has failed and the launch fails loudly. It bounds no serving, token or memory budget
const RELAY_READY_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);

/// Give the engine `pid` serving `port` a reader for `stderr`, with `watch` on every line. A
/// relay counts only once it serves this engine ([`follow::ready`]); an error means nothing
/// reads the engine, the relay is killed and reaped, and the caller must not leave that engine
/// serving.
pub(super) async fn attach(stderr: tokio::process::ChildStderr, pid: u32, port: u16, watch: Box<dyn LineWatch>) -> Result<(), String> {
    let engine = EngineIncarnation::of(pid, port)
        .ok_or_else(|| format!("engine {pid}'s start time is unreadable, so no relay can name it"))?;
    let socket = process::endpoint(port).ok_or("no home directory for the engine's relay")?;
    match process::spawn(stderr, engine).map_err(|e| format!("the engine's log relay did not start: {e}"))? {
        Handoff::Reader(relay) => {
            let mut pending = Pending { relay: Some(relay), watch: Some(watch), engine };
            let readiness = match pending.relay.as_mut() {
                Some(relay) => follow::ready(engine, &socket, relay, RELAY_READY_WITHIN).await,
                None => Err("the relay was already given up".to_string()),
            };
            let (Some(mut relay), Some(watch)) = (pending.relay.take(), pending.watch.take()) else {
                return Err("the relay was already given up".to_string());
            };
            match readiness {
                Ok(ready) => {
                    reap(relay, engine);
                    follow::follow_ready(engine, socket, ready, watch); // the follower runs for the engine's life; a second one is a no-op
                    Ok(())
                }
                Err(reason) => {
                    let _ = relay.start_kill(); // an exited relay refuses the signal; it is reaped either way
                    reap(relay, engine);
                    Err(reason)
                }
            }
        }
        Handoff::Kept(stderr) => {
            let path = child_log::llama_server_log(port).ok_or("no home directory for the engine's log")?;
            child_log::drain_capped(stderr, path, Some(pid), watch);
            Ok(())
        }
    }
}

/// A relay spawned and not yet proven ready. If the launch is cancelled while it waits, the
/// relay is still reaped and followed: the engine it reads is owned and may serve on.
struct Pending {
    relay: Option<tokio::process::Child>,
    watch: Option<Box<dyn LineWatch>>,
    engine: EngineIncarnation,
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let (Some(relay), Some(watch)) = (self.relay.take(), self.watch.take()) {
            reap(relay, self.engine);
            follow::follow(self.engine, watch);
        }
    }
}

/// Wait for the relay this core spawned, so it never lingers as a zombie, and say how it ended:
/// it ends on its own only when every writer of the engine's stderr has. An async wait, never a
/// parked thread: the relay outlives this core by design, and a blocking wait would hold the
/// runtime's shutdown for the engine's whole life (Codex on #5536). A core that exits first drops
/// this task; the relay is then the init process's to reap.
fn reap(mut relay: tokio::process::Child, engine: EngineIncarnation) {
    tokio::spawn(async move {
        let status = relay.wait().await;
        let ended = match &status {
            Ok(status) if status.success() => return,
            Ok(status) => status.to_string(),
            Err(error) => error.to_string(),
        };
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.llama.relay_ended",
            port = engine.port,
            engine_pid = engine.pid,
            ended = ended.as_str(),
            "the engine's log relay ended in failure: its lines are no longer followed"
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Codex on #5536): a relay's reap that parks a thread on its exit. The relay
    // outlives the core by design, and a runtime drop waits for running blocking tasks, so the core
    // could not shut down while any engine lived. The runtime must drop at once with a relay alive.
    // Spawns a stand-in relay process: unix, where the relay runs.
    #[cfg(unix)]
    #[test]
    fn a_reaped_relay_never_holds_the_runtimes_shutdown() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let engine = EngineIncarnation { pid: 4242, started_s: 1_793_000_000, port: 65_005 };
        let pid = runtime.block_on(async {
            let relay = tokio::process::Command::new("sleep").arg("30").spawn().expect("spawn a stand-in relay");
            let pid = relay.id().expect("a running child has a pid");
            reap(relay, engine);
            pid
        });
        // Dropped on a side thread, so a shutdown that waits on the relay fails this test at the
        // bound instead of hanging it.
        let (dropped, shut) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            drop(runtime);
            let _ = dropped.send(());
        });
        let shut_promptly = shut.recv_timeout(std::time::Duration::from_secs(5)).is_ok();
        let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
        assert!(shut_promptly, "the runtime's shutdown waited on a live relay");
    }
}
