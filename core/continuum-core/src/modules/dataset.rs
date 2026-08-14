//! Dataset Module — Import, manage, and query training datasets.
//!
//! All heavy data processing (CSV parsing, JSONL conversion, file I/O)
//! happens here in Rust off the main thread. TypeScript is a thin API layer.
//!
//! Supports RealClassEval (arxiv:2510.26130) and generic CSV imports.
//!
//! The six `dataset/*` verbs are typed [`ActionCommand`](crate::sdk_codegen::ActionCommand)s
//! living under `commands/dataset/`; this module owns the [`DatasetService`] they
//! capture and exposes them via [`ServiceModule::commands`]. The legacy
//! `handle_command` match is retired — it now fails loud.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ts_rs::TS;

/// Manifest persisted alongside imported datasets.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/dataset/DatasetManifest.ts"
)]
pub struct DatasetManifest {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    #[ts(type = "number")]
    pub total_examples: usize,
    #[ts(type = "number")]
    pub train_examples: usize,
    #[ts(type = "number")]
    pub eval_examples: usize,
    pub train_path: String,
    pub eval_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub metrics: Option<DatasetMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub pre_cutoff: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub post_cutoff: Option<usize>,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/dataset/DatasetMetrics.ts"
)]
pub struct DatasetMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub avg_cyclomatic_complexity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub avg_lines_of_code: Option<f64>,
}

/// Result of `dataset/list` — the manifests found under the datasets root.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/dataset/DatasetListResult.ts"
)]
pub struct DatasetListResult {
    /// One manifest per discovered dataset subdirectory.
    pub datasets: Vec<DatasetManifest>,
    /// Number of datasets found (`datasets.len()`).
    #[ts(type = "number")]
    pub count: usize,
    /// The resolved datasets root directory the listing came from.
    pub root: String,
}

// ============================================================================
// Command params — the typed input contracts for the `dataset/*` verbs.
// They live here with the service so the service methods take them directly;
// the command files under `commands/dataset/` capture the `DatasetService` and
// declare these as their `params:`.
// ============================================================================

fn default_split_ratio() -> f64 {
    0.8
}
fn default_true() -> bool {
    true
}
fn default_imported_name() -> String {
    "imported".to_string()
}
fn default_persona_turns_name() -> String {
    "persona-turns".to_string()
}
fn default_persona_captures_name() -> String {
    "persona-captures".to_string()
}
fn default_user_column() -> String {
    "input".to_string()
}
fn default_assistant_column() -> String {
    "output".to_string()
}

/// Params for `dataset/import-csv`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/dataset/ImportCsvParams.ts"
)]
pub struct ImportCsvParams {
    /// Path to the CSV file to import.
    pub csv_path: String,
    /// Override the datasets root directory (default `~/.continuum/datasets`).
    #[serde(default)]
    #[ts(optional)]
    pub output_dir: Option<String>,
    /// Dataset name (subdirectory under the datasets root). Default `imported`.
    #[serde(default = "default_imported_name")]
    pub name: String,
    /// Fraction of examples placed in the train split. Default `0.8`.
    #[serde(default = "default_split_ratio")]
    pub split_ratio: f64,
    /// CSV column holding the user/input text. Default `input`.
    #[serde(default = "default_user_column")]
    pub user_column: String,
    /// CSV column holding the assistant/output text. Default `output`.
    #[serde(default = "default_assistant_column")]
    pub assistant_column: String,
}

/// Params for `dataset/from-turns`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/dataset/FromTurnsParams.ts"
)]
pub struct FromTurnsParams {
    /// Directory of recorder per-turn JSON (default `~/.continuum/fixtures/persona-respond`).
    #[serde(default)]
    #[ts(optional)]
    pub turns_dir: Option<String>,
    /// Override the datasets root directory (default `~/.continuum/datasets`).
    #[serde(default)]
    #[ts(optional)]
    pub output_dir: Option<String>,
    /// Dataset name. Default `persona-turns`.
    #[serde(default = "default_persona_turns_name")]
    pub name: String,
    /// Fraction of examples placed in the train split. Default `0.8`.
    #[serde(default = "default_split_ratio")]
    pub split_ratio: f64,
    /// Include the system prompt as the first message. Default `true`.
    #[serde(default = "default_true")]
    pub include_system: bool,
    /// Include the recent room history before the user turn. Default `false`.
    #[serde(default)]
    pub include_history: bool,
    /// Only convert turns from this persona id.
    #[serde(default)]
    #[ts(optional)]
    pub persona_id: Option<crate::identity::PersonaRef>,
    /// Only convert turns from this room id.
    #[serde(default)]
    #[ts(optional)]
    pub room_id: Option<String>,
}

/// Params for `dataset/from-captures`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/dataset/FromCapturesParams.ts"
)]
pub struct FromCapturesParams {
    /// Directory of live prompt-captures (default `~/.continuum/fixtures/prompt-captures`).
    #[serde(default)]
    #[ts(optional)]
    pub captures_dir: Option<String>,
    /// Override the datasets root directory (default `~/.continuum/datasets`).
    #[serde(default)]
    #[ts(optional)]
    pub output_dir: Option<String>,
    /// Dataset name. Default `persona-captures`.
    #[serde(default = "default_persona_captures_name")]
    pub name: String,
    /// Fraction of examples placed in the train split. Default `0.8`.
    #[serde(default = "default_split_ratio")]
    pub split_ratio: f64,
    /// Include the system prompt as the first message. Default `true`.
    #[serde(default = "default_true")]
    pub include_system: bool,
    /// Only convert captures from this persona id.
    #[serde(default)]
    #[ts(optional)]
    pub persona_id: Option<crate::identity::PersonaRef>,
    /// Only convert captures from this room id.
    #[serde(default)]
    #[ts(optional)]
    pub room_id: Option<String>,
    /// Only convert turns on this skill axis: `"operational"` (the turn ACTED —
    /// emitted a tool call) or `"domain"` (prose/answer only). Omit for both. The
    /// chat-prose bridge passes `"domain"`; the dev-task/self-verify loop passes
    /// `"operational"`.
    #[serde(default)]
    #[ts(optional)]
    pub skill_axis: Option<String>,
}

