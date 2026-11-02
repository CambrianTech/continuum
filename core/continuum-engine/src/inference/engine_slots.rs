//! The engine's A/B slots (card 2c5d0ec0).
//!
//! An engine update used to overwrite the one installed binary in place. That works on
//! macOS and Linux only by accident of POSIX semantics (a live process keeps its unlinked
//! inode), and on Windows it cannot work at all (a running exe or DLL is locked), so #4464
//! skipped the engine there and left every Windows engine update to a manual
//! `continuum install`. Windows with CUDA may be someone's first and only machine (Joel,
//! 2026-09-28), so a manual step there fails the most common install.
//!
//! Every OS now uses the slots the Windows service already had (#4382): `<continuum_home>/bin/
//! engine-{a,b,c}/`, each holding a complete engine (binary, DLLs, `.llama-server.stamp`),
//! plus a `current` file naming the active one and a `previous` file naming the one it
//! replaced. A deploy builds the pinned engine into the IDLE slot, so nothing running is ever
//! touched, then [`promote`] verifies it and flips `current` atomically. The core's existing
//! convergence (#4464: a lane's `/props` build against the installed stamp) relaunches lanes
//! onto it. THREE slots, the Windows count and for the same reason: a lane can still run the
//! engine `current` replaced (a lane outlives its core) while a third is built, and the
//! previous engine stays intact as the rollback until a later build needs its slot.
//!
//! Idle is never inferred from running processes (the guess `Select-CoreEngineSlot` makes, and
//! on Windows a service-session lane's path is unreadable from the operator's session): it is
//! read from the lane records, which name the exe each lane launched.

use std::path::{Path, PathBuf};

mod attempts;
mod claim;
#[cfg(test)]
pub(crate) use claim::{claim_idle_slot, release_build_claim}; // the launch tests drive builds; production calls `claim::` here
pub use claim::SlotClaim;

/// The slots, bounded by construction: the current engine, one a lane may still run, and one to
/// build into.
pub const SLOTS: [&str; 3] = ["engine-a", "engine-b", "engine-c"];

/// The file naming the active slot.
const CURRENT_FILE: &str = "current";

/// The file naming the slot `current` replaced: the rollback when a promoted engine fails.
const PREVIOUS_FILE: &str = "previous";

/// Written inside a slot at the first launch from it that came up ready: the engine has proven
/// itself on this machine. Cleared whenever the slot is promoted or registered anew (a freshly
/// built engine proves nothing yet). Only an UNPROVEN current engine is rolled back after a
/// failed launch (Fable on #4491): NotReady also comes from memory, a model load or a wedge,
/// and demoting a good engine on one of those would silently run the node on the older one.
const VERIFIED_FILE: &str = ".verified";


/// The engine's stamp file inside a slot (`<commit>:<backend>`, written last by the installer
/// after the binary verified).
pub const STAMP_FILE: &str = ".llama-server.stamp";

/// The engine binary's file name on this OS.
pub fn exe_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// Installed payload `bin`, where slots and their rollback pointers live.
pub fn root(continuum_home: &Path) -> Result<PathBuf, String> {
    crate::paths::payload_root(continuum_home).map(|root| root.join("bin"))
}

/// The binary inside `slot`.
pub fn slot_bin(root: &Path, slot: &str) -> PathBuf {
    root.join(slot).join(exe_name())
}

/// The active slot, when `current` names one of [`SLOTS`]. A missing or unrecognised file is
/// no slot: the caller falls back to the pre-slot install path, never to a guess.
pub fn current_slot(root: &Path) -> Option<&'static str> {
    named_slot(root, CURRENT_FILE)
}

/// The slot `current` replaced at the last promote, when it names one of [`SLOTS`].
pub fn previous_slot(root: &Path) -> Option<&'static str> {
    named_slot(root, PREVIOUS_FILE)
}

/// The installed engine owner captures every managed slot, not only engines
/// named by the service receipts. Installation custody must span its lifetime.
#[cfg(windows)]
pub struct MaintenanceEngines {
    root: PathBuf,
    override_bin: Option<String>,
    selectors: Vec<(PathBuf, Option<String>)>,
    engines: Vec<(PathBuf, super::engine_install::EngineInstallReceipt)>,
    absent: Vec<PathBuf>,
}

