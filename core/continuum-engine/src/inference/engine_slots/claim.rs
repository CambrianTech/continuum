//! ONE exclusive, cross-process claim per engine slot (Codex on #5535).
//!
//! A slot is overwritten by a build and run by a launch. Each used to decide from a reading the
//! other could invalidate: `idle-slot` printed an unclaimed path, so a builder could select A,
//! pause, and overwrite A after a launch had validated it. Now both take the same claim before
//! acting: the file `<slot root>/reservations/<slot>`, so exactly one owner holds a slot. It is
//! written whole to a scratch file and published with `hard_link`, which fails when the claim
//! exists (the same exclusive create) and never exposes an empty or partial claim; a crash
//! between the two leaves only a scratch file, swept once its writer is verified gone. A build claims it in `continuum engine idle-slot --owner-pid`
//! (for the installer script's process, released by `release-slot` from its exit trap or
//! `finally`); a launch claims it from the pin hold until its lane record names the slot.
//!
//! A selection claims FIRST and then verifies against fresh lane records and `current`: a launch
//! releases its claim only after its record is written, so a claim that succeeded sees the record.
//!
//! A claim names its owner (kind, pid, OS start time). One whose owner is VERIFIED gone is
//! reaped by whoever next wants the slot, so a crashed installer or core never pins it forever;
//! one whose owner cannot be read holds (fail closed), and one that cannot be parsed can only be
//! corruption: it holds and is said (`serving.engine.claim_corrupt`).

use super::*;
use crate::inference::engine_residency::{
    probe_process, process_liveness_with, process_start_s, Liveness, ProcessProbe,
};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

/// The directory under the slot root holding the claims, one file per claimed slot.
const DIR: &str = "reservations";

/// Distinguishes this process's scratch files (staged claims, reap tombstones).
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
pub(super) struct Owner {
    kind: ClaimKind,
    pid: u32,
    started_s: u64,
}

impl Owner {
    /// `pid` as a claim owner: its start time must be readable, or the claim could never decay.
    pub(super) fn of(kind: ClaimKind, pid: u32) -> Result<Self, String> {
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

/// A unique scratch name in the claims dir, naming its writer (`.<what>.<pid>.<start_s>.<nonce>`)
/// so a stray one left by a crash is swept once that writer is verified gone.
fn scratch_name(what: &str) -> Result<String, String> {
    let pid = std::process::id();
    let started_s = process_start_s(pid)
        .ok_or("this process's start time is unreadable: its scratch files could never be swept")?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos()); // uniqueness salt only; the counter alone already separates this process's names
    Ok(format!(
        ".{what}.{pid}.{started_s}.{}{nanos:09}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Write `owner`'s claim, complete and synced, to a scratch file beside the claims: the claim is
/// then published whole by [`link_claim`], never observed empty or partial.
pub(super) fn stage(dir: &Path, owner: &Owner) -> Result<PathBuf, String> {
    let staged = dir.join(scratch_name("claim")?);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)
        .map_err(|e| format!("{}: {e}", staged.display()))?;
    file.write_all(owner.render().as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("{}: {e}", staged.display()))?;
    Ok(staged)
}

/// Publish `staged` as the claim at `path`: `hard_link` fails with AlreadyExists when the claim
/// exists, so it is the same exclusive create, and it never overwrites. A verified-dead owner's
/// claim is reaped first. `true` when this call holds the claim now.
fn link_claim(
    staged: &Path,
    path: &Path,
    probe: &dyn Fn(u32) -> ProcessProbe,
) -> Result<bool, String> {
    for _ in 0..2 {
        match std::fs::hard_link(staged, path) {
            Ok(()) => return Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if !reap_if_dead(path, probe)? {
                    return Ok(false);
                }
            }
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(false) // reaped, and another owner claimed it first: it is theirs
}

/// Remove a scratch file this process wrote; a failure leaves a stray its sweep removes later.
fn discard(scratch: &Path) {
    if let Err(e) = std::fs::remove_file(scratch) {
        if e.kind() != std::io::ErrorKind::NotFound {
            continuum_base::probe!(class = "serving.engine.claim_scratch_kept", path = %scratch.display(), error = %e,
                "an engine claim scratch file could not be removed: it is swept once this process is gone");
        }
    }
}

/// Remove scratch files (`.claim.` staged, `.reaped.` tombstones) whose writer, named in the
/// file name, is verified gone: what a crash between staging and linking leaves behind.
pub(super) fn sweep_strays(dir: &Path, probe: &dyn Fn(u32) -> ProcessProbe) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let mut fields = name.split('.').skip(1); // "", what, pid, start, nonce
        let writer = match (fields.next(), fields.next(), fields.next()) {
            (Some("claim" | "reaped"), Some(pid), Some(start)) => {
                pid.parse::<u32>().ok().zip(start.parse::<u64>().ok())
            }
            _ => continue,
        };
        if writer.is_some_and(|(pid, started_s)| {
            process_liveness_with(pid, started_s, probe) == Liveness::Dead
        }) {
            discard(&entry.path());
        }
    }
    Ok(())
}

/// Claim `slot` for `owner`, atomically and whole: `true` when this call holds it now, `false`
/// when another live (or unreadable) owner does.
fn try_claim(
    root: &Path,
    slot: &'static str,
    owner: &Owner,
    probe: &dyn Fn(u32) -> ProcessProbe,
) -> Result<bool, String> {
    let dir = root.join(DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    sweep_strays(&dir, probe)?;
    let staged = stage(&dir, owner)?;
    let claimed = link_claim(&staged, &claim_path(root, slot), probe);
    discard(&staged);
    claimed
}

/// Remove `path`'s claim when its owner is verified gone. The claim is MOVED aside first (an
/// atomic rename) and its owner read again from the moved file, so a claim another process took
/// after this one was read is put back (`hard_link` never overwrites) instead of deleted. Claims
/// are only ever published whole, so one that cannot be parsed is corruption: it holds, said.
fn reap_if_dead(path: &Path, probe: &dyn Fn(u32) -> ProcessProbe) -> Result<bool, String> {
    let dead = |file: &Path| match std::fs::read_to_string(file)
        .ok()
        .and_then(|text| Owner::parse(&text))
    {
        Some(owner) => process_liveness_with(owner.pid, owner.started_s, probe) == Liveness::Dead,
        None => {
            continuum_base::probe!(class = "serving.engine.claim_corrupt", path = %file.display(),
                "an engine slot claim is unreadable or corrupt: it keeps holding its slot until an operator removes it");
            false
        }
    };
    if !dead(path) {
        return Ok(false);
    }
    let tomb = path.with_file_name(scratch_name("reaped")?);
    match std::fs::rename(path, &tomb) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(true), // another reaper took it
        Err(e) => return Err(format!("{}: {e}", path.display())),
    }
    if !dead(&tomb) {
        let restored = std::fs::hard_link(&tomb, path);
        discard(&tomb);
        return restored.map(|()| false).map_err(|e| {
            format!(
                "a live claim on {} was moved and could not be restored: {e}",
                path.display()
            )
        });
    }
    discard(&tomb);
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
