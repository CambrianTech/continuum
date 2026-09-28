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

/// The slots, bounded by construction: the current engine, one a lane may still run, and one to
/// build into.
pub const SLOTS: [&str; 3] = ["engine-a", "engine-b", "engine-c"];

/// The file naming the active slot.
const CURRENT_FILE: &str = "current";

/// The file naming the slot `current` replaced: the rollback when a promoted engine fails.
const PREVIOUS_FILE: &str = "previous";

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

/// `<continuum_home>/bin`, where the slots and the pointer files live.
pub fn root(continuum_home: &Path) -> PathBuf {
    continuum_home.join("bin")
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

fn named_slot(root: &Path, file: &str) -> Option<&'static str> {
    let named = std::fs::read_to_string(root.join(file)).ok()?;
    let named = named.trim();
    SLOTS.into_iter().find(|s| *s == named)
}

/// PURE: the slot a binary path sits in, if any. How a legacy operator override that points
/// INSIDE a slot (what the Windows service-host set before `current` existed) is read as the
/// slot it names rather than as an operator's own engine.
pub fn slot_of(root: &Path, bin: &Path) -> Option<&'static str> {
    // The service-host's descriptor carries Windows verbatim paths (`\\?\C:\...`), which
    // `starts_with` never matches against a plain root.
    let text = bin.to_string_lossy();
    let bin = Path::new(text.strip_prefix(r"\\?\").unwrap_or(&text)); // unwrap_or: a path with no verbatim prefix is already plain
    SLOTS.into_iter().find(|slot| bin.starts_with(root.join(slot)))
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
    let unknown_lane = live.iter().any(Option::is_none);
    let free: Vec<&'static str> = SLOTS
        .into_iter()
        .filter(|slot| Some(*slot) != current)
        .filter(|slot| !live.iter().flatten().any(|bin| slot_of(root, bin) == Some(*slot)))
        .filter(|slot| !(unknown_lane && populated(slot)))
        .collect();
    free.iter()
        .find(|slot| Some(**slot) != previous)
        .or(free.first())
        .copied()
        .ok_or(NotIdle::AllInUse)
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

/// Why an `engine` verb failed: [`VerbError::NoIdleSlot`] is its own exit code (3), so the
/// install scripts skip the engine build this deploy and say so rather than fail the deploy.
#[derive(Debug)]
pub enum VerbError {
    NoIdleSlot,
    Failed(String),
}

impl std::fmt::Display for VerbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerbError::NoIdleSlot => write!(
                f,
                "every engine slot is current or run by a live lane: the engine build is skipped this deploy"
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
    let home = crate::commands::benchmark::continuum_home().map_err(|e| VerbError::Failed(e.to_string()))?;
    let root = root(&home);
    let usage = || {
        VerbError::Failed(
            "usage: continuum engine idle-slot | promote <slot> <commit:backend> | rollback <failed-slot>".into(),
        )
    };
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["idle-slot"] => {
            let live = live_lane_engines().map_err(VerbError::Failed)?;
            let slot = idle_slot(&root, current_slot(&root), previous_slot(&root), &live, |s| {
                slot_bin(&root, s).is_file()
            })
            .map_err(|NotIdle::AllInUse| VerbError::NoIdleSlot)?;
            Ok(root.join(slot).to_string_lossy().into_owned())
        }
        ["promote", slot, stamp] => {
            let live = live_lane_engines().map_err(VerbError::Failed)?;
            if live.iter().flatten().any(|bin| slot_of(&root, bin) == Some(*slot)) {
                return Err(VerbError::Failed(format!("{slot} is run by a live lane: it was never idle")));
            }
            promote(&root, slot, stamp).map_err(VerbError::Failed)?;
            Ok(format!("{slot} is the current engine ({stamp})"))
        }
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
    /// Nothing owned: the caller's bare PATH lookup, which fails loud at spawn.
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

/// A slot's stamp, trimmed, when it has one.
pub fn slot_stamp(root: &Path, slot: &str) -> Option<String> {
    std::fs::read_to_string(root.join(slot).join(STAMP_FILE)).ok().map(|s| s.trim().to_string())
}

/// Make `slot` the active engine: refused unless its binary exists and its stamp is exactly
/// `want_stamp` (the pin and backend the deploy built), so a failed or partial build is never
/// promoted. The slot it replaces becomes `previous`. Each pointer is replaced atomically
/// (write a temp file, then rename); `previous` is written first, so a crash between the two
/// leaves the old engine current and a harmless `previous`.
pub fn promote(root: &Path, slot: &str, want_stamp: &str) -> Result<(), String> {
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
    match current_slot(root) {
        Some(old) if old != slot => write_pointer(root, PREVIOUS_FILE, old)?,
        _ => {}
    }
    write_pointer(root, CURRENT_FILE, slot)
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
    match current_slot(root) {
        Some(same) if same == slot => return Ok(false),
        Some(old) => write_pointer(root, PREVIOUS_FILE, old)?,
        None => {}
    }
    write_pointer(root, CURRENT_FILE, slot).map(|()| true)
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
mod tests {
    use super::*;

    fn engine(root: &Path, slot: &str, stamp: &str) {
        std::fs::create_dir_all(root.join(slot)).unwrap();
        std::fs::write(slot_bin(root, slot), b"bin").unwrap();
        std::fs::write(root.join(slot).join(STAMP_FILE), format!("{stamp}\n")).unwrap();
    }

    // what this catches (card 2c5d0ec0): an engine build landing in a slot a lane still runs
    // from (on Windows a locked exe fails the copy; on Unix a live lane's binary is replaced
    // under it), or over the rollback while another slot is free. With every slot in use the
    // deploy is refused, never guessed.
    #[test]
    fn the_idle_slot_is_one_no_lane_runs_and_current_does_not_name_sparing_the_rollback() {
        let root = PathBuf::from("/h/.continuum/bin");
        let lane = |s: &str| Some(slot_bin(&root, s));
        let all = |_: &str| true;
        assert_eq!(idle_slot(&root, None, None, &[], all), Ok("engine-a"), "a fresh node: the first install is the same path");
        assert_eq!(idle_slot(&root, Some("engine-a"), None, &[lane("engine-a")], all), Ok("engine-b"));
        assert_eq!(idle_slot(&root, Some("engine-b"), Some("engine-a"), &[lane("engine-b")], all), Ok("engine-c"), "the rollback is spared");
        assert_eq!(idle_slot(&root, Some("engine-b"), Some("engine-c"), &[lane("engine-a")], all), Ok("engine-c"), "a lane on a holds it; the rollback is the only free slot");
        assert_eq!(idle_slot(&root, Some("engine-a"), None, &[lane("engine-b"), lane("engine-c")], all), Err(NotIdle::AllInUse));
        assert_eq!(idle_slot(&root, None, None, &[Some(root.join(exe_name()))], all), Ok("engine-a"), "a lane on the pre-slot install holds no slot");
        assert_eq!(
            idle_slot(&root, None, None, &[Some(root.join("engine-ab").join(exe_name()))], all),
            Ok("engine-a"),
            "a sibling dir sharing a prefix is not the slot"
        );
        // A lane recorded before lanes named their engine may run any populated slot: only an
        // empty one is idle (the 5090 today: a and b populated, a lane up, c empty).
        let a_and_b = |s: &str| s != "engine-c";
        assert_eq!(idle_slot(&root, None, None, &[None], a_and_b), Ok("engine-c"));
        assert_eq!(idle_slot(&root, None, None, &[None], all), Err(NotIdle::AllInUse), "fail closed, never over a maybe-running engine");
    }

    // what this catches: a failed or partial build promoted to current, and a rollback that
    // lands on nothing. Promotion needs the binary AND the exact stamp built; the replaced slot
    // becomes previous; rollback swaps back only onto an intact engine.
    #[test]
    fn only_a_verified_slot_is_promoted_and_a_rollback_swaps_back_onto_an_intact_engine() {
        let dir = tempfile::tempdir().expect("test: dir");
        let root = dir.path();
        assert_eq!(current_slot(root), None, "no current file: no slot");
        std::fs::create_dir_all(root.join("engine-b")).unwrap();
        assert!(promote(root, "engine-b", "abc123:cuda").is_err(), "no binary");
        std::fs::write(slot_bin(root, "engine-b"), b"bin").unwrap();
        assert!(promote(root, "engine-b", "abc123:cuda").is_err(), "no stamp: the build did not verify");
        std::fs::write(root.join("engine-b").join(STAMP_FILE), "old000:cuda\n").unwrap();
        assert!(promote(root, "engine-b", "abc123:cuda").is_err(), "stamp is not the pin built");
        engine(root, "engine-a", "aaa000:cuda");
        promote(root, "engine-a", "aaa000:cuda").expect("first install promotes");
        assert!(rollback(root, "engine-a").is_err(), "nothing recorded to roll back to");
        engine(root, "engine-b", "abc123:cuda");
        promote(root, "engine-b", "abc123:cuda").expect("verified slot promotes");
        assert_eq!((current_slot(root), previous_slot(root)), (Some("engine-b"), Some("engine-a")));
        assert!(rollback(root, "engine-a").is_err(), "a stale failure report for a slot no longer current");
        assert_eq!(current_slot(root), Some("engine-b"), "the newer promotion stands");
        assert_eq!(rollback(root, "engine-b"), Ok("engine-a"));
        assert_eq!((current_slot(root), previous_slot(root)), (Some("engine-a"), Some("engine-b")), "the failed slot is kept as previous");
        std::fs::remove_file(slot_bin(root, "engine-b")).unwrap();
        assert!(rollback(root, "engine-a").is_err(), "previous no longer intact");
        assert_eq!(current_slot(root), Some("engine-a"), "a refused rollback moves nothing");
        assert!(promote(root, "engine-z", "abc123:cuda").is_err(), "not a slot");
        std::fs::write(root.join(CURRENT_FILE), "../elsewhere\n").unwrap();
        assert_eq!(current_slot(root), None, "current naming anything but a slot is no slot");
    }

    // what this catches: the engine resolved from the wrong place. Every override is the
    // operator's, even inside a slot (provenance, not path). The Windows service-host's old
    // injection becomes `register`, which moves current to each newly registered release
    // (on Windows the registration is the promotion) and keeps the replaced one as previous.
    #[test]
    fn an_override_is_always_the_operators_and_a_service_registration_moves_current() {
        let dir = tempfile::tempdir().expect("test: dir");
        let root = dir.path();
        assert_eq!(resolve(root, None, None), Resolved::Path);
        std::fs::write(root.join(exe_name()), b"x").unwrap();
        assert_eq!(resolve(root, None, None), Resolved::Legacy(root.join(exe_name())));
        engine(root, "engine-a", "aaa000:cuda");
        let a = slot_bin(root, "engine-a").to_string_lossy().into_owned();
        assert_eq!(resolve(root, Some(&a), Some("engine-a")), Resolved::Operator(a.clone()), "a pin inside a slot is still a pin");
        assert!(register(root, "engine-c").is_err(), "no stamped engine: not registered");
        assert_eq!(register(root, "engine-a"), Ok(true));
        assert_eq!(register(root, "engine-a"), Ok(false), "every service start re-registers: idempotent");
        assert_eq!(resolve(root, None, current_slot(root)), Resolved::Slot("engine-a"));
        engine(root, "engine-b", "bbb000:cuda");
        assert_eq!(register(root, "engine-b"), Ok(true), "the next Windows release moves current");
        assert_eq!((current_slot(root), previous_slot(root)), (Some("engine-b"), Some("engine-a")));
        assert_eq!(resolve(root, None, Some("engine-c")), Resolved::Slot("engine-c"), "an empty current slot fails loud at spawn, never the legacy engine");
        assert_eq!(slot_of(root, Path::new(&format!(r"\\?\{a}"))), Some("engine-a"), "a verbatim-prefixed path is the same slot");
    }
}
