#![cfg(test)]
    //! Registry hot-swap tests. Verify that deregister removes from BOTH
    //! the adapters map AND the priority_order vec — drift between the
    //! two would leave a phantom in `available()` after deregister, which
    //! is exactly the bug a DMR watchdog needs to NOT have.
    use super::*;
    use crate::ai::types::{TextGenerationRequest, TextGenerationResponse};

    /// Minimal adapter for registry-shape tests. Doubles as the live
    /// proof of the trait's default impls: it implements ONLY the four
    /// required methods (provider_id, name, default_model, generate_text)
    /// plus `supports_model` for the model-routing test — everything else
    /// (capabilities, api_style, initialize, shutdown, health_check,
    /// get_available_models, device_type) comes from the trait defaults.
    /// If those defaults regress, this stops compiling or the
    /// `minimal_adapter_inherits_sensible_defaults` test below fails.
    struct StubAdapter {
        id: String,
        model: Option<String>,
    }

    #[async_trait]
    impl AIProviderAdapter for StubAdapter {
        fn provider_id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            &self.id
        }
        fn default_model(&self) -> &str {
            "stub"
        }
        async fn generate_text(
            &self,
            _r: TextGenerationRequest,
        ) -> Result<TextGenerationResponse, String> {
            Err("stub adapter — no inference".into())
        }
        fn supports_model(&self, model: &str) -> bool {
            self.model.as_deref().map_or(true, |m| m == model)
        }
    }

    fn stub(id: &str) -> Arc<dyn AIProviderAdapter> {
        Arc::new(StubAdapter {
            id: id.to_string(),
            model: None,
        })
    }

    fn stub_model(id: &str, model: &str) -> Arc<dyn AIProviderAdapter> {
        Arc::new(StubAdapter {
            id: id.to_string(),
            model: Some(model.to_string()),
        })
    }

    #[test]
    fn deregister_removes_from_both_map_and_priority_order() {
        let mut r = AdapterRegistry::new();
        r.register(stub("dmr"), 0);
        r.register(stub("vulkan"), 1);
        r.register(stub("cloud"), 2);

        assert!(r.is_registered("dmr"));
        assert!(r.deregister("dmr"));
        assert!(!r.is_registered("dmr"));

        let available = r.available();
        assert!(
            !available.contains(&"dmr"),
            "dmr must be gone from available()"
        );
        assert!(available.contains(&"vulkan"));
        assert!(available.contains(&"cloud"));
    }

    #[test]
    fn deregister_returns_false_for_unknown_adapter() {
        let mut r = AdapterRegistry::new();
        r.register(stub("vulkan"), 0);
        assert!(!r.deregister("nonexistent"));
        assert!(r.is_registered("vulkan"));
    }

    #[test]
    fn register_after_deregister_restores_full_state() {
        // The DMR watchdog hot-swap path: deregister on Docker crash,
        // re-register when Docker comes back. Must work cleanly across
        // many cycles without leaking phantom state.
        let mut r = AdapterRegistry::new();
        for _ in 0..5 {
            r.register(stub("dmr"), 0);
            assert!(r.is_registered("dmr"));
            assert!(r.deregister("dmr"));
            assert!(!r.is_registered("dmr"));
        }
        // Final cycle leaves it unregistered.
        assert_eq!(r.available().len(), 0);
    }

    #[tokio::test]
    async fn duplicate_provider_ids_remain_independently_selectable_by_model() {
        let mut r = AdapterRegistry::new();
        r.register(stub_model("llamacpp-local", "qwen3.5"), 0);
        r.register(stub_model("llamacpp-local", "qwen2-vl"), 0);

        assert_eq!(r.available().len(), 2);
        assert!(r.is_registered("llamacpp-local"));

        let (_, qwen35) = r
            .select(Some("local"), Some("qwen3.5"), InferenceDevice::Gpu)
            .expect("qwen3.5 adapter selected");
        assert_eq!(qwen35.default_model(), "stub");
        assert!(qwen35.supports_model("qwen3.5"));
        assert!(!qwen35.supports_model("qwen2-vl"));

        let (_, qwen2) = r
            .select(Some("local"), Some("qwen2-vl"), InferenceDevice::Gpu)
            .expect("qwen2-vl adapter selected");
        assert!(qwen2.supports_model("qwen2-vl"));
        assert!(!qwen2.supports_model("qwen3.5"));

        // Hold the selected model lease while the registry accepts a writer.
        // Re-looking up the provider ID would select the wrong model here.
        let registry = tokio::sync::RwLock::new(r);
        let leased = {
            let guard = registry.read().await;
            let (_, borrowed) = guard.select(Some("local"), Some("qwen2-vl"), InferenceDevice::Gpu).unwrap();
            let (_, leased) = guard.select_arc(Some("local"), Some("qwen2-vl"), InferenceDevice::Gpu).unwrap();
            assert!(std::ptr::eq(borrowed, leased.as_ref()));
            leased
        };
        {
            let mut writer = registry.try_write().expect("a retained model lease must not retain the registry guard");
            assert!(writer.deregister("llamacpp-local"));
        }
        assert!(leased.supports_model("qwen2-vl"), "deregistering must not invalidate in-flight work");
        assert!(!leased.supports_model("qwen3.5"));
    }

    // what this catches: regression in the AIProviderAdapter default impls.
    // A minimal adapter (4 required methods) must inherit sensible defaults
    // for the long tail so new providers + test fixtures don't boilerplate
    // 10 methods (cv::Algorithm anti-pattern — common case must be trivial).
    #[tokio::test]
    async fn minimal_adapter_inherits_sensible_defaults() {
        let mut a = StubAdapter {
            id: "minimal".to_string(),
            model: None,
        };

        // Lifecycle defaults are no-op-Ok.
        assert!(a.initialize().await.is_ok());
        assert!(a.shutdown().await.is_ok());

        // api_style defaults to local in-process inference.
        assert_eq!(a.api_style(), ApiStyle::Local);

        // capabilities default to a text-only set — one vocabulary.
        let caps = a.capabilities();
        assert!(caps.has(Capability::TextGeneration));
        assert!(caps.has(Capability::Chat));
        assert!(!caps.has(Capability::ToolUse));
        assert!(!caps.has(Capability::Vision));

        // health defaults to nominal-healthy (no remote probe).
        let health = a.health_check().await;
        assert_eq!(health.status, crate::ai::types::HealthState::Healthy);
        assert!(health.api_available);

        // no advertised catalog by default — callers use default_model.
        assert!(a.get_available_models().await.is_empty());

        // device defaults to GPU; production-capable unless opted out.
        assert_eq!(a.device_type(), InferenceDevice::Gpu);
        assert!(a.is_production_capable());
    }

    // what this catches: the codified `capabilities()` projection regressing —
    // builder() must seed the text-only floor (so an adapter declares only what
    // it adds), and each NativeProtocols variant must map to its coherent
    // (tool_call, structured_output) pair. If someone re-splits the protocol
    // pair or changes the floor, the next adapter silently gets a wrong/
    // incoherent surface; this pins both.
    #[test]
    fn builder_seeds_floor_and_native_protocols_pair_coherently() {
        // builder() with no overrides == the text-only floor.
        let floor = AdapterCapabilities::builder().build();
        assert_eq!(
            floor.capabilities,
            AdapterCapabilities::text_only().capabilities
        );
        assert!(floor.has(Capability::TextGeneration) && floor.has(Capability::Chat));
        assert!(!floor.has(Capability::ToolUse));
        assert!(!floor.is_local);
        assert_eq!(floor.tool_call_protocol, ToolProtocol::None);
        assert_eq!(
            floor.structured_output_protocol,
            StructuredOutputProtocol::None
        );

        // A rich declaration adds only the deltas on top of the floor.
        let rich = AdapterCapabilities::builder()
            .capabilities([
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
            ])
            .local()
            .context_window(200_000)
            .max_output_tokens(8_192)
            .protocols(NativeProtocols::FunctionCalling)
            .build();
        assert!(rich.has(Capability::ToolUse) && rich.is_local);
        assert_eq!(rich.max_context_window, Some(200_000));
        assert_eq!(rich.max_output_tokens, Some(8_192));

        // Each protocol profile maps to its coherent pair — the whole point of
        // NativeProtocols (an incoherent combo is unrepresentable).
        for (profile, tool, structured) in [
            (
                NativeProtocols::None,
                ToolProtocol::None,
                StructuredOutputProtocol::None,
            ),
            (
                NativeProtocols::PromptEmulated,
                ToolProtocol::None,
                StructuredOutputProtocol::PromptOnly,
            ),
            (
                NativeProtocols::FunctionCalling,
                ToolProtocol::NativeFunctionCalling,
                StructuredOutputProtocol::JsonSchema,
            ),
            (
                NativeProtocols::GrammarConstrained,
                ToolProtocol::JsonInPrompt,
                StructuredOutputProtocol::GrammarConstrained,
            ),
        ] {
            assert_eq!(profile.tool_call(), tool, "{profile:?} tool half");
            assert_eq!(
                profile.structured_output(),
                structured,
                "{profile:?} structured half"
            );
        }
    }
