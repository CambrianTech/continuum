//! Where a training job lives on disk, and the one reader of the request it was handed.
//!
//! ONE layout, `<genome root>/<persona>/<trait>/<job id>/`, used by the job that WRITES the
//! directory (`native_jobs::job_dir_for`) and by every reader that finds it again (the
//! trigger's orphan resume and partitions, `genome/training-trigger/return`). They were two
//! copies with two rules for the trait segment: the writer kept `-`, the readers turned it
//! into `_`, so a hyphenated trait (`speech-discipline`) was written to one directory and
//! looked for in another (card 32d2abbb).

use super::types::TrainingJobRequest;
use serde_json::Value;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Where this node keeps its persona genomes (and their job directories).
pub fn genome_root() -> Option<PathBuf> {
    crate::paths::ContinuumLayout::home().ok().map(|layout| layout.genome())
}

/// A job's directory under a genome root: THE one layout of persona/trait/job. The trait
/// keeps alphanumerics and `-` (the rule every job directory on disk was written with);
/// anything else, `/` included, becomes `_`.
pub fn job_dir_under(
    root: &Path,
    persona_name: &str,
    trait_kind: &str,
    local_id: Uuid,
) -> PathBuf {
    let trait_segment: String = trait_kind
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    root.join(persona_name.replace(['/', ' '], "_"))
        .join(trait_segment)
        .join(local_id.to_string())
}

/// Where a job's directory lives when its request named no `local_artifact_dir`, from the
/// fields a ledger keeps (persona, trait, job id).
pub fn default_job_dir(
    persona_name: &str,
    trait_kind: &str,
    local_id: Uuid,
) -> Option<PathBuf> {
    Some(job_dir_under(
        &genome_root()?,
        persona_name,
        trait_kind,
        local_id,
    ))
}

/// Publish `bytes` as `dir/name` exactly once: written, synced and placed whole, never replacing
/// a file already there (`continuum_host::durable::publish_new`). `Ok(None)` when this call
/// placed it; `Ok(Some(existing))` with what is already there, for the caller to compare. A
/// leased job's binding, request and manifest are each one such file (card 92cd2251): a retry
/// can confirm them, never rewrite them.
pub fn publish_job_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<Option<Vec<u8>>, String> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (staged, placed) = (dir.join(format!(".{name}.{}.partial", Uuid::new_v4())), dir.join(name));
    let written = std::fs::File::create(&staged).and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()));
    let outcome = written.and_then(|()| continuum_host::durable::publish_new(&staged, &placed));
    let _ = std::fs::remove_file(&staged); // the staging name is never the record: gone whether the publish moved it or not
    match outcome {
        Ok(()) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            std::fs::read(&placed).map(Some).map_err(|e| format!("{}: {e}", placed.display()))
        }
        Err(e) => Err(format!("{}: {e}", placed.display())),
    }
}

/// The directory of job `local_id` under `root`, found by the file `anchor` it holds: the lookup
/// for an owner that keeps only the job's id (a leased job's handle names its attempt, not its
/// persona or trait). `Ok(None)` when no `<persona>/<trait>/<local_id>/` holds it; an error when
/// two do, since which one is meant cannot be told.
pub fn find_job_dir_under(root: &Path, local_id: Uuid, anchor: &str) -> Result<Option<PathBuf>, String> {
    let id = local_id.to_string();
    let Ok(personas) = std::fs::read_dir(root) else { return Ok(None) };
    let mut found: Vec<PathBuf> = personas
        .flatten()
        .filter_map(|persona| std::fs::read_dir(persona.path()).ok())
        .flat_map(|traits| traits.flatten().map(|trait_dir| trait_dir.path().join(&id)))
        .filter(|dir| dir.join(anchor).is_file())
        .collect();
    match found.len() {
        0 | 1 => Ok(found.pop()),
        _ => Err(format!("job {local_id} has {} directories holding {anchor}: {found:?}", found.len())),
    }
}

/// Every directory job `local_id` can have under `root`, by the layout [`job_dir_under`] writes
/// (`<root>/<persona>/<trait>/<local_id>/`) and a request's own artifact root
/// (`<root>/<local_id>/`), each that EXISTS. Unlike [`find_job_dir_under`], which a caller uses
/// to find one directory and reads an unreadable branch as no match, every listing or check that
/// errs is an `Err` here: a caller deciding from "no record exists" must not decide from a read
/// that failed (Codex on #5548). A root or a branch that is absent, or an entry that is not a
/// directory, is not an error.
pub fn job_dirs_strict(root: &Path, local_id: Uuid) -> Result<Vec<PathBuf>, String> {
    let absent = |e: &std::io::Error| matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory);
    let list = |dir: &Path| -> Result<Vec<PathBuf>, String> {
        match std::fs::read_dir(dir) {
            Ok(entries) => entries.map(|entry| entry.map(|e| e.path()).map_err(|e| format!("{}: {e}", dir.display()))).collect(),
            Err(e) if absent(&e) => Ok(Vec::new()),
            Err(e) => Err(format!("{}: {e}", dir.display())),
        }
    };
    let exists = |dir: PathBuf| -> Result<Option<PathBuf>, String> {
        match std::fs::metadata(&dir) {
            Ok(meta) => Ok(meta.is_dir().then_some(dir)),
            Err(e) if absent(&e) => Ok(None),
            Err(e) => Err(format!("{}: {e}", dir.display())),
        }
    };
    let id = local_id.to_string();
    let mut found: Vec<PathBuf> = exists(root.join(&id))?.into_iter().collect();
    for persona in list(root)? {
        for trait_dir in list(&persona)? {
            found.extend(exists(trait_dir.join(&id))?);
        }
    }
    Ok(found)
}

