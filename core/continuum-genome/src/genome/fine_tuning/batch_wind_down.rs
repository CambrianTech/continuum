//! The batch trainer's last runs, wound down (card 4114a2c6): a migration, run once at boot.
//!
//! Before in-place learning, a batch job trained on a live engine through its `/train` route and
//! bound itself to that engine's incarnation in the residency record, which holds serving off the
//! engine until the engine acknowledges the run's end or dies. That trainer is gone, so a record
//! a previous core left on a still-live engine has no owner to end it, and serving would stay held
//! for as long as the engine lives. Its examples are not lost: the training trigger's
//! `reclaim_dispatched` returned them to pending, where in-place learning takes them.
//!
//! So each record is ended the one way the record allows: on a live engine, its memory lease is
//! re-taken first (the run's allocation outlived the core that leased it), the engine is told to
//! cancel the run, and the record is released only once the engine's own status says the run is
//! no longer training; on a verifiably dead engine it is released at once. An engine whose
//! liveness cannot be read, or whose status never answers, keeps its record: ownership unknown is
//! held, never guessed away. Idempotent: a second boot finds no record.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::inference::engine_residency::{self, Liveness, ResidentWork};
use crate::resources::ResourceDaemon;

/// How long a cancelled run is given to reach a terminal status before this boot leaves its
/// record held (the next boot asks again). The engine ends a cancelled run at its next batch
/// boundary; the batch trainer waited the same way, without a bound, in `stop_ours`.
const SETTLE: Duration = Duration::from_secs(600);
const POLL: Duration = Duration::from_secs(2);

/// The engine's `/train` status, read only for whether the run is still this record's.
#[derive(Deserialize)]
struct TrainStatus {
    state: String,
    #[serde(default)]
    out: Option<String>,
}

impl TrainStatus {
    fn still_running(&self, work: &ResidentWork) -> bool {
        self.out.as_deref() == Some(work.out.as_str()) && !matches!(self.state.as_str(), "idle" | "done" | "cancelled" | "error")
    }
}

/// Wind down every record the batch trainer left in this home's residency store. The boot runs
/// it in the background and does not wait.
pub async fn wind_down(home: PathBuf, daemon: std::sync::Arc<ResourceDaemon>) {
    let store = engine_residency::store_path(&home);
    let store = store.as_path();
    let bound = match engine_residency::all(store) {
        Ok(bound) => bound,
        Err(error) => {
            continuum_base::probe!(class = "training.batch_wind_down.store_unreadable", error = %error,
                "the batch trainer's engine bindings cannot be read: ownership is unknown, so serving stays held");
            return;
        }
    };
    let client = reqwest::Client::new();
    for work in bound {
        let outcome = end(store, &client, &daemon, &work).await;
        continuum_base::probe!(class = "training.batch_wind_down.run", job = %work.job, engine_pid = work.engine.pid,
            outcome = %outcome, "a batch run a previous core bound to an engine, wound down");
    }
}

async fn end(store: &Path, client: &reqwest::Client, daemon: &std::sync::Arc<ResourceDaemon>, work: &ResidentWork) -> String {
    match work.engine.liveness() {
        Liveness::Dead => return release(store, work, "its engine is gone"),
        Liveness::Unknown => return "held: its engine's liveness cannot be read".into(),
        Liveness::Alive => {}
    }
    // the run's memory is in use until the engine stops it: account for it while it winds down
    let _lease = (work.reserved_bytes > 0).then(|| {
        let consumer = if work.consumer.is_empty() { format!("genome-train:{}", work.job) } else { work.consumer.clone() };
        daemon.acquire_guarded(&crate::inference::training_admission::request(&consumer, work.bound_memory(), work.reserved_bytes))
    });
    let root = work.engine.root_url();
    let since = tokio::time::Instant::now();
    while since.elapsed() < SETTLE {
        let status = client.get(format!("{root}/train")).timeout(Duration::from_secs(10)).send().await;
        let status = match status {
            Ok(reply) if reply.status().is_success() => reply.json::<TrainStatus>().await.ok(),
            _ => None,
        };
        match status {
            Some(status) if !status.still_running(work) => return release(store, work, "its engine no longer trains it"),
            Some(_) => {
                // a refused or lost cancel is asked again; the status decides
                let _ = client.post(format!("{root}/train/cancel")).timeout(Duration::from_secs(10)).send().await;
            }
            None if work.engine.liveness() == Liveness::Dead => return release(store, work, "its engine died while it was being stopped"),
            None => {}
        }
        tokio::time::sleep(POLL).await;
    }
    format!("held: its engine did not report the run ended within {}s; the next boot asks again", SETTLE.as_secs())
}

fn release(store: &Path, work: &ResidentWork, why: &str) -> String {
    match engine_residency::release(store, work) {
        Ok(_) => format!("released: {why}"),
        Err(error) => format!("held: {why}, but its record could not be released ({error})"),
    }
}
