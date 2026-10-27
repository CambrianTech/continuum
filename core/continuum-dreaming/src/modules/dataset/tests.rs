#![cfg(test)]
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

    // what this catches: L1 fork contamination (#427, 2026-08-14 citizenship
    // audit AXIS 3). Benchmark/eval forks keep the persona's id, so their
    // nil-room ticks land in the SAME capture file as her live turns — and a
    // default dataset run was ingesting them as if they were her lived room
    // experience. Default must SKIP nil-room captures; `include_forks: true`
    // is the explicit opt-in. Regression here = benchmark work silently
    // re-enters the training corpus unlabeled.
    #[test]
    fn from_captures_excludes_nil_room_forks_by_default() {
        let tmp = TempDir::new().unwrap();
        let captures_dir = tmp.path().join("captures");
        std::fs::create_dir_all(&captures_dir).unwrap();

        let live = json!({
            "persona_id": "11111111-1111-1111-1111-111111111111",
            "room_id": "22222222-2222-2222-2222-222222222222",
            "system": "You are Asha.",
            "messages": [{ "role": "user", "content": "peer: how do rooms work?" }],
            "response": { "text": "A room is an activity with a lifetime." }
        });
        let fork = json!({
            "persona_id": "11111111-1111-1111-1111-111111111111",
            "room_id": uuid::Uuid::nil().to_string(),
            "system": "You are Asha.",
            "messages": [{ "role": "user", "content": "[measurement] solve the task" }],
            "response": { "text": "Working the benchmark instance now." }
        });
        create_test_csv(&captures_dir, "asha.jsonl", &format!("{live}\n{fork}\n"));

        let params = |name: &str, include_forks: bool| FromCapturesParams {
            captures_dir: Some(captures_dir.to_str().unwrap().to_string()),
            output_dir: Some(tmp.path().join("out").to_str().unwrap().to_string()),
            name: name.to_string(),
            split_ratio: 1.0,
            include_system: true,
            persona_id: None,
            room_id: None,
            skill_axis: None,
            include_forks,
        };

        // Default: the fork tick is excluded — only the lived room turn trains.
        let v = service()
            .from_captures(&params("live-only", false))
            .unwrap();
        assert_eq!(v.total_examples, 1, "nil-room fork must be excluded");

        // Explicit opt-in: both convert (a fork corpus is a legitimate goal,
        // it just must never be the silent default).
        let v = service()
            .from_captures(&params("with-forks", true))
            .unwrap();
        assert_eq!(v.total_examples, 2, "opt-in must include the fork tick");
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

    // what this catches (card 3c8b03ae, Codex's review of #5372): a capture writer and its
    // reader resolving different homes. With $CONTINUUM_HOME pointing away from the native
    // HOME and no per-thread test override, the turn the real recorder writes is the turn
    // dataset/from-turns reads by default, and nothing lands under HOME/.continuum.
    #[test]
    fn a_turn_recorded_under_a_relocated_continuum_home_is_the_turn_from_turns_reads() {
        use crate::cognition::PersonaSlot;
        use crate::persona::response::{PersonaResponse, RespondInput};
        let home = TempDir::new().unwrap();
        let continuum = TempDir::new().unwrap();
        let _home = crate::paths::test_home::HomeGuard::set_blocking(home.path());
        std::env::set_var("CONTINUUM_HOME", continuum.path()); // restored by the guard
        let input = RespondInput {
            persona: PersonaSlot {
                persona_id: uuid::Uuid::from_u128(7),
                specialty: "general".to_string(),
                display_name: "Relocated".to_string(),
            },
            turn_context: crate::persona::turn_context::TurnContext::arc(uuid::Uuid::from_u128(8), vec![], vec!["general".to_string()]),
            message_id: uuid::Uuid::from_u128(9),
            message_text: "where do my turns go?".to_string(),
            other_persona_names: vec![],
            system_prompt: "you are helpful".to_string(),
            model: "test-model".to_string(),
            is_voice: false,
            message_media: vec![],
            capabilities: std::collections::HashSet::new(),
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        };
        let response = PersonaResponse::Spoke {
            persona_id: uuid::Uuid::from_u128(7),
            text: "under the continuum home".to_string(),
            model_used: "test-model".to_string(),
            inference_ms: 1,
            total_ms: 2,
            think_blocks_emitted: 0,
        };
        crate::persona::recorder::record_turn(&input, &response, &crate::cognition::trace::CognitionTrace::new());

        assert!(continuum.path().join(crate::persona::recorder::RESPOND_FIXTURE_DIR).is_dir(), "written under CONTINUUM_HOME");
        assert!(!home.path().join(".continuum").exists(), "nothing under the native HOME");
        let output_dir = continuum.path().join("out");
        let v = service()
            .from_turns(&FromTurnsParams {
                turns_dir: None, // the default: the recorder's own place
                output_dir: Some(output_dir.to_str().unwrap().to_string()),
                name: "relocated-turns".to_string(),
                split_ratio: 1.0,
                include_system: true,
                include_history: false,
                persona_id: None,
                room_id: None,
            })
            .unwrap();
        assert_eq!(v.total_examples, 1, "the reader found the turn the writer wrote");
    }
