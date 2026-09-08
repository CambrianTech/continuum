//! Explicit selection of one legacy volatile checkpoint, never a merge or a
//! recency heuristic. Whole original bytes survive ordinary restore unchanged.
//! The shared checkpoint lock excludes participating readers/writers. Old
//! binaries cannot retroactively honor it: adoption additionally requires the
//! caller's offline check, and never claims the selected snapshot was flushed
//! at shutdown or authenticated to a process lifetime.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{volatile_path, OwnSpeechPersisted, PersistedVolatile};

// context-budget-exempt: bounds offline untrusted file decoding, not inference.
const MAX_CHECKPOINT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointSummary {
    pub build_sha: String,
    pub saved_at_ms: u64,
    pub next_action_seq: u64,
    pub entries: usize,
    pub result_receipts: usize,
    pub own_utterances: usize,
    /// Old roomless speech remains in the preserved bytes; ordinary restore
    /// cannot attribute it and intentionally does not inject it into a room.
    pub unscoped_own_utterances: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointSelection {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub summary: CheckpointSummary,
}

/// Public reviewable preconditions. The UUID is a caller declaration checked
/// against the selected directory; the legacy payload has no embedded UUID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptionPlan {
    pub format_version: u32,
    pub declared_persona_id: Uuid,
    pub source: CheckpointSelection,
    pub destination_path: PathBuf,
    pub prior_destination: Option<CheckpointSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdoptionAssurance {
    /// No old-core final-flush acknowledgment or embedded lifetime identity.
    SelectedLegacySnapshotWithDeclaredPersonaBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptionReceipt {
    pub plan: AdoptionPlan,
    pub assurance: AdoptionAssurance,
    /// Always false: adoption cannot manufacture an old-core flush receipt.
    pub legacy_final_flush_acknowledged: bool,
    pub source_archive: PathBuf,
    pub prior_destination_archive: Option<PathBuf>,
    pub receipt_path: PathBuf,
    /// True only when this exact completed plan still matches destination bytes.
    pub already_applied: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum AdoptionError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("invalid checkpoint selection: {0}")]
    Invalid(String),
    #[error("the selected source changed since inspection")]
    SourceChanged,
    #[error("the destination changed since inspection; nothing was overwritten")]
    DestinationChanged,
    #[error("adoption evidence conflicts or needs recovery: {0}")]
    EvidenceConflict(String),
}

fn invalid(detail: impl Into<String>) -> AdoptionError {
    AdoptionError::Invalid(detail.into())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, AdoptionError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(invalid("checkpoint/evidence must be a regular file"));
    }
    if metadata.len() > MAX_CHECKPOINT_BYTES {
        return Err(invalid("checkpoint exceeds the offline decoding limit"));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_CHECKPOINT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_CHECKPOINT_BYTES {
        return Err(invalid("checkpoint grew beyond the offline decoding limit"));
    }
    Ok(bytes)
}

fn summary(bytes: &[u8]) -> Result<CheckpointSummary, AdoptionError> {
    // Decode the existing storage schema without restoring, trimming, adding
    // resumed facts, or reserializing the selected memory.
    let persisted: PersistedVolatile = serde_json::from_slice(bytes)
        .map_err(|error| invalid(format!("volatile snapshot schema: {error}")))?;
    let wm = &persisted.wm;
    if wm.next_action_seq == 0 {
        return Err(invalid("next_action_seq must be positive"));
    }
    if wm.entries.iter().any(|entry| matches!(&entry.kind, super::super::working_memory::WmKind::Receipt { n } if *n == 0 || *n >= wm.next_action_seq)) {
        return Err(invalid("working-memory receipt sequence is outside the recorded counter"));
    }
    if wm
        .last_action
        .as_ref()
        .is_some_and(|(seq, _)| *seq == 0 || *seq >= wm.next_action_seq)
        || wm
            .recent_results
            .iter()
            .any(|(seq, _, _, _, _)| *seq == 0 || *seq >= wm.next_action_seq)
    {
        return Err(invalid("receipt sequence must precede next_action_seq"));
    }
    let mut previous = None;
    for (seq, _, _, _, _) in &wm.recent_results {
        if previous.is_some_and(|old| old >= *seq) {
            return Err(invalid("result receipt sequences must strictly increase"));
        }
        previous = Some(*seq);
    }
    let utterances = match &persisted.own_speech {
        OwnSpeechPersisted::ByRoom(rooms) => {
            let mut seen = std::collections::HashSet::new();
            for (room, _) in rooms {
                if !seen.insert(*room) {
                    return Err(invalid("duplicate own-speech room"));
                }
            }
            rooms.iter().map(|(_, speech)| speech.len()).sum()
        }
        OwnSpeechPersisted::Legacy(speech) => speech.len(),
    };
    Ok(CheckpointSummary {
        build_sha: wm.build_sha.clone(),
        saved_at_ms: wm.saved_at_ms,
        next_action_seq: wm.next_action_seq,
        entries: wm.entries.len(),
        result_receipts: wm.recent_results.len(),
        own_utterances: utterances,
        unscoped_own_utterances: match &persisted.own_speech {
            OwnSpeechPersisted::ByRoom(_) => 0,
            OwnSpeechPersisted::Legacy(speech) => speech.len(),
        },
    })
}

fn selection(path: &Path) -> Result<(CheckpointSelection, Vec<u8>), AdoptionError> {
    let bytes = read_bounded(path)?;
    let selected = CheckpointSelection {
        path: path.to_path_buf(),
        sha256: digest(&bytes),
        bytes: bytes.len() as u64,
        summary: summary(&bytes)?,
    };
    Ok((selected, bytes))
}

fn optional_selection(path: &Path) -> Result<Option<CheckpointSelection>, AdoptionError> {
    match selection(path) {
        Ok((selected, _)) => Ok(Some(selected)),
        Err(AdoptionError::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn resolved_path(path: &Path) -> Result<PathBuf, AdoptionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(invalid(
                    "symbolic checkpoint/evidence paths are not accepted",
                ));
            }
            Ok(fs::canonicalize(path)?)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| invalid("path has no parent"))?;
            let name = path
                .file_name()
                .ok_or_else(|| invalid("path has no filename"))?;
            Ok(resolved_path(parent)?.join(name))
        }
        Err(error) => Err(error.into()),
    }
}

