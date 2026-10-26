//! Engine install responsibility of the llama-server process owner.
use super::*;
/// Path to the `llama-server` binary: the engine WE OWN, built from the vendored llama.cpp
/// submodule into a slot under `<continuum_home>/bin` ([`engine_slots`]). Resolution:
///   1. `LLAMA_SERVER_BIN` (launch environment, then config): an operator's own engine,
///      launched as given and never converged, wherever it points;
///   2. the slot `current` names;
///   3. the pre-slot owned install `<continuum_home>/bin/llama-server[.exe]`;
///   4. bare `"llama-server"` (let the OS resolve it on `PATH`).
/// We do NOT silently fall back to a different engine: a missing binary surfaces loudly when
/// spawn is attempted ([[fallbacks-are-illegal-fail-loud]]).
///
/// [`engine_slots`]: crate::inference::engine_slots
pub(super) fn server_bin() -> Result<String, LlamaServerError> {
    use crate::inference::engine_slots::{self as slots, Resolved};
    let over = engine_override();
    let Ok(home) = crate::paths::continuum_home() else {
        return Ok(over.unwrap_or_else(|| "llama-server".to_string())); // no home: only an override or PATH can name an engine
    };
    if let Some(bin) = over { return Ok(bin); }
    let root = slots::root(&home).map_err(LlamaServerError::Spawn)?;
    let as_string = |p: PathBuf| p.to_string_lossy().into_owned();
    Ok(match slots::resolve(&root, None, slots::current_slot(&root)) {
        Resolved::Operator(bin) => bin,
        Resolved::Slot(slot) => as_string(slots::slot_bin(&root, slot)),
        Resolved::Legacy(bin) => as_string(bin),
        Resolved::Path => "llama-server".to_string(),
    })
}


/// `LLAMA_SERVER_BIN` (launch environment, then config): an operator's own engine.
pub(crate) fn engine_override() -> Option<String> {
    std::env::var("LLAMA_SERVER_BIN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| continuum_base::config_env::read("LLAMA_SERVER_BIN"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}


/// A launch failed at spawn or readiness: when it ran from the current engine slot, put the
/// previous engine back (card 2c5d0ec0, Fable on #4491). Never under an operator override (their
/// engine, not a slot) and never while a deploy holds the claim (it is mid-promote and owns the
/// pointers). Returns whether the next launch will take a different engine.
pub(super) fn roll_back_engine_after_failed_launch(error: &LlamaServerError) -> bool {
    if !matches!(error, LlamaServerError::Spawn(_) | LlamaServerError::NotReady(..)) || engine_override().is_some() {
        return false;
    }
    let Ok(home) = crate::paths::continuum_home() else {
        return false; // no home: no slots to roll between
    };
    let now_ms = continuum_base::time::EpochMs::now().millis(); // pre-1970 reads 0: a clock before the epoch reads as age 0, which keeps a live claim blocking
    if continuum_cli_lifecycle::deploy_claim::in_flight(&home, now_ms).blocks() {
        return false;
    }
    let failure_class = if matches!(error, LlamaServerError::Spawn(_)) { "spawn failed" } else { "readiness failed" };
    match crate::inference::engine_slots::root(&home).and_then(|root| crate::inference::engine_slots::rollback_after_failed_launch_reason(&root, failure_class)) {
        Ok(Some((failed, restored))) => {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.rolled_back",
                failed = failed,
                restored = restored,
                error = error.to_string().as_str(),
                "a launch from the current engine slot failed: the previous engine is current again \
                 and the lane relaunches on it"
            );
            true
        }
        Ok(None) => false,
        Err(reason) => {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.rollback_refused",
                reason = reason.as_str(),
                error = error.to_string().as_str(),
                "a launch from the current engine slot failed and no intact previous engine can be restored"
            );
            false
        }
    }
}


