#![cfg(test)]
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;

    // what this catches: the refusal for an unknown/omitted model must name the
    // model ids the registry actually serves, not just provider ids — a grid
    // consumer with no local registry was refused with a list it could not act
    // on (card a466fdd4, IntelMac → 5090, 2026-09-04).
    #[test]
    fn select_failure_message_names_served_models_for_model_and_no_specifier() {
        let heuristic = HeuristicInferenceAdapter::new();
        let provider = heuristic.provider_id().to_string();
        let model = heuristic.default_model().to_string();
        let mut registry = AdapterRegistry::new();
        registry.register(Arc::new(heuristic), 0);

        let by_model = select_failure_message(&registry, None, Some("no-such-model"));
        assert!(by_model.contains("no-such-model"), "{by_model}");
        assert!(
            by_model.contains(&format!("{provider}={model}")),
            "{by_model}"
        );

        let unspecified = select_failure_message(&registry, None, None);
        assert!(unspecified.contains("never picks one"), "{unspecified}");
        assert!(
            unspecified.contains(&format!("{provider}={model}")),
            "{unspecified}"
        );

        let bad_provider = select_failure_message(&registry, Some("ghost"), None);
        assert!(bad_provider.contains("\"ghost\""), "{bad_provider}");
        assert!(bad_provider.contains(&provider), "{bad_provider}");
    }

    // what this catches: a gateway adapter's catalog `default_model` is a
    // DERIVED value that can misname the live lane (5090 2026-07-24); once the
    // daemon has verified what the lane serves (`ensure_runtime_model`), the
    // refusal must point callers at THAT id, never the catalog placeholder —
    // otherwise the "actionable" refusal sends them into a second refusal
    // (#3696 review finding #1).
    #[test]
    fn served_models_prefers_verified_runtime_ids_over_catalog_default() {
        // Built from a plain config, not `from_registry`: the model registry
        // singleton is a boot-path concern, and this test is about the
        // adapter's own runtime set vs its declared default.
        let gateway =
            OpenAICompatibleAdapter::new(crate::ai::openai_adapter::OpenAICompatibleConfig {
                provider_id: crate::inference::llama_server::PROVIDER_ID.into(),
                name: "llama-server (test)".into(),
                base_url: "http://127.0.0.1:0".into(),
                api_key_env: None,
                default_model: "catalog-default".into(),
                capabilities: std::collections::BTreeSet::new(),
                models: Vec::new(),
                model_prefixes: Vec::new(),
                requires_auth: false,
                tool_protocol: crate::model_registry::ToolProtocol::NativeFunctionCalling,
                thinking: crate::ai::openai_adapter::ThinkingMode::Default,
                single_resident_model: false,
                dynamic_model_catalog: false,
                llamacpp_sampling_extensions: false,
            });
        let catalog_default = gateway.default_model().to_string();
        assert_eq!(gateway.served_model_ids(), vec![catalog_default.clone()]);

        gateway.ensure_runtime_model("ornith-ai/Ornith-1.5-35B-A3B-GGUF");
        assert_eq!(
            gateway.served_model_ids(),
            vec!["ornith-ai/Ornith-1.5-35B-A3B-GGUF".to_string()],
            "verified runtime set must replace the catalog default"
        );

        let mut registry = AdapterRegistry::new();
        registry.register(Arc::new(gateway), 0);
        let msg = select_failure_message(&registry, None, Some("nope"));
        assert!(msg.contains("=ornith-ai/Ornith-1.5-35B-A3B-GGUF"), "{msg}");
        assert!(!msg.contains(&format!("={catalog_default}")), "{msg}");
    }
