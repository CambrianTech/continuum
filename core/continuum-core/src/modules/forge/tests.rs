#![cfg(test)]
    use super::*;
    use crate::forge::{AlloyHardware, AlloySource, CorpusRef};
    use uuid::Uuid;

    /// A process+tag-unique manifest path so parallel gguf-lora export tests never
    /// collide and never touch the real `~/.continuum` manifest (DI mirrors the
    /// `register` vs `register_at` split — no env globals, no test pollution).
    fn tmp_manifest(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "forge_export_manifest_{}_{}.json",
            tag,
            std::process::id()
        ))
    }

    fn synthetic_recipe() -> ForgeRecipe {
        ForgeRecipe {
            id: Uuid::new_v4(),
            name: "test-recipe".to_string(),
            version: "0.1.0".to_string(),
            description: "test".to_string(),
            user_summary: "test summary".to_string(),
            author: "test".to_string(),
            tags: vec!["test".to_string()],
            license: "apache-2.0".to_string(),
            methodology_paper_url: None,
            limitations: vec![],
            prior_metric_baselines: vec![],
            source: AlloySource {
                base_model: "test-model".to_string(),
                architecture: "test-arch".to_string(),
                revision: None,
                is_moe: false,
                total_experts: None,
            },
            stages: vec![],
            cycles: 1,
            calibration_corpus: CorpusRef {
                name: "test-corpus".to_string(),
                content_hash: "sha256:test".to_string(),
                size_bytes: 0,
                source_url: None,
            },
            quant_tiers: vec![],
            evaluation_benchmarks: vec![],
            hardware: AlloyHardware {
                min_vram_gb: None,
                recommended_vram_gb: None,
                estimated_duration_minutes: None,
                supports_cpu: false,
                tested_on: vec![],
            },
            parent_recipe_id: None,
            authored_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    // ── ForgeArtifact::from_recipe — the recipe→artifact projection ──

    /// What this catches: `from_recipe` denormalizes every recipe field the
    /// model card renders (lineage + prose + config), and assigns a FRESH
    /// artifact id distinct from the recipe's. This is the single-source
    /// projection the stub (and the future foundry executor) build on;
    /// dropping a field here silently strips it from every artifact.
    #[test]
    fn from_recipe_projects_inherited_fields_with_fresh_id() {
        let recipe = synthetic_recipe();
        let a = ForgeArtifact::from_recipe(&recipe);

        assert_ne!(a.id, recipe.id, "artifact id must be fresh (1:N)");
        // Lineage.
        assert_eq!(a.recipe_id, recipe.id);
        assert_eq!(a.recipe_version, recipe.version);
        assert_eq!(a.recipe_name, recipe.name);
        // Denormalized prose / config snapshot.
        assert_eq!(a.description, recipe.description);
        assert_eq!(a.user_summary, recipe.user_summary);
        assert_eq!(a.author, recipe.author);
        assert_eq!(a.tags, recipe.tags);
        assert_eq!(a.license, recipe.license);
        assert_eq!(a.methodology_paper_url, recipe.methodology_paper_url);
        assert_eq!(a.limitations, recipe.limitations);
        assert_eq!(a.source.base_model, recipe.source.base_model);
        assert_eq!(
            a.calibration_corpus.content_hash,
            recipe.calibration_corpus.content_hash
        );
        assert_eq!(a.quant_tiers.len(), recipe.quant_tiers.len());
        assert_eq!(
            a.evaluation_benchmarks.len(),
            recipe.evaluation_benchmarks.len()
        );
        assert_eq!(a.hardware.supports_cpu, recipe.hardware.supports_cpu);
    }

    /// What this catches: `from_recipe` returns an UNFORGED skeleton — every
    /// foundry-execution field at its "not yet run" default (forged_at_ms=0,
    /// the rest None/empty). The foundry/stub stamps these AFTER running
    /// stages; if the projection pre-filled them it would fabricate
    /// execution facts no run produced.
    #[test]
    fn from_recipe_leaves_execution_fields_unforged() {
        let a = ForgeArtifact::from_recipe(&synthetic_recipe());
        assert_eq!(a.forged_at_ms, 0, "unforged sentinel");
        assert!(a.duration_minutes.is_none());
        assert!(a.forged_params_b.is_none());
        assert!(a.active_params_b.is_none());
        assert!(a.hardware_verified.is_empty());
        assert!(a.alloy_hash.is_none());
        assert!(a.results.is_none());
        assert!(a.receipt.is_none());
        assert!(a.integrity.is_none());
    }

    /// What this catches: stub artifact carries the recipe's lineage
    /// (recipe_id + recipe_version + recipe_name) frozen at synthesis
    /// time. If a Phase 5+ refactor accidentally drops the lineage,
    /// the artifact would lose its provenance anchor.
    #[test]
    fn stub_artifact_carries_recipe_lineage() {
        let recipe = synthetic_recipe();
        let recipe_id = recipe.id;
        let artifact = synthesize_stub_artifact(&recipe, None).expect("synth");
        assert_eq!(artifact.recipe_id, recipe_id);
        assert_eq!(artifact.recipe_version, "0.1.0");
        assert_eq!(artifact.recipe_name, "test-recipe");
    }

    /// What this catches: stub artifact has its OWN id, not the recipe's.
    /// Multiple artifacts can come from one recipe (re-runs on different
    /// hardware) and each must be distinguishable.
    #[test]
    fn stub_artifact_has_distinct_id_from_recipe() {
        let recipe = synthetic_recipe();
        let artifact = synthesize_stub_artifact(&recipe, None).expect("synth");
        assert_ne!(
            artifact.id, recipe.id,
            "artifact id MUST differ from recipe id (1:N relationship)"
        );
    }

    /// What this catches: alloy_hash uses the canonical "sha256:..."
    /// prefix matching admission's content_hash convention. Stub
    /// includes "stub-" suffix so it's distinguishable from real hashes
    /// in the wild.
    #[test]
    fn stub_alloy_hash_is_canonical_with_stub_marker() {
        let recipe = synthetic_recipe();
        let artifact = synthesize_stub_artifact(&recipe, None).expect("synth");
        let hash = artifact.alloy_hash.expect("stub hash present");
        assert!(hash.starts_with("sha256:stub-"), "got: {hash}");
    }

    /// What this catches: hardware_node parameter (when set) lands in
    /// hardware_verified as a stub HardwareProfile. Phase 5+ will
    /// actually dispatch + populate real measurements; for now the
    /// caller sees their requested node echoed back.
    #[test]
    fn stub_artifact_records_requested_hardware_node() {
        let recipe = synthetic_recipe();
        let artifact = synthesize_stub_artifact(&recipe, Some("m5-pro@local")).expect("synth");
        assert_eq!(artifact.hardware_verified.len(), 1);
        assert_eq!(artifact.hardware_verified[0].device, "m5-pro@local");
        assert_eq!(artifact.hardware_verified[0].format, "stub");
        assert!(
            !artifact.hardware_verified[0].verified,
            "stub is not verified"
        );
    }

    /// What this catches: with no hardware_node, hardware_verified
    /// stays empty (vs an entry with empty device label). Caller can
    /// distinguish "no hw requested" from "hw requested but no metrics".
    #[test]
    fn stub_artifact_without_hardware_node_is_empty_verified() {
        let recipe = synthetic_recipe();
        let artifact = synthesize_stub_artifact(&recipe, None).expect("synth");
        assert!(artifact.hardware_verified.is_empty());
    }

    /// What this catches: Phase 4 fields that Phase 5+ will populate
    /// (results, receipt, integrity, duration, params_b) all start as
    /// None on the stub. A Phase 5 refactor that accidentally fills
    /// them with placeholder data would silently claim measurements
    /// that didn't happen.
    #[test]
    fn stub_artifact_phase5_fields_are_none() {
        let recipe = synthetic_recipe();
        let artifact = synthesize_stub_artifact(&recipe, Some("m5-pro@local")).expect("synth");
        assert!(artifact.results.is_none());
        assert!(artifact.receipt.is_none());
        assert!(artifact.integrity.is_none());
        assert!(artifact.duration_minutes.is_none());
        assert!(artifact.forged_params_b.is_none());
        assert!(artifact.active_params_b.is_none());
    }

    // ── native MLX engine selection + spec resolution (#52) ──

    /// What this catches: engine selection is EXPLICIT-wins, never a silent
    /// fallback — "mlx"/"custodian" map deterministically, an unknown value
    /// fails loud (not silently treated as custodian), and `None` defers to
    /// the platform branch. A regression that swallowed a typo'd engine into
    /// the custodian path would be the exact "fallback hides the failure" bug.
    #[test]
    fn mlx_engine_selection_is_explicit_wins_fail_loud() {
        assert_eq!(mlx_engine_selected(Some("mlx")).unwrap(), true);
        assert_eq!(mlx_engine_selected(Some("custodian")).unwrap(), false);
        assert!(
            mlx_engine_selected(Some("tensorflow")).is_err(),
            "unknown engine must fail loud"
        );
        // None → platform auto-detect; on this Apple-Silicon host that's mlx,
        // and the fn must agree with the same cfg! the dispatch uses.
        let want = cfg!(all(target_os = "macos", target_arch = "aarch64"));
        assert_eq!(mlx_engine_selected(None).unwrap(), want);
    }

    /// What this catches: the managed-dir invariant Joel set — the native
    /// trainer interpreter resolves under the continuum home's `genome/venv`
    /// (one owner: `paths::continuum_home`), NEVER the legacy `~/.unsloth/...`
    /// venv. Read-only (only asserts when MLX_PYTHON is unset, so it never
    /// races env-mutating tests).
    #[test]
    fn mlx_python_defaults_to_managed_venv_not_unsloth() -> Result<(), String> {
        if std::env::var("MLX_PYTHON").is_ok() {
            return Ok(()); // operator override in effect; nothing to assert
        }
        let p = resolve_mlx_python();
        let s = p.display().to_string();
        assert_eq!(
            p,
            crate::paths::continuum_home()?.join("genome/venv/bin/python3"),
            "managed venv path, got {s}"
        );
        assert!(
            !s.contains(".unsloth"),
            "must not reference the legacy unsloth venv, got {s}"
        );
        Ok(())
    }

    /// What this catches: the native dry_run RESOLVES the full MlxTrainSpec
    /// without spawning — proving (a) scale == lora_alpha/lora_r (the one
    /// geometry contract lora_convert reads back), (b) ~/ paths expand, (c)
    /// the defaults (num_layers=-1, engine=mlx) ride through. A drift in the
    /// scale math would silently reintroduce the scale=20 destabilization.
    #[test]
    fn native_dry_run_resolves_spec_with_scale_from_alpha_over_rank() {
        let home = std::env::var("HOME").unwrap_or_default();
        let p: ForgeTrainParams = serde_json::from_value(json!({
            "dataset_path": "/unused.jsonl",
            "base_model": "qwen3.5-4b-code-forged",
            "engine": "mlx",
            "train_base_dir": "~/base-hf",
            "data_dir": "~/data-presplit",
            "adapter_out": "~/out-adapter",
            "lora_r": 16,
            "lora_alpha": 32,
            "dry_run": true,
        }))
        .expect("params");
        let v = match run_train_native_mlx(p, None).unwrap() {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["dry_run"], true);
        assert_eq!(v["engine"], "mlx");
        assert_eq!(v["scale"], 2.0, "scale must be alpha/rank = 32/16");
        assert_eq!(v["rank"], 16);
        assert_eq!(v["num_layers"], -1, "default = all transformer blocks");
        assert_eq!(v["base_model_dir"], format!("{home}/base-hf"));
        assert_eq!(v["data_dir"], format!("{home}/data-presplit"));
        assert_eq!(v["adapter_out"], format!("{home}/out-adapter"));
        // The convert-safe MLP target set rides through by default — never the
        // unconvertible attention projection. This is what makes every gene the
        // native path forges servable by construction.
        let keys = v["target_keys"].as_array().expect("target_keys array");
        let keys: Vec<&str> = keys.iter().map(|k| k.as_str().unwrap()).collect();
        assert_eq!(keys, vec!["mlp.gate_proj", "mlp.up_proj", "mlp.down_proj"]);
    }

    /// What this catches: the mlx engine REQUIRES train_base_dir and fails
    /// loud naming the train-base==serve-base reason — never guesses an
    /// on-disk dir (a guess would produce a washed-out ~0-lift gene).
    #[test]
    fn native_train_without_base_dir_fails_loud() {
        let p: ForgeTrainParams = serde_json::from_value(json!({
            "dataset_path": "/unused.jsonl",
            "base_model": "qwen3.5-4b-code-forged",
            "engine": "mlx",
            "dry_run": true,
        }))
        .expect("params");
        let err = run_train_native_mlx(p, None).unwrap_err();
        assert!(err.contains("train_base_dir is required"), "got: {err}");
        assert!(
            err.contains("serve-base"),
            "must name the train==serve reason, got: {err}"
        );
    }

    use std::sync::Mutex;

    /// Records the Contract C ([`crate::forge::protocol`]) gguf-lora requests the
    /// organism sends to the forge CUSTODIAN — so we assert the stateless wire
    /// shape (checkpoint-in-body, base, outtype) without a network.
    #[derive(Default)]
    struct RecordingForgeCustodian {
        exports: Mutex<Vec<crate::forge::protocol::GgufLoraRequest>>,
        succeed: bool,
    }
    impl RecordingForgeCustodian {
        fn ok() -> Self {
            Self {
                succeed: true,
                ..Default::default()
            }
        }
    }
    #[async_trait]
    impl crate::forge::custodian_client::ForgeCustodian for RecordingForgeCustodian {
        async fn health(
            &self,
        ) -> Result<
            crate::forge::protocol::HealthResponse,
            crate::forge::custodian_client::ForgeCustodianError,
        > {
            Ok(crate::forge::protocol::HealthResponse::ok_gguf_lora())
        }
        async fn export_gguf_lora(
            &self,
            req: &crate::forge::protocol::GgufLoraRequest,
        ) -> Result<
            crate::forge::protocol::ExportResult,
            crate::forge::custodian_client::ForgeCustodianError,
        > {
            self.exports.lock().unwrap().push(req.clone());
            // Mirror the real custodian's contract: a SUCCESS always reports the
            // produced gene path in `details.output` (forge_custodian.rs). The fake
            // derives the same `{save_dir}/{checkpoint_stem}-<job>.gguf` shape so the
            // producer's manifest registration has a real path to record.
            let ckpt_stem = std::path::Path::new(&req.checkpoint)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "gene".into());
            let output = format!("{}/{ckpt_stem}-testjob.gguf", req.save_directory);
            Ok(crate::forge::protocol::ExportResult {
                success: self.succeed,
                message: "packaged".into(),
                details: json!({ "output": output }),
            })
        }
    }

    // what this catches: the gguf-lora path sends the STATELESS Contract C request
    // (checkpoint named in body, base + outtype threaded) to the forge custodian —
    // this is the SUPPLY contract for the genome page-in (the gene cognition/eval
    // pages in). The pre-Pass-2 bug POSTed a checkpoint-less body to the WRONG
    // (unsloth) endpoint; a regression here = the gene is never produced loadably
    // and every LIFT is unmeasurable.
    #[tokio::test]
    async fn forge_export_gguf_lora_sends_stateless_contract_c_request() {
        let cust = RecordingForgeCustodian::ok();
        let p = ForgeExportParams {
            checkpoint_format: Default::default(),
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "gguf-lora".into(),
            quantization: "Q4_K_M".into(),
            // CANONICAL id (what the manifest/eval key on); the custodian must
            // receive the RESOLVED hf_base instead — the trinity split.
            base_model_id: Some("continuum-ai/qwen2.5-0.5b-instruct-GGUF".into()),
            outtype: "f16".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        let path = tmp_manifest("stateless");
        let _ = std::fs::remove_file(&path);
        run_export_gguf_lora(&cust, &p, "unsloth/Qwen2.5-0.5B-Instruct", &path)
            .await
            .unwrap();
        let reqs = cust.exports.lock().unwrap();
        assert_eq!(reqs.len(), 1, "exactly one export call");
        assert_eq!(
            reqs[0].checkpoint, "/ckpt",
            "checkpoint named in the body (stateless)"
        );
        assert_eq!(reqs[0].save_directory, "/out");
        // The custodian gets the SAFETENSORS hf_base, not the canonical GGUF id.
        assert_eq!(reqs[0].base_model_id, "unsloth/Qwen2.5-0.5B-Instruct");
        assert_eq!(reqs[0].outtype, "f16");
    }

    // what this catches: a gguf-lora export with NO base is rejected LOUDLY at the
    // organism boundary BEFORE the custodian is touched (the converter cannot make
    // a loadable adapter without the base architecture) — never silently defaulted.
    #[tokio::test]
    async fn forge_export_gguf_lora_without_base_fails_loud() {
        let cust = RecordingForgeCustodian::ok();
        let p = ForgeExportParams {
            checkpoint_format: Default::default(),
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "gguf-lora".into(),
            quantization: "Q4_K_M".into(),
            base_model_id: None,
            outtype: "f16".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        // hf_base is irrelevant here — the None base_model_id fails loud first.
        let err = run_export_gguf_lora(
            &cust,
            &p,
            "unsloth/Qwen2.5-0.5B-Instruct",
            &tmp_manifest("nobase"),
        )
        .await
        .expect_err("missing base must error");
        assert!(err.contains("base_model_id"), "got: {err}");
        assert_eq!(
            cust.exports.lock().unwrap().len(),
            0,
            "custodian never called"
        );
    }

    // what this catches: a custodian whose gguf-lora export fails is surfaced
    // LOUDLY (no silent no-op) — the same fail-loud contract as the unsloth path.
    #[tokio::test]
    async fn forge_export_gguf_lora_fails_loud_when_custodian_fails() {
        let cust = RecordingForgeCustodian::default(); // succeed=false
        let p = ForgeExportParams {
            checkpoint_format: Default::default(),
            checkpoint: "/ckpt".into(),
            save_directory: "/out".into(),
            format: "gguf-lora".into(),
            quantization: "Q4_K_M".into(),
            base_model_id: Some("b".into()),
            outtype: "f16".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        let err = run_export_gguf_lora(
            &cust,
            &p,
            "unsloth/Qwen2.5-0.5B-Instruct",
            &tmp_manifest("custfail"),
        )
        .await
        .expect_err("must error");
        assert!(err.contains("gguf-lora) failed"), "got: {err}");
    }

    // what this catches: the 5th wire of the genome loop — a SUCCESSFUL gguf-lora
    // export REGISTERS the produced gene in the manifest (path + continuum
    // base_model_id) so the serving daemon's reconcile loads it via `--lora`.
    // Without this the catalog stays empty forever and every page-in fails loud
    // (LIFT structurally unreachable). Asserts the gene is filterable by its
    // CONTINUUM id (what `for_base` keys on, not the HF base name).
    #[tokio::test]
    async fn forge_export_gguf_lora_registers_gene_in_manifest() {
        let cust = RecordingForgeCustodian::ok();
        let p = ForgeExportParams {
            checkpoint_format: Default::default(),
            checkpoint: "/ckpts/asha-code".into(),
            save_directory: "/genes".into(),
            format: "gguf-lora".into(),
            quantization: "Q4_K_M".into(),
            base_model_id: Some("continuum-ai/qwen3.5-4b-code-forged-GGUF".into()),
            outtype: "f16".into(),
            max_seq_length: 2048,
            load_in_4bit: true,
        };
        let path = tmp_manifest("registers");
        let _ = std::fs::remove_file(&path);

        // hf_base (safetensors) goes to the custodian; the manifest is keyed on the
        // CANONICAL p.base_model_id below — the split this test guards.
        run_export_gguf_lora(&cust, &p, "unsloth/Qwen2.5-0.5B-Instruct", &path)
            .await
            .unwrap();

        let all = crate::forge::adapter_manifest::load_from(&path).unwrap();
        let matched = crate::forge::adapter_manifest::for_base(
            &all,
            "continuum-ai/qwen3.5-4b-code-forged-GGUF",
        );
        assert_eq!(
            matched.len(),
            1,
            "the produced gene is registered under its continuum id"
        );
        // The fake mirrors the custodian's `{save_dir}/{ckpt_stem}-<job>.gguf` path.
        assert_eq!(
            matched[0].path,
            std::path::PathBuf::from("/genes/asha-code-testjob.gguf")
        );
        assert_eq!(
            matched[0].alias, "asha-code-testjob",
            "alias = gene file stem"
        );

        let _ = std::fs::remove_file(&path);
    }

    // what this catches: a custodian SUCCESS with no `details.output` (the gene
    // path) FAILS LOUD — never returns success for a gene we can't locate, which
    // would silently register nothing and report a green export. The custodian
    // contract ALWAYS reports `output` on success, so its absence is a breach.
    #[test]
    fn trained_adapter_from_export_fails_loud_without_output() {
        let err = trained_adapter_from_export(&json!({ "job_id": "x" }), "continuum-ai/base")
            .expect_err("missing output must error");
        assert!(err.contains("details.output"), "got: {err}");
    }

    // ── forge/health — Contract C handshake as a command (Pass 6 receiving end) ──

    /// A Contract C custodian that is down — `health()` errors. Stands in for an
    /// unreachable local custodian so `run_health` is pinned to fail loud rather
    /// than fabricate a healthy reading.
    struct DownForgeCustodian;
    #[async_trait]
    impl crate::forge::custodian_client::ForgeCustodian for DownForgeCustodian {
        async fn health(
            &self,
        ) -> Result<
            crate::forge::protocol::HealthResponse,
            crate::forge::custodian_client::ForgeCustodianError,
        > {
            Err(
                crate::forge::custodian_client::ForgeCustodianError::Unreachable(
                    "connection refused".into(),
                ),
            )
        }
        async fn export_gguf_lora(
            &self,
            _: &crate::forge::protocol::GgufLoraRequest,
        ) -> Result<
            crate::forge::protocol::ExportResult,
            crate::forge::custodian_client::ForgeCustodianError,
        > {
            Err(crate::forge::custodian_client::ForgeCustodianError::Unreachable("down".into()))
        }
    }

    // what this catches: forge/health surfaces the LOCAL custodian's Contract C
    // HealthResponse as JSON — the reading a remote node reads over the grid to
    // confirm contract version + readiness before leasing a forge here (Pass 6
    // receiving end). A serialization regression would blind remote contract checks.
    #[tokio::test]
    async fn forge_health_surfaces_contract_c_health() {
        let cust = RecordingForgeCustodian::ok();
        let v = match run_health(&cust).await.unwrap() {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        assert_eq!(
            v["contract_version"],
            crate::forge::protocol::CONTRACT_VERSION
        );
        assert_eq!(
            v["capability"],
            crate::forge::protocol::CAPABILITY_GGUF_LORA
        );
        assert_eq!(v["ready"], true);
    }

    // what this catches: an unreachable custodian makes forge/health FAIL LOUD with
    // the named cause — never a fabricated "healthy" envelope. A remote node must
    // see the truth (route elsewhere), not be told a down custodian is up.
    #[tokio::test]
    async fn forge_health_fails_loud_when_custodian_down() {
        let err = run_health(&DownForgeCustodian)
            .await
            .expect_err("down custodian must error");
        assert!(err.contains("connection refused"), "got: {err}");
    }

    // ── forge/probe — DISCOVER this node's native forge capability ──

    // what this catches: native probe DISCOVERS capability from the on-disk genome
    // dir + the live job watch — reachable is true on any host with the mlx path
    // (observed, never declared), and outputs_dir points at the byte-custody root.
    // No custodian, no network — the self-organizing primitive the grid routes
    // training demand against.
    #[test]
    fn native_probe_reports_reachable_from_on_disk_capability() {
        let cap = native_forge_capability();
        assert!(
            cap.reachable,
            "the native mlx forge path is always reachable on-host"
        );
        assert!(
            cap.outputs_dir.ends_with(".continuum/forge/lora"),
            "genome dir: {}",
            cap.outputs_dir
        );
    }

    // ── forge/decide — "assemble best self, or train" (product-thesis decision) ──

    // what this catches: when a market candidate clears the adopt margin, ASSEMBLE
    // it (zero training) and report the lift. This is the "shop before you build"
    // fast path — the whole point of not starting from zero.
    #[tokio::test]
    async fn decide_assembles_when_market_candidate_clears_margin() {
        let module = ForgeModule::new();
        let params = json!({
            "baseline": 0.60,
            "adopt_margin": 0.02,
            "candidates": [
                {"label": "peer-A/rust-lora", "score": 0.72},
                {"label": "peer-B/rust-lora", "score": 0.81},
            ]
        });
        let v = match module.handle_command("forge/decide", params).await.unwrap() {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["action"], "assemble");
        assert_eq!(v["chosen"], "peer-B/rust-lora"); // best score wins
        assert!((v["lift"].as_f64().unwrap() - 0.21).abs() < 1e-9);
    }

    // what this catches: when no candidate clears the margin, the market came up
    // short → TRAIN (forage + forge), reporting the gap to close. This is the slow
    // path the fitness GAP triggers.
    #[tokio::test]
    async fn decide_trains_when_market_falls_short() {
        let module = ForgeModule::new();
        let params = json!({
            "baseline": 0.80,
            "adopt_margin": 0.05,
            "candidates": [ {"label": "peer/old-lora", "score": 0.81} ] // +0.01 < 0.05
        });
        let v = match module.handle_command("forge/decide", params).await.unwrap() {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["action"], "train");
        assert_eq!(v["best_candidate"], "peer/old-lora");
        assert!((v["gap"].as_f64().unwrap() - 0.04).abs() < 1e-9);
    }

    // what this catches: an empty market (nothing to assemble) → train from base.
    #[tokio::test]
    async fn decide_trains_when_market_empty() {
        let module = ForgeModule::new();
        let v = match module
            .handle_command("forge/decide", json!({ "baseline": 0.5, "candidates": [] }))
            .await
            .unwrap()
        {
            CommandResult::Json(v) => v,
            _ => panic!("json"),
        };
        assert_eq!(v["action"], "train");
    }
