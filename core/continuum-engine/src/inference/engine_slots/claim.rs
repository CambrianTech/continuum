//! ONE exclusive, cross-process claim per engine slot (Codex on #5535).
//!
//! A slot is overwritten by a build and run by a launch. Each used to decide from a reading the
//! other could invalidate: `idle-slot` printed an unclaimed path, so a builder could select A,
//! pause, and overwrite A after a launch had validated it. Now both take the same claim before
//! acting: the file `<slot root>/reservations/<slot>`, so exactly one owner holds a slot. A build
//! claims it in `continuum engine idle-slot --owner-pid` (for the installer script's process,
//! released by `release-slot` from its exit trap or `finally`); a launch claims it from the pin
//! hold until its lane record names the slot.
//!
//! Every transition (judge and reap a dead claim, publish, release) runs under ONE exclusive OS
//! file lock, `reservations/.lock` ([`Transition`]), held for milliseconds and dropped by the OS
//! with its holder: no transition acts on a reading another has invalidated (two reapers of one
//! dead claim cannot displace the claim published between them). A claim is written whole to a
//! scratch file and published with `hard_link`, so it is never observed empty or partial; a crash
//! between the two leaves only a scratch file, swept once its writer is verified gone.
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

/// Distinguishes this process's staged scratch files.
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

/// The ONE lock every claim transition holds: an exclusive OS file lock (flock on unix,
/// LockFileEx on Windows) on `<claims dir>/.lock`. Judge-and-reap, publish and release each run
/// entirely under it, so no transition acts on a reading another one has invalidated. It is held
/// for the transition only (milliseconds), never across a build or a launch, and the OS drops it
/// with its holder, so a crash never leaves it held.
struct Transition {
    _lock: std::fs::File,
}

impl Transition {
    fn begin(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(".lock");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        file.lock()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self { _lock: file })
    }
}