/// A launch came up ready: when it ran from an engine slot, that engine has proven itself on this
/// machine, and a later failure of it is never read as a promotion fault
/// ([`crate::inference::engine_slots::rollback_after_failed_launch`]). `program` is the binary
/// the launch actually ran, not a fresh resolution (a promote may have moved `current` since).
pub(super) fn mark_engine_proven(program: Option<String>) {
    let (Some(program), Ok(home)) = (program, crate::paths::continuum_home()) else {
        return; // no launch record or no home: nothing to mark
    };
    let root = match crate::inference::engine_slots::root(&home) {
        Ok(root) => root,
        Err(reason) => {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.verify_mark_failed", reason = reason.as_str(), "payload location unavailable; engine proof was not written");
            return;
        }
    };
    if let Some(slot) = crate::inference::engine_slots::slot_of(&root, Path::new(&program)) {
        if let Err(reason) = crate::inference::engine_slots::mark_verified(&root, slot) {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.engine.verify_mark_failed",
                slot = slot,
                reason = reason.as_str(),
                "an engine came up ready but its proof could not be written: a later failure of it may be rolled back"
            );
        }
    }
}


/// The engine commit the INSTALLED binary was built from: the stamp install-llama-server.sh
/// writes beside it (`<commit>:<backend>`, written last, only after the build verified). The
/// operand a running lane's own `/props` build is compared against (card 7c5f139d). `None`
/// when the engine is an operator override (`LLAMA_SERVER_BIN`: theirs, not ours to converge)
/// or no stamp exists: nothing to compare, never a relaunch.
pub(crate) fn installed_engine_commit() -> Option<String> {
    let stamp = installed_engine_stamp()?;
    let commit = stamp.split(':').next()?.trim();
    (!commit.is_empty()).then(|| commit.to_string())
}


/// The stamp install-llama-server.sh writes beside the owned engine, trimmed.
pub(super) fn installed_engine_stamp() -> Option<String> {
    let bin = server_bin().ok()?;
    let path = std::path::Path::new(&bin);
    if !path.is_absolute() {
        return None; // a bare PATH lookup: not the owned install
    }
    let stamp = std::fs::read_to_string(path.parent()?.join(".llama-server.stamp")).ok()?;
    Some(stamp.trim().to_string())
}


/// PURE: whether a running lane's `/props` `build_info` (`b10765-965d38a90`) names the
/// installed engine commit, compared with the ONE SHA rule `deploy_provenance::sha_matches`
/// (prefix either way, credible hex). A lane is stale only when both commits are credible
/// SHAs and they disagree; anything unreadable is not evidence (BigMama on 7c5f139d).
pub(crate) fn lane_runs_installed_engine(build_info: &str, installed: &str) -> bool {
    use continuum_cli_lifecycle::deploy_provenance::sha_matches;
    let lane = build_commit(build_info);
    let credible = |sha: &str| sha_matches(sha, sha); // a SHA matches itself iff it is credible
    if !credible(lane) || !credible(installed) {
        return true;
    }
    sha_matches(lane, installed)
}


/// PURE: the commit a `/props` `build_info` (`b10765-965d38a90`) names: its last `-` piece,
/// trimmed. The one parse for every reader of a lane's build.
pub(super) fn build_commit(build_info: &str) -> &str {
    build_info.rsplit('-').next().unwrap_or(build_info).trim() // unwrap_or: rsplit always yields one piece
}


/// The stale-engine fact the reconcile last saw (`from -> to`), for the hour's health line
/// (BigMama on 7c5f139d): "this node cannot dream" is read there, never inferred.
pub(super) static ENGINE_STALE: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);


/// The `from->to` engine pair this core already relaunched a lane for: a second sighting is
/// reported, never relaunched again (Fable on #4464).
pub(super) static ENGINE_CONVERGE_TRIED: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);


/// `from->to` while the serving lane runs an older engine than the installed one; `None`
/// once it has converged, or when that cannot be known.
pub fn engine_stale() -> Option<String> {
    ENGINE_STALE.lock().clone()
}