#[cfg(windows)]
impl MaintenanceEngines {
    pub fn verified_identity(program: &Path) -> Result<String, String> {
        super::engine_install::EngineInstallReceipt::maintenance_snapshot(program)?.maintenance_identity()
    }

    pub fn capture(home: &Path) -> Result<Self, String> {
        Self::capture_at(root(home)?, super::llama_server::engine_override(), live_lane_engines()?)
    }

    pub(crate) fn capture_at(root: PathBuf, override_bin: Option<String>, live_engines: Vec<Option<PathBuf>>) -> Result<Self, String> {
        if let Some(value) = &override_bin {
            let path = Path::new(value);
            let slot = slot_of(&root, path).ok_or("operator engine is outside managed maintenance custody")?;
            if path.canonicalize().map_err(|e| e.to_string())?
                != slot_bin(&root, slot).canonicalize().map_err(|e| e.to_string())? {
                return Err("operator engine differs from the managed executable".into());
            }
        }
        let mut selectors = Vec::new();
        for name in [CURRENT_FILE, PREVIOUS_FILE] {
            let path = root.join(name);
            let text = match std::fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.to_string()),
                Ok(_) => Some(continuum_cli_lifecycle::installed_release::read_receipt(&path)?),
            };
            if text.as_ref().is_some_and(|value| !SLOTS.contains(&value.trim())) {
                return Err("unknown engine selector blocks maintenance".into());
            }
            selectors.push((path, text));
        }
        if selectors[0].1.is_none() {
            return Err("legacy engine fallback is not covered by managed slot maintenance".into());
        }
        let mut engines = Vec::new();
        let mut absent = Vec::new();
        for slot in SLOTS {
            let directory = root.join(slot);
            match std::fs::symlink_metadata(&directory) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => absent.push(directory),
                Err(e) => return Err(e.to_string()),
                Ok(_) => engines.push((directory, super::engine_install::EngineInstallReceipt::maintenance_snapshot(&slot_bin(&root, slot))?)),
            }
        }
        for (_, selected) in &selectors {
            if let Some(selected) = selected {
                if !engines.iter().any(|(directory, _)| *directory == root.join(selected.trim())) {
                    return Err("selected engine is absent from the sealed maintenance closure".into());
                }
            }
        }
        for engine in live_engines {
            let engine = engine.ok_or("live lane has no engine provenance")?;
            let slot = slot_of(&root, &engine).ok_or("live lane uses an unmanaged engine")?;
            if engine != slot_bin(&root, slot) && engine.canonicalize().map_err(|e| e.to_string())?
                != slot_bin(&root, slot).canonicalize().map_err(|e| e.to_string())? {
                return Err("live engine differs from managed executable".into());
            }
        }
        Ok(Self { root, override_bin, selectors, engines, absent })
    }

    pub fn exclude(&self, pins: &mut continuum_cli_lifecycle::installed_release::ExclusiveFiles) -> Result<(), String> {
        self.revalidate()?;
        for (path, text) in &self.selectors {
            if let Some(expected) = text {
                use std::io::Read;
                let mut actual = String::new();
                pins.pin(path)?.take(continuum_cli_lifecycle::installed_release::MAX_RECEIPT_BYTES + 1)
                    .read_to_string(&mut actual).map_err(|e| e.to_string())?;
                if &actual != expected { return Err("engine selector changed before exclusion".into()); }
            }
        }
        for (_, engine) in &self.engines { engine.exclude_for_maintenance(pins)?; }
        Ok(())
    }

    pub fn revalidate(&self) -> Result<(), String> {
        // Once selector handles are exclusive they cannot be re-opened here.
        // Their retained pins prevent mutation; namespace checks remain necessary
        // because pinning a directory does not prevent every child addition.
        continuum_cli_lifecycle::installed_release::unredirected(&self.root)?;
        if super::llama_server::engine_override() != self.override_bin {
            return Err("engine override changed during maintenance".into());
        }
        for (path, expected) in &self.selectors {
            if expected.is_none() && !matches!(path.symlink_metadata(), Err(e) if e.kind() == std::io::ErrorKind::NotFound) {
                return Err("engine selector appeared during maintenance".into());
            }
        }
        for path in &self.absent {
            match path.symlink_metadata() {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err("an engine slot appeared during maintenance".into()),
            }
        }
        for (_, engine) in &self.engines { engine.maintenance_namespace_unchanged()?; }
        Ok(())
    }
}

