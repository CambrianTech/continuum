use super::*;

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
    #[ts(optional)]
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
pub(super) fn default_split_ratio() -> f64 {
    0.8
}

pub(super) fn default_true() -> bool {
    true
}

pub(super) fn default_imported_name() -> String {
    "imported".to_string()
}

pub(super) fn default_persona_turns_name() -> String {
    "persona-turns".to_string()
}

pub(super) fn default_persona_captures_name() -> String {
    "persona-captures".to_string()
}

pub(super) fn default_user_column() -> String {
    "input".to_string()
}

pub(super) fn default_assistant_column() -> String {
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
    /// Include nil-room captures (benchmark/eval FORK ticks). Default `false`:
    /// forks share the persona's capture file (`fork_eval_cycle` keeps
    /// `persona_id`, the sink path is `<dir>/<persona_id>.jsonl`), so without
    /// this guard a default dataset run silently ingests measurement-fork turns
    /// as if they were her lived room experience — unlabeled benchmark work
    /// entering L1 while the SAME work is deliberately excluded from L2
    /// (2026-08-14 citizenship audit, AXIS 3). Set `true` only when a fork
    /// corpus is the explicit goal.
    #[serde(default)]
    pub include_forks: bool,
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
    pub(super) datasets_root: PathBuf,
}