fn source_path(path: &Path, persona: Uuid) -> Result<PathBuf, AdoptionError> {
    if persona.is_nil() {
        return Err(invalid("an explicit non-nil persona UUID is required"));
    }
    let path = resolved_path(&std::path::absolute(path)?)?;
    if path.file_name().and_then(|s| s.to_str()) != Some("volatile.json")
        || path
            .parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            != Some(persona)
    {
        return Err(invalid(
            "source must be <declared-persona-UUID>/volatile.json",
        ));
    }
    Ok(path)
}

/// Inspect only: no locks/files/directories are created and no winner is chosen
/// by sequence or timestamp. A missing destination is an explicit precondition.
/// Both the selected source and an existing destination must decode as the
/// current checkpoint schema. Recovery OVER a corrupt/incompatible destination
/// is deliberately unsupported here: its original bytes remain untouched and
/// the caller receives the decoding error, never an implicit overwrite policy.
pub fn inspect(source: &Path, persona_id: Uuid) -> Result<AdoptionPlan, AdoptionError> {
    let source = source_path(source, persona_id)?;
    let destination_path = resolved_path(&std::path::absolute(volatile_path(persona_id)?)?)?;
    if source == destination_path {
        return Err(invalid("source and destination must be distinct"));
    }
    let (source, _) = selection(&source)?;
    let prior_destination = optional_selection(&destination_path)?;
    Ok(AdoptionPlan {
        format_version: 1,
        declared_persona_id: persona_id,
        source,
        destination_path,
        prior_destination,
    })
}

fn exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Same file lock used by save, load and adoption. Callers keep the returned
/// handle alive until their complete file operation ends. Readers wait rather
/// than interpreting another owner's work as an absent checkpoint.
pub(super) fn lock_checkpoint(path: &Path, wait: bool) -> io::Result<File> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "checkpoint has no parent"))?;
    fs::create_dir_all(parent)?;
    let lock_path = parent.join(".volatile.lock");
    match fs::symlink_metadata(&lock_path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "checkpoint lock is not a regular file",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    if wait {
        FileExt::lock_exclusive(&file)?;
    } else {
        FileExt::try_lock_exclusive(&file)?;
    }
    Ok(file)
}