/// A job's own request, read from its directory as the trigger first sent it. THE one
/// reader of a job's `request.json` (the resume and `genome/training-trigger/return`).
pub fn read_job_request(dir: &Path) -> Result<TrainingJobRequest, String> {
    let path = dir.join("request.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: not JSON: {e}", path.display()))?;
    let decoded = serde_json::from_value::<TrainingJobRequest>(spec.clone()); // disk boundary: the request.json the trigger wrote for this job
    let mut request =
        decoded.map_err(|e| format!("{}: not a training request: {e}", path.display()))?;
    // The spec on disk is the PLANNER's: the adapter replaced `baseModel` with the
    // trainable HF base (hf_source) and kept the registry id in `canonicalBase`.
    // job-create takes the registry id, so the resume must too. Measured on the 5090
    // 2026-10-05: all five killed-by-reboot jobs failed to resume with "cannot resolve
    // hf_source for 'Qwen/Qwen3.8-27B'": the HF id fed back where a registry id belongs.
    if let Some(canonical) = spec
        .get("canonicalBase")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
    {
        request.base_model = canonical.to_string();
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card d11d9827): a genome store resolved outside every test home. Since
    // the kernel was cut out of core, its `cfg(test)`-only temp home never applied in this
    // crate's tests, and `home_dir().join(".continuum")` never honoured one at all, so a
    // genome test could write the operator's real genome tree and job ledger (the 2026-10-05
    // replay of 18 of Kimi's jobs). Any test home, pinned by a fixture or the per-process
    // default, lives under the system temp dir. The one exception is by design: an operator
    // who EXPLICITLY exports `CONTINUUM_HOME` for a test run is honoured (`continuum_home`
    // reads it first), so the assertion holds only when no home was declared from outside.
    #[test]
    fn the_genome_root_in_a_test_build_is_never_the_operators_home() {
        let root = genome_root().expect("a test build always has a home");
        let declared = std::env::var_os("CONTINUUM_HOME")
            .map(std::path::PathBuf::from)
            .filter(|home| !home.starts_with(std::env::temp_dir()));
        match declared {
            Some(home) => assert_eq!(root, home.join("genome"), "an explicit home is honoured"),
            None => assert!(
                root.starts_with(std::env::temp_dir()),
                "genome root {} escaped the test home",
                root.display()
            ),
        }
    }

    // what this catches (card 32d2abbb): the job writer and its readers must name the SAME
    // directory. The readers' old rule turned a hyphenated trait's `-` into `_`, so
    // `speech-discipline` jobs were written to one directory and resumed or returned from
    // another; `code/owner`, the only trait the tests used, maps the same either way.
    #[test]
    fn a_hyphenated_trait_names_the_directory_its_job_was_written_to() {
        let id = Uuid::from_u128(7);
        let root = Path::new("/genome");
        assert_eq!(
            job_dir_under(root, "Kimi", "speech-discipline", id),
            root.join("Kimi")
                .join("speech-discipline")
                .join(id.to_string())
        );
        assert_eq!(
            job_dir_under(root, "Kimi", "code/owner", id),
            root.join("Kimi").join("code_owner").join(id.to_string())
        );
    }

    // what this catches (card 92cd2251, Codex on #5419): a leased job's record rewritten by a
    // retry, half-written, or found ambiguously. A file publishes once and reads back whole; a
    // second publish leaves it and returns it for comparison; the job's directory is found by its
    // anchor, and two directories of one id are an error, never enumeration order.
    #[test]
    fn a_job_file_publishes_once_and_a_job_is_found_by_its_id() {
        let root = tempfile::tempdir().expect("root"); // JUSTIFIED: test fixture
        let id = Uuid::new_v4();
        let dir = job_dir_under(root.path(), "kimi", "code-review", id);
        assert_eq!(find_job_dir_under(root.path(), id, "lease.json"), Ok(None));
        assert_eq!(publish_job_file(&dir, "lease.json", b"first"), Ok(None));
        assert_eq!(publish_job_file(&dir, "lease.json", b"second"), Ok(Some(b"first".to_vec())), "never rewritten");
        assert_eq!(std::fs::read(dir.join("lease.json")).expect("read"), b"first"); // JUSTIFIED: test asserts the bytes
        assert!(std::fs::read_dir(&dir).expect("dir").flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".partial")), "nothing staged is left");
        assert_eq!(find_job_dir_under(root.path(), id, "lease.json"), Ok(Some(dir.clone())));
        assert_eq!(find_job_dir_under(root.path(), Uuid::new_v4(), "lease.json"), Ok(None));
        let other = job_dir_under(root.path(), "sahar", "code-review", id);
        assert_eq!(publish_job_file(&other, "lease.json", b"first"), Ok(None));
        assert!(find_job_dir_under(root.path(), id, "lease.json").is_err(), "two directories of one id are ambiguous");
    }
}