fn named_slot(root: &Path, file: &str) -> Option<&'static str> {
    let named = std::fs::read_to_string(root.join(file)).ok()?;
    let named = named.trim();
    SLOTS.into_iter().find(|s| *s == named)
}

/// PURE: the slot a binary path sits in, if any. How a legacy operator override that points
/// INSIDE a slot (what the Windows service-host set before `current` existed) is read as the
/// slot it names rather than as an operator's own engine.
pub fn slot_of(root: &Path, bin: &Path) -> Option<&'static str> {
    // Windows verbatim paths (`\\?\C:\...`) never `starts_with` a plain one. BOTH sides are
    // normalized: the service-host derives `root` from the descriptor's own engine path, so a
    // verbatim descriptor gives a verbatim root, and stripping only `bin` refused the slot and
    // kept the core from starting.
    SLOTS.into_iter().find(|slot| crate::paths::has_path_prefix(bin, &root.join(slot)))
}

/// Why no slot is idle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotIdle {
    /// Every slot is `current` or run by a live lane (relaunches onto the new engine have not
    /// finished). The deploy skips the engine build this time and says so, never guesses.
    AllInUse,
}

/// PURE: the slot a new engine may be built into: not the one `current` names, not one any
/// live lane runs, and, among those, not `previous` (the rollback) while another is free. `live`
/// holds each live lane's recorded engine; `None` is a lane recorded before lanes named their
/// engine, which may run ANY populated slot, so while one is up only an empty slot is idle
/// (fail closed: never build over an engine a lane might be running). `populated` says whether a
/// slot holds an engine binary. A fresh node (no pointer, no lane) gets `engine-a`, so the first
/// install takes the same path as every update.
pub fn idle_slot(
    root: &Path,
    current: Option<&str>,
    previous: Option<&str>,
    live: &[Option<PathBuf>],
    populated: impl Fn(&str) -> bool,
) -> Result<&'static str, NotIdle> {
    idle_slot_reserved(root, current, previous, live, &[], populated)
}

/// Installer custody is independent of lane lifetime and engine pointers.
/// Reservations may only remove candidates; they never make a live slot idle.
fn idle_slot_reserved(
    root: &Path,
    current: Option<&str>,
    previous: Option<&str>,
    live: &[Option<PathBuf>],
    reserved: &[&str],
    populated: impl Fn(&str) -> bool,
) -> Result<&'static str, NotIdle> {
    idle_candidates(root, current, previous, live, reserved, populated).first().copied().ok_or(NotIdle::AllInUse)
}

/// PURE: every idle slot (see [`idle_slot`]), the preferred first: `previous` (the rollback)
/// last. What a builder tries to claim, in order ([`claim`]).
fn idle_candidates(
    root: &Path,
    current: Option<&str>,
    previous: Option<&str>,
    live: &[Option<PathBuf>],
    reserved: &[&str],
    populated: impl Fn(&str) -> bool,
) -> Vec<&'static str> {
    let unknown_lane = live.iter().any(Option::is_none);
    let (mut free, rollback): (Vec<&'static str>, Vec<&'static str>) = SLOTS
        .into_iter()
        .filter(|slot| Some(*slot) != current)
        .filter(|slot| !reserved.contains(slot))
        .filter(|slot| !live.iter().flatten().any(|bin| slot_of(root, bin) == Some(*slot)))
        .filter(|slot| !(unknown_lane && populated(slot)))
        .partition(|slot| Some(*slot) != previous);
    free.extend(rollback);
    free
}

/// The recorded engine of every live lane (a record whose pid is still a llama-server), read
/// fail-closed: an unreadable registry is an error, never an empty census that would call a
/// running slot idle.
pub fn live_lane_engines() -> Result<Vec<Option<PathBuf>>, String> {
    let records = crate::inference::lane_registry::records_checked()
        .map_err(|e| format!("the lane registry is unreadable ({e}): no slot can be proven idle"))?;
    Ok(records
        .into_iter()
        .filter(|r| crate::inference::lane_process::is_llama_server(r.pid))
        .map(|r| r.engine_bin)
        .collect())
}