/// Params for `dataset/import-realclasseval`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/dataset/ImportRealClassEvalParams.ts"
)]
pub struct ImportRealClassEvalParams {
    /// Path to a cloned RealClassEval repo root (auto-discovers CSVs + tests).
    #[serde(default)]
    #[ts(optional)]
    pub repo_dir: Option<String>,
    /// Legacy single-CSV mode: path to one RealClassEval CSV (requires `testsDir`).
    #[serde(default)]
    #[ts(optional)]
    pub csv_path: Option<String>,
    /// Legacy single-CSV mode: directory of PYNGUIN test files.
    #[serde(default)]
    #[ts(optional)]
    pub tests_dir: Option<String>,
    /// Override the output directory (default `<datasets root>/realclasseval`).
    #[serde(default)]
    #[ts(optional)]
    pub output_dir: Option<String>,
    /// Fraction of examples placed in the train split. Default `0.8`.
    #[serde(default = "default_split_ratio")]
    pub split_ratio: f64,
}

/// Params for `dataset/list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/dataset/ListDatasetsParams.ts"
)]
pub struct ListDatasetsParams {
    /// Override the datasets root directory to list (default `~/.continuum/datasets`).
    #[serde(default)]
    #[ts(optional)]
    pub output_dir: Option<String>,
}

/// Params for `dataset/info`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/dataset/DatasetInfoParams.ts"
)]
pub struct DatasetInfoParams {
    /// Dataset name (subdirectory under the datasets root).
    pub name: String,
    /// Override the datasets root directory (default `~/.continuum/datasets`).
    #[serde(default)]
    #[ts(optional)]
    pub output_dir: Option<String>,
}

// ============================================================================
// DatasetService — the dataset domain logic the typed commands capture.
// ============================================================================

/// Owns the datasets root and the import/convert/query logic. Heavy CSV/JSONL
/// processing happens here (off the main thread). The `dataset/*` commands hold
/// an `Arc<DatasetService>` and are thin wrappers over these methods.
pub struct DatasetService {
    datasets_root: PathBuf,
}

impl DatasetService {
    pub fn new(datasets_root: PathBuf) -> Self {
        Self { datasets_root }
    }

