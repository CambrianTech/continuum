//! The engine this core was built against, and the rule that holds a node to it (card a3e92354).
//!
//! Before this rule, nothing tied the engine a core ran to the engine its own source pins. Every
//! launch took the slot `current` named. The M5 ran fork e7e1077b3 from 14:05Z to 16:38Z on
//! 2026-10-27 under cores that pin 741b0175b: a worktree `install --core` built the main
//! checkout's older pin and promoted it, and two `reboot --prebuilt` deploys never looked.
//! That engine saved pages with only their head checkpoint. A restore that matched 151,069 of
//! 154,040 tokens resumed at 11,159 and re-prefilled ~140k per resident.
//!
//! The core knows its pin: `vendor/LLAMA_CPP_PIN` is compiled in. Before every launch, and before
//! the reconcile compares a running lane with the installed engine, the current slot is held to
//! it. A slot already holding the pin on the same backend is promoted, a verified one first.
//! When none exists, the node keeps serving on what it has and says so loudly
//! (`serving.engine.unpinned`), because a mute resident is worse than a slow one. `deploy-verify`
//! reads the same fact from `ping` and fails the deploy.

/// The fork commit this tree pins, compiled in from the one file a PR edits to move it.
pub(crate) const PINNED_ENGINE: &str = include_str!("../../../../vendor/LLAMA_CPP_PIN");

/// The pinned commit, trimmed.
pub fn pinned_engine() -> &'static str {
    PINNED_ENGINE.trim()
}

/// One slot as read from disk: its name, its stamp (`<commit>:<backend>[:<arch>]`), and whether
/// a launch from it has come up ready on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SlotReading {
    pub name: &'static str,
    pub stamp: Option<String>,
    pub verified: bool,
}

/// What holding the node to its pin decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PinChoice {
    /// The current slot already runs the pinned engine.
    Pinned,
    /// Make this slot current: it holds the pin on the current engine's backend.
    Promote { slot: &'static str, stamp: String },
    /// No slot holds the pin on this backend: the node serves on `current` and says so.
    Unpinned { current: Option<String> },
}

fn commit(stamp: &str) -> &str {
    stamp.split(':').next().unwrap_or(stamp).trim() // unwrap_or: split always yields one piece
}

fn backend(stamp: &str) -> Option<&str> {
    stamp.split(':').nth(1).map(str::trim)
}

fn holds(stamp: &str, pin: &str) -> bool {
    continuum_cli_lifecycle::deploy_provenance::sha_matches(commit(stamp), pin)
}

/// PURE: hold `current` to `pin`. A candidate must share the current slot's backend (a pin is
/// never a reason to move a CUDA node to CPU), and a verified slot wins over an unproven one.
pub(crate) fn choose(current: Option<&'static str>, slots: &[SlotReading], pin: &str) -> PinChoice {
    let current_stamp = current.and_then(|name| slots.iter().find(|s| s.name == name)).and_then(|s| s.stamp.clone());
    if current_stamp.as_deref().is_some_and(|stamp| holds(stamp, pin)) {
        return PinChoice::Pinned;
    }
    let want_backend = current_stamp.as_deref().and_then(backend);
    let candidate = slots
        .iter()
        .filter(|s| Some(s.name) != current)
        .filter_map(|s| s.stamp.as_deref().map(|stamp| (s, stamp)))
        .filter(|(_, stamp)| holds(stamp, pin))
        .filter(|(_, stamp)| want_backend.map_or(true, |want| backend(stamp) == Some(want))) // map_or: no current stamp constrains no backend
        .max_by_key(|(s, _)| s.verified);
    match candidate {
        Some((slot, stamp)) => PinChoice::Promote { slot: slot.name, stamp: stamp.to_string() },
        None => PinChoice::Unpinned { current: current_stamp },
    }
}

/// The last verdict probed, so a reconcile that asks every tick speaks only on a change.
static LAST: parking_lot::Mutex<Option<PinChoice>> = parking_lot::Mutex::new(None);