/// The slot the last automatic rollback restored, so a restored engine that ALSO fails to launch
/// is not rolled "back" onto the engine it replaced: that would swap two failing engines
/// forever. A failure of the restored engine is not the promotion's fault; it stays up for a
/// human, loudly, as every other launch failure does.
static RESTORED: parking_lot::Mutex<Option<&'static str>> = parking_lot::const_mutex(None);

/// A launch from the current slot failed (spawn or readiness): put the previous engine back so
/// the next launch takes it (Fable on #4491: a promoted engine can pass its stamp and still fail
/// at launch, a CUDA DLL, a driver, and on a first-and-only machine nothing else would bring the
/// node back). Only an UNPROVEN current engine (no ready launch since its promotion): a proven one
/// that fails is the ordinary launch-failure path, since NotReady also comes from memory, a model
/// load or a wedge. Returns `(failed, restored)` when it rolled back, `Ok(None)` when there is
/// nothing to roll back from (no slot recorded), the current engine has proven itself, or it is
/// the one a rollback already restored. The caller skips this under an operator override (their engine, not a slot) and
/// while a deploy holds the claim (it is mid-promote).
pub fn rollback_after_failed_launch(root: &Path) -> Result<Option<(&'static str, &'static str)>, String> {
    rollback_after_failed_launch_reason(root, "launch failed")
}

/// Failure class only: diagnostic environment values are never serialized.
pub fn rollback_after_failed_launch_reason(root: &Path, reason: &str) -> Result<Option<(&'static str, &'static str)>, String> {
    let Some(failed) = current_slot(root).filter(|slot| !is_verified(root, slot)) else {
        return Ok(None);
    };
    let mut restored = RESTORED.lock();
    if *restored == Some(failed) {
        return Ok(None);
    }
    // Persisted evidence also prevents the inverse swap after a core restart.
    if let Some(back) = previous_slot(root) {
        match attempts::failed_selection(root, back) {
            Ok(true) => return Ok(None),
            Ok(false) => {}
            Err(reason) => continuum_base::probe!(class = "serving.engine.attempt_receipt_failed", reason = reason.as_str(),
                "fallback attempt disposition is unreadable; retain ordinary restoration authority"),
        }
    }
    // Failed bookkeeping cannot strand serving. Promotion still refuses corrupt
    // evidence, while restoration of the intact fallback remains available.
    if let Some(back) = previous_slot(root) {
        if let Err(reason) = attempts::failed(root, failed, back, reason) {
            continuum_base::probe!(class = "serving.engine.attempt_receipt_failed", reason = reason.as_str(),
                "failed attempt could not be persisted; restoring the intact fallback");
        }
    }
    let back = rollback(root, failed)?;
    *restored = Some(back);
    Ok(Some((failed, back)))
}

/// Why an `engine` verb failed: [`VerbError::NoIdleSlot`] is its own exit code (3), so the
/// install scripts skip the engine build this deploy and say so rather than fail the deploy.
#[derive(Debug)]
pub enum VerbError {
    NoIdleSlot,
    DeferredFailedAttempt(String),
    Failed(String),
}

impl std::fmt::Display for VerbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerbError::DeferredFailedAttempt(reason) => write!(f, "{reason}"),
            VerbError::NoIdleSlot => write!(
                f,
                "every engine slot is current, run by a live lane or claimed: the engine build is skipped this deploy"
            ),
            VerbError::Failed(reason) => f.write_str(reason),
        }
    }
}