    /// Resolve the datasets root directory, preferring the per-call override.
    fn resolve_root(&self, output_dir: Option<&str>) -> PathBuf {
        output_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| self.datasets_root.clone())
    }

    /// Import a generic CSV file as a JSONL training dataset.
    pub fn import_csv(&self, p: &ImportCsvParams) -> Result<DatasetManifest, String> {
        let output_dir = self.resolve_root(p.output_dir.as_deref()).join(&p.name);

        let csv_path = PathBuf::from(&p.csv_path);
        if !csv_path.exists() {
            return Err(format!("CSV file not found: {}", csv_path.display()));
        }

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .from_path(&csv_path)
            .map_err(|e| format!("Failed to open CSV: {e}"))?;

        let headers = reader
            .headers()
            .map_err(|e| format!("Failed to read CSV headers: {e}"))?
            .clone();

        let user_idx = headers
            .iter()
            .position(|h| h == p.user_column.as_str())
            .ok_or_else(|| {
                format!(
                    "Column '{}' not found in CSV. Available: {:?}",
                    p.user_column,
                    headers.iter().collect::<Vec<_>>()
                )
            })?;
        let assistant_idx = headers
            .iter()
            .position(|h| h == p.assistant_column.as_str())
            .ok_or_else(|| {
                format!(
                    "Column '{}' not found in CSV. Available: {:?}",
                    p.assistant_column,
                    headers.iter().collect::<Vec<_>>()
                )
            })?;

        let mut examples: Vec<Value> = Vec::new();
        for result in reader.records() {
            let record = result.map_err(|e| format!("CSV parse error: {e}"))?;
            let user_text = record.get(user_idx).unwrap_or("").trim();
            let assistant_text = record.get(assistant_idx).unwrap_or("").trim();

            if user_text.is_empty() || assistant_text.is_empty() {
                continue;
            }

            examples.push(json!({
                "messages": [
                    { "role": "user", "content": user_text },
                    { "role": "assistant", "content": assistant_text }
                ]
            }));
        }

        if examples.is_empty() {
            return Err("No valid examples found in CSV".to_string());
        }

        let manifest = Self::split_and_write(&p.name, &output_dir, &examples, p.split_ratio, None)?;
        Ok(manifest)
    }

    /// Convert recorded persona turns into a ShareGPT/chat training dataset.
    ///
    /// This is the rooms→training-data bridge of the coordination↔learning
    /// flywheel: a persona's recorded room turns (the system prompt + the user
    /// message → the persona's spoken response) become SFT examples in the SAME
    /// `{messages:[{role,content}]}` format the CSV importer emits, then flow
    /// through the SAME split/write/manifest path. The JSONL is the canonical
    /// ShareGPT/SFT training input — train a LoRA genome on "chats like this one".
    ///
    /// Source = the recorder's per-turn JSON captures (default
    /// `~/.continuum/fixtures/persona-respond`), NOT engrams — engrams are
    /// curated *recall* memory, not paired SFT turns. Only `spoke` turns yield a
    /// training pair; `silent` / errored / malformed turns are skipped.
    pub fn from_turns(&self, p: &FromTurnsParams) -> Result<DatasetManifest, String> {
        let turns_dir = p
            .turns_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".continuum/fixtures/persona-respond")
            });
        if !turns_dir.is_dir() {
            return Err(format!(
                "turnsDir not found: {} — point it at the recorder's per-turn JSON dir \
                 (default ~/.continuum/fixtures/persona-respond)",
                turns_dir.display()
            ));
        }

        let output_dir = self.resolve_root(p.output_dir.as_deref()).join(&p.name);

        let mut examples: Vec<Value> = Vec::new();
        let entries = std::fs::read_dir(&turns_dir)
            .map_err(|e| format!("Failed to read turnsDir {}: {e}", turns_dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(turn) = serde_json::from_str::<Value>(&text) else {
                continue;
            };

            if let Some(pid) = p.persona_id.as_ref().map(|r| r.as_str()) {
                if turn.get("personaId").and_then(|v| v.as_str()) != Some(pid) {
                    continue;
                }
            }
            if let Some(rid) = p.room_id.as_deref() {
                if turn.get("roomId").and_then(|v| v.as_str()) != Some(rid) {
                    continue;
                }
            }

            if let Some(example) = turn_to_example(&turn, p.include_system, p.include_history) {
                examples.push(example);
            }
        }

        if examples.is_empty() {
            return Err(format!(
                "No spoke turns found in {} (after filters) — nothing to train on",
                turns_dir.display()
            ));
        }

        let manifest = Self::split_and_write(&p.name, &output_dir, &examples, p.split_ratio, None)?;
        Ok(manifest)
    }

    /// Convert LIVE prompt-captures into a training dataset — the rooms→training
    /// bridge for the CURRENT cognition path. `from_turns` reads the legacy
    /// recorder dir (the old respond() path); the live WorkspaceCycle/heartbeat
    /// turns land in `~/.continuum/fixtures/prompt-captures` (one `<persona>.jsonl`
    /// per persona, one record per turn). This closes the gap where "the work is
    /// the data" had quietly stopped being true for the live path. Same one output
    /// shape, same split/write/manifest — only the SOURCE differs, so the genome
    /// loop trains on what the persona actually does today.
    pub fn from_captures(&self, p: &FromCapturesParams) -> Result<DatasetManifest, String> {
        let dir = p
            .captures_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".continuum/fixtures/prompt-captures")
            });
        if !dir.is_dir() {
            return Err(format!(
                "capturesDir not found: {} (default ~/.continuum/fixtures/prompt-captures)",
                dir.display()
            ));
        }
        // Fail loud on a bad axis filter rather than silently matching nothing (which
        // would surface as the misleading "No usable turns" error below).
        if let Some(axis) = p.skill_axis.as_deref() {
            if !matches!(axis, "operational" | "domain") {
                return Err(format!(
                    "skillAxis must be \"operational\" or \"domain\", got {axis:?}"
                ));
            }
        }

        let output_dir = self.resolve_root(p.output_dir.as_deref()).join(&p.name);

        let mut examples: Vec<Value> = Vec::new();
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| format!("Failed to read capturesDir {}: {e}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Ok(cap) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                if let Some(pid) = p.persona_id.as_ref().map(|r| r.as_str()) {
                    if cap.get("persona_id").and_then(|v| v.as_str()) != Some(pid) {
                        continue;
                    }
                }
                if let Some(rid) = p.room_id.as_deref() {
                    if cap.get("room_id").and_then(|v| v.as_str()) != Some(rid) {
                        continue;
                    }
                }
                if let Some(example) = capture_to_example(&cap, p.include_system) {
                    if let Some(axis) = p.skill_axis.as_deref() {
                        if example.get("skillAxis").and_then(|a| a.as_str()) != Some(axis) {
                            continue;
                        }
                    }
                    examples.push(example);
                }
            }
        }

        if examples.is_empty() {
            return Err(format!(
                "No usable turns in {} (after filters + structural curation) — nothing to train on",
                dir.display()
            ));
        }

        let manifest = Self::split_and_write(&p.name, &output_dir, &examples, p.split_ratio, None)?;
        Ok(manifest)
    }

    /// Import RealClassEval dataset from cloned repo directory → structured JSONL + manifest.
    ///
    /// The RealClassEval repo has this structure:
    ///   data/functional_correctness_data/{csn,post_cut-off}/
    ///     dfs/no_docstr.csv              — CSV with snippet_id, class_name, human_written_code, class_skeleton
    ///     pynguin_generated_tests/full_docstr/test_snippet_N.py — PYNGUIN test files
    ///
    /// Accepts either:
    ///   - `repoDir`: path to cloned repo root (auto-discovers CSVs + tests)
    ///   - `csvPath` + `testsDir`: legacy single-CSV mode (backward compat)
    pub fn import_realclasseval(
        &self,
        p: &ImportRealClassEvalParams,
    ) -> Result<DatasetManifest, String> {
        let output_dir = p
            .output_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.datasets_root.join("realclasseval"));

        let split_ratio = p.split_ratio;

        // Collect (csv_path, tests_dir, is_post_cutoff) pairs
        let splits: Vec<(PathBuf, PathBuf, bool)> = if let Some(repo_dir) = p.repo_dir.as_deref() {
            // Auto-discover from repo directory structure
            let base = PathBuf::from(repo_dir)
                .join("data")
                .join("functional_correctness_data");
            if !base.exists() {
                return Err(format!(
                    "RealClassEval repo structure not found at {}. Expected data/functional_correctness_data/",
                    base.display()
                ));
            }

            let mut found = Vec::new();
            // post_cut-off = unseen by LLMs (post-cutoff)
            // csn = CodeSearchNet (pre-cutoff)
            for (dir_name, is_post) in &[("post_cut-off", true), ("csn", false)] {
                let split_dir = base.join(dir_name);
                if !split_dir.exists() {
                    continue;
                }

                // Use no_docstr CSV (most challenging — no docstring hints)
                let csv_path = split_dir.join("dfs").join("no_docstr.csv");
                // Tests under pynguin_generated_tests/full_docstr/ (best coverage)
                let tests_dir = split_dir
                    .join("pynguin_generated_tests")
                    .join("full_docstr");

                if csv_path.exists() {
                    found.push((csv_path, tests_dir, *is_post));
                }
            }

            if found.is_empty() {
                return Err(format!(
                    "No RealClassEval CSVs found under {}. Expected {{csn,post_cut-off}}/dfs/no_docstr.csv",
                    base.display()
                ));
            }
            found
        } else if let Some(csv_path) = p.csv_path.as_deref() {
            // Legacy single-CSV mode
            let tests_dir = p
                .tests_dir
                .as_deref()
                .ok_or("Missing required param: testsDir (or use repoDir for auto-discovery)")?;
            vec![(PathBuf::from(csv_path), PathBuf::from(tests_dir), false)]
        } else {
            return Err(
                "Missing required param: repoDir (path to cloned RealClassEval repo) or csvPath"
                    .to_string(),
            );
        };

        let mut all_examples: Vec<Value> = Vec::new();
        let mut total_cc: f64 = 0.0;
        let mut total_loc: f64 = 0.0;
        let mut cc_count = 0usize;
        let mut loc_count = 0usize;
        let mut pre_cutoff = 0usize;
        let mut post_cutoff = 0usize;

        for (csv_path, tests_dir, is_post) in &splits {
            if !csv_path.exists() {
                return Err(format!("CSV file not found: {}", csv_path.display()));
            }

            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .flexible(true)
                .from_path(csv_path)
                .map_err(|e| format!("Failed to open CSV {}: {e}", csv_path.display()))?;

            let headers = reader
                .headers()
                .map_err(|e| format!("Failed to read CSV headers: {e}"))?
                .clone();

            let snippet_id_idx = find_column(&headers, "snippet_id")?;
            let _class_name_idx = find_column(&headers, "class_name")?;
            let human_code_idx = find_column(&headers, "human_written_code")?;
            let skeleton_idx = find_column(&headers, "class_skeleton")?;

            let cc_idx = headers.iter().position(|h| h == "cyclomatic_complexity");
            let loc_idx = headers.iter().position(|h| h == "lines_of_code");

            let mut split_count = 0usize;

            for result in reader.records() {
                let record = result.map_err(|e| format!("CSV parse error: {e}"))?;

                let snippet_id = record.get(snippet_id_idx).unwrap_or("").trim();
                let human_code = record.get(human_code_idx).unwrap_or("").trim();
                let skeleton = record.get(skeleton_idx).unwrap_or("").trim();

                if snippet_id.is_empty() || human_code.is_empty() || skeleton.is_empty() {
                    continue;
                }

                // Locate PYNGUIN test file for this snippet
                let test_code = if tests_dir.exists() {
                    find_test_file(tests_dir, snippet_id)
                } else {
                    None
                };

                let mut user_prompt = format!(
                    "Implement the following Python class:\n\n```python\n{}\n```",
                    skeleton
                );

                if let Some(ref tests) = test_code {
                    user_prompt.push_str(&format!(
                        "\n\nThe class should pass these tests:\n```python\n{}\n```",
                        tests
                    ));
                }

                all_examples.push(json!({
                    "messages": [
                        { "role": "user", "content": user_prompt },
                        { "role": "assistant", "content": format!("```python\n{}\n```", human_code) }
                    ]
                }));

                if let Some(idx) = cc_idx {
                    if let Some(val) = record.get(idx).and_then(|s| s.trim().parse::<f64>().ok()) {
                        total_cc += val;
                        cc_count += 1;
                    }
                }
                if let Some(idx) = loc_idx {
                    if let Some(val) = record.get(idx).and_then(|s| s.trim().parse::<f64>().ok()) {
                        total_loc += val;
                        loc_count += 1;
                    }
                }

                split_count += 1;
            }

            if *is_post {
                post_cutoff += split_count;
            } else {
                pre_cutoff += split_count;
            }
        }

        if all_examples.is_empty() {
            return Err("No valid examples found in RealClassEval dataset".to_string());
        }

        let metrics = DatasetMetrics {
            avg_cyclomatic_complexity: if cc_count > 0 {
                Some(total_cc / cc_count as f64)
            } else {
                None
            },
            avg_lines_of_code: if loc_count > 0 {
                Some(total_loc / loc_count as f64)
            } else {
                None
            },
        };

        let mut manifest = Self::split_and_write(
            "realclasseval",
            &output_dir,
            &all_examples,
            split_ratio,
            Some(metrics),
        )?;

        manifest.source = Some("arxiv:2510.26130".to_string());
        manifest.pre_cutoff = Some(pre_cutoff);
        manifest.post_cutoff = Some(post_cutoff);

        // Re-write manifest with source metadata
        let manifest_path = output_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {e}"))?;
        std::fs::write(&manifest_path, &manifest_json)
            .map_err(|e| format!("Failed to write manifest: {e}"))?;

        Ok(manifest)
    }

    /// List datasets in the datasets root directory.
    pub fn list_datasets(&self, p: &ListDatasetsParams) -> Result<DatasetListResult, String> {
        let root = self.resolve_root(p.output_dir.as_deref());

        let mut datasets: Vec<DatasetManifest> = Vec::new();

        if root.exists() {
            for entry in std::fs::read_dir(&root)
                .map_err(|e| format!("Failed to read datasets directory: {e}"))?
            {
                let entry = entry.map_err(|e| format!("Directory entry error: {e}"))?;
                let path = entry.path();

                if path.is_dir() {
                    let manifest_path = path.join("manifest.json");
                    if manifest_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                            if let Ok(manifest) = serde_json::from_str::<DatasetManifest>(&content)
                            {
                                datasets.push(manifest);
                            }
                        }
                    }
                }
            }
        }

        let count = datasets.len();
        Ok(DatasetListResult {
            datasets,
            count,
            root: root.to_string_lossy().into_owned(),
        })
    }

    /// Read manifest for a specific dataset.
    pub fn dataset_info(&self, p: &DatasetInfoParams) -> Result<DatasetManifest, String> {
        let root = self.resolve_root(p.output_dir.as_deref());
        let manifest_path = root.join(&p.name).join("manifest.json");

        if !manifest_path.exists() {
            return Err(format!(
                "Dataset '{}' not found at {}",
                p.name,
                manifest_path.display()
            ));
        }

        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read manifest: {e}"))?;
        let manifest: DatasetManifest =
            serde_json::from_str(&content).map_err(|e| format!("Failed to parse manifest: {e}"))?;

        Ok(manifest)
    }

    /// Split examples into train/eval, write JSONL files and manifest. An associated
    /// fn (no `&self` — it touches no service state), `pub` so out-of-module producers
    /// of validated ShareGPT examples (e.g. `genome/teach`) package to the SAME
    /// train.jsonl/eval.jsonl/manifest.json shape rather than re-rolling a parallel
    /// writer — one packaging path, one source of truth.
    pub fn split_and_write(
        name: &str,
        output_dir: &Path,
        examples: &[Value],
        split_ratio: f64,
        metrics: Option<DatasetMetrics>,
    ) -> Result<DatasetManifest, String> {
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("Failed to create output directory: {e}"))?;

        let split_point = (examples.len() as f64 * split_ratio).round() as usize;
        let (train, eval) = examples.split_at(split_point);

        let train_path = output_dir.join("train.jsonl");
        let eval_path = output_dir.join("eval.jsonl");

        write_jsonl(&train_path, train)?;
        write_jsonl(&eval_path, eval)?;

        let manifest = DatasetManifest {
            name: name.to_string(),
            version: "1.0".to_string(),
            source: None,
            total_examples: examples.len(),
            train_examples: train.len(),
            eval_examples: eval.len(),
            train_path: "train.jsonl".to_string(),
            eval_path: "eval.jsonl".to_string(),
            metrics,
            pre_cutoff: None,
            post_cutoff: None,
            imported_at: chrono::Utc::now().to_rfc3339(),
        };

        let manifest_path = output_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {e}"))?;
        std::fs::write(&manifest_path, &manifest_json)
            .map_err(|e| format!("Failed to write manifest: {e}"))?;

        Ok(manifest)
    }
}

