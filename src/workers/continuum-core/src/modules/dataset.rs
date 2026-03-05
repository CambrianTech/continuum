//! Dataset Module — Import, manage, and query training datasets.
//!
//! All heavy data processing (CSV parsing, JSONL conversion, file I/O)
//! happens here in Rust off the main thread. TypeScript is a thin API layer.
//!
//! Supports RealClassEval (arxiv:2510.26130) and generic CSV imports.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::path::{Path, PathBuf};

/// Manifest persisted alongside imported datasets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetManifest {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub total_examples: usize,
    pub train_examples: usize,
    pub eval_examples: usize,
    pub train_path: String,
    pub eval_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<DatasetMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_cutoff: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_cutoff: Option<usize>,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_cyclomatic_complexity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_lines_of_code: Option<f64>,
}

pub struct DatasetModule {
    datasets_root: PathBuf,
}

impl DatasetModule {
    pub fn new() -> Self {
        // Default datasets root — overridable per-request via params
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let datasets_root = PathBuf::from(home)
            .join(".continuum")
            .join("datasets");
        Self { datasets_root }
    }

    /// Resolve the datasets root directory, preferring param override.
    fn resolve_datasets_root(&self, params: &Value) -> PathBuf {
        params
            .get("outputDir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.datasets_root.clone())
    }

    /// Import a generic CSV file as a JSONL training dataset.
    async fn import_csv(&self, params: Value) -> Result<CommandResult, String> {
        let csv_path = params.get("csvPath")
            .and_then(|v| v.as_str())
            .ok_or("Missing required param: csvPath")?;

        let output_dir = self.resolve_datasets_root(&params);
        let name = params.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("imported");
        let split_ratio = params.get("splitRatio")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.8);

        let user_col = params.get("userColumn")
            .and_then(|v| v.as_str())
            .unwrap_or("input");
        let assistant_col = params.get("assistantColumn")
            .and_then(|v| v.as_str())
            .unwrap_or("output");

        // Parse CSV
        let csv_path = PathBuf::from(csv_path);
        if !csv_path.exists() {
            return Err(format!("CSV file not found: {}", csv_path.display()));
        }

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .from_path(&csv_path)
            .map_err(|e| format!("Failed to open CSV: {e}"))?;

        let headers = reader.headers()
            .map_err(|e| format!("Failed to read CSV headers: {e}"))?
            .clone();

        let user_idx = headers.iter().position(|h| h == user_col)
            .ok_or_else(|| format!("Column '{user_col}' not found in CSV. Available: {:?}", headers.iter().collect::<Vec<_>>()))?;
        let assistant_idx = headers.iter().position(|h| h == assistant_col)
            .ok_or_else(|| format!("Column '{assistant_col}' not found in CSV. Available: {:?}", headers.iter().collect::<Vec<_>>()))?;

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

        // Split and write
        let manifest = self.split_and_write(name, &output_dir, &examples, split_ratio, None)?;
        CommandResult::json(&manifest)
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
    async fn import_realclasseval(&self, params: Value) -> Result<CommandResult, String> {
        let output_dir = params.get("outputDir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.datasets_root.join("realclasseval"));

        let split_ratio = params.get("splitRatio")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.8);

        // Collect (csv_path, tests_dir, is_post_cutoff) pairs
        let splits: Vec<(PathBuf, PathBuf, bool)> = if let Some(repo_dir) = params.get("repoDir").and_then(|v| v.as_str()) {
            // Auto-discover from repo directory structure
            let base = PathBuf::from(repo_dir).join("data").join("functional_correctness_data");
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
                let tests_dir = split_dir.join("pynguin_generated_tests").join("full_docstr");

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
        } else if let Some(csv_path) = params.get("csvPath").and_then(|v| v.as_str()) {
            // Legacy single-CSV mode
            let tests_dir = params.get("testsDir")
                .and_then(|v| v.as_str())
                .ok_or("Missing required param: testsDir (or use repoDir for auto-discovery)")?;
            vec![(PathBuf::from(csv_path), PathBuf::from(tests_dir), false)]
        } else {
            return Err("Missing required param: repoDir (path to cloned RealClassEval repo) or csvPath".to_string());
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

            let headers = reader.headers()
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
            avg_cyclomatic_complexity: if cc_count > 0 { Some(total_cc / cc_count as f64) } else { None },
            avg_lines_of_code: if loc_count > 0 { Some(total_loc / loc_count as f64) } else { None },
        };

        let mut manifest = self.split_and_write(
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

        CommandResult::json(&manifest)
    }

    /// List datasets in the datasets root directory.
    async fn list_datasets(&self, params: Value) -> Result<CommandResult, String> {
        let root = self.resolve_datasets_root(&params);

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
                            if let Ok(manifest) = serde_json::from_str::<DatasetManifest>(&content) {
                                datasets.push(manifest);
                            }
                        }
                    }
                }
            }
        }

        Ok(CommandResult::Json(json!({
            "datasets": datasets,
            "count": datasets.len(),
            "root": root.to_string_lossy(),
        })))
    }

    /// Read manifest for a specific dataset.
    async fn dataset_info(&self, params: Value) -> Result<CommandResult, String> {
        let name = params.get("name")
            .and_then(|v| v.as_str())
            .ok_or("Missing required param: name")?;

        let root = self.resolve_datasets_root(&params);
        let manifest_path = root.join(name).join("manifest.json");

        if !manifest_path.exists() {
            return Err(format!("Dataset '{}' not found at {}", name, manifest_path.display()));
        }

        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read manifest: {e}"))?;
        let manifest: DatasetManifest = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse manifest: {e}"))?;

        CommandResult::json(&manifest)
    }

    /// Split examples into train/eval, write JSONL files and manifest.
    fn split_and_write(
        &self,
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

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "dataset/import-csv" => self.import_csv(params).await,
            "dataset/import-realclasseval" => self.import_realclasseval(params).await,
            "dataset/list" => self.list_datasets(params).await,
            "dataset/info" => self.dataset_info(params).await,
            _ => Err(format!("Unknown dataset command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn find_column(headers: &csv::StringRecord, name: &str) -> Result<usize, String> {
    headers
        .iter()
        .position(|h| h == name)
        .ok_or_else(|| {
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

/// Write examples as JSONL (one JSON object per line).
fn write_jsonl(path: &Path, examples: &[Value]) -> Result<(), String> {
    use std::io::Write;
    let file = std::fs::File::create(path)
        .map_err(|e| format!("Failed to create {}: {e}", path.display()))?;
    let mut writer = std::io::BufWriter::new(file);

    for example in examples {
        serde_json::to_writer(&mut writer, example)
            .map_err(|e| format!("Failed to write JSONL: {e}"))?;
        writeln!(&mut writer)
            .map_err(|e| format!("Failed to write newline: {e}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::io::Write;

    fn create_test_csv(dir: &Path, filename: &str, content: &str) -> PathBuf {
        let path = dir.join(filename);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[tokio::test]
    async fn test_import_csv_basic() {
        let tmp = TempDir::new().unwrap();
        let csv_path = create_test_csv(
            tmp.path(),
            "test.csv",
            "input,output\nWhat is 2+2?,4\nCapital of France?,Paris\n",
        );
        let output_dir = tmp.path().join("out");

        let module = DatasetModule::new();
        let params = json!({
            "csvPath": csv_path.to_str().unwrap(),
            "outputDir": output_dir.to_str().unwrap(),
            "name": "test-dataset",
            "splitRatio": 0.5,
        });

        let result = module.import_csv(params).await.unwrap();
        if let CommandResult::Json(v) = result {
            assert_eq!(v["name"], "test-dataset");
            assert_eq!(v["total_examples"], 2);
            assert_eq!(v["train_examples"], 1);
            assert_eq!(v["eval_examples"], 1);
        } else {
            panic!("Expected JSON result");
        }

        // Verify files exist
        assert!(output_dir.join("train.jsonl").exists());
        assert!(output_dir.join("eval.jsonl").exists());
        assert!(output_dir.join("manifest.json").exists());
    }

    #[tokio::test]
    async fn test_import_csv_missing_file() {
        let module = DatasetModule::new();
        let params = json!({
            "csvPath": "/nonexistent/path.csv",
            "outputDir": "/tmp/test-out",
        });

        let result = module.import_csv(params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_import_realclasseval_legacy_csv_mode() {
        let tmp = TempDir::new().unwrap();

        // Create a minimal RealClassEval CSV
        let csv_content = r#"snippet_id,class_name,human_written_code,class_skeleton,cyclomatic_complexity,lines_of_code
snippet_0,Calculator,"class Calculator:\n    def add(self, a, b):\n        return a + b","class Calculator:\n    def add(self, a, b):\n        pass",1,3
snippet_200,Parser,"class Parser:\n    def parse(self, text):\n        return text.split()","class Parser:\n    def parse(self, text):\n        pass",2,5"#;

        let csv_path = create_test_csv(tmp.path(), "RealClassEval.csv", csv_content);

        // Create test directory with test files
        let tests_dir = tmp.path().join("tests");
        std::fs::create_dir_all(&tests_dir).unwrap();
        create_test_csv(
            &tests_dir,
            "test_snippet_0.py",
            "def test_add():\n    c = Calculator()\n    assert c.add(1, 2) == 3\n",
        );

        let output_dir = tmp.path().join("out");

        let module = DatasetModule::new();
        let params = json!({
            "csvPath": csv_path.to_str().unwrap(),
            "testsDir": tests_dir.to_str().unwrap(),
            "outputDir": output_dir.to_str().unwrap(),
            "splitRatio": 0.5,
        });

        let result = module.import_realclasseval(params).await.unwrap();
        if let CommandResult::Json(v) = result {
            assert_eq!(v["name"], "realclasseval");
            assert_eq!(v["total_examples"], 2);
            assert_eq!(v["source"], "arxiv:2510.26130");
            // Legacy mode: all counted as pre-cutoff (is_post=false)
            assert_eq!(v["pre_cutoff"], 2);
            assert!(v["metrics"]["avg_cyclomatic_complexity"].as_f64().unwrap() > 0.0);
        } else {
            panic!("Expected JSON result");
        }
    }

    #[tokio::test]
    async fn test_import_realclasseval_repo_dir_mode() {
        let tmp = TempDir::new().unwrap();

        // Simulate RealClassEval repo directory structure:
        //   data/functional_correctness_data/csn/dfs/no_docstr.csv
        //   data/functional_correctness_data/csn/pynguin_generated_tests/full_docstr/test_snippet_10.py
        //   data/functional_correctness_data/post_cut-off/dfs/no_docstr.csv
        //   data/functional_correctness_data/post_cut-off/pynguin_generated_tests/full_docstr/test_snippet_300.py

        let base = tmp.path().join("data").join("functional_correctness_data");

        // CSN split (pre-cutoff)
        let csn_dfs = base.join("csn").join("dfs");
        std::fs::create_dir_all(&csn_dfs).unwrap();
        create_test_csv(&csn_dfs, "no_docstr.csv",
            "snippet_id,class_name,human_written_code,class_skeleton\nsnippet_10,Foo,\"class Foo:\\n    pass\",\"class Foo:\\n    pass\"\n");

        let csn_tests = base.join("csn").join("pynguin_generated_tests").join("full_docstr");
        std::fs::create_dir_all(&csn_tests).unwrap();
        create_test_csv(&csn_tests, "test_snippet_10.py", "def test_foo(): pass\n");

        // Post-cutoff split
        let post_dfs = base.join("post_cut-off").join("dfs");
        std::fs::create_dir_all(&post_dfs).unwrap();
        create_test_csv(&post_dfs, "no_docstr.csv",
            "snippet_id,class_name,human_written_code,class_skeleton\nsnippet_300,Bar,\"class Bar:\\n    pass\",\"class Bar:\\n    pass\"\n");

        let post_tests = base.join("post_cut-off").join("pynguin_generated_tests").join("full_docstr");
        std::fs::create_dir_all(&post_tests).unwrap();
        create_test_csv(&post_tests, "test_snippet_300.py", "def test_bar(): pass\n");

        let output_dir = tmp.path().join("out");

        let module = DatasetModule::new();
        let params = json!({
            "repoDir": tmp.path().to_str().unwrap(),
            "outputDir": output_dir.to_str().unwrap(),
            "splitRatio": 0.5,
        });

        let result = module.import_realclasseval(params).await.unwrap();
        if let CommandResult::Json(v) = result {
            assert_eq!(v["name"], "realclasseval");
            assert_eq!(v["total_examples"], 2);
            assert_eq!(v["source"], "arxiv:2510.26130");
            assert_eq!(v["pre_cutoff"], 1);  // csn
            assert_eq!(v["post_cutoff"], 1); // post_cut-off
        } else {
            panic!("Expected JSON result");
        }

        // Verify output files
        assert!(output_dir.join("train.jsonl").exists());
        assert!(output_dir.join("eval.jsonl").exists());
        assert!(output_dir.join("manifest.json").exists());
    }

    #[tokio::test]
    async fn test_list_datasets() {
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
        ).unwrap();

        let module = DatasetModule::new();
        let params = json!({ "outputDir": tmp.path().to_str().unwrap() });

        let result = module.list_datasets(params).await.unwrap();
        if let CommandResult::Json(v) = result {
            assert_eq!(v["count"], 1);
            assert_eq!(v["datasets"][0]["name"], "my-dataset");
        } else {
            panic!("Expected JSON result");
        }
    }

    #[tokio::test]
    async fn test_dataset_info() {
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
        ).unwrap();

        let module = DatasetModule::new();
        let params = json!({
            "name": "test-ds",
            "outputDir": tmp.path().to_str().unwrap(),
        });

        let result = module.dataset_info(params).await.unwrap();
        if let CommandResult::Json(v) = result {
            assert_eq!(v["name"], "test-ds");
            assert_eq!(v["total_examples"], 50);
        } else {
            panic!("Expected JSON result");
        }
    }

    #[tokio::test]
    async fn test_dataset_info_not_found() {
        let tmp = TempDir::new().unwrap();
        let module = DatasetModule::new();
        let params = json!({
            "name": "nonexistent",
            "outputDir": tmp.path().to_str().unwrap(),
        });

        let result = module.dataset_info(params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