/// A unique scratch name in the claims dir, naming its writer (`.claim.<pid>.<start_s>.<nonce>`)
/// so a stray one left by a crash is swept once that writer is verified gone.
fn scratch_name() -> Result<String, String> {
    let pid = std::process::id();
    let started_s = process_start_s(pid)
        .ok_or("this process's start time is unreadable: its scratch files could never be swept")?;
    // Unique without a clock: pid + start time name this process, and the counter separates its
    // names; a later process reusing the pid has another start time.
    Ok(format!(".claim.{pid}.{started_s}.{}", NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// Write `owner`'s claim, complete and synced, to a scratch file beside the claims: the claim is
/// then published whole by `hard_link`, never observed empty or partial.
pub(super) fn stage(dir: &Path, owner: &Owner) -> Result<PathBuf, String> {
    let staged = dir.join(scratch_name()?);
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

/// Remove a scratch file this process wrote; a failure leaves a stray its sweep removes later.
fn discard(scratch: &Path) {
    if let Err(e) = std::fs::remove_file(scratch) {
        if e.kind() != std::io::ErrorKind::NotFound {
            continuum_base::probe!(class = "serving.engine.claim_scratch_kept", path = %scratch.display(), error = %e,
                "an engine claim scratch file could not be removed: it is swept once this process is gone");
        }
    }
}

/// Remove staged scratch files whose writer, named in the file name, is verified gone: what a
/// crash between staging and publishing leaves behind. Run under the transition lock.
fn sweep_strays(dir: &Path, probe: &dyn Fn(u32) -> ProcessProbe) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let mut fields = name.split('.').skip(1); // "", "claim", pid, start, nonce
        let writer = match (fields.next(), fields.next(), fields.next()) {
            (Some("claim"), Some(pid), Some(start)) => {
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

/// Whether the claim at `path` is held: absent is free; a verified-dead owner's claim is removed
/// (a plain conditional remove: the caller holds the transition lock); an unreadable owner holds;
/// an unparseable claim can only be corruption (claims are published whole): it holds, said.
fn held_after_reaping(path: &Path, probe: &dyn Fn(u32) -> ProcessProbe) -> Result<bool, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let Some(owner) = Owner::parse(&text) else {
        continuum_base::probe!(class = "serving.engine.claim_corrupt", path = %path.display(),
            "an engine slot claim is corrupt: it keeps holding its slot until an operator removes it");
        return Ok(true);
    };
    if process_liveness_with(owner.pid, owner.started_s, probe) != Liveness::Dead {
        return Ok(true);
    }
    std::fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(false)
}

/// Claim `slot` for `owner`, atomically and whole, under the transition lock: `true` when this
/// call holds it now, `false` when another live (or unreadable) owner does.
pub(super) fn try_claim(
    root: &Path,
    slot: &'static str,
    owner: &Owner,
    probe: &dyn Fn(u32) -> ProcessProbe,
) -> Result<bool, String> {
    let dir = root.join(DIR);
    let _transition = Transition::begin(&dir)?;
    sweep_strays(&dir, probe)?;
    let path = claim_path(root, slot);
    if held_after_reaping(&path, probe)? {
        return Ok(false);
    }
    let staged = stage(&dir, owner)?;
    let published = std::fs::hard_link(&staged, &path);
    discard(&staged);
    match published {
        Ok(()) => Ok(true),
        // Only a writer outside the lock could get here first; whatever it wrote holds.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Remove `slot`'s claim only when `owner` holds it: the same kind, pid AND start time (a reused
/// pid is another owner), under the transition lock. `false` when it holds none.
fn release_owned(root: &Path, slot: &str, owner: &Owner) -> Result<bool, String> {
    let dir = root.join(DIR);
    let _transition = Transition::begin(&dir)?;
    let path = claim_path(root, slot);
    let held = match std::fs::read_to_string(&path) {
        Ok(text) => Owner::parse(&text).as_ref() == Some(owner),
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
        release_owned(root, slot, &owner).map_err(VerbError::Failed)?;
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
    let owner = Owner::of(ClaimKind::Build, owner_pid)?;
    Ok(match release_owned(root, slot, &owner)? {
        true => format!("{slot} released"),
        false => format!("{slot} holds no build claim of pid {owner_pid}; nothing released"),
    })
}

/// This process's launch claims: one file per slot, shared by its concurrent launches (the live
/// and an ephemeral lane can both run the current slot), removed when the last one drops.
static LAUNCHES: parking_lot::Mutex<Vec<(PathBuf, usize, Owner)>> =
    parking_lot::Mutex::new(Vec::new());

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
        if let Some((_, count, _)) = launches.iter_mut().find(|(held, _, _)| *held == path) {
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
            launches.push((path, 1, owner));
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
        let Some(at) = launches.iter().position(|(held, _, _)| *held == path) else {
            return;
        };
        launches[at].1 -= 1;
        if launches[at].1 > 0 {
            return;
        }
        let (_, _, owner) = launches.swap_remove(at);
        if let Err(e) = release_owned(&self.root, self.slot, &owner) {
            continuum_base::probe!(class = "serving.engine.claim_stuck", slot = self.slot, error = e.as_str(),
                "an engine slot launch claim could not be released: builds skip the slot until this process exits");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const DEAD: u32 = 4_000_010;

    fn owner(pid: u32) -> Owner {
        Owner {
            kind: ClaimKind::Build,
            pid,
            started_s: 1_700_000_000 + u64::from(pid),
        }
    }

    /// Every fixture owner is alive at its own start time, except `DEAD`, verified gone.
    fn alive_or_dead(pid: u32) -> ProcessProbe {
        if pid == DEAD {
            ProcessProbe::Absent
        } else {
            ProcessProbe::Present(1_700_000_000 + u64::from(pid))
        }
    }

    // what this catches (Codex on #5535 at 06d21dc1a, the reaper race): reapers R1 and R2 both
    // read the dead claim D; R1 removes it and an owner publishes; R2, acting on its stale read,
    // displaced that LIVE claim and a third owner published too: two holders of one slot. Under
    // the transition lock, R1 is stopped INSIDE its judgment of D (a barrier in its probe) while
    // R2 and a fresh publisher A start: neither can act until R1 finishes, and then each judges
    // the claim as it is now. Exactly one winner, and the claim on disk is the winner's.
    #[test]
    fn two_reapers_and_a_publisher_never_produce_a_second_winner() {
        let dir = tempfile::tempdir().expect("test: tempdir");
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(DIR)).expect("test: claims dir");
        std::fs::write(claim_path(&root, "engine-a"), owner(DEAD).render())
            .expect("test: the dead claim D");

        let (judging_tx, judging_rx) = mpsc::channel::<()>();
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let r1 = {
            let root = root.clone();
            std::thread::spawn(move || {
                let probe = move |pid: u32| {
                    if pid == DEAD {
                        judging_tx.send(()).expect("test: main listens");
                        go_rx.recv().expect("test: main releases R1");
                    }
                    alive_or_dead(pid)
                };
                try_claim(&root, "engine-a", &owner(4_000_011), &probe).expect("R1 claims")
            })
        };
        judging_rx
            .recv()
            .expect("R1 is judging D, inside the transition");
        let contender = |pid: u32| {
            let root = root.clone();
            std::thread::spawn(move || {
                try_claim(&root, "engine-a", &owner(pid), &alive_or_dead)
                    .expect("a contender claims")
            })
        };
        let r2 = contender(4_000_012);
        let a = contender(4_000_013);
        std::thread::sleep(std::time::Duration::from_millis(200)); // room to act, were the transition not exclusive
        assert!(
            !r2.is_finished() && !a.is_finished(),
            "no transition runs beside R1's"
        );
        go_tx.send(()).expect("test: R1 listens");

        let won = [
            r1.join().expect("R1"),
            r2.join().expect("R2"),
            a.join().expect("A"),
        ];
        assert_eq!(
            won,
            [true, false, false],
            "R1 reaped D and published; R2 and A found it held"
        );
        let on_disk = std::fs::read_to_string(claim_path(&root, "engine-a")).expect("the claim");
        assert_eq!(
            Owner::parse(&on_disk),
            Some(owner(4_000_011)),
            "the claim on disk is the one winner's"
        );
    }
}