pub(super) fn sync_parent(path: &Path) -> io::Result<()> {
    // POSIX directory durability. Windows file contents are flushed below;
    // this receipt does not promise power-loss directory-journal guarantees.
    #[cfg(unix)]
    File::open(
        path.parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent"))?,
    )?
    .sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn preserve(path: &Path, bytes: &[u8]) -> Result<(), AdoptionError> {
    if exists(path)? {
        return if read_bounded(path)? == bytes {
            // A previous publication may have reached its final name before
            // parent sync failed; equality alone does not complete that step.
            sync_parent(path)?;
            Ok(())
        } else {
            Err(AdoptionError::EvidenceConflict(path.display().to_string()))
        };
    }
    let parent = path
        .parent()
        .ok_or_else(|| invalid("evidence has no parent"))?;
    let temporary = parent.join(format!(".preserve-{}.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    // Publish a complete inode without overwriting existing evidence. Unlike a
    // direct create+write, an interrupted write never occupies the final name.
    // Both paths are on the same filesystem. Unsupported hard links are an IO
    // refusal, not a fallback to a partially visible or destructive write.
    let published = fs::hard_link(&temporary, path);
    match published {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if read_bounded(path)? != bytes {
                return Err(AdoptionError::EvidenceConflict(path.display().to_string()));
            }
        }
        Err(error) => return Err(error.into()),
    }
    fs::remove_file(&temporary)?;
    sync_parent(path)?;
    Ok(())
}

/// Adopt the selected bytes under the checkpoint owner's exclusion. The
/// callback MUST reject any running core, including unresponsive/legacy cores;
/// it runs after acquiring the lock and immediately before publication. This
/// condition is essential because legacy binaries do not honor the new lock.
pub fn adopt(
    plan: &AdoptionPlan,
    mut ensure_offline: impl FnMut() -> io::Result<()>,
) -> Result<AdoptionReceipt, AdoptionError> {
    if plan.format_version != 1 {
        return Err(invalid("unsupported adoption plan version"));
    }
    ensure_offline()?;
    let destination = resolved_path(&std::path::absolute(volatile_path(
        plan.declared_persona_id,
    )?)?)?;
    if destination != plan.destination_path
        || source_path(&plan.source.path, plan.declared_persona_id)? != plan.source.path
    {
        return Err(invalid(
            "plan paths no longer match declared identity and native destination",
        ));
    }
    let _lock = lock_checkpoint(&destination, false)?;
    ensure_offline()?;
    let encoded_plan = serde_json::to_vec(plan).map_err(|error| invalid(error.to_string()))?;
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("destination has no parent"))?;
    let evidence = parent
        .join(".checkpoint-adoptions")
        .join(digest(&encoded_plan));
    if resolved_path(&evidence)? != evidence {
        return Err(invalid("evidence path is redirected"));
    }
    let archive = evidence.join("source.json");
    let prior = evidence.join("prior.json");
    let intent = evidence.join("plan.json");
    let receipt_path = evidence.join("receipt.json");
    let mut receipt = AdoptionReceipt {
        plan: plan.clone(),
        assurance: AdoptionAssurance::SelectedLegacySnapshotWithDeclaredPersonaBinding,
        legacy_final_flush_acknowledged: false,
        source_archive: archive.clone(),
        prior_destination_archive: plan.prior_destination.as_ref().map(|_| prior.clone()),
        receipt_path: receipt_path.clone(),
        already_applied: false,
    };

    let (current_source, source_bytes) = selection(&plan.source.path)?;
    if current_source != plan.source {
        return Err(AdoptionError::SourceChanged);
    }
    let current_destination = optional_selection(&destination)?;
    // A receipt/intent plus matching archives proves this request's prior
    // attempt. Matching destination content alone is not adoption evidence.
    let receipt_exists = exists(&receipt_path)?;
    if receipt_exists || exists(&intent)? {
        if read_bounded(&intent)? != encoded_plan || read_bounded(&archive)? != source_bytes {
            return Err(AdoptionError::EvidenceConflict(
                "intent/source archive mismatch".into(),
            ));
        }
        if let Some(expected) = &plan.prior_destination {
            let bytes = read_bounded(&prior)?;
            if digest(&bytes) != expected.sha256 {
                return Err(AdoptionError::EvidenceConflict(
                    "prior destination archive mismatch".into(),
                ));
            }
        }
        if current_destination
            .as_ref()
            .is_some_and(|current| current.sha256 == plan.source.sha256)
        {
            // Complete a previous rename's directory durability step before
            // acknowledging recovery, even if its first attempt failed here.
            sync_parent(&destination)?;
            let encoded_receipt =
                serde_json::to_vec(&receipt).map_err(|error| invalid(error.to_string()))?;
            preserve(&receipt_path, &encoded_receipt)?;
            receipt.already_applied = true;
            return Ok(receipt);
        }
        if receipt_exists {
            return Err(AdoptionError::DestinationChanged);
        }
    }
    if current_destination != plan.prior_destination {
        return Err(AdoptionError::DestinationChanged);
    }
    fs::create_dir_all(&evidence)?;
    // Archives before intent: a visible plan always has complete preserved
    // inputs. Partial files are a refusal on retry, never silently overwritten.
    preserve(&archive, &source_bytes)?;
    if let Some(expected) = &plan.prior_destination {
        let (current, bytes) = selection(&destination)?;
        if &current != expected {
            return Err(AdoptionError::DestinationChanged);
        }
        preserve(&prior, &bytes)?;
    }
    preserve(&intent, &encoded_plan)?;
    // Separate inode from immutable evidence, so future in-place writes to the
    // live file cannot alter the source archive.
    // One staged payload per explicit plan: repeated offline/precondition
    // refusals reuse its verified bytes instead of accumulating full copies.
    let staged = evidence.join("staged.json");
    preserve(&staged, &source_bytes)?;
    ensure_offline()?;
    if selection(&plan.source.path)?.0 != plan.source {
        return Err(AdoptionError::SourceChanged);
    }
    if optional_selection(&destination)? != plan.prior_destination {
        return Err(AdoptionError::DestinationChanged);
    }
    fs::rename(&staged, &destination)?;
    sync_parent(&destination)?;
    let published = optional_selection(&destination)?;
    if !published
        .as_ref()
        .is_some_and(|value| value.sha256 == plan.source.sha256)
    {
        return Err(AdoptionError::EvidenceConflict(
            "destination changed after publication".into(),
        ));
    }
    let encoded_receipt =
        serde_json::to_vec(&receipt).map_err(|error| invalid(error.to_string()))?;
    preserve(&receipt_path, &encoded_receipt)?;
    Ok(receipt)
}
