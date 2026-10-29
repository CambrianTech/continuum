//! ONE exclusive, cross-process claim per engine slot (Codex on #5535).
//!
//! A slot is overwritten by a build and run by a launch. Each used to decide from a reading the
//! other could invalidate: `idle-slot` printed an unclaimed path, so a builder could select A,
//! pause, and overwrite A after a launch had validated it. Now both take the same claim before
//! acting: the file `<slot root>/reservations/<slot>`, created with `create_new` (O_EXCL), so
//! exactly one owner holds a slot. A build claims it in `continuum engine idle-slot --owner-pid`
//! (for the installer script's process, released by `release-slot` from its exit trap or
//! `finally`); a launch claims it from the pin hold until its lane record names the slot.
//!
//! A selection claims FIRST and then verifies against fresh lane records and `current`: a launch
//! releases its claim only after its record is written, so a claim that succeeded sees the record.
//!
//! A claim names its owner (kind, pid, OS start time). One whose owner is VERIFIED gone is
//! reaped by whoever next wants the slot, so a crashed installer or core never pins it forever;
//! one whose owner cannot be read, or whose file cannot be parsed, holds (fail closed).

use super::*;
use crate::inference::engine_residency::{
    probe_process, process_liveness_with, process_start_s, Liveness, ProcessProbe,
};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

/// The directory under the slot root holding the claims, one file per claimed slot.
const DIR: &str = "reservations";

/// Distinguishes this process's reap tombstones.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Who holds a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaimKind {
    /// An installer building into the slot.
    Build,
    /// A launch about to run the slot's engine.
    Launch,
}

impl ClaimKind {
    fn word(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Launch => "launch",
        }
    }
}

/// A claim's owner, as written in its file: `<kind> <pid> <start_s>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Owner {
    kind: ClaimKind,
    pid: u32,
    started_s: u64,
}

impl Owner {
    /// `pid` as a claim owner: its start time must be readable, or the claim could never decay.
    fn of(kind: ClaimKind, pid: u32) -> Result<Self, String> {
        let started_s = process_start_s(pid).ok_or_else(|| {
            format!(
                "the start time of pid {pid} is unreadable: a claim it held could never be reaped"
            )
        })?;
        Ok(Self {
            kind,
            pid,
            started_s,
        })
    }

    fn render(&self) -> String {
        format!("{} {} {}\n", self.kind.word(), self.pid, self.started_s)
    }

    fn parse(text: &str) -> Option<Self> {
        let mut fields = text.split_whitespace();
        let kind = match fields.next()? {
            "build" => ClaimKind::Build,
            "launch" => ClaimKind::Launch,
            _ => return None,
        };
        Some(Self {
            kind,
            pid: fields.next()?.parse().ok()?,
            started_s: fields.next()?.parse().ok()?,
        })
    }
}

fn claim_path(root: &Path, slot: &str) -> PathBuf {
    root.join(DIR).join(slot)
}

