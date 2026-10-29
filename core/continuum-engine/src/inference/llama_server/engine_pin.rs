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
//! it. A slot already holding the pin for this host's verified target (backend and CUDA
//! architectures, [`EngineStamp`]) is promoted, a verified one first. When none exists, or the
//! promote fails, a NEW launch is refused with a typed [`EngineUnpinned`]: the core never starts
//! an engine its source does not pin. An incumbent already serving is not killed for it: the
//! refusal comes before any transition, so its custody and state stand and the reconcile reports
//! the blocked convergence (`serving.engine.convergence_blocked`). Promoting the pointer is not
//! the running engine: the reconcile relaunches the lane onto it, and `deploy-verify` passes only
//! when the LIVE engine's own build is the pin ([`super::engine_report`]). A held pin is returned
//! as the binary it validated ([`CheckedEngine`]), and the launch runs that binary only.

use std::path::Path;

use super::checked_engine::CheckedEngine;

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

/// A slot stamp read as its two halves: the fork commit, and the build target the installer
/// verified on THIS host before writing the stamp (`<backend>[:<cuda archs>]`: `STAMP_WANT` in
/// install-llama-server.sh, which decides the backend and, for CUDA, the architectures from the
/// host's own devices, then stamps only after the built binary reports that backend's devices).
/// The target is the host-compatibility fact: two stamps with the same target are builds of one
/// program for the same hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EngineStamp<'a> {
    pub commit: &'a str,
    pub target: &'a str,
}

impl<'a> EngineStamp<'a> {
    /// `None` for a stamp that names no commit or no target: it says nothing about compatibility.
    pub(crate) fn parse(stamp: &'a str) -> Option<Self> {
        let (commit, target) = stamp.trim().split_once(':')?;
        let (commit, target) = (commit.trim(), target.trim());
        (!commit.is_empty() && !target.is_empty()).then_some(Self { commit, target })
    }

    /// Whether this build is of `pin` (the ONE SHA rule: prefix either way, credible hex).
    pub(crate) fn holds(&self, pin: &str) -> bool {
        continuum_cli_lifecycle::deploy_provenance::sha_matches(self.commit, pin)
    }
}