/// `continuum engine <verb>`: the ONE implementation the bash and PowerShell installers share.
///   `idle-slot`                      print the directory to build the next engine into
///   `promote <slot> <commit:backend>` make a verified slot current
///   `rollback <failed-slot>`          put the previous engine back while `failed-slot` is current
/// Callers hold the deploy lock across idle-slot, the build and promote (Codex on the card: a
/// snapshot of idle is only good while no other builder or launch can move it).
pub fn run_verb(args: &[String]) -> Result<String, VerbError> {
    // The same home `server_bin` resolves the engine under: one root, never two.
    let home = crate::paths::continuum_home().map_err(|e| VerbError::Failed(e.to_string()))?;
    let root = root(&home).map_err(VerbError::Failed)?;
    let usage = || {
        VerbError::Failed(
            "usage: continuum engine root | idle-slot --owner-pid <pid> [--exclude-slot <slot>]... | release-slot <slot> --owner-pid <pid> | promote <slot> <commit:backend> | attempt <slot> | retry <slot> <commit:backend> <artifact-identity> | rollback <failed-slot>".into(),
        )
    };
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["root"] => Ok(root.to_string_lossy().into_owned()),
        ["idle-slot", options @ ..] => {
            let mut reserved = Vec::new();
            let mut owner = None;
            for pair in options.chunks(2) {
                match pair {
                    ["--exclude-slot", slot] if SLOTS.contains(slot) => reserved.push(*slot),
                    ["--owner-pid", pid] => owner = Some(pid.parse::<u32>().map_err(|_| usage())?),
                    _ => return Err(usage()),
                }
            }
            // The claim is the builder's (the installer script's process), so it outlives this
            // verb; the path is printed only once it is held.
            let owner = owner.ok_or_else(usage)?;
            let slot = claim::claim_idle_slot(&root, &reserved, owner, live_lane_engines, &crate::inference::engine_residency::probe_process)?;
            Ok(root.join(slot).to_string_lossy().into_owned())
        }
        ["release-slot", slot, "--owner-pid", pid] => {
            let pid = pid.parse::<u32>().map_err(|_| usage())?;
            claim::release_build_claim(&root, slot, pid).map_err(VerbError::Failed)
        }
        ["promote", slot, stamp] | ["retry", slot, stamp, _] => {
            // A live lane may run the slot only when promoting it switches nothing: it is
            // already current with exactly this stamp (Codex installing on the 5090: the node
            // already ran the verified engine the release names, and the refusal blocked the
            // whole install). A real switch onto a slot a lane runs is still refused.
            if !is_current_with(&root, slot, stamp) {
                let live = live_lane_engines().map_err(VerbError::Failed)?;
                if live.iter().flatten().any(|bin| slot_of(&root, bin) == Some(*slot)) {
                    return Err(VerbError::Failed(format!("{slot} is run by a live lane: it was never idle")));
                }
            }
            let retry = (args.first().map(String::as_str) == Some("retry"))
                .then(|| args[3].as_str());
            match promote_with_retry(&root, slot, stamp, retry).map_err(VerbError::Failed)? {
                Promotion::Deferred(fallback) => Err(VerbError::DeferredFailedAttempt(format!(
                    "failed engine attempt held; verified {fallback} remains current; use engine retry with the exact artifact identity reported by engine attempt"
                ))),
                Promotion::AlreadyCurrent => Ok(format!("{slot} is already the current engine ({stamp}); nothing changed")),
                Promotion::Switched => Ok(format!("{slot} is the current engine ({stamp})")),
            }
        }
        ["attempt", slot] => attempts::describe(&root, known(slot).map_err(VerbError::Failed)?)
            .map_err(VerbError::Failed),
        ["rollback", failed] => {
            let back = rollback(&root, failed).map_err(VerbError::Failed)?;
            Ok(format!("{back} is the current engine again ({failed} kept as previous)"))
        }
        _ => Err(usage()),
    }
}

/// Where the engine the core launches came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// `LLAMA_SERVER_BIN`: the operator's engine, launched as given and never converged, even
    /// when it points inside a slot (Codex on the card: a path cannot tell an installer's
    /// injection from a deliberate pin, so provenance is fixed where the injection was, the
    /// Windows service-host, which now records `current` instead, see [`register`]).
    Operator(String),
    /// The active slot's binary.
    Slot(&'static str),
    /// The pre-slot owned install (`<bin>/llama-server`), today's Macs until their first slot
    /// install.
    Legacy(PathBuf),
    /// Nothing owned: the caller's bare PATH lookup. Never launched: a launch runs only the
    /// slot its pin hold checked (`llama_server::checked_engine`).
    Path,
}

/// PURE: the engine the core launches: an operator override, then the slot `current` names,
/// then the pre-slot install when it exists.
pub fn resolve(root: &Path, operator_override: Option<&str>, current: Option<&'static str>) -> Resolved {
    if let Some(bin) = operator_override {
        return Resolved::Operator(bin.to_string());
    }
    // A recorded `current` is the engine even when its binary is gone: the spawn then fails
    // loud naming that path, never quietly launching the pre-slot engine instead.
    if let Some(slot) = current {
        return Resolved::Slot(slot);
    }
    let legacy = root.join(exe_name());
    if legacy.is_file() {
        return Resolved::Legacy(legacy);
    }
    Resolved::Path
}

