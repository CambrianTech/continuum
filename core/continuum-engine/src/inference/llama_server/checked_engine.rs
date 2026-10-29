//! The engine a launch runs, as ONE checked artifact (Codex on #5535 at 1dd2baa2b).
//!
//! Holding the node to its pin used to return only a verdict; the launch then re-read `current`
//! for its binary. A deploy promoting another slot between the two launched the replacement
//! unchecked, and a selector removed between them launched the pre-slot install or a bare PATH
//! `llama-server`. The hold now returns the binary it checked, and the launch runs that path and
//! nothing else: there is no second resolution on the launch path.
//!
//! Custody until the spawned engine owns the slot. Once spawned, the lane record names the
//! binary, and `engine_slots::idle_slot` never offers a slot a live lane runs to a build. Before
//! that, the slot's protection is the other half of the same rule: `idle_slot` never offers the
//! CURRENT slot. [`CheckedEngine::confirm`] re-reads that protection at the launch's brackets,
//! beside the install receipt's own confirm (before the transition, and right before the spawn):
//! the validated slot must still be current with the validated stamp. A promotion, a vanished
//! selector or a rewritten stamp refuses with [`EngineUnpinned::Superseded`]; the replacement is
//! never launched, and the next reconcile validates the new selection. After the last confirm,
//! rebuilding the slot needs a promotion away from it AND a further engine build that chooses it,
//! while the lane record lands as the child spawns.

use std::path::{Path, PathBuf};

use super::engine_pin::{pinned_engine, EngineUnpinned, PinHeld};
use crate::inference::engine_slots as slots;

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

    /// Whether the validated slot is still protected (current, with the validated stamp). An
    /// operator's engine is theirs: nothing to confirm.
    pub(super) fn confirm(&self) -> Result<(), EngineUnpinned> {
        match self {
            Self::Operator { .. } => Ok(()),
            Self::Slot {
                root, slot, stamp, ..
            } => custody(
                *slot,
                stamp,
                slots::current_slot(root),
                slots::slot_stamp(root, slot),
            ),
        }
    }
}

/// PURE: the validated `slot` (stamped `stamp`) is still in the custody `current` gives it.
fn custody(
    slot: &'static str,
    stamp: &str,
    current: Option<&'static str>,
    now_stamp: Option<String>,
) -> Result<(), EngineUnpinned> {
    if current == Some(slot) && now_stamp.as_deref() == Some(stamp) {
        return Ok(());
    }
    Err(EngineUnpinned::Superseded {
        pin: pinned_engine().to_string(),
        slot,
        stamp: stamp.to_string(),
        current,
        now_stamp,
    })
}

#[cfg(test)]
mod tests {
    use super::super::engine_pin::hold_slots_to_pin;
    use super::*;

    fn engine(root: &Path, slot: &str, stamp: &str) {
        std::fs::create_dir_all(root.join(slot)).expect("test: slot dir");
        std::fs::write(slots::slot_bin(root, slot), b"bin").expect("test: slot binary");
        std::fs::write(
            root.join(slot).join(slots::STAMP_FILE),
            format!("{stamp}\n"),
        )
        .expect("test: slot stamp");
    }

    // what this catches (Codex on #5535 at 1dd2baa2b, the check-then-use race): the pin held on
    // engine-a, then a deploy promotes engine-b (or the selector vanishes, or engine-a is
    // rebuilt) before the spawn, and the launch re-resolves `current` and runs engine-b, the
    // pre-slot install or PATH unchecked. The artifact's program stays engine-a's binary, and
    // its custody refuses once engine-a is no longer current with the validated stamp; with a
    // deploy holding the claim (the kept pinned-slot exception) the same promotion still cannot
    // change what launches.
    #[test]
    fn a_selection_moved_after_validation_never_changes_what_launches() {
        let pinned = format!("{}:metal", pinned_engine());
        for deploy_owns_pointers in [false, true] {
            let dir = tempfile::tempdir().expect("test: tempdir");
            let root = dir.path();
            engine(root, "engine-a", &pinned);
            engine(root, "engine-b", &pinned);
            slots::promote(root, "engine-a", &pinned).expect("test: engine-a current");
            std::fs::write(root.join(slots::exe_name()), b"legacy")
                .expect("test: a pre-slot install");

            let checked =
                hold_slots_to_pin(root, deploy_owns_pointers).expect("engine-a holds the pin");
            let validated = slots::slot_bin(root, "engine-a")
                .to_string_lossy()
                .into_owned();
            assert_eq!(checked.program(), validated);
            assert_eq!(
                checked.confirm(),
                Ok(()),
                "nothing moved: the validated slot launches"
            );

            slots::promote(root, "engine-b", &pinned).expect("test: a deploy promotes engine-b");
            assert_eq!(
                checked.program(),
                validated,
                "the promotion never redirects the launch"
            );
            assert!(
                matches!(
                    checked.confirm(),
                    Err(EngineUnpinned::Superseded {
                        slot: "engine-a",
                        current: Some("engine-b"),
                        ..
                    })
                ),
                "the unprotected slot refuses (deploy_owns_pointers = {deploy_owns_pointers})"
            );

            slots::promote(root, "engine-a", &pinned).expect("test: engine-a current again");
            std::fs::remove_file(root.join("current")).expect("test: the selector vanishes");
            assert_eq!(
                checked.program(),
                validated,
                "never the pre-slot install or PATH"
            );
            assert!(matches!(
                checked.confirm(),
                Err(EngineUnpinned::Superseded { current: None, .. })
            ));

            slots::promote(root, "engine-a", &pinned).expect("test: engine-a current again");
            engine(root, "engine-a", "e7e1077b3:metal");
            assert!(
                matches!(
                    checked.confirm(),
                    Err(EngineUnpinned::Superseded { now_stamp: Some(ref s), .. }) if s == "e7e1077b3:metal"
                ),
                "a slot rebuilt under the launch refuses"
            );
        }
    }
}
