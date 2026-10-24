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
    // default, lives under the system temp dir.
    #[test]
    fn the_genome_root_in_a_test_build_is_never_the_operators_home() {
        let root = genome_root().expect("a test build always has a home");
        assert!(
            root.starts_with(std::env::temp_dir()),
            "genome root {} escaped the test home",
            root.display()
        );
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
}