/// What holding the node to its pin decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PinChoice {
    /// The current slot already holds the pinned engine: `slot`, stamped `stamp`.
    Pinned { slot: &'static str, stamp: String },
    /// Make this slot current: it holds the pin for exactly the current engine's target.
    Promote { slot: &'static str, stamp: String },
    /// No slot holds the pin for the current target: the node serves on `current` and says so.
    Unpinned { current: String },
    /// The current slot has no readable stamp, so this host's verified target is unknown and no
    /// slot can be shown to fit it: nothing is promoted.
    Untargeted { current: Option<String> },
}

/// PURE: hold `current` to `pin`. A candidate must carry exactly the current slot's verified
/// target (backend AND CUDA architectures: a pin is never a reason to move a CUDA node to CPU, or
/// an sm_120 node onto an sm_86 build), and a verified slot wins over an unproven one. With no
/// readable current target there is no compatibility evidence, so nothing is promoted.
pub(crate) fn choose(current: Option<&'static str>, slots: &[SlotReading], pin: &str) -> PinChoice {
    let current_reading = current.and_then(|name| slots.iter().find(|s| s.name == name));
    let current_stamp = current_reading.and_then(|s| s.stamp.as_deref());
    let Some((reading, stamp, target)) =
        current_reading.zip(current_stamp).and_then(|(reading, stamp)| Some((reading, stamp, EngineStamp::parse(stamp)?)))
    else {
        return PinChoice::Untargeted { current: current_stamp.map(str::to_string) };
    };
    if target.holds(pin) {
        return PinChoice::Pinned { slot: reading.name, stamp: stamp.to_string() };
    }
    let candidate = slots
        .iter()
        .filter(|s| Some(s.name) != current)
        .filter_map(|s| {
            let stamp = s.stamp.as_deref()?;
            let parsed = EngineStamp::parse(stamp)?;
            (parsed.holds(pin) && parsed.target == target.target).then_some((s, stamp))
        })
        .max_by_key(|(s, _)| s.verified);
    match candidate {
        Some((slot, stamp)) => PinChoice::Promote { slot: slot.name, stamp: stamp.to_string() },
        None => PinChoice::Unpinned { current: stamp.to_string() },
    }
}

/// The last verdict probed, so a reconcile that asks every tick speaks only on a change.
static LAST: parking_lot::Mutex<Option<Result<PinHeld, EngineUnpinned>>> = parking_lot::Mutex::new(None);

/// Why no owned engine may launch now: no slot can be shown to hold this core's pin for this
/// host. Typed so the reconcile keeps an incumbent on it instead of reading a launch failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineUnpinned {
    /// The continuum home or its engine payload root could not be read: no owned slot exists.
    #[error("no engine slots to hold to pin {pin}: {reason}")]
    NoSlots { pin: String, reason: String },
    /// No slot holds the pin for the current slot's target.
    #[error("no engine slot holds pin {pin} for this host's target (current {current}); `continuum install` builds it")]
    Unpinned { pin: String, current: String },
    /// The current slot has no readable target, so no slot can be shown to fit this host.
    #[error("the current engine slot has no readable target (stamp {current:?}), so no slot can be shown to hold pin {pin} for this host")]
    Untargeted { pin: String, current: Option<String> },
    /// A slot holds the pin, but promoting it failed: `current` still names another engine.
    #[error("slot {slot} holds pin {pin} ({stamp}) but promoting it failed: {reason}")]
    PromoteRefused { pin: String, slot: &'static str, stamp: String, reason: String },
    /// A deploy holds the claim and owns the slot pointers, and the current slot is not the pin.
    #[error("a deploy owns the engine slot pointers and the current slot does not hold pin {pin}: no launch until it finishes")]
    DeployInFlight { pin: String },
    /// The slot validated for this launch no longer holds the validated build (a build chose it
    /// before the launch reserved it): the launch refuses rather than run what nobody checked.
    #[error("engine slot {slot} was validated against pin {pin} as {stamp} but now holds {now_stamp:?}: nothing launches; the next reconcile validates the slots again")]
    Rebuilt { pin: String, slot: &'static str, stamp: String, now_stamp: Option<String> },
    /// The validated slot could not be claimed (a build holds it, or the claim could not be
    /// written): nothing launches and the incumbent keeps serving.
    #[error("engine slot {slot} (pin {pin}) could not be claimed for the launch: {reason}")]
    Unreserved { pin: String, slot: &'static str, reason: String },
}

/// What holding the node to its pin settled, when a launch may proceed: the slot that holds it
/// and the stamp it was validated with ([`super::checked_engine::CheckedEngine`] carries both
/// to the spawn).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PinHeld {
    /// The current slot holds the pin.
    Pinned { slot: &'static str, stamp: String },
    /// A slot holding the pin was made current now.
    Promoted { slot: &'static str, stamp: String },
}

/// PURE: settle a [`PinChoice`] into a launch decision. `deploy_owns_pointers` forbids moving
/// a pointer (a deploy is mid-promote) but still lets an already-pinned slot launch: a deploy
/// builds only into an idle slot, never the current one, and a promotion it makes before the
/// spawn is refused by the launch's custody check ([`super::checked_engine`]), so it cannot
/// change what launches. `promote` runs only for a `Promote` choice outside a deploy.
pub(crate) fn settle(
    choice: &PinChoice,
    pin: &str,
    deploy_owns_pointers: bool,
    promote: impl FnOnce(&'static str, &str) -> Result<(), String>,
) -> Result<PinHeld, EngineUnpinned> {
    let pin = pin.to_string();
    match choice {
        PinChoice::Pinned { slot, stamp } => Ok(PinHeld::Pinned { slot: *slot, stamp: stamp.clone() }),
        _ if deploy_owns_pointers => Err(EngineUnpinned::DeployInFlight { pin }),
        PinChoice::Promote { slot, stamp } => promote(slot, stamp)
            .map(|()| PinHeld::Promoted { slot: *slot, stamp: stamp.clone() })
            .map_err(|reason| EngineUnpinned::PromoteRefused { pin, slot: *slot, stamp: stamp.clone(), reason }),
        PinChoice::Unpinned { current } => Err(EngineUnpinned::Unpinned { pin, current: current.clone() }),
        PinChoice::Untargeted { current } => Err(EngineUnpinned::Untargeted { pin, current: current.clone() }),
    }
}

/// Hold the current engine slot to this core's pin before a launch (and before the reconcile
/// compares a running lane with the installed engine), and return the engine the launch runs:
/// the binary checked here, never one re-resolved from `current` at spawn. Never moves a pointer
/// under an operator override (their engine) or while a deploy holds the claim (it owns the
/// pointers mid-promote). `Err` means no owned engine may launch now.
pub(super) fn hold_current_engine_to_pin() -> Result<CheckedEngine, EngineUnpinned> {
    if let Some(program) = super::engine_install::engine_override() {
        return Ok(CheckedEngine::Operator { program });
    }
    let pin = pinned_engine();
    let no_slots = |reason: String| EngineUnpinned::NoSlots { pin: pin.to_string(), reason };
    let home = crate::paths::continuum_home().map_err(|e| no_slots(format!("no continuum home: {e}")))?;
    let root = crate::inference::engine_slots::root(&home).map_err(|e| no_slots(format!("no engine payload root: {e}")))?;
    let now_ms = continuum_base::time::EpochMs::now().millis(); // pre-1970 reads 0: as in the rollback guard
    let deploy_owns_pointers = continuum_cli_lifecycle::deploy_claim::in_flight(&home, now_ms).blocks();
    hold_slots_to_pin(&root, deploy_owns_pointers)
}

/// Hold the slots under `root` to this core's pin (see [`hold_current_engine_to_pin`]); each
/// verdict is probed once per change.
pub(super) fn hold_slots_to_pin(root: &Path, deploy_owns_pointers: bool) -> Result<CheckedEngine, EngineUnpinned> {
    use crate::inference::engine_slots::{self as slots, SLOTS};
    let pin = pinned_engine();
    let readings: Vec<SlotReading> = SLOTS
        .iter()
        .map(|&name| SlotReading { name, stamp: slots::slot_stamp(root, name), verified: slots::is_verified(root, name) })
        .collect();
    let current = slots::current_slot(root);
    let choice = choose(current, &readings, pin);
    let held = settle(&choice, pin, deploy_owns_pointers, |slot, stamp| slots::promote(root, slot, stamp).map(|_| ()));
    let changed = LAST.lock().as_ref() != Some(&held);
    if changed {
        report(&held, current);
    }
    *LAST.lock() = Some(held.clone());
    held.map(|held| CheckedEngine::held(root, held))
}

/// Say one settled verdict (only on a change, so a reconcile that asks every tick is quiet).
fn report(held: &Result<PinHeld, EngineUnpinned>, current: Option<&'static str>) {
    let slot = current.unwrap_or(""); // probe field: no current slot reads empty
    match held {
        Ok(PinHeld::Pinned { .. }) => continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.pinned",
            slot,
            pin = pinned_engine(),
            "the current engine slot runs the engine this core pins"
        ),
        Ok(PinHeld::Promoted { .. }) => continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.repinned",
            from = slot,
            pin = pinned_engine(),
            "the current engine slot did not run this core's pinned engine: a slot that does is \
             current now, and the next launch or convergence runs it"
        ),
        Err(refusal) => {
            let class = match refusal {
                EngineUnpinned::NoSlots { .. } => "serving.engine.no_slots",
                EngineUnpinned::Unpinned { .. } => "serving.engine.unpinned",
                EngineUnpinned::Untargeted { .. } => "serving.engine.untargeted",
                EngineUnpinned::PromoteRefused { .. } => "serving.engine.repin_refused",
                EngineUnpinned::DeployInFlight { .. } => "serving.engine.pin_deferred_to_deploy",
                EngineUnpinned::Rebuilt { .. } => "serving.engine.slot_rebuilt",
                EngineUnpinned::Unreserved { .. } => "serving.engine.slot_unreserved",
            };
            let why = refusal.to_string();
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = class,
                slot,
                pin = pinned_engine(),
                why = why.as_str(),
                "no owned engine launches until a slot holding this core's pin is current; an \
                 incumbent already serving keeps serving"
            );
        }
    }
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
        assert_eq!(choose(Some("engine-c"), &m5, PIN), PinChoice::Pinned { slot: "engine-c", stamp: "741b0175b:metal".into() });

        let other_backend = [slot("engine-a", "e7e1077b3:metal", true), slot("engine-b", "741b0175b:cpu", true)];
        assert_eq!(choose(Some("engine-a"), &other_backend, PIN), PinChoice::Unpinned { current: "e7e1077b3:metal".into() });

        let proven_first = [
            slot("engine-a", "e7e1077b3:metal", true),
            slot("engine-b", "741b0175b:metal", false),
            slot("engine-c", "741b0175b:metal", true),
        ];
        assert_eq!(choose(Some("engine-a"), &proven_first, PIN), PinChoice::Promote { slot: "engine-c", stamp: "741b0175b:metal".into() });

        let none = [slot("engine-a", "e7e1077b3:metal", true)];
        assert_eq!(choose(Some("engine-a"), &none, PIN), PinChoice::Unpinned { current: "e7e1077b3:metal".into() });
    }

    // what this catches (Codex on #5535): a CUDA pin build for other GPU architectures promoted
    // onto this node because only the backend word was compared (sm_86 kernels on an sm_120
    // card), and a missing current stamp read as "any backend will do".
    #[test]
    fn a_candidate_must_fit_the_current_target_and_a_missing_target_promotes_nothing() {
        let cuda = [
            slot("engine-a", "e7e1077b3:cuda:120", true),
            slot("engine-b", "741b0175b:cuda:86", true),
            slot("engine-c", "741b0175b:cuda:120", false),
        ];
        assert_eq!(choose(Some("engine-a"), &cuda, PIN), PinChoice::Promote { slot: "engine-c", stamp: "741b0175b:cuda:120".into() });
        let other_arch = [slot("engine-a", "e7e1077b3:cuda:120", true), slot("engine-b", "741b0175b:cuda:86", true)];
        assert_eq!(choose(Some("engine-a"), &other_arch, PIN), PinChoice::Unpinned { current: "e7e1077b3:cuda:120".into() });

        let pinned_elsewhere = slot("engine-b", "741b0175b:metal", true);
        let unstamped = [SlotReading { name: "engine-a", stamp: None, verified: true }, pinned_elsewhere.clone()];
        assert_eq!(choose(Some("engine-a"), &unstamped, PIN), PinChoice::Untargeted { current: None });
        let no_target = [slot("engine-a", "e7e1077b3", true), pinned_elsewhere.clone()];
        assert_eq!(choose(Some("engine-a"), &no_target, PIN), PinChoice::Untargeted { current: Some("e7e1077b3".into()) });
        assert_eq!(choose(None, &[pinned_elsewhere], PIN), PinChoice::Untargeted { current: None });
    }

    // what this catches (Codex on #5535): a node with no slot holding its pin, an unreadable
    // target, or a failed promote returning to `prepare` as if held, which then launched the
    // stale `current` binary. Each refuses; only a pinned or promoted slot launches.
    #[test]
    fn a_launch_proceeds_only_on_a_slot_holding_the_pin() {
        // A promote that ran would surface as `PromoteRefused`, so each verdict below proves it did not.
        let never = |_: &'static str, _: &str| -> Result<(), String> { Err("a promote ran".into()) };
        let pinned = PinChoice::Pinned { slot: "engine-a", stamp: "741b0175b:metal".into() };
        let held = Ok(PinHeld::Pinned { slot: "engine-a", stamp: "741b0175b:metal".into() });
        assert_eq!(settle(&pinned, PIN, false, never), held);
        assert!(matches!(
            settle(&PinChoice::Unpinned { current: "e7e1077b3:metal".into() }, PIN, false, never),
            Err(EngineUnpinned::Unpinned { .. })
        ), "missing pin");
        assert!(matches!(
            settle(&PinChoice::Untargeted { current: None }, PIN, false, never),
            Err(EngineUnpinned::Untargeted { .. })
        ), "unknown target");

        let promote = PinChoice::Promote { slot: "engine-c", stamp: "741b0175b:metal".into() };
        assert!(matches!(
            settle(&promote, PIN, false, |_, _| Err("rename refused".into())),
            Err(EngineUnpinned::PromoteRefused { slot: "engine-c", .. })
        ), "promotion failure");
        assert_eq!(
            settle(&promote, PIN, false, |_, _| Ok(())),
            Ok(PinHeld::Promoted { slot: "engine-c", stamp: "741b0175b:metal".into() })
        );

        // A deploy owns the pointers: nothing is promoted, and only an already-pinned slot is
        // validated (its custody at spawn: `checked_engine`'s interleaving test).
        assert!(matches!(settle(&promote, PIN, true, never), Err(EngineUnpinned::DeployInFlight { .. })));
        assert_eq!(settle(&pinned, PIN, true, never), held);
    }
}
