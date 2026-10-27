use super::*;

impl DatasetService {
    pub fn new(datasets_root: PathBuf) -> Self {
        Self { datasets_root }
    }

    /// Resolve the datasets root directory, preferring the per-call override.
    pub(super) fn resolve_root(&self, output_dir: Option<&str>) -> PathBuf {
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
        // Where the recorder writes turns, through the same owner it writes through (card 2ceab504):
        // a reader that resolved its own path would miss the turns once the homes differ.
        let turns_dir = match p.turns_dir.as_deref() {
            Some(dir) => PathBuf::from(dir),
            None => crate::cognition::capture::fixture_dir(crate::persona::recorder::RESPOND_FIXTURE_DIR)
                .ok_or("no home directory: cannot find the recorder's turns; pass turnsDir")?,
        };
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
            .or_else(|| {
                crate::cognition::capture::fixture_dir(crate::cognition::prompt_capture::FIXTURE_DIR)
            })
            .ok_or_else(|| "cannot resolve capture home directory".to_string())?;
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
            let captures = crate::cognition::prompt_capture::completed_file(&path, usize::MAX)
                .map_err(|error| format!("capture {}: {error}", path.display()))?;
            for cap in captures {
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
                // FORK GUARD (#427): benchmark/eval fork ticks land in the SAME
                // per-persona capture file as live turns and must not teach.
                // Schema v3 captures carry the turn's CAUSE — "synthetic" IS the
                // fork/fixture marker (every turn now has a real room, #425, so
                // nil-room inference is dead going forward). The nil-room check
                // stays only for pre-v3 capture lines. An explicit `room_id`
                // filter above already expresses the caller's intent, so the
                // guard applies only to unfiltered runs.
                if !p.include_forks && p.room_id.is_none() {
                    if cap.get("cause").and_then(|v| v.as_str()) == Some("synthetic") {
                        continue;
                    }
                    let nil = uuid::Uuid::nil().to_string();
                    if cap.get("room_id").and_then(|v| v.as_str()) == Some(nil.as_str()) {
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
    pub(super) service: Arc<DatasetService>,
}

/// The ONE default datasets root (`<continuum home>/datasets`). Producers
/// (`dataset/*` commands) and consumers (`genome/job-create` by `datasetName`)
/// both resolve through here — the location is defined once, and the home through
/// [`crate::paths::continuum_home`], the resolution every other store uses.
/// It read `HOME` alone before, and a Windows core started by its scheduled task has no
/// `HOME`, so `dataset/list` on the 5090 read `/tmp\.continuum\datasets` and found nothing
/// (Codex, 2026-09-28, preparing Kimi's first dream).
/// With no home at all it fails loud (Cormac on #4496): a dataset written under a temp dir
/// is a dataset the next boot cannot find.
pub fn default_datasets_root() -> Result<PathBuf, crate::sdk_codegen::CommandError> {
    Ok(crate::paths::ContinuumLayout::home()?.datasets())
}

impl Default for DatasetModule {
    fn default() -> Self {
        Self {
            // A module constructor cannot fail; with no home the service is rooted where
            // nothing persists, and says so once. Every command path resolves the root itself
            // and fails loud.
            service: Arc::new(DatasetService::new(default_datasets_root().unwrap_or_else(|e| {
                continuum_base::probe!(
                    class = "dataset.root.unresolved",
                    error = e.to_string().as_str(),
                    "no continuum home: dataset/* writes land in the temp dir and will not survive a boot"
                );
                // Not a continuum home: a temp dir named for what it is.
                std::env::temp_dir().join("continuum-datasets-unrooted")
            }))),
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
pub(super) fn find_column(headers: &csv::StringRecord, name: &str) -> Result<usize, String> {
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
pub(super) fn find_test_file(tests_dir: &Path, snippet_id: &str) -> Option<String> {
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
