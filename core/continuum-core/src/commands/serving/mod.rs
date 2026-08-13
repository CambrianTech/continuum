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

use crate::cognition::serving_plan::ServingPlan;
use crate::inference::llama_server::ServingSnapshot;
use crate::model_registry::live::ModelCatalog;
use crate::modules::serving_daemon::PinFitChecker;
use crate::sdk_codegen::DynCommand;

pub mod load;
pub mod pin;
pub mod plan;
pub mod status;
pub mod unload;
pub mod unpin;

use load::ServingLoad;
use pin::ServingPin;
use plan::ServingPlanQuery;
use status::ServingStatus;
use unload::ServingUnload;
use unpin::ServingUnpin;

/// The dep-holding `serving/*` family the
/// [`ServingDaemonModule`](crate::modules::serving_daemon::ServingDaemonModule)
/// contributes to the kernel's typed object map. Two axes:
///
/// - **VRAM-axis deallocation pair** (`serving/unload` ↔ `serving/load`): share the
///   daemon's suppress-set writer (the VRAM-axis allocation ledger), its published
///   [`ServingSnapshot`] receiver (to observe the lane actually free / re-fill), and
///   the live [`ModelCatalog`] (to fail loud on an unknown model id rather than pin
///   a typo that silently does nothing). The daemon's own plan + reconcile loop
///   turns the suppress-set edits into actual (un)loads.
/// - **VRAM-axis force pair** (`serving/pin` ↔ `serving/unpin`): the promote/demote
///   mechanism. Share the daemon's force-pin writer (intersect-to-one, the dual of
///   the suppress-set's subtract) plus its [`PinFitChecker`] so `serving/pin` can
///   refuse loud BEFORE pinning when the model won't fit a lane — never a silent
///   best-fit fallback. The daemon's plan + reconcile loop turns a set pin into the
///   actual swap.
/// - **Read surfaces** (`serving/plan` + `serving/status`): cheap `watch` borrows of
///   the daemon's published plan (intent) and snapshot (reality), so personas and
///   operators inspect the decision without probing the process.
pub fn command_objects(
    suppress: watch::Sender<Arc<HashSet<String>>>,
    pin: watch::Sender<Option<String>>,
    fit: PinFitChecker,
    serving: watch::Receiver<ServingSnapshot>,
    plan: watch::Receiver<Option<ServingPlan>>,
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
            serving: serving.clone(),
            catalog: catalog.clone(),
        }),
        Arc::new(ServingPin {
            pin: pin.clone(),
            fit,
            catalog,
            serving: serving.clone(),
        }),
        Arc::new(ServingUnpin { pin }),
        Arc::new(ServingStatus { serving }),
        Arc::new(ServingPlanQuery { plan }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the dep-holding family wires the full serving surface —
    // the VRAM-axis deallocation pair (load↔unload), the force pair (pin↔unpin =
    // promote/demote), PLUS the two read surfaces (plan = intent, status =
    // reality). A regression that drops any of the six is caught.
    #[test]
    fn family_exposes_the_full_serving_surface() {
        let reg = crate::model_registry::catalog::registry().expect("Rust catalog must validate");
        let catalog = Arc::new(ModelCatalog::from_registry(&reg));
        let (suppress, _) = watch::channel(Arc::new(HashSet::new()));
        let (pin, _) = watch::channel(None);
        let fit: PinFitChecker = Arc::new(|_m| crate::modules::serving_daemon::PinFit {
            plan: None,
            weights_bytes: 0,
            budget_bytes: 0,
        });
        let (_tx, serving) = watch::channel(ServingSnapshot::empty());
        let (_ptx, plan) = watch::channel(None);
        let objs = command_objects(suppress, pin, fit, serving, plan, catalog);
        let names: Vec<&str> = objs.iter().map(|o| o.name()).collect();
        assert!(names.contains(&"serving/unload"), "the VRAM-axis free verb");
        assert!(
            names.contains(&"serving/load"),
            "its inverse — re-loadable without reboot"
        );
        assert!(
            names.contains(&"serving/pin"),
            "the force-serve verb (promote/demote)"
        );
        assert!(
            names.contains(&"serving/unpin"),
            "its inverse — release to autonomic"
        );
        assert!(
            names.contains(&"serving/status"),
            "the reality read surface"
        );
        assert!(names.contains(&"serving/plan"), "the intent read surface");
    }
}