/// The build (`<commit>:<backend>` stamp) of the engine binary `bin`: the stamp of the slot it was
/// launched from. `None` for a binary outside the slots (a pre-slot install), where one engine
/// cannot be told from another.
pub fn stamp_of_bin(bin: &Path) -> Option<String> {
    let home = crate::paths::continuum_home().ok()?;
    let root = root(&home).ok()?;
    slot_stamp(&root, slot_of(&root, bin)?)
}

/// The build of the engine the live lane RUNS (not the slot most recently promoted: a lane
/// relaunches onto a new engine after it is installed): the identity a training measurement was
/// taken against (card de9e1342).
pub fn live_lane_build() -> Option<String> {
    stamp_of_bin(crate::inference::lane_registry::live_lane()?.engine_bin.as_deref()?)
}

/// A slot's stamp, trimmed, when it has one.
pub fn slot_stamp(root: &Path, slot: &str) -> Option<String> {
    std::fs::read_to_string(root.join(slot).join(STAMP_FILE)).ok().map(|s| s.trim().to_string())
}

/// What a successful [`promote`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Promotion {
    /// This artifact failed previously; a verified fallback stays current.
    Deferred(&'static str),
    /// `slot` became current; the slot it replaced is `previous`, and `slot` must come up
    /// ready again before it counts as verified.
    Switched,
    /// `slot` was already current with this exact stamp: nothing moved, so `previous` and its
    /// verification stand.
    AlreadyCurrent,
}

/// Whether `slot` is the current engine, with its binary present and exactly `want_stamp`.
pub fn is_current_with(root: &Path, slot: &str, want_stamp: &str) -> bool {
    current_slot(root) == Some(slot)
        && slot_bin(root, slot).is_file()
        && slot_stamp(root, slot).as_deref() == Some(want_stamp.trim())
}

/// Make `slot` the active engine: refused unless its binary exists and its stamp is exactly
/// `want_stamp` (the pin and backend the deploy built), so a failed or partial build is never
/// promoted. The slot it replaces becomes `previous`. Each pointer is replaced atomically
/// (write a temp file, then rename); `previous` is written first, so a crash between the two
/// leaves the old engine current and a harmless `previous`.
pub fn promote(root: &Path, slot: &str, want_stamp: &str) -> Result<Promotion, String> {
    promote_with_retry(root, slot, want_stamp, None)
}

fn promote_with_retry(root: &Path, slot: &str, want_stamp: &str, retry: Option<&str>) -> Result<Promotion, String> {
    let slot = known(slot)?;
    let bin = slot_bin(root, slot);
    if !bin.is_file() {
        return Err(format!("{} has no engine binary; build it before promoting", bin.display()));
    }
    let stamp = slot_stamp(root, slot)
        .ok_or_else(|| format!("{slot} has no stamp: the build did not verify, so it is not promoted"))?;
    if stamp != want_stamp.trim() {
        return Err(format!("{slot} holds {stamp} but the deploy built {}: not promoted", want_stamp.trim()));
    }
    if current_slot(root) == Some(slot) {
        return Ok(Promotion::AlreadyCurrent);
    }
    let admission = attempts::admit(root, slot, &stamp, retry)?;
    if !admission {
        let fallback = current_slot(root)
            .filter(|s| is_verified(root, s) && slot_bin(root, s).is_file() && slot_stamp(root, s).is_some())
            .ok_or("failed engine attempt held but no verified fallback exists; explicit retry required")?;
        return Ok(Promotion::Deferred(fallback));
    }
    match current_slot(root) {
        // Already current with this stamp: a reinstall of the engine the node runs. Clearing
        // VERIFIED here demoted a proven engine to unverified for no change (Codex, 5090).
        Some(old) if old == slot => return Ok(Promotion::AlreadyCurrent),
        Some(old) => write_pointer(root, PREVIOUS_FILE, old)?,
        None => {}
    }
    clear_verified(root, slot)?;
    write_pointer(root, CURRENT_FILE, slot)?;
    Ok(Promotion::Switched)
}

