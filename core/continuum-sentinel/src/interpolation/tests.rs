#![cfg(test)]
    use super::super::types::StepResult;
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_ctx() -> ExecutionContext {
        ExecutionContext {
            step_results: vec![
                StepResult {
                    step_index: 0,
                    step_type: "shell".to_string(),
                    success: true,
                    duration_ms: 42,
                    output: Some("hello world".to_string()),
                    error: None,
                    exit_code: Some(0),
                    data: json!({ "stdout": "hello world", "exitCode": 0 }),
                },
                StepResult {
                    step_index: 1,
                    step_type: "shell".to_string(),
                    success: false,
                    duration_ms: 10,
                    output: None,
                    error: Some("not found".to_string()),
                    exit_code: Some(127),
                    data: json!({ "stderr": "command not found", "exitCode": 127 }),
                },
            ],
            inputs: {
                let mut m = HashMap::new();
                m.insert("name".to_string(), json!("test-pipeline"));
                m.insert("count".to_string(), json!(5));
                m
            },
            working_dir: PathBuf::from("/tmp/test"),
            named_outputs: {
                let mut m = HashMap::new();
                m.insert(
                    "build".to_string(),
                    StepResult {
                        step_index: 0,
                        step_type: "shell".to_string(),
                        success: true,
                        duration_ms: 100,
                        output: Some("build OK".to_string()),
                        error: None,
                        exit_code: Some(0),
                        data: json!({ "artifacts": ["a.o", "b.o"] }),
                    },
                );
                m
            },
        }
    }

    // ── Steps path resolution ──

    #[test]
    fn test_steps_output() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.0.output}}", &ctx), "hello world");
    }

    #[test]
    fn test_steps_default_to_output() {
        let ctx = make_ctx();
        // Bare steps.N without field returns output
        assert_eq!(interpolate("{{steps.0}}", &ctx), "hello world");
    }

    #[test]
    fn test_steps_success() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.0.success}}", &ctx), "true");
        assert_eq!(interpolate("{{steps.1.success}}", &ctx), "false");
    }

    #[test]
    fn test_steps_error() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.1.error}}", &ctx), "not found");
        assert_eq!(interpolate("{{steps.0.error}}", &ctx), "");
    }

    #[test]
    fn test_steps_exit_code() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.0.exitCode}}", &ctx), "0");
        assert_eq!(interpolate("{{steps.1.exit_code}}", &ctx), "127");
    }

    #[test]
    fn test_steps_data_field() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.0.data.stdout}}", &ctx), "hello world");
        assert_eq!(interpolate("{{steps.0.data.exitCode}}", &ctx), "0");
    }

    #[test]
    fn test_steps_nested_data() {
        let ctx = make_ctx();
        assert_eq!(
            interpolate("{{steps.1.data.stderr}}", &ctx),
            "command not found"
        );
    }

    #[test]
    fn test_steps_metadata() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.0.type}}", &ctx), "shell");
        assert_eq!(interpolate("{{steps.0.stepType}}", &ctx), "shell");
        assert_eq!(interpolate("{{steps.0.index}}", &ctx), "0");
        assert_eq!(interpolate("{{steps.0.durationMs}}", &ctx), "42");
    }

    #[test]
    fn test_steps_out_of_bounds() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.99.output}}", &ctx), "");
    }

    // ── Input path resolution ──

    #[test]
    fn test_input_string() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{input.name}}", &ctx), "test-pipeline");
        assert_eq!(interpolate("{{inputs.name}}", &ctx), "test-pipeline");
    }

    #[test]
    fn test_input_number() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{input.count}}", &ctx), "5");
    }

    #[test]
    fn test_input_missing() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{input.nonexistent}}", &ctx), "");
    }

    // ── Named output resolution ──

    #[test]
    fn test_named_output() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{named.build.output}}", &ctx), "build OK");
    }

    #[test]
    fn test_named_success() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{named.build.success}}", &ctx), "true");
    }

    #[test]
    fn test_named_data_field() {
        let ctx = make_ctx();
        assert_eq!(
            interpolate("{{named.build.data.artifacts}}", &ctx),
            "[\"a.o\",\"b.o\"]"
        );
    }

    #[test]
    fn test_named_missing() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{named.deploy.output}}", &ctx), "");
    }

    // ── Env resolution ──

    #[test]
    fn test_env_var() {
        let ctx = make_ctx();
        std::env::set_var("SENTINEL_TEST_VAR", "sentinel_value");
        assert_eq!(
            interpolate("{{env.SENTINEL_TEST_VAR}}", &ctx),
            "sentinel_value"
        );
        std::env::remove_var("SENTINEL_TEST_VAR");
    }

    #[test]
    fn test_env_missing() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{env.NONEXISTENT_VAR_12345}}", &ctx), "");
    }

    // ── Mixed interpolation ──

    #[test]
    fn test_multiple_refs_in_string() {
        let ctx = make_ctx();
        let result = interpolate("Name: {{input.name}}, Output: {{steps.0.output}}", &ctx);
        assert_eq!(result, "Name: test-pipeline, Output: hello world");
    }

    #[test]
    fn test_no_refs_passthrough() {
        let ctx = make_ctx();
        assert_eq!(interpolate("plain text", &ctx), "plain text");
    }

    #[test]
    fn test_unknown_root_preserved() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{unknown.path}}", &ctx), "{{unknown.path}}");
    }

    #[test]
    fn test_whitespace_trimmed() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{ steps.0.output }}", &ctx), "hello world");
    }

    // ── interpolate_value (JSON) ──

    #[test]
    fn test_interpolate_json_string() {
        let ctx = make_ctx();
        let val = json!("result: {{steps.0.output}}");
        let result = interpolate_value(&val, &ctx);
        assert_eq!(result, json!("result: hello world"));
    }

    #[test]
    fn test_interpolate_json_object() {
        let ctx = make_ctx();
        let val = json!({ "name": "{{input.name}}", "out": "{{steps.0.output}}" });
        let result = interpolate_value(&val, &ctx);
        assert_eq!(
            result,
            json!({ "name": "test-pipeline", "out": "hello world" })
        );
    }

    #[test]
    fn test_interpolate_json_array() {
        let ctx = make_ctx();
        let val = json!(["{{input.name}}", "{{steps.0.output}}"]);
        let result = interpolate_value(&val, &ctx);
        assert_eq!(result, json!(["test-pipeline", "hello world"]));
    }

    #[test]
    fn test_interpolate_preserves_numbers() {
        let ctx = make_ctx();
        let val = json!(42);
        assert_eq!(interpolate_value(&val, &ctx), json!(42));
    }

    #[test]
    fn test_interpolate_preserves_booleans() {
        let ctx = make_ctx();
        let val = json!(true);
        assert_eq!(interpolate_value(&val, &ctx), json!(true));
    }

    // ── evaluate_condition ──

    #[test]
    fn test_condition_true_false() {
        assert!(evaluate_condition("true"));
        assert!(!evaluate_condition("false"));
    }

    #[test]
    fn test_condition_truthy_strings() {
        assert!(evaluate_condition("hello"));
        assert!(evaluate_condition("1"));
        assert!(evaluate_condition("yes"));
    }

    #[test]
    fn test_condition_falsy_values() {
        assert!(!evaluate_condition(""));
        assert!(!evaluate_condition("0"));
        assert!(!evaluate_condition("null"));
        assert!(!evaluate_condition("undefined"));
        assert!(!evaluate_condition("false"));
    }

    #[test]
    fn test_condition_whitespace() {
        assert!(evaluate_condition("  true  "));
        assert!(!evaluate_condition("  false  "));
        assert!(!evaluate_condition("   "));
    }

    // ── Array indexing ──

    #[test]
    fn test_data_array_indexing() {
        let ctx = ExecutionContext {
            step_results: vec![StepResult {
                step_index: 0,
                step_type: "command".to_string(),
                success: true,
                duration_ms: 10,
                output: None,
                error: None,
                exit_code: None,
                data: json!({
                    "items": ["alpha", "beta", "gamma"],
                    "topics": [
                        { "name": "Generics", "difficulty": "beginner" },
                        { "name": "Constraints", "difficulty": "intermediate" },
                    ]
                }),
            }],
            inputs: HashMap::new(),
            working_dir: PathBuf::from("/tmp"),
            named_outputs: HashMap::new(),
        };

        // Simple array indexing
        assert_eq!(interpolate("{{steps.0.data.items.0}}", &ctx), "alpha");
        assert_eq!(interpolate("{{steps.0.data.items.2}}", &ctx), "gamma");

        // Nested object inside array
        assert_eq!(
            interpolate("{{steps.0.data.topics.0.name}}", &ctx),
            "Generics"
        );
        assert_eq!(
            interpolate("{{steps.0.data.topics.1.difficulty}}", &ctx),
            "intermediate"
        );

        // Out of bounds returns empty
        assert_eq!(interpolate("{{steps.0.data.items.99}}", &ctx), "");
    }

    // ── JSON string auto-parsing in output ──

    #[test]
    fn test_output_json_string_traversal() {
        let curriculum_json = json!({
            "skill": "typescript",
            "topics": [
                { "name": "Basics", "difficulty": "beginner" },
                { "name": "Advanced Types", "difficulty": "advanced" },
            ]
        })
        .to_string();

        let ctx = ExecutionContext {
            step_results: vec![StepResult {
                step_index: 0,
                step_type: "llm".to_string(),
                success: true,
                duration_ms: 5000,
                output: Some(curriculum_json),
                error: None,
                exit_code: None,
                data: json!({}),
            }],
            inputs: HashMap::new(),
            working_dir: PathBuf::from("/tmp"),
            named_outputs: HashMap::new(),
        };

        // Traverse into LLM output (JSON string → parsed → path access)
        assert_eq!(interpolate("{{steps.0.output.skill}}", &ctx), "typescript");
        assert_eq!(
            interpolate("{{steps.0.output.topics.0.name}}", &ctx),
            "Basics"
        );
        assert_eq!(
            interpolate("{{steps.0.output.topics.1.difficulty}}", &ctx),
            "advanced"
        );

        // Bare output still returns raw string
        assert!(interpolate("{{steps.0.output}}", &ctx).contains("typescript"));
    }

    // ── Nested interpolation ──

    #[test]
    fn test_nested_interpolation_with_loop_index() {
        let curriculum_json = json!({
            "topics": [
                { "name": "Basics", "difficulty": "beginner" },
                { "name": "Advanced", "difficulty": "advanced" },
            ]
        })
        .to_string();

        let ctx = ExecutionContext {
            step_results: vec![StepResult {
                step_index: 0,
                step_type: "llm".to_string(),
                success: true,
                duration_ms: 100,
                output: Some(curriculum_json),
                error: None,
                exit_code: None,
                data: json!({}),
            }],
            inputs: {
                let mut m = HashMap::new();
                m.insert("iteration".to_string(), json!(1));
                m
            },
            working_dir: PathBuf::from("/tmp"),
            named_outputs: HashMap::new(),
        };

        // Nested: inner {{input.iteration}} resolves to 1, then outer resolves topic
        assert_eq!(
            interpolate("{{steps.0.output.topics.{{input.iteration}}.name}}", &ctx),
            "Advanced"
        );
        assert_eq!(
            interpolate(
                "{{steps.0.output.topics.{{input.iteration}}.difficulty}}",
                &ctx
            ),
            "advanced"
        );

        // With iteration=0
        let mut ctx0 = ctx;
        ctx0.inputs.insert("iteration".to_string(), json!(0));
        assert_eq!(
            interpolate("{{steps.0.output.topics.{{input.iteration}}.name}}", &ctx0),
            "Basics"
        );
    }

    // ── Loop-relative step referencing ──

    #[test]
    fn test_loop_relative_referencing() {
        let ctx = ExecutionContext {
            step_results: vec![
                // Step 0 (before loop): LLM output
                StepResult {
                    step_index: 0,
                    step_type: "llm".to_string(),
                    success: true,
                    duration_ms: 100,
                    output: Some("curriculum".to_string()),
                    error: None,
                    exit_code: None,
                    data: json!({}),
                },
                // Step 1 (before loop): data/create
                StepResult {
                    step_index: 1,
                    step_type: "command".to_string(),
                    success: true,
                    duration_ms: 5,
                    output: None,
                    error: None,
                    exit_code: None,
                    data: json!({ "id": "curr-123" }),
                },
                // Step 2 (loop iteration 0, sub-step 0): dataset-synthesize
                StepResult {
                    step_index: 2,
                    step_type: "command".to_string(),
                    success: true,
                    duration_ms: 3000,
                    output: None,
                    error: None,
                    exit_code: None,
                    data: json!({ "datasetPath": "/path/to/dataset.jsonl", "exampleCount": 20 }),
                },
                // Step 3 (loop iteration 0, sub-step 1): emit
                StepResult {
                    step_index: 3,
                    step_type: "emit".to_string(),
                    success: true,
                    duration_ms: 0,
                    output: None,
                    error: None,
                    exit_code: None,
                    data: json!({}),
                },
            ],
            inputs: {
                let mut m = HashMap::new();
                m.insert("iteration".to_string(), json!(0));
                m.insert("_loop_base".to_string(), json!(2)); // Loop starts at index 2
                m
            },
            working_dir: PathBuf::from("/tmp"),
            named_outputs: HashMap::new(),
        };

        // loop.0 = step_results[2] (the dataset-synthesize result)
        assert_eq!(
            interpolate("{{loop.0.data.datasetPath}}", &ctx),
            "/path/to/dataset.jsonl"
        );
        assert_eq!(interpolate("{{loop.0.data.exampleCount}}", &ctx), "20");

        // loop.1 = step_results[3] (the emit result)
        assert_eq!(interpolate("{{loop.1.success}}", &ctx), "true");

        // Can still reference pre-loop steps by global index
        assert_eq!(interpolate("{{steps.1.data.id}}", &ctx), "curr-123");
    }

    // ── JSON string auto-parsing in nested data fields ──

    #[test]
    fn test_data_json_string_auto_parse() {
        let ctx = ExecutionContext {
            step_results: vec![StepResult {
                step_index: 0,
                step_type: "llm".to_string(),
                success: true,
                duration_ms: 100,
                output: None,
                error: None,
                exit_code: None,
                data: json!({
                    "text": "{\"name\": \"Alice\", \"scores\": [95, 87, 92]}"
                }),
            }],
            inputs: HashMap::new(),
            working_dir: PathBuf::from("/tmp"),
            named_outputs: HashMap::new(),
        };

        // data.text is a JSON string → auto-parsed → traverse deeper
        assert_eq!(interpolate("{{steps.0.data.text.name}}", &ctx), "Alice");
        assert_eq!(interpolate("{{steps.0.data.text.scores.0}}", &ctx), "95");
        assert_eq!(interpolate("{{steps.0.data.text.scores.2}}", &ctx), "92");
    }

    // ── Markdown fence stripping ──

    #[test]
    fn test_strip_markdown_fences_json() {
        let fenced = "```json\n{\"score\": 100, \"passed\": true}\n```";
        let result = strip_markdown_fences(fenced);
        assert_eq!(
            result,
            Some("{\"score\": 100, \"passed\": true}".to_string())
        );
    }

    #[test]
    fn test_strip_markdown_fences_no_lang() {
        let fenced = "```\n{\"key\": \"value\"}\n```";
        let result = strip_markdown_fences(fenced);
        assert_eq!(result, Some("{\"key\": \"value\"}".to_string()));
    }

    #[test]
    fn test_strip_markdown_fences_not_fenced() {
        let plain = "{\"key\": \"value\"}";
        let result = strip_markdown_fences(plain);
        assert_eq!(result, None);
    }

    #[test]
    fn test_strip_markdown_fences_whitespace() {
        let fenced = "  ```json\n  {\"a\": 1}  \n  ```  ";
        let result = strip_markdown_fences(fenced);
        assert_eq!(result, Some("{\"a\": 1}".to_string()));
    }

    #[test]
    fn test_traverse_json_path_with_markdown_fenced_output() {
        // Simulates LLM grading output wrapped in markdown fences
        let ctx = ExecutionContext {
            step_results: vec![
                StepResult {
                    step_index: 0,
                    step_type: "llm".to_string(),
                    success: true,
                    duration_ms: 5000,
                    output: Some("```json\n{\"overallScore\": 100, \"passed\": true, \"feedback\": \"Excellent\"}\n```".to_string()),
                    error: None,
                    exit_code: None,
                    data: json!({}),
                },
            ],
            inputs: HashMap::new(),
            working_dir: PathBuf::from("/tmp"),
            named_outputs: HashMap::new(),
        };

        // These would previously fail (return "") because JSON parse failed on fenced content
        assert_eq!(interpolate("{{steps.0.output.passed}}", &ctx), "true");
        assert_eq!(interpolate("{{steps.0.output.overallScore}}", &ctx), "100");
        assert_eq!(
            interpolate("{{steps.0.output.feedback}}", &ctx),
            "Excellent"
        );
    }

    #[test]
    fn test_traverse_json_path_with_markdown_fenced_data() {
        // Markdown fences in data fields (e.g., LLM text stored in data.text)
        let ctx = ExecutionContext {
            step_results: vec![StepResult {
                step_index: 0,
                step_type: "llm".to_string(),
                success: true,
                duration_ms: 100,
                output: None,
                error: None,
                exit_code: None,
                data: json!({
                    "text": "```json\n{\"items\": [\"alpha\", \"beta\"]}\n```"
                }),
            }],
            inputs: HashMap::new(),
            working_dir: PathBuf::from("/tmp"),
            named_outputs: HashMap::new(),
        };

        assert_eq!(interpolate("{{steps.0.data.text.items.0}}", &ctx), "alpha");
        assert_eq!(interpolate("{{steps.0.data.text.items.1}}", &ctx), "beta");
    }

    #[test]
    fn test_wildcard_collection() {
        // Simulates loop iteration results: iterations[i][j] = sub-step result
        let ctx = ExecutionContext {
            step_results: vec![StepResult {
                step_index: 0,
                step_type: "loop".to_string(),
                success: true,
                duration_ms: 100,
                output: None,
                error: None,
                exit_code: None,
                data: json!({
                    "iterations": [
                        [
                            {"stepType": "watch", "data": {}},
                            {"stepType": "command", "data": {"layerId": "aaa-111"}},
                        ],
                        [
                            {"stepType": "watch", "data": {}},
                            {"stepType": "command", "data": {"layerId": "bbb-222"}},
                        ],
                        [
                            {"stepType": "watch", "data": {}},
                            {"stepType": "command", "data": {"layerId": "ccc-333"}},
                        ],
                    ],
                    "iterationsCompleted": 3,
                }),
            }],
            inputs: HashMap::new(),
            working_dir: PathBuf::from("/tmp"),
            named_outputs: HashMap::new(),
        };

        // Collect layerId from sub-step 1 of each iteration
        let result = interpolate("{{steps.0.data.iterations.*.1.data.layerId}}", &ctx);
        assert_eq!(result, r#"["aaa-111","bbb-222","ccc-333"]"#);
    }

    #[test]
    fn test_wildcard_empty_array() {
        let ctx = ExecutionContext {
            step_results: vec![StepResult {
                step_index: 0,
                step_type: "loop".to_string(),
                success: true,
                duration_ms: 100,
                output: None,
                error: None,
                exit_code: None,
                data: json!({"iterations": []}),
            }],
            inputs: HashMap::new(),
            working_dir: PathBuf::from("/tmp"),
            named_outputs: HashMap::new(),
        };

        assert_eq!(
            interpolate("{{steps.0.data.iterations.*.1.data.layerId}}", &ctx),
            "[]"
        );
    }
