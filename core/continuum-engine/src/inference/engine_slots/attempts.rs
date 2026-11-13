//! Durable admission disposition owned by engine slots, not by a core process.
//! One record per managed slot bounds state with the existing payload slots.
//! Reusing a slot for a different admitted artifact replaces that slot's history;
//! this is resident deployment disposition, not a permanent artifact blacklist.
use super::*;
use continuum_cli_lifecycle::installed_release::MAX_RECEIPT_BYTES;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

const FILE: &str = ".engine-attempt.json";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attempt {
    schema: u32,
    stamp: String,
    /// Complete verified application closure, or absent for a legacy stamp.
    artifact: Option<String>,
    admitted_at_ms: u64,
    failed_at_ms: Option<u64>,
    restored: Option<String>,
    reason: Option<String>,
}

impl Attempt {
    fn identity(&self) -> String {
        match &self.artifact {
            Some(hash) => format!("receipt:{hash}"),
            None => format!("legacy:{}", self.stamp),
        }
    }
}

fn read(root: &Path, slot: &str) -> Result<Option<Attempt>, String> {
    let path = root.join(slot).join(FILE);
    let meta = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("engine attempt metadata: {e}")),
    };
    if meta.len() > MAX_RECEIPT_BYTES {
        return Err("engine attempt receipt oversized; promotion refused".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&path)
        .and_then(|file| file.take(MAX_RECEIPT_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("engine attempt read: {e}"))?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(
            "engine attempt receipt grew beyond lifecycle receipt bound; promotion refused".into(),
        );
    }
    let record: Attempt = serde_json::from_slice(&bytes)
        .map_err(|e| format!("engine attempt receipt corrupt; promotion refused: {e}"))?;
    if record.schema != 1 || record.stamp.is_empty() {
        return Err("unsupported engine attempt receipt; promotion refused".into());
    }
    Ok(Some(record))
}

fn write(root: &Path, slot: &str, record: &Attempt) -> Result<(), String> {
    let path = root.join(slot).join(FILE);
    let temporary = path.with_extension("tmp");
    let bytes = serde_json::to_vec(record).map_err(|e| e.to_string())?; // Disk boundary: durable per-slot attempt receipt read by later core/deploy processes.
    let mut staged =
        std::fs::File::create(&temporary).map_err(|e| format!("engine attempt create: {e}"))?;
    staged
        .write_all(&bytes)
        .and_then(|_| staged.sync_all())
        .map_err(|e| format!("engine attempt write: {e}"))?;
    drop(staged);
    continuum_host::durable::replace(&temporary, &path)
        .map_err(|e| format!("engine attempt commit: {e}"))
}

pub(super) fn describe(root: &Path, slot: &str) -> Result<String, String> {
    let candidate = candidate(
        root,
        slot,
        &slot_stamp(root, slot).ok_or("engine stamp absent")?,
    )?;
    let record = read(root, slot)?;
    Ok(format!(
        "{}: candidate={} recorded={} (failed={}, environment=unknown)",
        slot,
        candidate.identity(),
        record
            .as_ref()
            .map(Attempt::identity)
            .unwrap_or_else(|| "none".into()),
        record
            .as_ref()
            .is_some_and(|record| record.failed_at_ms.is_some())
    ))
}

pub(super) fn failed_selection(root: &Path, slot: &str) -> Result<bool, String> {
    Ok(read(root, slot)?.is_some_and(|record| {
        record.failed_at_ms.is_some()
            && slot_stamp(root, slot).as_deref() == Some(record.stamp.as_str())
    }))
}

pub(super) fn ready(root: &Path, slot: &str) -> Result<(), String> {
    let Some(current) = read(root, slot)? else {
        return Ok(());
    };
    for other in SLOTS {
        if let Some(mut record) = read(root, other)? {
            if (record.identity() == current.identity()
                || (record.artifact.is_none() && record.stamp == current.stamp))
                && record.failed_at_ms.is_some()
            {
                record.failed_at_ms = None;
                record.reason = None;
                record.restored = None;
                write(root, other, &record)?;
            }
        }
    }
    Ok(())
}

pub(super) fn admit(
    root: &Path,
    slot: &str,
    stamp: &str,
    retry: Option<&str>,
) -> Result<bool, String> {
    let record = candidate(root, slot, stamp)?;
    let identity = record.identity();
    if retry.is_some_and(|supplied| supplied != identity) {
        return Err(
            "retry artifact identity differs from verified candidate; nothing cleared".into(),
        );
    }
    for other in SLOTS {
        if let Some(previous) = read(root, other)? {
            if previous.failed_at_ms.is_some()
                && (previous.identity() == identity
                    || (previous.artifact.is_none() && previous.stamp == stamp))
                && retry.is_none()
            {
                return Ok(false);
            }
        }
    }
    write(root, slot, &record)?;
    Ok(true)
}

fn candidate(root: &Path, slot: &str, stamp: &str) -> Result<Attempt, String> {
    // Offline installer path only. Do not hash application bytes on launch failure.
    let artifact = super::super::engine_install::EngineInstallReceipt::promotion_identity(
        &slot_bin(root, slot),
    )?;
    Ok(Attempt {
        schema: 1,
        stamp: stamp.into(),
        artifact,
        admitted_at_ms: continuum_base::time::EpochMs::now().millis(),
        failed_at_ms: None,
        restored: None,
        reason: None,
    })
}

pub(super) fn failed(root: &Path, slot: &str, restored: &str, reason: &str) -> Result<(), String> {
    let stamp = slot_stamp(root, slot).ok_or("failed engine has no stamp")?;
    let mut record = match read(root, slot)? {
        Some(record) if record.stamp == stamp => record,
        // Old installations have no complete artifact evidence. Same-stamp binary
        // or DLL changes cannot silently certify a different application closure.
        _ => Attempt {
            schema: 1,
            stamp,
            artifact: None,
            admitted_at_ms: 0,
            failed_at_ms: None,
            restored: None,
            reason: None,
        },
    };
    record.failed_at_ms = Some(continuum_base::time::EpochMs::now().millis());
    record.restored = Some(restored.into());
    record.reason = Some(reason.into());
    write(root, slot, &record)
}
