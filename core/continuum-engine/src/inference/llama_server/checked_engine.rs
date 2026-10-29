//! The engine a launch runs, as ONE checked artifact (Codex on #5535 at 1dd2baa2b).
//!
//! Holding the node to its pin used to return only a verdict; the launch then re-read `current`
//! for its binary. A deploy promoting another slot between the two launched the replacement
//! unchecked, and a selector removed between them launched the pre-slot install or a bare PATH
//! `llama-server`. The hold now returns the binary it checked, and the launch runs that path and
//! nothing else: there is no second resolution on the launch path.
//!
//! Custody is the slot's ONE claim ([`SlotClaim`], the same exclusive file a build takes in
//! `continuum engine idle-slot`), taken right after the hold (in `prepare`) or, for a retained
//! recipe relaunched, before the transition retires the incumbent. While a build holds the slot
//! the claim is refused (`Unreserved`) and the incumbent keeps serving; while the launch holds
//! it no build can claim it, even after a deploy promotes another slot. The launch drops it once
//! the lane record names the slot (the record is the custody from then on, as for any running
//! lane). [`CheckedEngine::confirm`] is an observation, not custody: the slot still holds the
//! validated stamp. It is read right after claiming and again right before the spawn.

use std::path::{Path, PathBuf};

use super::engine_pin::{pinned_engine, EngineUnpinned, PinHeld};
use crate::inference::engine_slots::{self as slots, SlotClaim};

/// The engine a launch runs, fixed when the pin was held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CheckedEngine {
    /// An engine named outside the slots (an operator's `LLAMA_SERVER_BIN`, a test fixture's):
    /// launched as given, never held to the pin.
    Operator { program: String },
    /// `slot`'s binary under `root`, validated against this core's pin with `stamp`.
    Slot {
        program: String,
        root: PathBuf,
        slot: &'static str,
        stamp: String,
    },
}

impl CheckedEngine {
    /// The artifact for a held pin: the binary of the slot the hold validated (or promoted).
    pub(super) fn held(root: &Path, held: PinHeld) -> Self {
        let (PinHeld::Pinned { slot, stamp } | PinHeld::Promoted { slot, stamp }) = held;
        let program = slots::slot_bin(root, slot).to_string_lossy().into_owned();
        Self::Slot {
            program,
            root: root.to_path_buf(),
            slot,
            stamp,
        }
    }

    /// The binary this launch runs.
    pub(crate) fn program(&self) -> &str {
        match self {
            Self::Operator { program } | Self::Slot { program, .. } => program,
        }
    }

    /// Claim the validated slot, then observe that it still holds the validated build. `None`
    /// for an operator's engine: theirs, not a slot.
    pub(super) fn claim_slot(&self) -> Result<Option<SlotClaim>, EngineUnpinned> {
        let Self::Slot { root, slot, .. } = self else {
            return Ok(None);
        };
        let claim =
            SlotClaim::acquire(root, slot).map_err(|reason| EngineUnpinned::Unreserved {
                pin: pinned_engine().to_string(),
                slot: *slot,
                reason,
            })?;
        self.confirm()?;
        Ok(Some(claim))
    }

    /// Observe that the validated slot still holds the validated stamp. An operator's engine is
    /// theirs: nothing to observe.
    pub(super) fn confirm(&self) -> Result<(), EngineUnpinned> {
        match self {
            Self::Operator { .. } => Ok(()),
            Self::Slot {
                root, slot, stamp, ..
            } => intact(*slot, stamp, slots::slot_stamp(root, slot)),
        }
    }
}

/// PURE: the validated `slot` still holds `stamp`, its validated build.
fn intact(
    slot: &'static str,
    stamp: &str,
    now_stamp: Option<String>,
) -> Result<(), EngineUnpinned> {
    if now_stamp.as_deref() == Some(stamp) {
        return Ok(());
    }
    Err(EngineUnpinned::Rebuilt {
        pin: pinned_engine().to_string(),
        slot,
        stamp: stamp.to_string(),
        now_stamp,
    })
}

#[cfg(test)]
mod tests {
    use super::super::engine_pin::hold_slots_to_pin;
    use super::*;
    use crate::inference::engine_residency::probe_process;

