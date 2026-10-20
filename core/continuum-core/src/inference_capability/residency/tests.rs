    use super::*;

    // ---- Synthetic Qwen variants (published HF model card values) ----

    fn qwen25_7b_q4km() -> QwenModelMetadata {
        QwenModelMetadata {
            model_name: "Qwen2.5-7B-Instruct".into(),
            architecture: "qwen2".into(),
            layer_count: 28,
            parameter_count_billions: 7.0,
            bytes_per_parameter_quantized: 0.5, // Q4_K_M
            layer_kinds_needing_check: vec![],
        }
    }

    fn qwen25_3b_q4km() -> QwenModelMetadata {
        QwenModelMetadata {
            model_name: "Qwen2.5-3B-Instruct".into(),
            architecture: "qwen2".into(),
            layer_count: 36,
            parameter_count_billions: 3.0,
            bytes_per_parameter_quantized: 0.5,
            layer_kinds_needing_check: vec![],
        }
    }

    fn qwen25_coder_7b_q4km() -> QwenModelMetadata {
        QwenModelMetadata {
            model_name: "Qwen2.5-Coder-7B-Instruct".into(),
            architecture: "qwen2".into(),
            layer_count: 28,
            parameter_count_billions: 7.0,
            bytes_per_parameter_quantized: 0.5,
            layer_kinds_needing_check: vec![],
        }
    }

    fn qwen3_30b_a3b_q4km() -> QwenModelMetadata {
        QwenModelMetadata {
            model_name: "Qwen3-30B-A3B-Instruct".into(),
            architecture: "qwen3moe".into(),
            layer_count: 48,
            parameter_count_billions: 30.0,
            bytes_per_parameter_quantized: 0.5,
            // MoE gate is a Vulkan gap today
            layer_kinds_needing_check: vec!["moe_gate".into()],
        }
    }

    fn qwen2vl_7b_q4km() -> QwenModelMetadata {
        QwenModelMetadata {
            model_name: "Qwen2-VL-7B-Instruct".into(),
            architecture: "qwen2vl".into(),
            layer_count: 28,
            parameter_count_billions: 7.0,
            bytes_per_parameter_quantized: 0.5,
            layer_kinds_needing_check: vec![],
        }
    }

    // ---- Synthetic hardware tiers (matches probe.rs test fixtures) ----

    fn macbook_air_m2_8gb() -> HardwareProfile {
        HardwareProfile {
            platform: "macos-arm64-m2".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 5 * 1024 * 1024 * 1024, // 5 GB
            total_vram_bytes: 8 * 1024 * 1024 * 1024,
            cpu_cores: 8,
            system_ram_bytes: 8 * 1024 * 1024 * 1024,
        }
    }

    fn m5_pro_48gb() -> HardwareProfile {
        HardwareProfile {
            platform: "macos-arm64-m5pro".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 32 * 1024 * 1024 * 1024,
            total_vram_bytes: 48 * 1024 * 1024 * 1024,
            cpu_cores: 16,
            system_ram_bytes: 64 * 1024 * 1024 * 1024,
        }
    }

    fn blackwell_rtx_5090() -> HardwareProfile {
        HardwareProfile {
            platform: "linux-x86_64-blackwell".into(),
            has_metal: false,
            has_cuda: true,
            has_vulkan: true,
            free_vram_bytes: 28 * 1024 * 1024 * 1024,
            total_vram_bytes: 32 * 1024 * 1024 * 1024,
            cpu_cores: 32,
            system_ram_bytes: 128 * 1024 * 1024 * 1024,
        }
    }

    fn generic_dell_no_gpu() -> HardwareProfile {
        HardwareProfile {
            platform: "linux-x86_64-generic".into(),
            has_metal: false,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 0,
            total_vram_bytes: 0,
            cpu_cores: 12,
            system_ram_bytes: 32 * 1024 * 1024 * 1024,
        }
    }

    fn amd_with_vulkan_only() -> HardwareProfile {
        HardwareProfile {
            platform: "linux-x86_64-amd-rdna3".into(),
            has_metal: false,
            has_cuda: false,
            has_vulkan: true,
            free_vram_bytes: 16 * 1024 * 1024 * 1024,
            total_vram_bytes: 24 * 1024 * 1024 * 1024,
            cpu_cores: 16,
            system_ram_bytes: 64 * 1024 * 1024 * 1024,
        }
    }

    // ===== select_backend =====

    /// What this catches: select_backend picks Metal on Mac (Apple
    /// Silicon path). If this regresses, every Mac host silently routes
    /// inference through CUDA-or-nothing.
    #[test]
    fn select_backend_picks_metal_on_mac() {
        assert_eq!(
            select_backend(&macbook_air_m2_8gb()),
            Some(BackendChoice::Metal)
        );
        assert_eq!(select_backend(&m5_pro_48gb()), Some(BackendChoice::Metal));
    }

    /// What this catches: CUDA wins over Vulkan on a host that has
    /// both (NVIDIA cards expose Vulkan too). llama.cpp's CUDA kernels
    /// are more complete than its Vulkan kernels today; CUDA must win
    /// the precedence.
    #[test]
    fn select_backend_picks_cuda_over_vulkan_on_nvidia() {
        // Blackwell has BOTH has_cuda + has_vulkan
        assert_eq!(
            select_backend(&blackwell_rtx_5090()),
            Some(BackendChoice::Cuda)
        );
    }

    /// What this catches: Vulkan-only host (AMD without CUDA) gets
    /// Vulkan as the selection. Without this, AMD hosts would be
    /// silently CPU-only.
    #[test]
    fn select_backend_picks_vulkan_when_amd_only() {
        assert_eq!(
            select_backend(&amd_with_vulkan_only()),
            Some(BackendChoice::Vulkan)
        );
    }

    /// What this catches: no GPU at all → None. The gate then
    /// surfaces NoGpuBackendOnNode. Critical — silent CPU fallback is
    /// the bug this whole module exists to prevent.
    #[test]
    fn select_backend_returns_none_on_cpu_only() {
        assert_eq!(select_backend(&generic_dell_no_gpu()), None);
    }

    // ===== check_residency_gate — happy paths =====

    /// What this catches: M5 Pro Metal + Qwen2.5-7B Q4_K_M passes the
    /// gate with full evidence. The flagship Mac tier × the workhorse
    /// model — if this regresses, no Mac runs Qwen.
    #[test]
    fn m5_pro_runs_qwen25_7b_q4km() {
        let result = check_residency_gate(&qwen25_7b_q4km(), &m5_pro_48gb());
        assert!(result.is_pass(), "expected Pass; got {result:?}");
        if let ResidencyGateResult::Pass(ev) = result {
            assert_eq!(ev.backend, BackendChoice::Metal);
            assert_eq!(ev.gpu_layer_count, 28);
            assert_eq!(ev.model_name, "Qwen2.5-7B-Instruct");
            assert_eq!(ev.platform, "macos-arm64-m5pro");
        }
    }

    /// What this catches: MacBook Air M2 8GB has 5GB free VRAM; a 3B
    /// Q4_K_M (≈ 1.65 GB estimated) fits cleanly. The smallest-Mac ×
    /// smallest-Qwen path must pass — this is the m2-8gb-baseline.
    #[test]
    fn macbook_air_m2_runs_qwen25_3b_q4km() {
        let result = check_residency_gate(&qwen25_3b_q4km(), &macbook_air_m2_8gb());
        assert!(result.is_pass(), "expected Pass; got {result:?}");
    }

    /// What this catches: Blackwell + Qwen2.5-Coder-7B passes via CUDA
    /// (not Vulkan, even though both available). Codepath used in CI
    /// for code-completion bench.
    #[test]
    fn blackwell_runs_qwen25_coder_7b_via_cuda() {
        let result = check_residency_gate(&qwen25_coder_7b_q4km(), &blackwell_rtx_5090());
        assert!(result.is_pass());
        if let ResidencyGateResult::Pass(ev) = result {
            assert_eq!(ev.backend, BackendChoice::Cuda);
        }
    }

    /// What this catches: Qwen2-VL on Metal passes — vision variant
    /// uses qwen2vl architecture, which Metal handles cleanly. If this
    /// regresses, Vision AI persona is silently unavailable on Mac.
    #[test]
    fn m5_pro_runs_qwen2vl_7b_via_metal() {
        let result = check_residency_gate(&qwen2vl_7b_q4km(), &m5_pro_48gb());
        assert!(result.is_pass());
        if let ResidencyGateResult::Pass(ev) = result {
            assert_eq!(ev.backend, BackendChoice::Metal);
            assert_eq!(ev.architecture, "qwen2vl");
        }
    }

    // ===== check_residency_gate — block paths =====

    /// What this catches: CPU-only host blocks with NoGpuBackendOnNode
    /// and ONLY that reason (other checks are bypassed). Per
    /// no_cpu_fallback rule — never silently route to CPU.
    #[test]
    fn cpu_only_host_blocks_with_no_gpu_reason() {
        let result = check_residency_gate(&qwen25_3b_q4km(), &generic_dell_no_gpu());
        assert!(!result.is_pass());
        match result {
            ResidencyGateResult::Block { reasons } => {
                assert_eq!(reasons.len(), 1, "no-GPU is exclusive; got {reasons:?}");
                match &reasons[0] {
                    BlockReason::NoGpuBackendOnNode { platform } => {
                        assert_eq!(platform, "linux-x86_64-generic");
                    }
                    other => panic!("expected NoGpuBackendOnNode, got {other:?}"),
                }
            }
            other => panic!("expected Block, got {other:?}"),
        }
    }

    /// What this catches: MacBook Air M2 (5GB free) trying to run
    /// Qwen2.5-7B Q4_K_M (≈ 3.85 GB estimated, plus headroom) — should
    /// PASS at 5GB free. But Qwen3-30B-A3B on M2 (60GB Q4 + 10%
    /// headroom = 16.5GB) should BLOCK with PartialGpuSplit.
    #[test]
    fn m2_air_blocks_qwen3_30b_for_vram() {
        let result = check_residency_gate(&qwen3_30b_a3b_q4km(), &macbook_air_m2_8gb());
        assert!(!result.is_pass(), "30B on 5GB free must block");
        match result {
            ResidencyGateResult::Block { reasons } => {
                assert!(reasons
                    .iter()
                    .any(|r| matches!(r, BlockReason::PartialGpuSplit { .. })));
            }
            _ => panic!("expected Block"),
        }
    }

    /// What this catches: AMD Vulkan-only + Qwen3 MoE blocks with
    /// UnsupportedLayer (Vulkan llama.cpp lacks MoE gate). This is
    /// the per-model second check beyond the probe — probe.rs already
    /// excludes Vulkan-only hosts from llamacpp advertisement, but if
    /// something forces backend selection through, the gate catches.
    #[test]
    fn amd_vulkan_blocks_qwen3_moe_with_unsupported_layer() {
        let result = check_residency_gate(&qwen3_30b_a3b_q4km(), &amd_with_vulkan_only());
        assert!(!result.is_pass());
        match result {
            ResidencyGateResult::Block { reasons } => {
                let has_unsupported = reasons
                    .iter()
                    .any(|r| matches!(r, BlockReason::UnsupportedLayer { layer_kind, .. } if layer_kind == "moe_gate"));
                assert!(
                    has_unsupported,
                    "expected UnsupportedLayer moe_gate; got {reasons:?}"
                );
            }
            _ => panic!("expected Block"),
        }
    }

    /// What this catches: AMD Vulkan + Qwen2 (NOT MoE) PASSES — Vulkan
    /// supports qwen2 architecture today per the vulkan_safe_archs
    /// list. If this regresses, AMD-fleet onboarding loses Qwen2.5
    /// silently.
    #[test]
    fn amd_vulkan_runs_qwen25_7b_via_vulkan() {
        let result = check_residency_gate(&qwen25_7b_q4km(), &amd_with_vulkan_only());
        assert!(result.is_pass(), "qwen2 should run on Vulkan: {result:?}");
        if let ResidencyGateResult::Pass(ev) = result {
            assert_eq!(ev.backend, BackendChoice::Vulkan);
        }
    }

    /// What this catches: a Qwen variant that lists a
    /// layer_kinds_needing_check but the backend is Metal (full
    /// coverage) → no UnsupportedLayer reason. The supported-on-native
    /// guarantee is preserved.
    #[test]
    fn metal_backend_passes_qwen3_moe_no_unsupported() {
        // Hypothetical M5 Pro with enough VRAM for 30B Q4 (16.5GB est)
        let mut hw = m5_pro_48gb();
        hw.free_vram_bytes = 20 * 1024 * 1024 * 1024;
        let result = check_residency_gate(&qwen3_30b_a3b_q4km(), &hw);
        assert!(result.is_pass(), "Metal should handle qwen3moe: {result:?}");
        if let ResidencyGateResult::Pass(ev) = result {
            assert_eq!(ev.backend, BackendChoice::Metal);
            assert_eq!(ev.architecture, "qwen3moe");
        }
    }

    /// What this catches: a block can carry MULTIPLE reasons. If a
    /// host has both an unsupported layer AND insufficient VRAM, the
    /// caller sees both, not just the first. Important for diagnosis
    /// — "you'd fail for two reasons" beats "you'd fail because X
    /// (then later: oh also Y)".
    #[test]
    fn block_accumulates_multiple_reasons() {
        // Vulkan-only host, very low VRAM, Qwen3 MoE — both
        // UnsupportedLayer + PartialGpuSplit.
        let mut hw = amd_with_vulkan_only();
        hw.free_vram_bytes = 2 * 1024 * 1024 * 1024; // 2GB, way under 30B Q4 ≈ 16.5GB
        let result = check_residency_gate(&qwen3_30b_a3b_q4km(), &hw);
        match result {
            ResidencyGateResult::Block { reasons } => {
                assert!(
                    reasons.len() >= 2,
                    "expected multi-reason block; got {reasons:?}"
                );
                assert!(reasons
                    .iter()
                    .any(|r| matches!(r, BlockReason::UnsupportedLayer { .. })));
                assert!(reasons
                    .iter()
                    .any(|r| matches!(r, BlockReason::PartialGpuSplit { .. })));
            }
            _ => panic!("expected Block"),
        }
    }

    // ===== estimated_vram_bytes =====

    /// What this catches: Q4_K_M 7B estimate stays within the expected
    /// rough band (3.5–4.5 GB). Pins the formula; refactors that drift
    /// the multiplier will trip this test.
    #[test]
    fn vram_estimate_q4_7b_within_expected_band() {
        let m = qwen25_7b_q4km();
        let est = m.estimated_vram_bytes();
        let gb = 1024u64 * 1024 * 1024;
        assert!(
            est >= 3 * gb && est <= 5 * gb,
            "Q4 7B should estimate 3-5GB; got {} ({} GB)",
            est,
            est as f64 / gb as f64
        );
    }

    /// What this catches: 30B Q4 estimate stays in the 14–18 GB band
    /// (theoretical: 30 × 0.5 × 1.10 = 16.5 GB).
    #[test]
    fn vram_estimate_q4_30b_within_expected_band() {
        let m = qwen3_30b_a3b_q4km();
        let est = m.estimated_vram_bytes();
        let gb = 1024u64 * 1024 * 1024;
        assert!(
            est >= 14 * gb && est <= 18 * gb,
            "30B Q4: got {est} ({} GB)",
            est as f64 / gb as f64
        );
    }

    /// What this catches: bigger quantization → bigger estimate.
    /// Sanity check the linear-in-bytes-per-param relationship; a
    /// regression that ignored the field would break this.
    #[test]
    fn vram_estimate_scales_with_quantization() {
        let mut q4 = qwen25_7b_q4km();
        let q4_est = q4.estimated_vram_bytes();
        q4.bytes_per_parameter_quantized = 1.0; // Q8_0
        let q8_est = q4.estimated_vram_bytes();
        assert!(q8_est > q4_est, "Q8 must estimate higher than Q4");
        assert!(
            q8_est >= 2 * q4_est - 1024 * 1024 * 1024,
            "Q8 should be ~2× Q4"
        );
    }

    // ===== Pass with full evidence =====

    /// What this catches: passing gate emits every field the
    /// CBAR-SUBSTRATE spec requires — model_name, backend, gpu layer
    /// count, vram estimate, free vram, platform. Omission would be a
    /// no_silent violation by missing evidence.
    #[test]
    fn pass_evidence_has_all_required_fields() {
        let result = check_residency_gate(&qwen25_7b_q4km(), &m5_pro_48gb());
        match result {
            ResidencyGateResult::Pass(ev) => {
                assert!(!ev.model_name.is_empty());
                assert!(!ev.architecture.is_empty());
                assert!(!ev.platform.is_empty());
                assert!(ev.gpu_layer_count > 0);
                assert!(ev.estimated_vram_bytes > 0);
                assert!(ev.free_vram_bytes > 0);
                // backend is non-Option enum, always set
                let _ = ev.backend;
            }
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    // ===== Determinism + serde =====

    /// What this catches: same inputs → same gate result. Pure-function
    /// guarantee — no I/O, no globals, no thread-local state. PR-3
    /// can cache the result keyed on (model, hw) without worrying
    /// about silent drift.
    #[test]
    fn gate_is_deterministic() {
        let m = qwen25_7b_q4km();
        let hw = m5_pro_48gb();
        let a = check_residency_gate(&m, &hw);
        let b = check_residency_gate(&m, &hw);
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }

    /// What this catches: BackendChoice serializes as lowercase string
    /// (matching LatencyClass + the rest of the ts-rs surface). Wire
    /// stability for PR-3 + PR-4 + the eventual cross-node dispatcher.
    #[test]
    fn backend_choice_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&BackendChoice::Metal).unwrap(),
            "\"metal\""
        );
        assert_eq!(
            serde_json::to_string(&BackendChoice::Cuda).unwrap(),
            "\"cuda\""
        );
        assert_eq!(
            serde_json::to_string(&BackendChoice::Vulkan).unwrap(),
            "\"vulkan\""
        );
    }

    /// What this catches: BlockReason serde round-trip (tagged-union
    /// with `kind` discriminator). PR-3's caller will deserialize
    /// these from grid wire / recorder fixtures; the shape must round-
    /// trip cleanly.
    #[test]
    fn block_reason_serde_round_trip() {
        let reasons = vec![
            BlockReason::ModelMetadataUnreadable {
                model_path: "/models/qwen.gguf".into(),
                error: "missing general.architecture".into(),
            },
            BlockReason::NoGpuBackendOnNode {
                platform: "test".into(),
            },
            BlockReason::UnsupportedLayer {
                backend: BackendChoice::Vulkan,
                architecture: "qwen3moe".into(),
                layer_kind: "moe_gate".into(),
            },
            BlockReason::PartialGpuSplit {
                backend: BackendChoice::Metal,
                estimated_required_bytes: 16_000_000_000,
                free_vram_bytes: 5_000_000_000,
            },
        ];
        for r in &reasons {
            let j = serde_json::to_string(r).unwrap();
            let back: BlockReason = serde_json::from_str(&j).unwrap();
            assert_eq!(*r, back);
            assert!(j.contains("\"kind\":\""), "tag missing: {j}");
        }
    }

    /// What this catches: ResidencyGateResult Pass/Block tagged-union
    /// round-trips with `outcome` discriminator + nested fields.
    #[test]
    fn gate_result_serde_round_trip() {
        let pass = check_residency_gate(&qwen25_7b_q4km(), &m5_pro_48gb());
        let j = serde_json::to_string(&pass).unwrap();
        let back: ResidencyGateResult = serde_json::from_str(&j).unwrap();
        assert_eq!(pass, back);
        assert!(j.contains("\"outcome\":\"pass\""), "outcome tag: {j}");

        let block = check_residency_gate(&qwen25_3b_q4km(), &generic_dell_no_gpu());
        let j = serde_json::to_string(&block).unwrap();
        let back: ResidencyGateResult = serde_json::from_str(&j).unwrap();
        assert_eq!(block, back);
        assert!(j.contains("\"outcome\":\"block\""));
    }

    /// What this catches: QwenModelMetadata round-trips with camelCase.
    /// PR-2 will populate this from GGUF + ship to the recorder; field
    /// names must match what TypeScript consumers expect.
    #[test]
    fn qwen_model_metadata_serde_camelcase() {
        let m = qwen3_30b_a3b_q4km();
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains("\"modelName\":"));
        assert!(j.contains("\"layerCount\":48"));
        assert!(j.contains("\"parameterCountBillions\":30.0"));
        assert!(j.contains("\"bytesPerParameterQuantized\":0.5"));
        assert!(j.contains("\"layerKindsNeedingCheck\":[\"moe_gate\"]"));
        let back: QwenModelMetadata = serde_json::from_str(&j).unwrap();
        assert_eq!(back, m);
    }

    /// What this catches: ResidencyEvidence round-trips with camelCase
    /// + every field's JSON name matches PR-3/PR-4 contracts.
    #[test]
    fn residency_evidence_serde_camelcase() {
        let result = check_residency_gate(&qwen25_7b_q4km(), &blackwell_rtx_5090());
        if let ResidencyGateResult::Pass(ev) = result {
            let j = serde_json::to_string(&ev).unwrap();
            assert!(j.contains("\"modelName\":"));
            assert!(j.contains("\"gpuLayerCount\":28"));
            assert!(j.contains("\"estimatedVramBytes\":"));
            assert!(j.contains("\"freeVramBytes\":"));
            assert!(j.contains("\"backend\":\"cuda\""));
        } else {
            panic!("expected Pass");
        }
    }

    // ===== Edge cases =====

    /// What this catches: free VRAM exactly equal to estimate → pass
    /// (inclusive boundary). Symmetric with probe.rs
    /// find_capable_matches_on_exact_vram_boundary.
    #[test]
    fn vram_exactly_at_estimate_passes() {
        let m = qwen25_7b_q4km();
        let est = m.estimated_vram_bytes();
        let mut hw = m5_pro_48gb();
        hw.free_vram_bytes = est;
        let result = check_residency_gate(&m, &hw);
        assert!(
            result.is_pass(),
            "VRAM == estimate must pass; got {result:?}"
        );
    }

    /// What this catches: free VRAM one byte below estimate → block.
    /// Establishes the inclusive-min boundary explicitly.
    #[test]
    fn vram_one_byte_under_estimate_blocks() {
        let m = qwen25_7b_q4km();
        let est = m.estimated_vram_bytes();
        let mut hw = m5_pro_48gb();
        hw.free_vram_bytes = est - 1;
        let result = check_residency_gate(&m, &hw);
        assert!(!result.is_pass());
    }

    /// What this catches: tiny Qwen variant (e.g. Qwen2.5-0.5B) on
    /// a CPU-only host still blocks. Size doesn't rescue the gate —
    /// no GPU = block, period.
    #[test]
    fn tiny_model_on_cpu_only_still_blocks() {
        let mut m = qwen25_3b_q4km();
        m.parameter_count_billions = 0.5;
        let result = check_residency_gate(&m, &generic_dell_no_gpu());
        assert!(!result.is_pass());
        assert!(result
            .reasons()
            .iter()
            .any(|r| matches!(r, BlockReason::NoGpuBackendOnNode { .. })));
    }

    /// What this catches: a model variant the local probe would have
    /// included but the gate now rejects per residency. The two layers
    /// (probe + residency) must compose: probe says "node can take
    /// llamacpp," residency says "can take THIS llamacpp model." Both
    /// guarantees are needed; this test pins the gap.
    #[test]
    fn probe_passes_but_residency_blocks_partial_split() {
        use crate::inference_capability::probe::probe_inference_capabilities;
        use crate::inference_capability::types::kinds;

        let hw = macbook_air_m2_8gb();
        let probe_caps = probe_inference_capabilities(&hw);
        // probe advertises llamacpp on this host
        assert!(probe_caps
            .iter()
            .any(|c| c.kind.as_str() == kinds::LLAMACPP));

        // but residency gate blocks a 30B model on it
        let result = check_residency_gate(&qwen3_30b_a3b_q4km(), &hw);
        assert!(!result.is_pass());
    }

    /// What this catches: BackendChoice::as_str() returns the lowercase
    /// wire-stable string for each variant. Used in error messages +
    /// log lines; if it drifts, grep-by-backend-name breaks.
    #[test]
    fn backend_choice_as_str() {
        assert_eq!(BackendChoice::Metal.as_str(), "metal");
        assert_eq!(BackendChoice::Cuda.as_str(), "cuda");
        assert_eq!(BackendChoice::Vulkan.as_str(), "vulkan");
    }

    /// What this catches: layer_kinds_needing_check with MULTIPLE
    /// entries on a Vulkan + qwen3moe combo emits one UnsupportedLayer
    /// reason per kind. PR-3 surfaces every gap, not just the first.
    #[test]
    fn vulkan_qwen3_emits_one_unsupported_per_layer_kind() {
        let mut m = qwen3_30b_a3b_q4km();
        m.layer_kinds_needing_check = vec!["moe_gate".into(), "sliding_window_attn".into()];
        let mut hw = amd_with_vulkan_only();
        hw.free_vram_bytes = 64 * 1024 * 1024 * 1024; // enough VRAM; only layer issues
        let result = check_residency_gate(&m, &hw);
        let kinds: Vec<&str> = result
            .reasons()
            .iter()
            .filter_map(|r| match r {
                BlockReason::UnsupportedLayer { layer_kind, .. } => Some(layer_kind.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(kinds.len(), 2);
        assert!(kinds.contains(&"moe_gate"));
        assert!(kinds.contains(&"sliding_window_attn"));
    }

    /// What this catches: empty layer_kinds_needing_check NEVER emits
    /// UnsupportedLayer regardless of backend. Default-case safety —
    /// models that don't declare tricky layers shouldn't be blocked.
    #[test]
    fn empty_layer_kinds_never_emits_unsupported() {
        let m = qwen25_7b_q4km();
        for hw in &[
            macbook_air_m2_8gb(),
            m5_pro_48gb(),
            blackwell_rtx_5090(),
            amd_with_vulkan_only(),
        ] {
            let result = check_residency_gate(&m, hw);
            for r in result.reasons() {
                assert!(
                    !matches!(r, BlockReason::UnsupportedLayer { .. }),
                    "empty layer_kinds emitted UnsupportedLayer on {}",
                    hw.platform
                );
            }
        }
    }

    /// What this catches: free_vram_bytes = 0 on a GPU-equipped host
    /// (another process holds all VRAM) blocks with PartialGpuSplit
    /// even for the smallest model. Probe (#1315) deadheads below 2GB
    /// at probe time; this catches the race where VRAM dropped between
    /// probe + gate.
    #[test]
    fn zero_free_vram_on_gpu_host_blocks_smallest_model() {
        let mut hw = m5_pro_48gb();
        hw.free_vram_bytes = 0;
        let mut tiny = qwen25_3b_q4km();
        tiny.parameter_count_billions = 0.5;
        let result = check_residency_gate(&tiny, &hw);
        assert!(!result.is_pass());
        assert!(result
            .reasons()
            .iter()
            .any(|r| matches!(r, BlockReason::PartialGpuSplit { .. })));
    }

    /// What this catches: a Pass returns an empty reasons slice. Lets
    /// callers iterate uniformly without conditional pattern-matching.
    #[test]
    fn pass_reasons_is_empty_slice() {
        let pass = check_residency_gate(&qwen25_7b_q4km(), &m5_pro_48gb());
        assert!(pass.is_pass());
        assert_eq!(pass.reasons(), &[] as &[BlockReason]);
    }

    /// What this catches: FP16 Qwen 7B estimate (~15GB) blocks on an
    /// 8GB Mac. Pins bytes_per_parameter_quantized's load-bearing role
    /// — dropping it would silently route FP16 onto undersized hosts.
    #[test]
    fn fp16_7b_blocks_on_8gb_mac() {
        let mut m = qwen25_7b_q4km();
        m.bytes_per_parameter_quantized = 2.0; // FP16
        let result = check_residency_gate(&m, &macbook_air_m2_8gb());
        assert!(!result.is_pass(), "FP16 7B on 5GB free must block");
    }

    /// What this catches: BlockReason::WrongBackendForPlatform variant
    /// exists in the type even if no current code path emits it.
    /// Defensive — future codepaths that force backend selection
    /// (e.g. user override) need this variant to surface the mismatch
    /// instead of a runtime panic. Variant must round-trip cleanly.
    #[test]
    fn wrong_backend_variant_serde_round_trips() {
        let r = BlockReason::WrongBackendForPlatform {
            platform: "macos-arm64-m2".into(),
            backend: BackendChoice::Cuda,
        };
        let j = serde_json::to_string(&r).unwrap();
        let back: BlockReason = serde_json::from_str(&j).unwrap();
        assert_eq!(r, back);
        assert!(j.contains("\"kind\":\"wrongBackendForPlatform\""));
    }

    /// What this catches: `is_pass()` helper agrees with the variant.
    /// Defensive — callers will use is_pass() instead of pattern-
    /// matching most of the time; if the helper drifts, the gate
    /// becomes a footgun.
    #[test]
    fn is_pass_matches_variant() {
        let p = check_residency_gate(&qwen25_7b_q4km(), &m5_pro_48gb());
        assert!(p.is_pass());
        assert_eq!(p.reasons().len(), 0);

        let b = check_residency_gate(&qwen25_7b_q4km(), &generic_dell_no_gpu());
        assert!(!b.is_pass());
        assert!(!b.reasons().is_empty());
    }