/// Hold the current engine slot to this core's pin. Never under an operator override (their
/// engine) and never while a deploy holds the claim (it owns the slot pointers mid-promote).
pub(super) fn hold_current_engine_to_pin() {
    use crate::inference::engine_slots::{self as slots, SLOTS};
    if super::engine_install::engine_override().is_some() {
        return;
    }
    let Ok(home) = crate::paths::continuum_home() else {
        return; // no home: no slots
    };
    let now_ms = continuum_base::time::EpochMs::now().millis(); // pre-1970 reads 0: as in the rollback guard
    if continuum_cli_lifecycle::deploy_claim::in_flight(&home, now_ms).blocks() {
        return;
    }
    let Ok(root) = slots::root(&home) else {
        return; // no payload root: the pre-slot install, nothing to hold
    };
    let readings: Vec<SlotReading> = SLOTS
        .iter()
        .map(|&name| SlotReading { name, stamp: slots::slot_stamp(&root, name), verified: slots::is_verified(&root, name) })
        .collect();
    let current = slots::current_slot(&root);
    let choice = choose(current, &readings, pinned_engine());
    let changed = LAST.lock().as_ref() != Some(&choice);
    match &choice {
        PinChoice::Pinned => {
            if changed {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.pinned",
                    slot = current.unwrap_or(""), // probe field: Pinned implies a current slot
                    pin = pinned_engine(),
                    "the current engine slot runs the engine this core pins"
                );
            }
        }
        PinChoice::Promote { slot, stamp } => match slots::promote(&root, slot, stamp) {
            Ok(_) => continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.repinned",
                from = current.unwrap_or(""), // probe field: no current slot reads empty
                to = *slot,
                stamp = stamp.as_str(),
                pin = pinned_engine(),
                "the current engine slot did not run this core's pinned engine: a slot that does is \
                 current now, and the next launch or convergence runs it"
            ),
            Err(reason) => continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.repin_refused",
                to = *slot,
                stamp = stamp.as_str(),
                reason = reason.as_str(),
                "a slot holds this core's pinned engine but could not be promoted: the node serves on \
                 an engine its source does not pin"
            ),
        },
        PinChoice::Unpinned { current: stamp } => {
            if changed {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.unpinned",
                    current = stamp.as_deref().unwrap_or(""), // probe field: no stamp reads empty
                    pin = pinned_engine(),
                    "no engine slot holds this core's pinned engine on this backend: the node serves on \
                     an engine its source does not pin until `continuum install` builds it"
                );
            }
        }
    }
    *LAST.lock() = Some(choice);
}

/// The current slot's stamp, for `ping` and `deploy-verify`: `None` under an operator override
/// or with no owned install.
pub fn current_engine_stamp() -> Option<String> {
    super::engine_install::installed_engine_stamp()
}

/// Whether `stamp` names this core's pinned engine.
pub fn stamp_holds_pin(stamp: &str) -> bool {
    holds(stamp, pinned_engine())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIN: &str = "741b0175b9a728530b7dcccaa2b6a039c47e5d9e";

    fn slot(name: &'static str, stamp: &str, verified: bool) -> SlotReading {
        SlotReading { name, stamp: Some(stamp.to_string()), verified }
    }

    // what this catches: the M5 on 2026-10-27, current = engine-a at a stale e7e1077b3 while
    // engine-c held the pin; and the near misses (a CPU build of the pin on a Metal node, an
    // unproven slot chosen over a verified one, a node with no slot holding the pin).
    #[test]
    fn the_current_slot_is_held_to_the_pin_on_its_own_backend() {
        let m5 = [
            slot("engine-a", "e7e1077b3:metal", true),
            slot("engine-b", "f291f6e7f:metal", true),
            slot("engine-c", "741b0175b:metal", true),
        ];
        assert_eq!(choose(Some("engine-a"), &m5, PIN), PinChoice::Promote { slot: "engine-c", stamp: "741b0175b:metal".into() });
        assert_eq!(choose(Some("engine-c"), &m5, PIN), PinChoice::Pinned);

        let other_backend = [slot("engine-a", "e7e1077b3:metal", true), slot("engine-b", "741b0175b:cpu", true)];
        assert_eq!(choose(Some("engine-a"), &other_backend, PIN), PinChoice::Unpinned { current: Some("e7e1077b3:metal".into()) });

        let proven_first = [
            slot("engine-a", "e7e1077b3:metal", true),
            slot("engine-b", "741b0175b:metal", false),
            slot("engine-c", "741b0175b:metal", true),
        ];
        assert_eq!(choose(Some("engine-a"), &proven_first, PIN), PinChoice::Promote { slot: "engine-c", stamp: "741b0175b:metal".into() });

        let cuda = [slot("engine-a", "e7e1077b3:cuda:120", true), slot("engine-b", "741b0175b:cuda:120", false)];
        assert_eq!(choose(Some("engine-a"), &cuda, PIN), PinChoice::Promote { slot: "engine-b", stamp: "741b0175b:cuda:120".into() });

        let none = [slot("engine-a", "e7e1077b3:metal", true)];
        assert_eq!(choose(Some("engine-a"), &none, PIN), PinChoice::Unpinned { current: Some("e7e1077b3:metal".into()) });
    }
}