/// Thin `ServiceModule` shell owning the [`DatasetService`] the typed
/// `dataset/*` commands capture.
pub struct DatasetModule {
    service: Arc<DatasetService>,
}

/// The ONE default datasets root (`~/.continuum/datasets`). Producers
/// (`dataset/*` commands) and consumers (`genome/job-create` by `datasetName`)
/// both resolve through here — the location is defined once.
pub fn default_datasets_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".continuum").join("datasets")
}

impl Default for DatasetModule {
    fn default() -> Self {
        Self {
            service: Arc::new(DatasetService::new(default_datasets_root())),
        }
    }
}

impl DatasetModule {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ServiceModule for DatasetModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "dataset",
            priority: ModulePriority::Background,
            command_prefixes: &["dataset/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!(
            "dataset command surface is migrated to the typed registry; \
             '{command}' has no legacy handler"
        ))
    }

    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::dataset::command_objects(self.service.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn find_column(headers: &csv::StringRecord, name: &str) -> Result<usize, String> {
    headers.iter().position(|h| h == name).ok_or_else(|| {
        format!(
            "Required column '{}' not found in CSV. Available: {:?}",
            name,
            headers.iter().collect::<Vec<_>>()
        )
    })
}

/// Locate a test file for a given snippet_id in the tests directory.
/// Tries common naming patterns: test_{id}.py, {id}_test.py, {id}.py
fn find_test_file(tests_dir: &Path, snippet_id: &str) -> Option<String> {
    let candidates = [
        format!("test_{}.py", snippet_id),
        format!("{}_test.py", snippet_id),
        format!("{}.py", snippet_id),
    ];

    for candidate in &candidates {
        let path = tests_dir.join(candidate);
        if path.exists() {
            return std::fs::read_to_string(&path).ok();
        }
    }

    // Fallback: search recursively for any file containing the snippet_id
    if let Ok(entries) = std::fs::read_dir(tests_dir) {
        for entry in entries.flatten() {
            let filename = entry.file_name();
            let filename = filename.to_string_lossy();
            if filename.contains(snippet_id) && filename.ends_with(".py") {
                return std::fs::read_to_string(entry.path()).ok();
            }
        }
    }

    None
}

/// Convert one recorded persona turn (recorder JSON) into a chat SFT example.
///
/// Returns `None` for non-`spoke` / empty / malformed turns — they aren't a
/// training pair. The recorder shape: `rustRequest.{systemPrompt,messageText,
/// recentHistory[]}` + `rustResponse` (a `PersonaResponse` enum, so a spoken
/// turn is `{"kind":"spoke","text":...}`). History items are attributed to
/// `assistant` when the sender is this persona, else `user`.
fn turn_to_example(turn: &Value, include_system: bool, include_history: bool) -> Option<Value> {
    let req = turn.get("rustRequest")?;
    let resp = turn.get("rustResponse")?;

    // Only a turn the persona actually spoke is a (user → assistant) pair.
    if resp.get("kind").and_then(|k| k.as_str()) != Some("spoke") {
        return None;
    }
    let assistant = resp.get("text").and_then(|t| t.as_str())?.trim();
    let user = req.get("messageText").and_then(|t| t.as_str())?.trim();
    if user.is_empty() || assistant.is_empty() {
        return None;
    }

    let persona_name = turn
        .get("personaName")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mut messages: Vec<Value> = Vec::new();

    if include_system {
        if let Some(sys) = req.get("systemPrompt").and_then(|s| s.as_str()) {
            let sys = sys.trim();
            if !sys.is_empty() {
                messages.push(json!({ "role": "system", "content": sys }));
            }
        }
    }

    if include_history {
        if let Some(hist) = req.get("recentHistory").and_then(|h| h.as_array()) {
            for h in hist {
                let Some(text) = h.get("text").and_then(|t| t.as_str()) else {
                    continue;
                };
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                let sender = h.get("senderName").and_then(|s| s.as_str()).unwrap_or("");
                let role = if !persona_name.is_empty() && sender == persona_name {
                    "assistant"
                } else {
                    "user"
                };
                messages.push(json!({ "role": role, "content": text }));
            }
        }
    }

    messages.push(json!({ "role": "user", "content": user }));
    messages.push(json!({ "role": "assistant", "content": assistant }));
    Some(json!({ "messages": messages }))
}

/// Convert ONE live prompt-capture record — the glass-box turn: the system prompt
/// + the consolidated burst (`messages`) + the model's `response.text` — into an
/// SFT `{messages}` example, the SAME shape [`turn_to_example`] emits. The
/// prompt-capture IS the canonical experience: the LIVE WorkspaceCycle path writes
/// it on every turn, so the trainer reads the work the persona actually did without
/// a second recorder on the hot path (one turn-truth).
///
/// Structural curation only — QUALITY scoring is a later, pluggable slice (the
/// genome-loop curation layer). Here we drop only what is never a valid learning
/// target: an empty response. Everything else passes through, tagged with its
/// `skillAxis` so the consumer can select; whether a turn is GOOD is a judgment for
/// the curator, not this projection.
///
/// `skillAxis` is intrinsic to the turn: `"operational"` when the turn ACTED —
/// emitted a tool call, either as the model's literal answer (JSON-in-prompt:
/// `response.text` parses as a call) or as a structured `response.toolCalls` array
/// the adapter extracted — else `"domain"` (prose/answer only). Acting turns are the
/// OPERATIONAL training signal the genome loop needs (the "run the test, don't just
/// narrate" habit the Phase-0 baseline showed missing — `selfVerifyRate 0.0`). The
/// old code BLANKET-dropped any tool-call answer; that only ever protected the chat
/// axis (control-plane JSON leaking into room prose) while destroying exactly the
/// dev-task signal. We keep both and let the consumer filter: a chat-prose dataset
/// asks for `"domain"`, the dev-task/self-verify loop for `"operational"`.
fn capture_to_example(cap: &Value, include_system: bool) -> Option<Value> {
    let assistant = cap.get("response")?.get("text")?.as_str()?.trim();
    if assistant.is_empty() {
        return None;
    }
    let acted = crate::ai::json_in_prompt_tools::parse_tool_call(assistant).is_some()
        || cap
            .get("response")
            .and_then(|r| r.get("toolCalls"))
            .and_then(|t| t.as_array())
            .is_some_and(|a| !a.is_empty());
    let skill_axis = if acted { "operational" } else { "domain" };
    let mut messages: Vec<Value> = Vec::new();
    if include_system {
        if let Some(sys) = cap.get("system").and_then(|s| s.as_str()) {
            let sys = sys.trim();
            if !sys.is_empty() {
                messages.push(json!({ "role": "system", "content": sys }));
            }
        }
    }
    if let Some(arr) = cap.get("messages").and_then(|m| m.as_array()) {
        for m in arr {
            let (Some(role), Some(content)) = (
                m.get("role").and_then(|r| r.as_str()),
                m.get("content").and_then(|c| c.as_str()),
            ) else {
                continue;
            };
            let content = content.trim();
            if content.is_empty() {
                continue;
            }
            messages.push(json!({ "role": role, "content": content }));
        }
    }
    // Need at least one non-system (burst) message → an actual (context → reply) pair.
    if messages
        .iter()
        .all(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"))
    {
        return None;
    }
    messages.push(json!({ "role": "assistant", "content": assistant }));
    Some(json!({ "messages": messages, "skillAxis": skill_axis }))
}

/// Write examples as JSONL (one JSON object per line).
fn write_jsonl(path: &Path, examples: &[Value]) -> Result<(), String> {
    use std::io::Write;
    let file = std::fs::File::create(path)
        .map_err(|e| format!("Failed to create {}: {e}", path.display()))?;
    let mut writer = std::io::BufWriter::new(file);

    for example in examples {
        serde_json::to_writer(&mut writer, example)
            .map_err(|e| format!("Failed to write JSONL: {e}"))?;
        writeln!(&mut writer).map_err(|e| format!("Failed to write newline: {e}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn service() -> DatasetService {
        // datasets_root is only used when a call omits outputDir; every test below
        // passes outputDir explicitly, so a temp root is a safe default.
        DatasetService::new(std::env::temp_dir().join("continuum-dataset-tests"))
    }

    fn create_test_csv(dir: &Path, filename: &str, content: &str) -> PathBuf {
        let path = dir.join(filename);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    // what this catches: a generic CSV imports to a split JSONL dataset + manifest
    // via the typed ImportCsvParams (the value the command returns is the manifest
    // JSON directly, no CommandResult wrapper).
    #[test]
    fn import_csv_basic() {
        let tmp = TempDir::new().unwrap();
        let csv_path = create_test_csv(
            tmp.path(),
            "test.csv",
            "input,output\nWhat is 2+2?,4\nCapital of France?,Paris\n",
        );
        let output_dir = tmp.path().join("out");

        let v = service()
            .import_csv(&ImportCsvParams {
                csv_path: csv_path.to_str().unwrap().to_string(),
                output_dir: Some(output_dir.to_str().unwrap().to_string()),
                name: "test-dataset".to_string(),
                split_ratio: 0.5,
                user_column: default_user_column(),
                assistant_column: default_assistant_column(),
            })
            .unwrap();

        assert_eq!(v.name, "test-dataset");
        assert_eq!(v.total_examples, 2);
        assert_eq!(v.train_examples, 1);
        assert_eq!(v.eval_examples, 1);

        // Files land under <root>/<name>/, never flat at the root — a second
        // import must not clobber the first, and dataset/list only scans subdirs.
        let ds_dir = output_dir.join("test-dataset");
        assert!(ds_dir.join("train.jsonl").exists());
        assert!(ds_dir.join("eval.jsonl").exists());
        assert!(ds_dir.join("manifest.json").exists());
    }

    // what this catches: the LIVE rooms→training bridge AND the L1 axis contract —
    // a prompt-capture (system + burst + response) becomes a clean {messages} SFT
    // pair tagged with its `skillAxis`. A prose answer → `"domain"`; a turn that
    // ACTED (tool call, either JSON-in-prompt OR a structured `response.toolCalls`
    // array) is KEPT and tagged `"operational"` — NOT dropped. Regression here =
    // either the live cognition turns stop reaching the trainer, or (the L1 bug we
    // fixed) the operational self-verify signal gets blanket-dropped so the genome
    // loop can never learn the "run the test, don't just narrate" habit. Garbage
    // (empty response, system-only) is still structurally dropped.
    #[test]
    fn capture_to_example_tags_skill_axis_and_keeps_acting_turns() {
        // A prose turn: system + a room burst + a spoken reply → one SFT pair on
        // the `domain` axis.
        let good = json!({
            "system": "You are Asha.",
            "messages": [{ "role": "user", "content": "[room x]\npeer: where is render_ai_help?" }],
            "response": { "text": "It's in core/continuum-core/src/commands/help.rs." }
        });
        let ex = capture_to_example(&good, true).expect("clean turn → example");
        let msgs = ex.get("messages").and_then(|m| m.as_array()).unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs.last().unwrap()["role"], "assistant");
        assert!(msgs.last().unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("help.rs"));
        assert_eq!(ex["skillAxis"], "domain", "prose turn → domain axis");

        // A tool-call envelope the model emitted as its literal answer (JSON-in-
        // prompt) → KEPT, tagged operational. This is the act we want to train.
        let tool_json = json!({
            "system": "You are Asha.",
            "messages": [{ "role": "user", "content": "ping it" }],
            "response": { "text": "{\"tool_call\": {\"name\": \"ping\", \"arguments\": {}}}" }
        });
        let ex = capture_to_example(&tool_json, true).expect("acting turn → example");
        assert_eq!(
            ex["skillAxis"], "operational",
            "JSON-in-prompt call → operational"
        );

        // A structured `response.toolCalls` array (adapter-extracted) with prose
        // preamble → KEPT, tagged operational.
        let structured = json!({
            "system": "You are Asha.",
            "messages": [{ "role": "user", "content": "write merge_intervals and test it" }],
            "response": {
                "text": "I'll write it then run the tests.",
                "toolCalls": [{ "id": "c1", "input": { "code": "fn merge_intervals() {}" } }]
            }
        });
        let ex = capture_to_example(&structured, true).expect("structured-call turn → example");
        assert_eq!(
            ex["skillAxis"], "operational",
            "structured toolCalls → operational"
        );

        // Empty response → dropped (no pair).
        let empty = json!({
            "system": "You are Asha.",
            "messages": [{ "role": "user", "content": "hi" }],
            "response": { "text": "   " }
        });
        assert!(
            capture_to_example(&empty, true).is_none(),
            "empty response must be dropped"
        );

        // No burst (only system) → dropped (not a context→reply pair).
        let no_burst = json!({
            "system": "You are Asha.",
            "messages": [],
            "response": { "text": "hello" }
        });
        assert!(
            capture_to_example(&no_burst, true).is_none(),
            "system-only must be dropped"
        );
    }

    // what this catches: the rooms→training-data bridge — recorded persona
    // turns (the recorder's per-turn JSON) convert to {messages:[{role,content}]}
    // SFT examples through the SAME split/write path as the CSV importer, and
    // only `spoke` turns become training pairs (silent turns are dropped).
    #[test]
    fn from_turns_builds_sft_dataset_from_spoke_turns() {
        let tmp = TempDir::new().unwrap();
        let turns_dir = tmp.path().join("turns");
        std::fs::create_dir_all(&turns_dir).unwrap();

        // A spoke turn → becomes one (system + user + assistant) example.
        let spoke = json!({
            "schemaVersion": 1,
            "personaId": "11111111-1111-1111-1111-111111111111",
            "personaName": "Gastro",
            "roomId": "22222222-2222-2222-2222-222222222222",
            "rustRequest": {
                "systemPrompt": "You are a gastroenterology specialist.",
                "messageText": "What causes reflux?",
                "recentHistory": []
            },
            "rustResponse": { "kind": "spoke", "text": "Lower esophageal sphincter dysfunction." }
        });
        create_test_csv(&turns_dir, "a-rust.json", &spoke.to_string());

        // A silent turn → NOT a training pair, must be skipped.
        let silent = json!({
            "personaId": "11111111-1111-1111-1111-111111111111",
            "roomId": "22222222-2222-2222-2222-222222222222",
            "rustRequest": { "systemPrompt": "sys", "messageText": "ignored?", "recentHistory": [] },
            "rustResponse": { "kind": "silent" }
        });
        create_test_csv(&turns_dir, "b-rust.json", &silent.to_string());

        let output_dir = tmp.path().join("out");
        let v = service()
            .from_turns(&FromTurnsParams {
                turns_dir: Some(turns_dir.to_str().unwrap().to_string()),
                output_dir: Some(output_dir.to_str().unwrap().to_string()),
                name: "gastro-turns".to_string(),
                split_ratio: 1.0,
                include_system: true,
                include_history: false,
                persona_id: None,
                room_id: None,
            })
            .unwrap();

        assert_eq!(v.name, "gastro-turns");
        assert_eq!(v.total_examples, 1, "only the spoke turn is a pair");

        // The written example is a chat SFT record: system + user + assistant —
        // landed under <root>/<name>/, never flat at the root (a second dataset
        // must not clobber the first; dataset/list only scans subdirectories).
        let train =
            std::fs::read_to_string(output_dir.join("gastro-turns").join("train.jsonl")).unwrap();
        let example: Value = serde_json::from_str(train.lines().next().unwrap()).unwrap();
        let msgs = example["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "What causes reflux?");
        assert_eq!(msgs[2]["role"], "assistant");
        assert_eq!(
            msgs[2]["content"],
            "Lower esophageal sphincter dysfunction."
        );
    }

    // what this catches: a turnsDir with no spoke turns is an explicit error
    // (nothing to train on), not a silent empty dataset.
    #[test]
    fn from_turns_errors_when_no_spoke_turns() {
        let tmp = TempDir::new().unwrap();
        let turns_dir = tmp.path().join("turns");
        std::fs::create_dir_all(&turns_dir).unwrap();
        let silent = json!({
            "rustRequest": { "systemPrompt": "s", "messageText": "m", "recentHistory": [] },
            "rustResponse": { "kind": "silent" }
        });
        create_test_csv(&turns_dir, "only-silent-rust.json", &silent.to_string());

        let err = service()
            .from_turns(&FromTurnsParams {
                turns_dir: Some(turns_dir.to_str().unwrap().to_string()),
                output_dir: Some(tmp.path().join("out").to_str().unwrap().to_string()),
                name: default_persona_turns_name(),
                split_ratio: default_split_ratio(),
                include_system: true,
                include_history: false,
                persona_id: None,
                room_id: None,
            })
            .unwrap_err();
        assert!(err.contains("No spoke turns"), "got: {err}");
    }

    // what this catches: a missing CSV is a loud error naming the path, not a panic.
    #[test]
    fn import_csv_missing_file() {
        let err = service()
            .import_csv(&ImportCsvParams {
                csv_path: "/nonexistent/path.csv".to_string(),
                output_dir: Some("/tmp/test-out".to_string()),
                name: default_imported_name(),
                split_ratio: default_split_ratio(),
                user_column: default_user_column(),
                assistant_column: default_assistant_column(),
            })
            .unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    // what this catches: RealClassEval legacy single-CSV mode imports with metrics
    // and the arxiv source stamp; all rows count as pre-cutoff (is_post=false).
    #[test]
    fn import_realclasseval_legacy_csv_mode() {
        let tmp = TempDir::new().unwrap();

        let csv_content = r#"snippet_id,class_name,human_written_code,class_skeleton,cyclomatic_complexity,lines_of_code
snippet_0,Calculator,"class Calculator:\n    def add(self, a, b):\n        return a + b","class Calculator:\n    def add(self, a, b):\n        pass",1,3
snippet_200,Parser,"class Parser:\n    def parse(self, text):\n        return text.split()","class Parser:\n    def parse(self, text):\n        pass",2,5"#;

        let csv_path = create_test_csv(tmp.path(), "RealClassEval.csv", csv_content);

        let tests_dir = tmp.path().join("tests");
        std::fs::create_dir_all(&tests_dir).unwrap();
        create_test_csv(
            &tests_dir,
            "test_snippet_0.py",
            "def test_add():\n    c = Calculator()\n    assert c.add(1, 2) == 3\n",
        );

        let output_dir = tmp.path().join("out");

        let v = service()
            .import_realclasseval(&ImportRealClassEvalParams {
                repo_dir: None,
                csv_path: Some(csv_path.to_str().unwrap().to_string()),
                tests_dir: Some(tests_dir.to_str().unwrap().to_string()),
                output_dir: Some(output_dir.to_str().unwrap().to_string()),
                split_ratio: 0.5,
            })
            .unwrap();

        assert_eq!(v.name, "realclasseval");
        assert_eq!(v.total_examples, 2);
        assert_eq!(v.source.as_deref(), Some("arxiv:2510.26130"));
        // Legacy mode: all counted as pre-cutoff (is_post=false)
        assert_eq!(v.pre_cutoff, Some(2));
        assert!(
            v.metrics
                .as_ref()
                .and_then(|m| m.avg_cyclomatic_complexity)
                .unwrap()
                > 0.0
        );
    }

    // what this catches: RealClassEval repo-dir auto-discovery splits csn (pre) +
    // post_cut-off (post) and labels the cutoff counts correctly.
    #[test]
    fn import_realclasseval_repo_dir_mode() {
        let tmp = TempDir::new().unwrap();

        let base = tmp.path().join("data").join("functional_correctness_data");

        // CSN split (pre-cutoff)
        let csn_dfs = base.join("csn").join("dfs");
        std::fs::create_dir_all(&csn_dfs).unwrap();
        create_test_csv(&csn_dfs, "no_docstr.csv",
            "snippet_id,class_name,human_written_code,class_skeleton\nsnippet_10,Foo,\"class Foo:\\n    pass\",\"class Foo:\\n    pass\"\n");

        let csn_tests = base
            .join("csn")
            .join("pynguin_generated_tests")
            .join("full_docstr");
        std::fs::create_dir_all(&csn_tests).unwrap();
        create_test_csv(&csn_tests, "test_snippet_10.py", "def test_foo(): pass\n");

        // Post-cutoff split
        let post_dfs = base.join("post_cut-off").join("dfs");
        std::fs::create_dir_all(&post_dfs).unwrap();
        create_test_csv(&post_dfs, "no_docstr.csv",
            "snippet_id,class_name,human_written_code,class_skeleton\nsnippet_300,Bar,\"class Bar:\\n    pass\",\"class Bar:\\n    pass\"\n");

        let post_tests = base
            .join("post_cut-off")
            .join("pynguin_generated_tests")
            .join("full_docstr");
        std::fs::create_dir_all(&post_tests).unwrap();
        create_test_csv(&post_tests, "test_snippet_300.py", "def test_bar(): pass\n");

        let output_dir = tmp.path().join("out");

        let v = service()
            .import_realclasseval(&ImportRealClassEvalParams {
                repo_dir: Some(tmp.path().to_str().unwrap().to_string()),
                csv_path: None,
                tests_dir: None,
                output_dir: Some(output_dir.to_str().unwrap().to_string()),
                split_ratio: 0.5,
            })
            .unwrap();

        assert_eq!(v.name, "realclasseval");
        assert_eq!(v.total_examples, 2);
        assert_eq!(v.source.as_deref(), Some("arxiv:2510.26130"));
        assert_eq!(v.pre_cutoff, Some(1)); // csn
        assert_eq!(v.post_cutoff, Some(1)); // post_cut-off

        assert!(output_dir.join("train.jsonl").exists());
        assert!(output_dir.join("eval.jsonl").exists());
        assert!(output_dir.join("manifest.json").exists());
    }

    // what this catches: list returns the manifests under the root + a count.
    #[test]
    fn list_datasets_reads_manifests() {
        let tmp = TempDir::new().unwrap();
        let dataset_dir = tmp.path().join("my-dataset");
        std::fs::create_dir_all(&dataset_dir).unwrap();

        let manifest = DatasetManifest {
            name: "my-dataset".to_string(),
            version: "1.0".to_string(),
            source: None,
            total_examples: 100,
            train_examples: 80,
            eval_examples: 20,
            train_path: "train.jsonl".to_string(),
            eval_path: "eval.jsonl".to_string(),
            metrics: None,
            pre_cutoff: None,
            post_cutoff: None,
            imported_at: "2026-03-05T00:00:00Z".to_string(),
        };

        std::fs::write(
            dataset_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let v = service()
            .list_datasets(&ListDatasetsParams {
                output_dir: Some(tmp.path().to_str().unwrap().to_string()),
            })
            .unwrap();
        assert_eq!(v.count, 1);
        assert_eq!(v.datasets[0].name, "my-dataset");
    }

    // what this catches: info reads one named dataset's manifest.
    #[test]
    fn dataset_info_reads_manifest() {
        let tmp = TempDir::new().unwrap();
        let dataset_dir = tmp.path().join("test-ds");
        std::fs::create_dir_all(&dataset_dir).unwrap();

        let manifest = DatasetManifest {
            name: "test-ds".to_string(),
            version: "1.0".to_string(),
            source: Some("test".to_string()),
            total_examples: 50,
            train_examples: 40,
            eval_examples: 10,
            train_path: "train.jsonl".to_string(),
            eval_path: "eval.jsonl".to_string(),
            metrics: None,
            pre_cutoff: None,
            post_cutoff: None,
            imported_at: "2026-03-05T00:00:00Z".to_string(),
        };

        std::fs::write(
            dataset_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let v = service()
            .dataset_info(&DatasetInfoParams {
                name: "test-ds".to_string(),
                output_dir: Some(tmp.path().to_str().unwrap().to_string()),
            })
            .unwrap();
        assert_eq!(v.name, "test-ds");
        assert_eq!(v.total_examples, 50);
    }

    // what this catches: info on a missing dataset is a loud error, not an empty ok.
    #[test]
    fn dataset_info_not_found() {
        let tmp = TempDir::new().unwrap();
        let err = service()
            .dataset_info(&DatasetInfoParams {
                name: "nonexistent".to_string(),
                output_dir: Some(tmp.path().to_str().unwrap().to_string()),
            })
            .unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    // what this catches: the legacy handle_command surface fails loud (the typed
    // registry owns dataset/* now) rather than silently dispatching.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let module = DatasetModule::new();
        let err = module
            .handle_command("dataset/list", json!({}))
            .await
            .unwrap_err();
        assert!(err.contains("migrated to the typed registry"), "got: {err}");
    }
}
