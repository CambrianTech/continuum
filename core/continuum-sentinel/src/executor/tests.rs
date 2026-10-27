#![cfg(test)]
    use super::*;
    use crate::types::{Pipeline, PipelineStep};
    use continuum_kernel::runtime::{message_bus::MessageBus, ModuleRegistry};
    use serde_json::json;
    use std::sync::Arc;

    fn make_registry() -> Arc<ModuleRegistry> {
        Arc::new(ModuleRegistry::new())
    }

    fn make_bus() -> Arc<MessageBus> {
        Arc::new(MessageBus::new())
    }

    /// Test a simple linear pipeline: echo a → echo b → echo c
    #[tokio::test]
    async fn test_linear_pipeline() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-linear");

        let pipeline = Pipeline {
            name: Some("linear-test".to_string()),
            steps: vec![
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["step-a".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["step-b".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["step-c".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-linear",
            pipeline,
            Some(&bus),
            Some(&registry),
            None,
        )
        .await;

        assert!(result.success);
        assert_eq!(result.steps_completed, 3);
        assert_eq!(result.steps_total, 3);
        assert_eq!(result.step_results[0].output.as_deref(), Some("step-a\n"));
        assert_eq!(result.step_results[1].output.as_deref(), Some("step-b\n"));
        assert_eq!(result.step_results[2].output.as_deref(), Some("step-c\n"));
        assert!(result.error.is_none());
    }

    /// Test pipeline stops on first failure
    #[tokio::test]
    async fn test_pipeline_stops_on_failure() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-fail");

        let pipeline = Pipeline {
            name: Some("fail-test".to_string()),
            steps: vec![
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["ok".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Shell {
                    cmd: "/bin/sh".into(),
                    args: vec!["-c".into(), "exit 42".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["never-reached".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-fail",
            pipeline,
            Some(&bus),
            Some(&registry),
            None,
        )
        .await;

        assert!(!result.success);
        assert_eq!(result.steps_completed, 2); // echo ok + failing step
        assert_eq!(result.steps_total, 3);
        assert!(result.error.is_some());
    }

    /// Test pipeline with condition branching
    #[tokio::test]
    async fn test_pipeline_with_condition() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-cond");

        let mut inputs = HashMap::new();
        inputs.insert("should_build".to_string(), json!("true"));

        let pipeline = Pipeline {
            name: Some("cond-test".to_string()),
            steps: vec![
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["start".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Condition {
                    condition: "{{input.should_build}}".to_string(),
                    then_steps: vec![PipelineStep::Shell {
                        cmd: "echo".into(),
                        args: vec!["building".into()],
                        timeout_secs: Some(10),
                        working_dir: None,
                        allow_failure: None,
                        env: None,
                    }],
                    else_steps: vec![PipelineStep::Shell {
                        cmd: "echo".into(),
                        args: vec!["skipping".into()],
                        timeout_secs: Some(10),
                        working_dir: None,
                        allow_failure: None,
                        env: None,
                    }],
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["done".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs,
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-cond",
            pipeline,
            Some(&bus),
            Some(&registry),
            None,
        )
        .await;

        assert!(result.success);
        // step 0: echo start, step 1: condition (which runs echo building as substep), step 2: echo done
        // But the condition step pushes its substep results into the context
        // So we get: echo start → (echo building pushed by condition) → condition result → echo done
        assert!(result.steps_completed >= 3);
    }

    /// Test pipeline with loop and variable interpolation
    #[tokio::test]
    async fn test_pipeline_with_loop() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-loop");

        let pipeline = Pipeline {
            name: Some("loop-test".to_string()),
            steps: vec![PipelineStep::Loop {
                count: Some(3),
                steps: vec![PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["iteration-{{input.iteration}}".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                }],
                while_condition: None,
                until: None,
                max_iterations: None,
            }],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-loop",
            pipeline,
            Some(&bus),
            Some(&registry),
            None,
        )
        .await;

        assert!(result.success);
    }

    /// Test pipeline with parallel branches
    #[tokio::test]
    async fn test_pipeline_with_parallel() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-parallel");

        let pipeline = Pipeline {
            name: Some("parallel-test".to_string()),
            steps: vec![
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["before-fork".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Parallel {
                    branches: vec![
                        vec![PipelineStep::Shell {
                            cmd: "echo".into(),
                            args: vec!["branch-a".into()],
                            timeout_secs: Some(10),
                            working_dir: None,
                            allow_failure: None,
                            env: None,
                        }],
                        vec![PipelineStep::Shell {
                            cmd: "echo".into(),
                            args: vec!["branch-b".into()],
                            timeout_secs: Some(10),
                            working_dir: None,
                            allow_failure: None,
                            env: None,
                        }],
                    ],
                    fail_fast: false,
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["after-join".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-par",
            pipeline,
            Some(&bus),
            Some(&registry),
            None,
        )
        .await;

        assert!(result.success);
        assert_eq!(result.steps_total, 3);
    }

    /// Test emit + watch composition across spawned task
    #[tokio::test]
    async fn test_emit_watch_composition() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-emit-watch");

        // Use parallel to run emit and watch concurrently — emit fires, watch catches
        let pipeline = Pipeline {
            name: Some("emit-watch-test".to_string()),
            steps: vec![PipelineStep::Parallel {
                branches: vec![
                    // Branch 0: small delay then emit
                    vec![
                        PipelineStep::Shell {
                            cmd: "sleep".into(),
                            args: vec!["0.1".into()],
                            timeout_secs: Some(5),
                            working_dir: None,
                            allow_failure: None,
                            env: None,
                        },
                        PipelineStep::Emit {
                            event: "test:signal".to_string(),
                            payload: json!({"msg": "hello"}),
                        },
                    ],
                    // Branch 1: watch for the event
                    vec![PipelineStep::Watch {
                        event: "test:signal".to_string(),
                        timeout_secs: Some(5),
                    }],
                ],
                fail_fast: false,
            }],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-ew",
            pipeline,
            Some(&bus),
            Some(&registry),
            None,
        )
        .await;

        assert!(result.success);
    }

    /// Test nested sentinel step (pipeline within pipeline)
    #[tokio::test]
    async fn test_nested_sentinel_pipeline() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-nested");

        let pipeline = Pipeline {
            name: Some("parent".to_string()),
            steps: vec![
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["parent-start".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Sentinel {
                    pipeline: Box::new(Pipeline {
                        name: Some("child".to_string()),
                        steps: vec![
                            PipelineStep::Shell {
                                cmd: "echo".into(),
                                args: vec!["child-a".into()],
                                timeout_secs: Some(10),
                                working_dir: None,
                                allow_failure: None,
                                env: None,
                            },
                            PipelineStep::Shell {
                                cmd: "echo".into(),
                                args: vec!["child-b".into()],
                                timeout_secs: Some(10),
                                working_dir: None,
                                allow_failure: None,
                                env: None,
                            },
                        ],
                        working_dir: None,
                        timeout_secs: None,
                        inputs: HashMap::new(),
                    }),
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["parent-end".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-nested",
            pipeline,
            Some(&bus),
            Some(&registry),
            None,
        )
        .await;

        assert!(result.success);
        assert_eq!(result.steps_total, 3);
    }

    /// Test pipeline with variable forwarding between steps
    #[tokio::test]
    async fn test_step_output_forwarding() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-fwd");

        let pipeline = Pipeline {
            name: Some("forward-test".to_string()),
            steps: vec![
                // Step 0: produce output
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["hello-from-step-0".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                // Step 1: reference step 0's output via interpolation
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["got: {{steps.0.data.stdout}}".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-fwd",
            pipeline,
            Some(&bus),
            Some(&registry),
            None,
        )
        .await;

        assert!(result.success);
        assert_eq!(result.steps_completed, 2);
        // Step 1 should have interpolated step 0's stdout
        let step1_output = result.step_results[1].output.as_deref().unwrap_or("");
        assert!(
            step1_output.contains("hello-from-step-0"),
            "Expected forwarded output, got: {step1_output}"
        );
    }

    /// Test empty pipeline succeeds
    #[tokio::test]
    async fn test_empty_pipeline() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-empty");

        let pipeline = Pipeline {
            name: Some("empty".to_string()),
            steps: vec![],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-empty",
            pipeline,
            Some(&bus),
            Some(&registry),
            None,
        )
        .await;

        assert!(result.success);
        assert_eq!(result.steps_completed, 0);
        assert_eq!(result.steps_total, 0);
    }

    /// Test pipeline without registry returns error
    #[tokio::test]
    async fn test_pipeline_requires_registry() {
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-noreg");

        let pipeline = Pipeline {
            name: Some("no-reg".to_string()),
            steps: vec![PipelineStep::Shell {
                cmd: "echo".into(),
                args: vec!["test".into()],
                timeout_secs: Some(10),
                working_dir: None,
                allow_failure: None,
                env: None,
            }],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result =
            execute_pipeline_direct(&logs_dir, "test-noreg", pipeline, Some(&bus), None, None)
                .await;

        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("registry"));
    }
