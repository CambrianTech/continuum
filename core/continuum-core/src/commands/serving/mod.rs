//! `serving/<verb>` — the VRAM axis of the allocation catalog as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! ## The concern this owns
//!
//! Disk and VRAM are two separate axes of the same model catalog. `models/pull`
//! ↔ `models/remove` is the disk axis (bytes on storage). This module is the VRAM
//! axis (bytes resident in a serving lane): **`serving/load` ↔ `serving/unload`**.
//! Everything allocated can be deallocated — VRAM included, at runtime, without a
//! reboot.
//!
//! ## Why a suppress-set, not a "kill the server" verb
//!
//! The [`ServingDaemonModule`](crate::modules::serving_daemon::ServingDaemonModule)
//! is holistically in charge of VRAM: it converges to a demand+budget-derived
//! plan and reconciles the live llama-server to match. You do not reach past it to
//! kill a process — that would fight the daemon, which would just re-serve on its
//! next tick. Instead the operator edits the daemon's **suppress-set** (the
//! exclude list its planner honors): `serving/unload` adds a model id, so the next
//! plan recompute can no longer pick it and the reconcile drops it (relaunch to
//! the next-best fit, or empty) — the lane frees live. `serving/load` removes the
//! id, permitting the planner to serve it again **when it fits the budget**. Load
//! permits; it never forces — the daemon stays the authority on what actually
//! occupies VRAM. The command only ever subtracts from / restores to the candidate
//! set; the plan + reconcile (owned by the daemon) turn that into a real
//! (un)load. No daemon restart, ever — a suppressed model re-loads by un-pinning,
//! not by rebooting the substrate.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::watch;

use crate::inference::llama_server::ServingSnapshot;
use crate::model_registry::live::ModelCatalog;
use crate::sdk_codegen::DynCommand;

pub mod load;
pub mod unload;

use load::ServingLoad;
use unload::ServingUnload;

/// The dep-holding `serving/*` deallocation pair the
/// [`ServingDaemonModule`](crate::modules::serving_daemon::ServingDaemonModule)
/// contributes to the kernel's typed object map. Both share the daemon's
/// suppress-set writer (the VRAM-axis allocation ledger), its published
/// [`ServingSnapshot`] receiver (to observe the lane actually free / re-fill), and
/// the live [`ModelCatalog`] (to fail loud on an unknown model id rather than pin
/// a typo that silently does nothing). The daemon's own plan + reconcile loop
/// turns the suppress-set edits into actual (un)loads.
pub fn command_objects(
    suppress: watch::Sender<Arc<HashSet<String>>>,
    serving: watch::Receiver<ServingSnapshot>,
    catalog: Arc<ModelCatalog>,
) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(ServingUnload {
            suppress: suppress.clone(),
            serving: serving.clone(),
            catalog: catalog.clone(),
        }),
        Arc::new(ServingLoad {
            suppress,
            serving,
            catalog,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the dep-holding family wires exactly the VRAM-axis
    // deallocation pair. A regression that drops one half (breaking the
    // load↔unload symmetry the catalog requires) is caught.
    #[test]
    fn family_exposes_the_load_unload_pair() {
        let reg = crate::model_registry::catalog::registry().expect("Rust catalog must validate");
        let catalog = Arc::new(ModelCatalog::from_registry(&reg));
        let (suppress, _) = watch::channel(Arc::new(HashSet::new()));
        let (_tx, serving) = watch::channel(ServingSnapshot::empty());
        let objs = command_objects(suppress, serving, catalog);
        let names: Vec<&str> = objs.iter().map(|o| o.name()).collect();
        assert!(names.contains(&"serving/unload"), "the VRAM-axis free verb");
        assert!(
            names.contains(&"serving/load"),
            "its inverse — re-loadable without reboot"
        );
    }
}
