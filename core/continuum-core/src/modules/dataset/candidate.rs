//! Create-only dataset candidates, before the training trigger owns acceptance.
//!
//! This is local artifact storage, not an acceptance ledger or retry worker.
//! A reservation survives failure as an incomplete directory: callers must not
//! regenerate under that identity. Only a complete, verified publication loads.
//! Like other DatasetService operations, callers run this file I/O off the main
//! thread. No command or training dispatch is introduced here.
//!
//! Unix publication flushes files and the full ancestor directory chain. On
//! Windows std provides file flushing but no directory sync: process-restart
//! recovery is supported, but namespace survival across power loss is weaker.
//! This API does not claim equivalent power-loss durability on Windows.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::DatasetService;
use crate::commands::training_trigger::submit::SubmitParams;
use crate::forge::recipe::CorpusRef;
use crate::persona::inbox_admission::content_hash_sha256;

const SCHEMA_VERSION: u32 = 1;
const SUBMISSION_FILE: &str = "submission.json";
const READY_FILE: &str = "ready.json";

/// Exclusive permission to publish once. Deliberately neither Clone nor
/// Deserialize: an existing directory is not permission to resume generation.
#[derive(Debug)]
pub struct CandidateReservation {
    id: Uuid,
    directory: PathBuf,
}

/// Verified exact request, ready for a future explicit caller to submit.
/// This proves local persistence, not validation, acceptance, or training.
#[derive(Debug)]
pub struct DatasetCandidate {
    pub submission: SubmitParams,
    pub content: CorpusRef,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadyManifest {
    schema_version: u32,
    id: Uuid,
    content: CorpusRef,
}

impl DatasetService {
    /// Reserve an identity before synthesis. Already-existing identities,
    /// including incomplete ones, fail without changing their files.
    pub fn reserve_candidate(&self, id: Uuid) -> Result<CandidateReservation, String> {
        let root = self.datasets_root.join(".candidates");
        fs::create_dir_all(&root).map_err(|e| format!("candidate root: {e}"))?;
        let root = fs::canonicalize(root).map_err(|e| format!("candidate root: {e}"))?;
        let directory = root.join(id.to_string());
        fs::create_dir(&directory).map_err(|e| format!("reserve candidate {id}: {e}"))?;
        sync_directory_chain(&directory)?;
        Ok(CandidateReservation { id, directory })
    }

    /// Consume the reservation and freeze the complete typed request, including
    /// examples, metadata and policy. The caller must supply the same stable
    /// submission ID. No defaults are generated here and no submission occurs.
    pub fn publish_candidate(
        &self,
        reservation: CandidateReservation,
        submission: SubmitParams,
    ) -> Result<DatasetCandidate, String> {
        let id = reservation.id;
        let directory = self.candidate_directory(id)?;
        if directory != reservation.directory {
            return Err(format!(
                "candidate {id} belongs to a different dataset root"
            ));
        }
        if submission.submission_id != Some(id) {
            return Err(format!("candidate {id} requires the same submission_id"));
        }
        // serde_json maps nonfinite floats to null; for Option<f32> that would
        // silently turn an explicit policy into None on reload. Refuse before
        // writing anything, without imposing a trainer's numeric policy here.
        for (field, finite) in [
            (
                "validation_split",
                submission.validation_split.is_none_or(f32::is_finite),
            ),
            (
                "lora.dropout",
                submission
                    .lora
                    .as_ref()
                    .is_none_or(|p| p.dropout.is_finite()),
            ),
            (
                "schedule.learning_rate",
                submission
                    .schedule
                    .as_ref()
                    .is_none_or(|p| p.learning_rate.is_finite()),
            ),
        ] {
            if !finite {
                return Err(format!("candidate {id} cannot persist nonfinite {field}"));
            }
        }
        let text = serde_json::to_string(&submission)
            .map_err(|e| format!("serialize candidate {id}: {e}"))?;
        let content = CorpusRef {
            name: id.to_string(),
            content_hash: content_hash_sha256(&text),
            size_bytes: text.len() as u64,
            source_url: None,
        };
        write_new(&directory.join(SUBMISSION_FILE), text.as_bytes())?;
        // Persist the payload before publishing its ready marker. A partial or
        // absent marker is refused on load; it is never treated as an empty batch.
        sync_directory(&directory)?;
        let manifest = ReadyManifest {
            schema_version: SCHEMA_VERSION,
            id,
            content,
        };
        let ready = serde_json::to_vec(&manifest)
            .map_err(|e| format!("serialize candidate manifest {id}: {e}"))?;
        let pending = directory.join("ready.pending");
        write_new(&pending, &ready)?;
        // The checkpoint evidence owner uses this same complete-inode,
        // no-clobber publication pattern. Unsupported hard links fail loudly.
        fs::hard_link(&pending, directory.join(READY_FILE))
            .map_err(|e| format!("publish candidate {id}: {e}"))?;
        fs::remove_file(&pending).map_err(|e| format!("finish candidate {id}: {e}"))?;
        sync_directory(&directory)?;
        self.load_candidate(id)
    }

