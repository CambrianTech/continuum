//! Repair metadata from retained launch custody, never from listener discovery.
use super::*;
use crate::inference::engine_residency::{Liveness, ProcessProbe};
use std::io;

/// The caller must hold the child owner through this operation. A record discovered on disk
/// is not launch custody and must never be passed here as authority to recreate itself.
pub(crate) fn restore_owned(expected: &LaneRecord) -> io::Result<()> {
    let dir = lanes_dir().ok_or_else(|| io::Error::other("lane registry home unavailable"))?;
    let pidfile = lane_pidfile::pidfile_path()
        .ok_or_else(|| io::Error::other("lane pidfile home unavailable"))?;
    restore_in(
        &dir,
        &pidfile,
        expected,
        &super::super::engine_residency::probe_process,
    )
}

fn restore_in(
    dir: &Path,
    pidfile: &Path,
    expected: &LaneRecord,
    probe: &impl Fn(u32) -> ProcessProbe,
) -> io::Result<()> {
    let _writes = writes();
    if expected.role != LaneRole::Live
        || expected.incarnation().liveness_with(probe) != Liveness::Alive
    {
        return Err(io::Error::other(
            "retained live engine incarnation is not proven alive",
        ));
    }
    // Unknown is not dead. In particular a conflicting pid with no readable record may be a
    // replacement being published; leave it alone unless the OS proves the process absent.
    let prior_pid = read_optional(pidfile)?;
    if let Some(pid) = prior_pid
        .as_ref()
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|pid| *pid != expected.pid)
    {
        if !matches!(probe(pid), ProcessProbe::Absent) {
            return Err(io::Error::other(
                "another canonical process has not been proven absent",
            ));
        }
    }
    let path = record_path(dir, expected.pid);
    let prior_record = read_optional(&path)?;
    // A missing canonical pointer does not erase a competing lifecycle's claim. Check all
    // other records, without using their directory order to select an engine to adopt.
    if dir.exists() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let other_path = entry.path();
            if other_path == path || other_path.extension().and_then(|s| s.to_str()) != Some("lane")
            {
                continue;
            }
            let other: LaneRecord = serde_json::from_slice(&std::fs::read(other_path)?)?;
            if other.role == LaneRole::Live
                && other.incarnation().liveness_with(probe) != Liveness::Dead
            {
                return Err(io::Error::other(
                    "another live lane record has not been proven dead",
                ));
            }
        }
    }
    if let Some(raw) = &prior_record {
        // A malformed file is damaged metadata, but a valid different incarnation is evidence
        // of a replacement, not permission to overwrite it.
        if let Ok(record) = serde_json::from_slice::<LaneRecord>(raw) {
            if record.incarnation() != expected.incarnation() || record.role != LaneRole::Live {
                return Err(io::Error::other(
                    "lane record belongs to a different incarnation",
                ));
            }
        }
    }
    // Preserve all other records. record_in is a fresh-launch operation and clears LIVE rows;
    // using it here would erase evidence belonging to a competing/replacement lifecycle.
    if expected.incarnation().liveness_with(probe) != Liveness::Alive
        || read_optional(pidfile)? != prior_pid
        || read_optional(&path)? != prior_record
    {
        return Err(io::Error::other("lane changed during metadata recovery"));
    }
    std::fs::create_dir_all(dir)?;
    if prior_record
        .as_ref()
        .and_then(|b| serde_json::from_slice::<LaneRecord>(b).ok())
        .is_none()
    {
        continuum_host::durable::write_replace(&path, encode(expected)?.as_bytes())?;
    }
    // A partial failure leaves a factual record and no fabricated canonical pointer. The next
    // existing reconciliation retries it; a process exit after this write is just a stale row.
    if expected.incarnation().liveness_with(probe) != Liveness::Alive
        || read_optional(pidfile)? != prior_pid
    {
        return Err(io::Error::other(
            "lane changed before canonical publication",
        ));
    }
    let desired = expected.pid.to_string();
    if prior_pid.as_deref() != Some(desired.as_bytes()) {
        if let Some(parent) = pidfile.parent() {
            std::fs::create_dir_all(parent)?;
        }
        continuum_host::durable::write_replace(pidfile, desired.as_bytes())?;
    }
    Ok(())
}

fn read_optional(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests;