/// Create `slot`'s claim for `owner`, atomically: `true` when this call holds it now, `false`
/// when another live (or unreadable) owner does. A verified-dead owner's claim is reaped first.
fn try_claim(
    root: &Path,
    slot: &'static str,
    owner: &Owner,
    probe: &dyn Fn(u32) -> ProcessProbe,
) -> Result<bool, String> {
    let path = claim_path(root, slot);
    let dir = root.join(DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let written = file
                    .write_all(owner.render().as_bytes())
                    .and_then(|()| file.sync_all());
                return match written {
                    Ok(()) => Ok(true),
                    Err(e) => {
                        drop(file);
                        std::fs::remove_file(&path).map_err(|r| {
                            format!("{}: {e}; and removing it: {r}", path.display())
                        })?;
                        Err(format!("{}: {e}", path.display()))
                    }
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if !reap_if_dead(&path, probe)? {
                    return Ok(false);
                }
            }
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(false) // reaped, and another owner claimed it first: it is theirs
}

/// Remove `path`'s claim when its owner is verified gone. The claim is MOVED aside first (an
/// atomic rename) and its owner read again from the moved file, so a claim another process took
/// after this one was read is put back (`hard_link` never overwrites) instead of deleted.
fn reap_if_dead(path: &Path, probe: &dyn Fn(u32) -> ProcessProbe) -> Result<bool, String> {
    let dead = |file: &Path| {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|text| Owner::parse(&text))
            .is_some_and(|owner| {
                process_liveness_with(owner.pid, owner.started_s, probe) == Liveness::Dead
            })
    };
    if !dead(path) {
        return Ok(false);
    }
    let tomb = path.with_file_name(format!(
        ".reaped.{}.{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    match std::fs::rename(path, &tomb) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true), // another reaper took it
        Err(e) => return Err(format!("{}: {e}", path.display())),
    }
    if !dead(&tomb) {
        let restored = std::fs::hard_link(&tomb, path);
        std::fs::remove_file(&tomb).map_err(|e| format!("{}: {e}", tomb.display()))?;
        return restored.map(|()| false).map_err(|e| {
            format!(
                "a live claim on {} was moved and could not be restored: {e}",
                path.display()
            )
        });
    }
    std::fs::remove_file(&tomb).map_err(|e| format!("{}: {e}", tomb.display()))?;
    Ok(true)
}

/// Remove `slot`'s claim only when `owner` (kind and pid) holds it. `false` when it holds none.
fn release_owned(root: &Path, slot: &str, kind: ClaimKind, pid: u32) -> Result<bool, String> {
    let path = claim_path(root, slot);
    let held = match std::fs::read_to_string(&path) {
        Ok(text) => Owner::parse(&text).is_some_and(|owner| owner.kind == kind && owner.pid == pid),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if held {
        std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(held)
}

/// The installer's select-AND-claim (`continuum engine idle-slot --owner-pid`): the first slot,
/// in the idle preference order, that this builder claims AND that fresh lane records and
/// `current` still leave idle. The claim stays after this returns, owned by `owner_pid`, until
/// `release-slot` (or its owner's death).
pub(crate) fn claim_idle_slot(
    root: &Path,
    excluded: &[&str],
    owner_pid: u32,
    mut live_lanes: impl FnMut() -> Result<Vec<Option<PathBuf>>, String>,
    probe: &dyn Fn(u32) -> ProcessProbe,
) -> Result<&'static str, VerbError> {
    let owner = Owner::of(ClaimKind::Build, owner_pid).map_err(VerbError::Failed)?;
    let populated = |s: &str| slot_bin(root, s).is_file();
    let idle_now = |live: &[Option<PathBuf>]| {
        idle_candidates(
            root,
            current_slot(root),
            previous_slot(root),
            live,
            excluded,
            populated,
        )
    };
    let candidates = idle_now(&live_lanes().map_err(VerbError::Failed)?);
    for slot in candidates {
        if !try_claim(root, slot, &owner, probe).map_err(VerbError::Failed)? {
            continue;
        }
        let still_idle = live_lanes().map(|live| idle_now(&live).contains(&slot));
        if still_idle == Ok(true) {
            return Ok(slot);
        }
        release_owned(root, slot, ClaimKind::Build, owner_pid).map_err(VerbError::Failed)?;
        still_idle.map_err(VerbError::Failed)?;
    }
    Err(VerbError::NoIdleSlot)
}

/// `continuum engine release-slot <slot> --owner-pid`: the builder's claim is released.
pub(crate) fn release_build_claim(
    root: &Path,
    slot: &str,
    owner_pid: u32,
) -> Result<String, String> {
    let slot = known(slot)?;
    Ok(
        match release_owned(root, slot, ClaimKind::Build, owner_pid)? {
            true => format!("{slot} released"),
            false => format!("{slot} holds no build claim of pid {owner_pid}; nothing released"),
        },
    )
}

/// This process's launch claims: one file per slot, shared by its concurrent launches (the live
/// and an ephemeral lane can both run the current slot), removed when the last one drops.
static LAUNCHES: parking_lot::Mutex<Vec<(PathBuf, usize)>> = parking_lot::Mutex::new(Vec::new());

/// A launch's claim on its slot: no build claims the slot while this lives.
#[must_use = "dropping the claim releases the slot to builds"]
#[derive(Debug)]
pub struct SlotClaim {
    root: PathBuf,
    slot: &'static str,
}

impl SlotClaim {
    /// Claim `slot` under `root` for a launch of this process; refused while a build (or another
    /// process's launch) holds it.
    pub fn acquire(root: &Path, slot: &str) -> Result<Self, String> {
        Self::acquire_with(root, slot, &probe_process)
    }

    pub(crate) fn acquire_with(
        root: &Path,
        slot: &str,
        probe: &dyn Fn(u32) -> ProcessProbe,
    ) -> Result<Self, String> {
        let slot = known(slot)?;
        let path = claim_path(root, slot);
        let mut launches = LAUNCHES.lock();
        if let Some((_, count)) = launches.iter_mut().find(|(held, _)| *held == path) {
            *count += 1;
        } else {
            let owner = Owner::of(ClaimKind::Launch, std::process::id())?;
            if !try_claim(root, slot, &owner, probe)? {
                let holder = match std::fs::read_to_string(&path) {
                    Ok(text) => text.trim().to_string(),
                    Err(e) => format!("holder unreadable: {e}"),
                };
                return Err(format!("{slot} is claimed by another owner ({holder})"));
            }
            launches.push((path, 1));
        }
        Ok(Self {
            root: root.to_path_buf(),
            slot,
        })
    }
}

impl Drop for SlotClaim {
    fn drop(&mut self) {
        let path = claim_path(&self.root, self.slot);
        let mut launches = LAUNCHES.lock();
        let Some(at) = launches.iter().position(|(held, _)| *held == path) else {
            return;
        };
        launches[at].1 -= 1;
        if launches[at].1 > 0 {
            return;
        }
        launches.swap_remove(at);
        if let Err(e) = release_owned(&self.root, self.slot, ClaimKind::Launch, std::process::id())
        {
            continuum_base::probe!(class = "serving.engine.claim_stuck", slot = self.slot, error = e.as_str(),
                "an engine slot launch claim could not be released: builds skip the slot until this process exits");
        }
    }
}