    /// Load only a complete, matching publication. Hashes detect damaged or
    /// changed payload bytes; they do not authenticate a hostile storage owner
    /// who can replace both the payload and its manifest.
    pub fn load_candidate(&self, id: Uuid) -> Result<DatasetCandidate, String> {
        let directory = self.candidate_directory(id)?;
        let ready = fs::read(directory.join(READY_FILE))
            .map_err(|e| format!("candidate {id} is not ready: {e}"))?;
        let manifest: ReadyManifest = serde_json::from_slice(&ready)
            .map_err(|e| format!("candidate {id} has an incomplete or invalid manifest: {e}"))?;
        if manifest.schema_version != SCHEMA_VERSION || manifest.id != id {
            return Err(format!("candidate {id} manifest identity/version mismatch"));
        }
        // A prior writer may have linked the ready marker before its final
        // directory flush failed. Complete the whole chain before reuse, even
        // if the first writer created previously missing dataset ancestors.
        sync_directory_chain(&directory)?;
        let text = fs::read_to_string(directory.join(SUBMISSION_FILE))
            .map_err(|e| format!("read candidate {id}: {e}"))?;
        if text.len() as u64 != manifest.content.size_bytes
            || content_hash_sha256(&text) != manifest.content.content_hash
        {
            return Err(format!("candidate {id} payload integrity mismatch"));
        }
        let submission: SubmitParams =
            serde_json::from_str(&text).map_err(|e| format!("decode candidate {id}: {e}"))?;
        if submission.submission_id != Some(id) {
            return Err(format!("candidate {id} submission identity mismatch"));
        }
        Ok(DatasetCandidate {
            submission,
            content: manifest.content,
        })
    }