    fn engine(root: &Path, slot: &str, stamp: &str) {
        std::fs::create_dir_all(root.join(slot)).expect("test: slot dir");
        std::fs::write(slots::slot_bin(root, slot), b"bin").expect("test: slot binary");
        std::fs::write(
            root.join(slot).join(slots::STAMP_FILE),
            format!("{stamp}\n"),
        )
        .expect("test: slot stamp");
    }

    // what this catches (Codex on #5535 at 29906ec31, check-then-act between a build and a
    // launch): `idle-slot` printed an unclaimed path, so a builder could select engine-a, pause,
    // and overwrite it after a launch validated it; and the launch re-resolving `current` after a
    // promotion. Both now take the slot's ONE claim. A builder holding engine-a refuses the
    // launch (`Unreserved`, raised in prepare, before any retirement); a launch holding engine-a
    // keeps every build off it (the builder claims engine-c or gets NoIdleSlot) while a promotion
    // and a vanished selector never change the binary it runs; dropping it releases the slot.
    #[test]
    fn a_build_and_a_launch_never_hold_one_slot_and_the_launch_runs_what_it_validated() {
        let pinned = format!("{}:metal", pinned_engine());
        let no_lanes = || Ok(Vec::new());
        let builder = std::process::id(); // a live owner with a readable start time
        for deploy_owns_pointers in [false, true] {
            let dir = tempfile::tempdir().expect("test: tempdir");
            let root = dir.path();
            engine(root, "engine-a", &pinned);
            engine(root, "engine-b", &pinned);
            slots::promote(root, "engine-b", &pinned).expect("test: engine-b current");
            std::fs::write(root.join(slots::exe_name()), b"legacy")
                .expect("test: a pre-slot install");

            // A builder selects AND claims engine-a, then pauses before touching it.
            let built =
                slots::claim_idle_slot(root, &["engine-c"], builder, no_lanes, &probe_process);
            assert!(matches!(built, Ok("engine-a")), "{built:?}");
            slots::promote(root, "engine-a", &pinned).expect("test: engine-a current");
            let checked =
                hold_slots_to_pin(root, deploy_owns_pointers).expect("engine-a holds the pin");
            assert!(
                matches!(checked.claim_slot(), Err(EngineUnpinned::Unreserved { slot: "engine-a", .. })),
                "a launch never claims a slot a build holds (deploy_owns_pointers = {deploy_owns_pointers})"
            );
            slots::release_build_claim(root, "engine-a", builder).expect("test: the builder exits");

            // The reverse: the launch holds engine-a; a deploy promotes engine-b and removes the
            // selector, and no build ever claims engine-a.
            let validated = slots::slot_bin(root, "engine-a")
                .to_string_lossy()
                .into_owned();
            let claim = checked
                .claim_slot()
                .expect("engine-a claimed")
                .expect("a slot engine is claimed");
            slots::promote(root, "engine-b", &pinned).expect("test: a deploy promotes engine-b");
            let built = slots::claim_idle_slot(root, &[], builder, no_lanes, &probe_process);
            assert!(
                matches!(built, Ok("engine-c")),
                "a build takes another slot: {built:?}"
            );
            let built =
                slots::claim_idle_slot(root, &["engine-c"], builder, no_lanes, &probe_process);
            assert!(
                matches!(built, Err(slots::VerbError::NoIdleSlot)),
                "and never the claimed one: {built:?}"
            );
            std::fs::remove_file(root.join("current")).expect("test: the selector vanishes");
            assert_eq!(
                checked.program(),
                validated,
                "never engine-b, the pre-slot install or PATH"
            );
            assert_eq!(
                checked.confirm(),
                Ok(()),
                "the validated build is intact: it launches"
            );

            slots::promote(root, "engine-b", &pinned).expect("test: engine-b current again");
            drop(claim);
            let built =
                slots::claim_idle_slot(root, &["engine-c"], builder, no_lanes, &probe_process);
            assert!(
                matches!(built, Ok("engine-a")),
                "the dropped claim releases the slot: {built:?}"
            );
            slots::release_build_claim(root, "engine-a", builder).expect("test: the builder exits");
            slots::release_build_claim(root, "engine-c", builder).expect("test: the builder exits");
        }
    }
}
