//! The engine a launch runs, as ONE checked artifact (Codex on #5535 at 1dd2baa2b).
//!
//! Holding the node to its pin used to return only a verdict; the launch then re-read `current`
//! for its binary. A deploy promoting another slot between the two launched the replacement
//! unchecked, and a selector removed between them launched the pre-slot install or a bare PATH
//! `llama-server`. The hold now returns the binary it checked, and the launch runs that path and
//! nothing else: there is no second resolution on the launch path.
//!
//! Custody is a [`SlotReservation`] in the slot owner, taken right after the hold (in `prepare`)
//! or, for a retained recipe relaunched, before the transition retires the incumbent. Every
//! builder's idle-slot selection honors it, so no build can choose the slot while the launch
//! holds it, even after a deploy promotes another slot. The launch drops it once the lane
//! record names the slot (the record is the custody from then on, as for any running lane).
//! [`CheckedEngine::confirm`] is an observation, not custody: the slot still holds the
//! validated stamp. It is read right after reserving (a build that chose the slot first has
//! already removed its stamp) and again right before the spawn.

use std::path::{Path, PathBuf};

use super::engine_pin::{pinned_engine, EngineUnpinned, PinHeld};
use crate::inference::engine_slots::{self as slots, SlotReservation};

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

    /// Take custody of the validated slot, then observe that it still holds the validated build.
    /// `None` for an operator's engine: theirs, not a slot.
    pub(super) fn reserve(&self) -> Result<Option<SlotReservation>, EngineUnpinned> {
        let Self::Slot { root, slot, .. } = self else {
            return Ok(None);
        };
        let reservation =
            SlotReservation::acquire(root, slot).map_err(|reason| EngineUnpinned::Unreserved {
                pin: pinned_engine().to_string(),
                slot: *slot,
                reason,
            })?;
        self.confirm()?;
        Ok(Some(reservation))
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
    // engine-a, then a deploy promotes engine-b (or the selector vanishes) before the spawn, and
    // either the launch re-resolves `current` (running engine-b, the pre-slot install or PATH
    // unchecked) or the next build chooses engine-a and replaces it under the launch. While the
    // reservation lives the selection never chooses engine-a and the launch keeps running the
    // validated binary; dropping the reservation releases it; a build that took the slot first
    // is observed (its stamp is gone or changed) and refused. With and without a deploy holding
    // the claim (the kept pinned-slot exception).
    #[test]
    fn a_reserved_slot_is_never_built_over_and_the_launch_runs_what_it_validated() {
        let pinned = format!("{}:metal", pinned_engine());
        let no_lanes = || Ok(Vec::new());
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
            let custody = checked
                .reserve()
                .expect("engine-a reserved")
                .expect("a slot engine has custody");

            // A deploy promotes engine-b; engine-c is held by an installer exclusion, so without
            // the reservation the only idle slot would be engine-a.
            slots::promote(root, "engine-b", &pinned).expect("test: a deploy promotes engine-b");
            assert!(
                matches!(
                    slots::select_idle_slot(root, &["engine-c"], no_lanes),
                    Err(slots::VerbError::NoIdleSlot)
                ),
                "a reserved slot is never chosen (deploy_owns_pointers = {deploy_owns_pointers})"
            );
            assert_eq!(
                checked.program(),
                validated,
                "the promotion never redirects the launch"
            );
            assert_eq!(
                checked.confirm(),
                Ok(()),
                "the validated build is intact: it launches"
            );

            std::fs::remove_file(root.join("current")).expect("test: the selector vanishes");
            assert_eq!(
                checked.program(),
                validated,
                "never the pre-slot install or PATH"
            );
            assert_eq!(checked.confirm(), Ok(()));
            slots::promote(root, "engine-b", &pinned).expect("test: engine-b current again");

            drop(custody);
            assert!(
                matches!(
                    slots::select_idle_slot(root, &["engine-c"], no_lanes),
                    Ok("engine-a")
                ),
                "the dropped reservation releases the slot to builds"
            );

            engine(root, "engine-a", "e7e1077b3:metal");
            assert!(
                matches!(
                    checked.reserve(),
                    Err(EngineUnpinned::Rebuilt {
                        slot: "engine-a",
                        ..
                    })
                ),
                "a slot a build took before the reservation is observed and refused"
            );
            assert!(
                matches!(
                    slots::select_idle_slot(root, &["engine-c"], no_lanes),
                    Ok("engine-a")
                ),
                "and that refused reservation released the slot again"
            );
        }
    }
}