    fn candidate_directory(&self, id: Uuid) -> Result<PathBuf, String> {
        let root = fs::canonicalize(self.datasets_root.join(".candidates"))
            .map_err(|e| format!("candidate {id} root unavailable: {e}"))?;
        Ok(root.join(id.to_string()))
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("persist {}: {e}", path.display()))
}

fn sync_directory(path: &Path) -> Result<(), String> {
    // Unix needs a directory fsync for newly created names. Windows does not
    // expose directory sync through std; each file is flushed with sync_all.
    #[cfg(unix)]
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| format!("persist directory {}: {e}", path.display()))?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn sync_directory_chain(path: &Path) -> Result<(), String> {
    // A child's fsync does not persist its entry in the parent. Walking all
    // canonical ancestors covers first use and interrupted create_dir_all,
    // without guessing which directories a previous process created.
    for directory in path.ancestors() {
        sync_directory(directory)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::provenance::{GenerationOutcome, GenerationReceipt};
    use crate::genome::fine_tuning::types::{LoRAHyperparams, ScheduleParams};
    use serde_json::json;
    use tempfile::TempDir;

    fn submission(id: Uuid) -> SubmitParams {
        let receipt = GenerationReceipt {
            submitted_request_id: "fixture-request".into(),
            outcome: GenerationOutcome::Served {
                model: "actual-teacher".into(),
                provider: "fixture-provider".into(),
                provider_request_id: Some("served-request".into()),
            },
        };
        serde_json::from_value(json!({
            "submissionId": id,
            "personaId": Uuid::new_v4(),
            "personaName": "fixture-recipient",
            "baseModel": "fixture-base",
            "traitKind": "code",
            "examples": [{"prompt": "public task", "completion": "public answer",
                "metadata": {"teacherGenerations": [receipt]}}],
            "source": "teacher_synthesized",
            "evalSet": "independent-gym.jsonl",
            "minExamples": 16,
            "validationSplit": 0.0
        }))
        .unwrap()
    }

    // what this catches: a retry must reload exact examples/receipts/policy, never
    // overwrite or reinterpret the candidate, while ordinary exports stay mutable.
    #[test]
    fn candidate_round_trip_preserves_request_and_default_export() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("missing").join("nested").join("datasets");
        let service = DatasetService::new(root.clone());
        let id = Uuid::new_v4();
        let request = submission(id);
        let expected = serde_json::to_value(&request).unwrap();
        let reservation = service.reserve_candidate(id).unwrap();
        let published = service.publish_candidate(reservation, request).unwrap();
        drop(service);
        let service = DatasetService::new(root);
        let loaded = service.load_candidate(id).unwrap();
        assert_eq!(serde_json::to_value(loaded.submission).unwrap(), expected);
        assert_eq!(loaded.content, published.content);
        assert!(service.reserve_candidate(id).is_err());

        let output = temp.path().join("ordinary");
        DatasetService::split_and_write("ordinary", &output, &[json!({"first": true})], 1.0, None)
            .unwrap();
        DatasetService::split_and_write("ordinary", &output, &[json!({"second": true})], 1.0, None)
            .unwrap();
        assert_eq!(
            fs::read_to_string(output.join("train.jsonl"))
                .unwrap()
                .trim(),
            r#"{"second":true}"#
        );
        assert_eq!(
            serde_json::to_value(service.load_candidate(id).unwrap().submission).unwrap(),
            expected
        );
    }

    // what this catches: crashes before/during final publication and later byte
    // tampering must fail closed, without turning an incomplete candidate into work.
    #[test]
    fn candidate_refuses_incomplete_and_tampered_publications() {
        let temp = TempDir::new().unwrap();
        let service = DatasetService::new(temp.path().into());
        let incomplete_id = Uuid::new_v4();
        let incomplete = service.reserve_candidate(incomplete_id).unwrap();
        write_new(
            &incomplete.directory.join(SUBMISSION_FILE),
            &serde_json::to_vec(&submission(incomplete_id)).unwrap(),
        )
        .unwrap();
        drop(incomplete); // crash after payload persistence, before publication
        assert!(service
            .load_candidate(incomplete_id)
            .unwrap_err()
            .contains("not ready"));
        assert!(service.reserve_candidate(incomplete_id).is_err());

        let mismatched_id = Uuid::new_v4();
        let mismatched = service.reserve_candidate(mismatched_id).unwrap();
        assert!(service
            .publish_candidate(mismatched, submission(Uuid::new_v4()))
            .unwrap_err()
            .contains("same submission_id"));
        assert!(service.load_candidate(mismatched_id).is_err());

        let id = Uuid::new_v4();
        let reservation = service.reserve_candidate(id).unwrap();
        assert!(service
            .load_candidate(id)
            .unwrap_err()
            .contains("not ready"));
        let directory = reservation.directory.clone();
        service
            .publish_candidate(reservation, submission(id))
            .unwrap();
        let ready_path = directory.join(READY_FILE);
        let ready = fs::read(&ready_path).unwrap();
        fs::write(&ready_path, b"{").unwrap();
        assert!(service
            .load_candidate(id)
            .unwrap_err()
            .contains("invalid manifest"));
        fs::write(&ready_path, ready).unwrap();
        let payload_path = directory.join(SUBMISSION_FILE);
        let payload = fs::read_to_string(&payload_path).unwrap();
        fs::write(
            &payload_path,
            payload.replace("public answer", "mutant answer"),
        )
        .unwrap();
        assert!(service
            .load_candidate(id)
            .unwrap_err()
            .contains("integrity mismatch"));
        assert!(service.reserve_candidate(id).is_err());
    }

    // what this catches: native callers can supply floats JSON cannot represent;
    // null must never silently replace an explicit policy or reach a ready marker.
    #[test]
    fn candidate_refuses_nonfinite_policy_before_writing() {
        let temp = TempDir::new().unwrap();
        let service = DatasetService::new(temp.path().into());
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for field in ["validation_split", "lora.dropout", "schedule.learning_rate"] {
                let id = Uuid::new_v4();
                let reservation = service.reserve_candidate(id).unwrap();
                let directory = reservation.directory.clone();
                let mut request = submission(id);
                match field {
                    "validation_split" => request.validation_split = Some(value as f32),
                    "lora.dropout" => {
                        request.lora = Some(LoRAHyperparams {
                            rank: 8,
                            alpha: 16,
                            dropout: value as f32,
                            target_modules: vec![],
                        })
                    }
                    "schedule.learning_rate" => {
                        request.schedule = Some(ScheduleParams {
                            epochs: 1,
                            batch_size: 1,
                            sequence_length: 512,
                            learning_rate: value,
                        })
                    }
                    _ => unreachable!(),
                }
                let error = service.publish_candidate(reservation, request).unwrap_err();
                assert!(error.contains(&format!("nonfinite {field}")), "{error}");
                assert!(!directory.join(SUBMISSION_FILE).exists());
                assert!(!directory.join(READY_FILE).exists());
            }
        }
    }

    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;

        // what this catches: competing producers cannot both synthesize/publish
        // under one identity; exclusion belongs to the filesystem, not a local lock.
        #[test]
        fn concurrent_candidate_reservation_has_one_owner() {
            let temp = TempDir::new().unwrap();
            let id = Uuid::new_v4();
            let barrier = std::sync::Barrier::new(2);
            let results = std::thread::scope(|scope| {
                let attempt = || {
                    let service = DatasetService::new(temp.path().into());
                    barrier.wait();
                    service.reserve_candidate(id)
                };
                let a = scope.spawn(attempt);
                let b = scope.spawn(attempt);
                [a.join().unwrap(), b.join().unwrap()]
            });
            assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
            let reservation = results.into_iter().find_map(Result::ok).unwrap();
            let service = DatasetService::new(temp.path().into());
            service
                .publish_candidate(reservation, submission(id))
                .unwrap();
            assert!(service.load_candidate(id).is_ok());
        }
    }
}
