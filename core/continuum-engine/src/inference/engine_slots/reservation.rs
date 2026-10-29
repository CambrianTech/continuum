//! A launch's custody of its engine slot before the lane record exists (Codex on #5535).
//!
//! A slot is protected from builds by being `current` or by a live lane's record naming it
//! ([`super::idle_slot`]). Between a launch validating its slot and its lane record landing,
//! neither need hold: a deploy can promote another slot and a second build can choose this one.
//! A [`SlotReservation`] closes that window. It is a file under `<root>/reservations/`, so the
//! installer's `continuum engine idle-slot` (another process: install-llama-server.sh and the
//! Windows service-host both select through it) honors it the same way it honors lane records.
//! The launch takes it before retiring the incumbent and drops it once the lane record names
//! the slot; a record that could not be written keeps it beside the child until its exit.
//!
//! A reservation names its owner (pid and OS start time). One whose owner is VERIFIED gone
//! decays, so a crashed core never pins a slot forever; an unreadable one holds (fail closed).

use super::*;
use crate::inference::engine_residency::{
    probe_process, process_liveness_with, process_start_s, Liveness, ProcessProbe,
};
use std::sync::atomic::{AtomicU64, Ordering};

/// The directory under the slot root holding live launch reservations.
const DIR: &str = "reservations";

/// Distinguishes this process's concurrent reservations of one slot (live and ephemeral lanes).
static NEXT: AtomicU64 = AtomicU64::new(0);

/// A launch's hold on `slot`: no idle-slot selection chooses it while this lives.
#[must_use = "dropping the reservation releases the slot to builds"]
#[derive(Debug)]
pub struct SlotReservation {
    path: PathBuf,
}

impl SlotReservation {
    /// Reserve `slot` under `root` for this process.
    pub fn acquire(root: &Path, slot: &str) -> Result<Self, String> {
        let slot = known(slot)?;
        let pid = std::process::id();
        let started_s = process_start_s(pid)
            .ok_or("this process's start time is unreadable: a reservation it wrote could never be released")?;
        let dir = root.join(DIR);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = format!("{slot}.{pid}.{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let staged = dir.join(format!(".{name}.tmp")); // a dot name is never read as a reservation
        std::fs::write(&staged, format!("{pid} {started_s}\n"))
            .map_err(|e| format!("{}: {e}", staged.display()))?;
        let path = dir.join(name);
        continuum_host::durable::replace(&staged, &path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for SlotReservation {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                continuum_base::probe!(class = "serving.engine.reservation_stuck", path = %self.path.display(), error = %e,
                    "an engine slot reservation could not be removed: builds skip the slot until this process exits");
            }
        }
    }
}

/// The slots a live launch reserves, read fail-closed: a reservation whose owner cannot be
/// shown gone holds its slot; one whose file cannot be named as a slot fails the read. A
/// reservation whose owner is verified gone is removed (it can never be released otherwise).
pub(super) fn reserved_slots(root: &Path) -> Result<Vec<&'static str>, String> {
    reserved_slots_with(root, probe_process)
}

pub(super) fn reserved_slots_with(
    root: &Path,
    probe: impl Fn(u32) -> ProcessProbe,
) -> Result<Vec<&'static str>, String> {
    let dir = root.join(DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(format!(
                "engine reservations unreadable ({}: {e}): no slot can be proven idle",
                dir.display()
            ))
        }
    };
    let mut reserved = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("engine reservations unreadable: {e}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue; // a reservation still being written: its owner has not been handed it yet
        }
        let slot = name
            .split('.')
            .next()
            .and_then(|slot| SLOTS.into_iter().find(|s| *s == slot))
            .ok_or_else(|| {
                format!("unknown engine reservation {name:?}: no slot can be proven idle")
            })?;
        let owner = std::fs::read_to_string(entry.path()).ok().and_then(|text| {
            let mut fields = text.split_whitespace().map(str::parse::<u64>);
            Some((
                u32::try_from(fields.next()?.ok()?).ok()?,
                fields.next()?.ok()?,
            ))
        });
        let gone = owner.is_some_and(|(pid, started_s)| {
            process_liveness_with(pid, started_s, &probe) == Liveness::Dead
        });
        if gone {
            // The owner is verified gone, so nothing will remove this: decay it here. A removal
            // that fails only leaves a dead file, which the next read skips again.
            let _ = std::fs::remove_file(entry.path()); // best-effort decay of a verified-dead owner's file
        } else if !reserved.contains(&slot) {
            reserved.push(slot);
        }
    }
    Ok(reserved)
}
