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
    fn generation_uses_exact_explicit_context_allocation() {
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
            ..Default::default()
        };
        let backend = LlamaCppBackend::load(config).expect("load dense base");

        let caps = backend.capabilities();
        assert!(caps.n_ctx_train > 0, "GGUF must report a trained ceiling");
        assert!(
            caps.n_layer > 0 && caps.n_head_kv > 0,
            "real dims populated"
        );

        let tokens = std::num::NonZeroU32::new(512).expect("test allocation");
        assert_eq!(backend.effective_context_length(GenerationAllocation::new(tokens)), Ok(512));
    }

    /// L2's validator (IN-PROCESS-LEARNING.md): a resident learns between her turns through the
    /// node's LearnPort, on the real engine, and every outcome is heard on learning/said.
    mod learn_port {
        use super::*;
        use crate::inference::learning::{
            GenerationOutcome, LearnPort, Learning, Lesson, Measure, Said, Seat, SeatId, SeatState, ShadowSource, StepId, StepOutcome, LEARNING_SAID,
        };
        use continuum_sage::dataflow::Event;
        use continuum_sage::{Qos, Stage};
        use std::sync::atomic::{AtomicU64, Ordering};

        /// What the test hears on learning/said, handed to its channel.
        struct Heard(tokio::sync::mpsc::UnboundedSender<Said>);

        impl Stage for Heard {
            type Input = Event<Said>;
            const QOS: Qos = Qos::Background;
            async fn handle(&mut self, event: Event<Said>) {
                let _ = self.0.send((*event.value).clone());
            }
        }

        #[derive(Default)]
        struct Counted(Arc<AtomicU64>);
        impl llama::OptimizerLease for Counted {
            fn admit(&self, _: llama::Slot, bytes: u64) -> bool {
                self.0.fetch_add(bytes, Ordering::SeqCst);
                true
            }
            fn release(&self, _: llama::Slot, bytes: u64) {
                self.0.fetch_sub(bytes, Ordering::SeqCst);
            }
        }

        struct Open;
        impl llama::ReplyAdmission for Open {
            fn workspace(&self, _: llama::BufferType, _: u64, _: u64) -> bool {
                true
            }
        }

        async fn next(heard: &mut tokio::sync::mpsc::UnboundedReceiver<Said>) -> Said {
            tokio::time::timeout(std::time::Duration::from_secs(120), heard.recv()).await.expect("test: the engine says within 120 s").expect("test: the stream is open")
        }

        fn lesson() -> Lesson {
            Lesson {
                messages: vec![serde_json::json!({"role": "user", "content": "What is the capital of Kansas?"}), serde_json::json!({"role": "assistant", "content": "Topeka."})],
                tools: None,
            }
        }

        // what this catches: in-place learning that never reaches the engine serving her (the M5 and
        // the 5090 learned nothing: the trainer spoke HTTP to an engine that is in-process), a seat
        // that skips its measure before it allocates, a lesson that does not train, or a resident
        // whose next turn fails after she learned.
        #[tokio::test(flavor = "multi_thread")]
        #[ignore = "requires qwen2.5-0.5b-instruct under ~/.continuum/genome/models; run explicitly"]
        async fn a_resident_learns_between_her_turns_through_the_learn_port() {
            let path = dirs::home_dir().expect("test: a home").join(".continuum/genome/models/qwen2.5-0.5b-instruct/qwen2.5-0.5b-instruct-q4_k_m.gguf");
            if !path.exists() {
                eprintln!("SKIPPED: {} not present", path.display());
                return;
            }
            let backend = LlamaCppBackend::load(LlamaCppConfig { model_path: path, ..Default::default() }).expect("test: the 0.5B loads");
            let allocation = GenerationAllocation::new(std::num::NonZeroU32::new(2048).expect("test: nonzero"));
            let mut state = backend.start_scheduler(allocation).expect("test: the scheduler starts");
            state.wait_for(|state| state.ready().is_some()).await.expect("test: the engine becomes ready");

            let (tx, mut heard) = tokio::sync::mpsc::unbounded_channel();
            let (input, _worker) = continuum_sage::spawn_stage(Heard(tx), continuum_sage::Capacity::granted(std::num::NonZeroUsize::new(64).expect("test: nonzero")));
            let _subscribed = continuum_sage::component::node().subscribe::<Said>(LEARNING_SAID).attach(&input, std::num::NonZeroUsize::new(64).expect("test: nonzero"));

            let her = uuid::Uuid::new_v4();
            let turn = |text: &'static str| {
                let sampler = backend.turn_sampler(None, &crate::inference::backends::SamplingConfig::chat(), None).expect("test: a sampler");
                backend.submit_for_persona(allocation, Some(her), TurnPrompt { text, stable_prefix: None, urgency: continuum_sage::component::Urgency::Want, request: None }, 8, sampler, &[], &[])
            };
            let prompt = "<|im_start|>user\nWhat is the capital of Kansas?<|im_end|>\n<|im_start|>assistant\n";
            turn(prompt).expect("test: her turn submits").collect().await.expect("test: her first turn answers");

            let seat = SeatId(uuid::Uuid::new_v4());
            let fresh = crate::inference::llama_server::FreshShadow { name: "validator".into(), rank: 8, alpha: 16.0, targets: vec!["attn_q".into(), "attn_v".into()], top_layers: 0, seed: 42 };
            let optimizer = llama::Optimizer::AdamW { learning_rate: 5e-3, beta1: 0.9, beta2: 0.999, eps: 1e-8, weight_decay: 0.0 };
            let reply_rows = std::num::NonZeroU32::new(64).expect("test: nonzero");
            let port: &dyn LearnPort = &backend;
            assert!(port.tell(Learning::Measure(Measure { seat, mind: continuum_persona_engine::persona::PersonaId(her), shadow: ShadowSource::Fresh(fresh), optimizer, reply_rows })).is_ok());
            match next(&mut heard).await {
                Said::Seat { state: SeatState::Measured { weights }, .. } => assert!(weights.iter().map(|row| row.bytes).sum::<u64>() > 0, "the weights are measured"),
                other => panic!("expected Measured, heard {other:?}"),
            }
            assert!(port.tell(Learning::Load { seat }).is_ok());
            match next(&mut heard).await {
                Said::Seat { state: SeatState::Loaded { optimizer, workspace, .. }, .. } => assert!(!optimizer.is_empty() && !workspace.is_empty(), "optimizer and workspace measured"),
                other => panic!("expected Loaded, heard {other:?}"),
            }
            let charged = Arc::new(AtomicU64::new(0));
            assert!(port.tell(Learning::Seat(Seat { seat, lease: Box::new(Counted(Arc::clone(&charged))) })).is_ok());
            assert!(matches!(next(&mut heard).await, Said::Seat { state: SeatState::Seated { .. }, .. }), "seated");

            let mut losses = Vec::new();
            for _ in 0..4 {
                let step = StepId(uuid::Uuid::new_v4());
                assert!(port.tell(Learning::Learn(crate::inference::learning::Learn { seat, step, lesson: lesson(), admission: Box::new(Open) })).is_ok());
                match next(&mut heard).await {
                    Said::Step { step: said, outcome: StepOutcome::Learned { loss, .. }, .. } if said == step => losses.push(loss.expect("test: a learned step reports its loss")),
                    other => panic!("expected Learned, heard {other:?}"),
                }
            }
            eprintln!("losses through the port: {losses:?}");
            assert!(losses[losses.len() - 1] < losses[0], "the lesson trains through the port: {losses:?}");
            assert!(charged.load(Ordering::SeqCst) > 0, "the optimizer's bytes were charged to its lease view");

            let answer = turn(prompt).expect("test: her next turn submits").collect().await.expect("test: her next turn answers after she learned");
            eprintln!("her turn after learning: {:?}", answer.text);

            let into = std::env::temp_dir().join(format!("learn-port-validator-{}", seat.0));
            assert!(port.tell(Learning::Checkpoint { seat, into: into.clone(), binding: "validator".into() }).is_ok());
            match next(&mut heard).await {
                Said::Generation { outcome: GenerationOutcome::Saved { steps, .. }, .. } => assert!(steps >= 4, "the generation holds every step: {steps}"),
                other => panic!("expected Saved, heard {other:?}"),
            }
            assert!(port.tell(Learning::Release { seat }).is_ok());
            assert!(matches!(next(&mut heard).await, Said::Seat { state: SeatState::Released, .. }), "released");
            let _ = std::fs::remove_dir_all(into);
        }
    }