/// Record that `slot`'s engine came up ready on this machine. Idempotent.
pub fn mark_verified(root: &Path, slot: &str) -> Result<(), String> {
    let slot = known(slot)?;
    if let Err(reason) = attempts::ready(root, slot) {
        continuum_base::probe!(class = "serving.engine.attempt_receipt_failed", reason = reason.as_str(),
            "engine is ready but prior attempt bookkeeping could not be reconciled");
    }
    let path = root.join(slot).join(VERIFIED_FILE);
    if path.is_file() {
        return Ok(());
    }
    std::fs::write(&path, b"").map_err(|e| format!("{}: {e}", path.display()))
}

/// Whether `slot`'s engine has come up ready on this machine since it was last promoted.
pub fn is_verified(root: &Path, slot: &str) -> bool {
    root.join(slot).join(VERIFIED_FILE).is_file()
}

fn clear_verified(root: &Path, slot: &str) -> Result<(), String> {
    match std::fs::remove_file(root.join(slot).join(VERIFIED_FILE)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{slot}: {e}")),
        _ => Ok(()),
    }
}

/// Put `previous` back as the active engine because `failed` failed at launch (Fable on the
/// card: a promoted engine can pass its verify and still fail at launch, a CUDA DLL, a driver,
/// and on a first-and-only machine that is a dark node). Only while `current` is still
/// `failed` (Codex: a delayed failure report must not undo a newer, good promotion), and only
/// onto a `previous` whose binary and stamp are intact. The failed slot becomes `previous`.
/// Returns the slot now current. Callers hold the deploy lock, as for [`promote`].
pub fn rollback(root: &Path, failed: &str) -> Result<&'static str, String> {
    let current = current_slot(root);
    if current != Some(failed) {
        return Err(format!(
            "current is {} not {failed}: a newer promotion stands, not rolled back",
            current.unwrap_or("unrecorded") // unwrap_or: names the absent pointer in the refusal
        ));
    }
    let back = previous_slot(root).ok_or("no previous engine is recorded: nothing to roll back to")?;
    if back == failed || !slot_bin(root, back).is_file() || slot_stamp(root, back).is_none() {
        return Err(format!("the previous engine ({back}) is not intact: not rolled back"));
    }
    write_pointer(root, PREVIOUS_FILE, failed)?;
    write_pointer(root, CURRENT_FILE, back)?;
    Ok(back)
}

/// The actual selection before a supervised core handoff. The caller holds the
/// deployment authority throughout capture, promotion and recovery. A release's
/// descriptor is not the selection: an earlier engine convergence may have moved it.
pub struct HandoffSelection {
    root: PathBuf,
    promoted: &'static str,
    prior: Option<&'static str>,
}

impl HandoffSelection {
    pub fn capture(root: &Path, candidate: &Path) -> Result<Self, String> {
        let promoted = slot_of(root, candidate).ok_or("handoff engine is not a managed slot")?;
        let prior = match std::fs::read_to_string(root.join(CURRENT_FILE)) {
            Ok(value) => Some(known(value.trim())?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("read prior engine selection: {error}")),
        };
        Ok(Self { root: root.to_owned(), promoted, prior })
    }

    pub fn confirm(&self) -> Result<(), String> {
        if current_slot(&self.root) != Some(self.promoted) {
            return Err("engine selection changed during handoff promotion".into());
        }
        Ok(())
    }

    /// A deferred candidate permits the core handoff only while the captured,
    /// verified fallback is still selected. Keep this selection's restore
    /// authority: later core launch failure must not invent a candidate swap.
    pub fn confirm_preserved(&self) -> Result<(), String> {
        let prior = self.prior.ok_or("deferred engine has no prior selection")?;
        if current_slot(&self.root) != Some(prior)
            || !is_verified(&self.root, prior)
            || !slot_bin(&self.root, prior).is_file()
            || slot_stamp(&self.root, prior).is_none()
        {
            return Err("deferred engine fallback is not the captured verified selection".into());
        }
        Ok(())
    }

