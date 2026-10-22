#![cfg(test)]
    use super::*;

    // what this catches: the KV-per-token cost is derived from the model's
    // REAL dimensions (the rule behind task #46), not a hardcoded "typical"
    // number. A GQA model (n_head_kv << n_head) must cost far less than its
    // query-head count implies — get the formula wrong and the memory budget
    // either OOMs or throttles. Worked example: n_embd=2048, n_head=16 →
    // head_dim=128; n_head_kv=2 → kv_width=256; F16=2B → 256*2 per K and per
    // V = 1024B/layer/token; ×36 layers = 36864 B/token.
    #[test]
    fn kv_bytes_per_token_from_real_dims() {
        let caps = ModelCapabilities::uniform(32768, 36, 16, 2, 2048);
        assert_eq!(
            caps.kv_bytes_per_token(KvCacheType::F16, KvCacheType::F16),
            36864
        );
        // Q8_0 K (1.0625 B/elem) + F16 V (2 B/elem): per layer/token =
        // 256*1.0625 + 256*2 = 272 + 512 = 784 → ×36 = 28224 B/token.
        // (Integer centibyte math: 256*106 + 256*200 = 27136+51200 = 78336
        // centibytes/layer ×36 /100 = 28200 — the .0625 rounds down in the
        // ×100 fixed point, which is the SAFE direction for a budget.)
        assert_eq!(
            caps.kv_bytes_per_token(KvCacheType::Q8_0, KvCacheType::F16),
            28200
        );
        // Degenerate guard: a model that reports zero heads can't be sized;
        // return 0 (caller falls back to the trained ceiling) rather than
        // dividing by zero.
        let bad = ModelCapabilities::uniform(0, 0, 0, 0, 0);
        assert_eq!(
            bad.kv_bytes_per_token(KvCacheType::F16, KvCacheType::F16),
            0
        );
    }

    // what this catches: BigMama's registered 5090 issue 2 (#238) — hybrid
    // KDA/MLA models must be KV-priced from their PER-LAYER geometry, never
    // scalar n_head_kv × n_layer. Fixture: the real Kimi-K3 UD-IQ2 GGUF
    // (scratchpad k3-ud-iq2-tensor-fixture.txt, gist 931517a3): 93 layers,
    // head_count_kv = [0,0,0,1]×23 + [1] (69 recurrent KDA layers with ZERO
    // per-token KV; 24 MLA layers caching ONE compressed head), key_length
    // 576 (kv_lora 512 + rope 64), value_length 74. Honest cost @F16 =
    // 24 × 1 × (576+74) × 2 = 31,200 B/token. The old scalar formula priced
    // this either at 0 (layer-0 KDA scalar) or ~2.6 MB/token (uniform-96-head
    // assumption) — both wrong by orders of magnitude, strangling or OOMing
    // the derived context window.
    #[test]
    fn kv_bytes_per_token_hybrid_kda_mla_per_layer() {
        let kv_layers: Vec<KvLayer> = (0..93u32)
            .map(|il| KvLayer {
                // The fixture's array: every 4th layer (3,7,…,91) plus the
                // final layer 92 is MLA (1 compressed KV head); rest are KDA.
                n_head_kv: if il % 4 == 3 || il == 92 { 1 } else { 0 },
                k_width: 576,
                v_width: 74,
            })
            .collect();
        assert_eq!(
            kv_layers.iter().filter(|l| l.n_head_kv > 0).count(),
            24,
            "fixture sanity: 24 MLA layers"
        );
        let caps = ModelCapabilities {
            n_ctx_train: 1_048_576,
            n_layer: 93,
            n_head: 96,
            n_head_kv: 0, // scalar summary (layer 0 is KDA) — display only
            n_embd: 7168,
            kv_layers,
        };
        assert_eq!(
            caps.kv_bytes_per_token(KvCacheType::F16, KvCacheType::F16),
            31_200
        );
    }

    // what this catches: with NO hand-set context_length, the backend must
    // derive a usable window from the model's real n_ctx_train bounded by real
    // memory — never panic (the old `.expect()`), never blindly allocate the
    // full ceiling (the 262144-token Metal OOM, 2026-04). Proves task #46
    // end-to-end against a real GGUF. Run:
    //   cargo test -p continuum-core --features metal,accelerate,test-fixtures \
    //     --lib inference::backends::llamacpp::tests::effective_context -- --ignored
    #[test]
    #[ignore = "requires the dense base GGUF on disk; run explicitly"]
    fn effective_context_derives_from_real_limits_without_override() {
        let path = dirs::home_dir()
            .unwrap()
            .join(".continuum/models/qwen2.5-coder-3b-instruct-f16.gguf");
        if !path.exists() {
            eprintln!("skip: {} not present", path.display());
            return;
        }
        // No context_length override — force the real-limit derivation path.
        let config = LlamaCppConfig {
            model_path: path,
            context_length: None,
            ..Default::default()
        };
        let backend = LlamaCppBackend::load(config).expect("load dense base");

        let caps = backend.capabilities();
        assert!(caps.n_ctx_train > 0, "GGUF must report a trained ceiling");
        assert!(
            caps.n_layer > 0 && caps.n_head_kv > 0,
            "real dims populated"
        );

        let ctx = backend.effective_context_length();
        // Derived, not panicked; within the model's real ceiling; usable.
        assert!(ctx >= 1024, "must give a usable window, got {ctx}");
        assert!(
            ctx <= caps.n_ctx_train,
            "must never exceed the model's real ceiling ({} > {})",
            ctx,
            caps.n_ctx_train
        );
        eprintln!(
            "effective_context_length={} (n_ctx_train={}, kv={}B/tok)",
            ctx,
            caps.n_ctx_train,
            caps.kv_bytes_per_token(KvCacheType::F16, KvCacheType::F16),
        );
    }
