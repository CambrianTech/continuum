use crate::vdd::record::{StandardVddRecord, VddError};
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactBundle {
    pub dir: PathBuf,
    pub record_jsonl: PathBuf,
    pub manifest_toml: PathBuf,
    pub summary_md: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ArtifactWriter {
    root: PathBuf,
}

impl ArtifactWriter {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn continuum_default() -> Self {
        let home = dirs::home_dir().expect("home directory must exist for VDD artifacts");
        Self::new(home.join(".continuum").join("vdd"))
    }

    pub fn write(
        &self,
        record: &StandardVddRecord,
        manifest: &ReproducibilityManifest,
    ) -> Result<ArtifactBundle, VddError> {
        let dir = self.root.join(&record.git_sha).join(&record.scenario);
        fs::create_dir_all(&dir).map_err(|source| VddError::Io {
            path: dir.clone(),
            source,
        })?;

        let record_jsonl = dir.join("record.jsonl");
        let manifest_toml = dir.join("manifest.toml");
        let summary_md = dir.join("summary.md");

        write_file(
            &record_jsonl,
            format!("{}\n", serde_json::to_string(record)?),
        )?;
        write_file(&manifest_toml, toml::to_string_pretty(manifest)?)?;
        write_file(&summary_md, render_summary(record))?;

        Ok(ArtifactBundle {
            dir,
            record_jsonl,
            manifest_toml,
            summary_md,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReproducibilityManifest {
    pub git_sha: String,
    pub scenario: String,
    pub command: String,
    pub hardware: String,
    pub backend: String,
    pub policy_version: Option<String>,
    pub cascade_step: Option<u8>,
    pub env: Vec<ManifestEnvVar>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManifestEnvVar {
    pub name: String,
    pub value: String,
}

impl ReproducibilityManifest {
    pub fn from_record(record: &StandardVddRecord, env_names: &[&str]) -> Self {
        let env = env_names
            .iter()
            .filter_map(|name| {
                std::env::var(name).ok().map(|value| ManifestEnvVar {
                    name: (*name).to_string(),
                    value,
                })
            })
            .collect();
        Self {
            git_sha: record.git_sha.clone(),
            scenario: record.scenario.clone(),
            command: record.command.clone(),
            hardware: record.hardware.clone(),
            backend: record.backend.clone(),
            policy_version: record.policy_version.clone(),
            cascade_step: record.cascade_step,
            env,
        }
    }
}

fn write_file(path: &Path, body: impl AsRef<[u8]>) -> Result<(), VddError> {
    let mut file = fs::File::create(path).map_err(|source| VddError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.write_all(body.as_ref())
        .map_err(|source| VddError::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn render_summary(record: &StandardVddRecord) -> String {
    format!(
        "# VDD: {}\n\n| Field | Value |\n|---|---|\n| status | {:?} |\n| git_sha | {} |\n| hardware | {} |\n| backend | {} |\n| first_response_ms | {} |\n| all_responses_ms | {} |\n| responses | {}/{} |\n| degraded_reason | {} |\n| silence_reasons | {} |\n",
        record.scenario,
        record.status,
        record.git_sha,
        record.hardware,
        record.backend,
        opt_u64(record.first_response_ms),
        opt_u64(record.all_responses_ms),
        record.responses_observed,
        record.responses_expected,
        record.degraded_reason.as_deref().unwrap_or("none"),
        if record.silence_reasons.is_empty() {
            "none".to_string()
        } else {
            record.silence_reasons.join(", ")
        }
    )
}

fn opt_u64(value: Option<u64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string())
}