    /// Restore before starting the previous core, whose reader may not understand
    /// the new engine receipt. Normal launch-failure rollback deliberately refuses
    /// during deployment; this caller already owns that authority.
    pub fn restore(&self) -> Result<(), String> {
        // Preparation can refuse before promotion, or a previous recovery may
        // already have restored this exact selection. Neither changes a pointer.
        if current_slot(&self.root) == self.prior { return Ok(()); }
        self.confirm()?;
        match self.prior {
            Some(prior) => {
                if previous_slot(&self.root) != Some(prior) {
                    return Err("previous engine changed during handoff; recovery refused".into());
                }
                rollback(&self.root, self.promoted)?;
            }
            None => {
                // A first installation had no selection; do not invent one from
                // a stale release descriptor or an unrelated previous pointer.
                std::fs::remove_file(self.root.join(CURRENT_FILE))
                    .map_err(|error| format!("restore absent engine selection: {error}"))?;
            }
        }
        Ok(())
    }
}

/// Record the engine a Windows service registration names as `current`: the service-host's
/// replacement for injecting `LLAMA_SERVER_BIN` (which the core must read as an operator's pin,
/// so Windows never converged). On Windows the registered release IS the promotion
/// (`Prepare-CoreServiceEngine` built it into a slot and checked its drift against the pin
/// before registering), so this moves `current` to it like [`promote`], with the replaced slot
/// kept as `previous`; it needs the binary and a stamp, not a pin (the script already compared
/// that). Returns whether `current` changed.
pub fn register(root: &Path, slot: &str) -> Result<bool, String> {
    let slot = known(slot)?;
    if !slot_bin(root, slot).is_file() || slot_stamp(root, slot).is_none() {
        return Err(format!("{slot} has no stamped engine: not registered"));
    }
    let stamp = slot_stamp(root, slot).ok_or("registered engine stamp disappeared")?;
    promote(root, slot, &stamp).map(|result| matches!(result, Promotion::Switched))
}

/// The Windows service-host's start: BOOTSTRAP `current` from the engine its registered release
/// names, only when `current` is absent or names a slot without a stamped engine. Otherwise
/// `current` stands and the core launches it (card d5584dfc, option (b), Fable): `current` is the
/// one truth on every OS, so a promotion by an unattended deploy (`continuum engine promote`, an
/// unprivileged file write) and an automatic rollback both survive a restart, and a Windows
/// engine update never needs the task re-registered (an elevation no unattended deploy can
/// answer). The slot root is derived from the engine path (`<root>/<slot>/llama-server.exe`).
/// A standing `current` wins before the release is examined; only with nothing standing is an
/// engine outside the selected root (or unstamped) refused. A managed installation
/// must not turn that refusal into an operator pin. Returns whether `current` changed.
pub fn bootstrap_service_engine(root: &Path, engine: &Path) -> Result<bool, String> {
    // A standing engine wins before anything about the release is checked: the core runs
    // `current`, and a release that names a stale or foreign path does not stop it.
    let standing = current_slot(root).filter(|c| slot_bin(root, c).is_file() && slot_stamp(root, c).is_some());
    if standing.is_some() {
        return Ok(false);
    }
    let slot = slot_of(root, engine).ok_or_else(|| format!("{} is not in an engine slot", engine.display()))?;
    register(root, slot)
}

/// The engine directory the core runs: the slot `current` names when it holds a stamped engine
/// under `<continuum_home>/bin`. What the Windows deploy's drift check compares the pin against,
/// since with `current` as the truth the registered descriptor can name an older slot.
pub fn active_engine_dir() -> Result<Option<PathBuf>, String> {
    let home = crate::paths::continuum_home().map_err(|e| e.to_string())?;
    let root = root(&home)?;
    Ok(current_slot(&root)
        .filter(|slot| slot_bin(&root, slot).is_file() && slot_stamp(&root, slot).is_some())
        .map(|slot| root.join(slot)))
}

fn known(slot: &str) -> Result<&'static str, String> {
    SLOTS
        .into_iter()
        .find(|s| *s == slot)
        .ok_or_else(|| format!("'{slot}' is not an engine slot ({})", SLOTS.join(", ")))
}

fn write_pointer(root: &Path, file: &str, slot: &str) -> Result<(), String> {
    let tmp = root.join(format!("{file}.tmp"));
    std::fs::write(&tmp, format!("{slot}\n")).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, root.join(file)).map_err(|e| format!("{}: {e}", root.join(file).display()))
}

#[cfg(test)]
mod tests;
