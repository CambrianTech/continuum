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

    // what this catches (the M5, 2026-11-25 17:02Z: Benchy 38 asked, 0 completed, four minds on one
    // Core): a running turn keeping the Core at equal need until it ends, so a short turn from another
    // mind waits out a long answer. Two minds at one urgency: A asks for a long answer, B for a short one
    // just after. Under the lane, B is stepped within her wait bound and finishes while A still decodes.
    //     cargo test -p continuum-engine --features llama/metal --lib a_short_turn_never -- --ignored --nocapture
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires qwen2.5-0.5b-instruct under ~/.continuum/genome/models; run explicitly"]
    async fn a_short_turn_never_waits_out_another_minds_long_one() {
        let path = dirs::home_dir().expect("test: a home").join(".continuum/genome/models/qwen2.5-0.5b-instruct/qwen2.5-0.5b-instruct-q4_k_m.gguf"); // expect: the test's own precondition
        if !path.exists() {
            eprintln!("SKIPPED: {} not present", path.display());
            return;
        }
        let backend = LlamaCppBackend::load(LlamaCppConfig { model_path: path, ..Default::default() }).expect("test: the 0.5B loads"); // expect: the test's own precondition
        let allocation = GenerationAllocation::new(std::num::NonZeroU32::new(2048).expect("test: nonzero")); // expect: the test's own precondition
        let mut state = backend.start_scheduler(allocation).expect("test: the scheduler starts"); // expect: the test's own precondition
        state.wait_for(|state| state.ready().is_some()).await.expect("test: the engine becomes ready"); // expect: the test's own precondition
        let turn = |text: &'static str, tokens: usize| {
            let sampler = backend.turn_sampler(None, &crate::inference::backends::SamplingConfig::chat(), None).expect("test: a sampler"); // expect: the test's own precondition
            let prompt = TurnPrompt { text, stable_prefix: None, urgency: continuum_sage::component::Urgency::Want, request: None };
            backend.submit_for_persona(allocation, Some(uuid::Uuid::new_v4()), prompt, tokens, sampler, &[], &[]).expect("test: the turn submits") // expect: the test's own precondition
        };
        let mut long = turn("<|im_start|>user\nWrite a long story about a lighthouse keeper, in detail.<|im_end|>\n<|im_start|>assistant\n", 256);
        long.until_first_token().await; // her turn is running: the next turn arrives while she decodes
        let short = turn("<|im_start|>user\nSay hi.<|im_end|>\n<|im_start|>assistant\n", 8);
        // the order the two completion events arrive in, never a stopwatch
        let finished = std::sync::atomic::AtomicU8::new(0);
        let rank = || finished.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (long, short) = tokio::join!(async { (long.collect().await, rank()) }, async { (short.collect().await, rank()) });
        let ((long_answer, long_rank), (short_answer, short_rank)) = (long, short);
        let (long_answer, short_answer) = (long_answer.expect("test: the long turn answers"), short_answer.expect("test: the short turn answers")); // expect: the test's own precondition
        eprintln!("long: {} tokens in {:?} (engine); short: {} tokens in {:?} (engine); finished first: {}", long_answer.facts.generated, long_answer.facts.elapsed, short_answer.facts.generated, short_answer.facts.elapsed, if short_rank < long_rank { "short" } else { "long" });
        assert!(short_rank < long_rank, "the short turn finished while the long one still decoded");
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
        struct Heard(tokio::sync::mpsc::UnboundedSender<Arc<Said>>);

        impl Stage for Heard {
            type Input = Event<Said>;
            const QOS: Qos = Qos::Background;
            async fn handle(&mut self, event: Event<Said>) {
                if self.0.send(event.value).is_err() {
                    eprintln!("the test stopped hearing learning/said");
                }
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

        async fn next(heard: &mut tokio::sync::mpsc::UnboundedReceiver<Arc<Said>>) -> Said {
            Arc::unwrap_or_clone(tokio::time::timeout(std::time::Duration::from_secs(120), heard.recv()).await.expect("test: the engine says within 120 s").expect("test: the stream is open"))
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
            let port: &dyn LearnPort = &backend;
            assert!(port.tell(Learning::Measure(Measure { seat, mind: continuum_persona_engine::persona::PersonaId(her), shadow: ShadowSource::Fresh(fresh), optimizer, sized_for: lesson() })).is_ok());
            match next(&mut heard).await {
                Said::Seat { state: SeatState::Measured { weights }, .. } => assert!(weights.iter().map(|row| row.bytes).sum::<u64>() > 0, "the weights are measured"),
                other => panic!("expected Measured, heard {other:?}"),
            }
            // Out of order: a Seat before its Load is rejected, and the seat keeps what it holds.
            assert!(port.tell(Learning::Seat(Seat { seat, lease: Box::new(Counted::default()) })).is_ok());
            assert!(matches!(next(&mut heard).await, Said::Seat { state: SeatState::Kept { .. }, .. }), "a Seat before its Load is kept, never ended");
            assert!(port.tell(Learning::Load { seat }).is_ok());
            match next(&mut heard).await {
                Said::Seat { state: SeatState::Loaded { optimizer, workspace, .. }, .. } => assert!(!optimizer.is_empty() && !workspace.is_empty(), "optimizer and workspace measured"),
                other => panic!("expected Loaded, heard {other:?}"),
            }
            assert!(port.tell(Learning::Load { seat }).is_ok());
            assert!(matches!(next(&mut heard).await, Said::Seat { state: SeatState::Kept { .. }, .. }), "a second Load is kept, never ended");
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

            let generation = || std::env::temp_dir().join(format!("learn-port-validator-{}", seat.0));
            assert!(port.tell(Learning::Checkpoint { seat, into: generation(), binding: "validator".into() }).is_ok());
            match next(&mut heard).await {
                Said::Generation { outcome: GenerationOutcome::Saved { steps, .. }, .. } => assert!(steps >= 4, "the generation holds every step: {steps}"),
                other => panic!("expected Saved, heard {other:?}"),
            }
            assert!(port.tell(Learning::Release { seat }).is_ok());
            assert!(matches!(next(&mut heard).await, Said::Seat { state: SeatState::Released, .. }), "released");
            // A shadow loaded but never seated is released whole, and a seat never told is released too.
            let loaded = SeatId(uuid::Uuid::new_v4());
            let fresh = crate::inference::llama_server::FreshShadow { name: "loaded".into(), rank: 4, alpha: 8.0, targets: vec!["attn_q".into()], top_layers: 0, seed: 7 };
            assert!(port.tell(Learning::Measure(Measure { seat: loaded, mind: continuum_persona_engine::persona::PersonaId(her), shadow: ShadowSource::Fresh(fresh), optimizer, sized_for: lesson() })).is_ok());
            assert!(matches!(next(&mut heard).await, Said::Seat { state: SeatState::Measured { .. }, .. }));
            assert!(port.tell(Learning::Load { seat: loaded }).is_ok());
            assert!(matches!(next(&mut heard).await, Said::Seat { state: SeatState::Loaded { .. }, .. }));
            // A repeated Measure whose lesson cannot render (no reply of hers) on that loaded seat keeps it.
            let unrenderable = Lesson { messages: vec![serde_json::json!({"role": "user", "content": "no reply"})], tools: None };
            let refit = crate::inference::llama_server::FreshShadow { name: "loaded".into(), rank: 4, alpha: 8.0, targets: vec!["attn_q".into()], top_layers: 0, seed: 7 };
            assert!(port.tell(Learning::Measure(Measure { seat: loaded, mind: continuum_persona_engine::persona::PersonaId(her), shadow: ShadowSource::Fresh(refit), optimizer, sized_for: unrenderable })).is_ok());
            assert!(matches!(next(&mut heard).await, Said::Seat { state: SeatState::Kept { .. }, .. }), "a malformed repeat Measure on a loaded seat keeps it, never ends it");
            assert!(port.tell(Learning::Release { seat: loaded }).is_ok());
            assert!(matches!(next(&mut heard).await, Said::Seat { state: SeatState::Released, .. }), "a loaded shadow is released");
            assert!(port.tell(Learning::Release { seat: SeatId(uuid::Uuid::new_v4()) }).is_ok());
            assert!(matches!(next(&mut heard).await, Said::Seat { state: SeatState::Released, .. }), "a seat never told holds nothing");
            if let Err(kept) = std::fs::remove_dir_all(generation()) {
                eprintln!("the validator's generation stays at {}: {kept}", generation().display());
            }
        }

        // what this catches (Codex on c2d7a457 and 67af8fb2): a learn step that re-decoded her served tail
        // under the shadow, so she then served from KV it computed; a hybrid's learn refused because her
        // recurrent state was past the served prefix; and her prefix prefilled again under genes she was
        // not served with. She serves under a learned gene that changes her answers, her history runs on
        // past an older lesson, she learns it (Learned twice), and her next turn under the same gene
        // answers exactly as a mind that never learned answers the same history (greedy). She reuses her
        // served prefix, more than nothing: never a cold replay. (How much the mind that never learned
        // reuses differs by architecture: a hybrid resumes only at a checkpoint.)
        //     cargo test -p continuum-engine --features metal --lib an_older_lesson -- --ignored --nocapture
        //     CONTINUUM_LEARN_MODEL=<a hybrid GGUF> for the recurrent path
        #[tokio::test(flavor = "multi_thread")]
        #[ignore = "requires qwen2.5-0.5b-instruct under ~/.continuum/genome/models, or CONTINUUM_LEARN_MODEL; run explicitly"]
        async fn an_older_lesson_learned_under_her_genes_leaves_her_answering_as_a_mind_that_never_learned() {
            let default = || dirs::home_dir().expect("test: a home").join(".continuum/genome/models/qwen2.5-0.5b-instruct/qwen2.5-0.5b-instruct-q4_k_m.gguf");
            let path = std::env::var_os("CONTINUUM_LEARN_MODEL").map_or_else(default, std::path::PathBuf::from);
            if !path.exists() {
                eprintln!("SKIPPED: {} not present", path.display());
                return;
            }
            eprintln!("model: {}", path.display());
            let backend = LlamaCppBackend::load(LlamaCppConfig { model_path: path, ..Default::default() }).expect("test: the model loads");
            let nonzero = |n| std::num::NonZeroU32::new(n).expect("test: nonzero");
            let allocation = GenerationAllocation::new(nonzero(backend.capabilities().n_ctx_train.min(4096)));
            let mut state = backend.start_scheduler(allocation).expect("test: the scheduler starts");
            state.wait_for(|state| state.ready().is_some()).await.expect("test: the engine becomes ready");
            let (tx, mut heard) = tokio::sync::mpsc::unbounded_channel();
            let (input, _worker) = continuum_sage::spawn_stage(Heard(tx), continuum_sage::Capacity::granted(std::num::NonZeroUsize::new(64).expect("test: nonzero")));
            let _subscribed = continuum_sage::component::node().subscribe::<Said>(LEARNING_SAID).attach(&input, std::num::NonZeroUsize::new(64).expect("test: nonzero"));
            let port: &dyn LearnPort = &backend;

            // a serving gene that changes answers: a shadow learned for a throwaway mind, saved
            let gene_dir = std::env::temp_dir().join(format!("served-gene-{}", uuid::Uuid::new_v4()));
            learned(port, &mut heard, Learns { mind: uuid::Uuid::new_v4(), steps: 3, into: Some(&gene_dir) }).await;
            backend.ensure_adapter("served", &gene_dir.join("adapter.gguf")).expect("test: the gene loads");
            let served = [("served".to_string(), 1.0_f32)];

            // her view as the lesson renders it (they share a prefix), then a view sharing nothing with it
            // (Codex on #6260: a served prefix of 0 with no checkpoint there)
            for (view, shares) in [("<|im_start|>user\nWhat is the capital of Kansas?<|im_end|>\n", true), ("Notes on the river and the mill beside it.\n", false)] {
                // greedy, under the gene, her view kept as a checkpoint at its end
                let turn = |who: uuid::Uuid, text: &str| {
                    let sampler = backend.turn_sampler(None, &crate::inference::backends::SamplingConfig::code(), None).expect("test: a sampler");
                    let prompt = TurnPrompt { text, stable_prefix: Some(view), urgency: continuum_sage::component::Urgency::Want, request: None };
                    backend.submit_for_persona(allocation, Some(who), prompt, 16, sampler, &[], &served).expect("test: the turn submits")
                };
                let (her, never) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
                let ask = |history: &str, said: &str, question: &str| format!("{history}{said}<|im_end|>\n<|im_start|>user\n{question}<|im_end|>\n<|im_start|>assistant\n");
                let first = format!("{view}<|im_start|>user\nAnswer in one word.<|im_end|>\n<|im_start|>assistant\n");
                let a1 = turn(her, &first).collect().await.expect("test: her first turn answers");
                let _ = turn(never, &first).collect().await.expect("test: the other mind's first turn answers");
                let second = ask(&first, &a1.text, "And of Kentucky?");
                let a2 = turn(her, &second).collect().await.expect("test: her second turn answers");
                let _ = turn(never, &second).collect().await.expect("test: the other mind's second turn answers");

                // she learns the older lesson while her history has run on past it
                learned(port, &mut heard, Learns { mind: her, steps: 2, into: None }).await;

                let third = ask(&second, &a2.text, "And of Ohio?");
                let hers = turn(her, &third).collect().await.expect("test: her next turn answers");
                let theirs = turn(never, &third).collect().await.expect("test: the other mind's next turn answers");
                eprintln!("view {view:?}: after learning {:?} (reused {} of {}); never learned {:?} (reused {} of {})", hers.text, hers.facts.reused_tokens, hers.facts.prompt_tokens, theirs.text, theirs.facts.reused_tokens, theirs.facts.prompt_tokens);
                assert_eq!(hers.text, theirs.text, "under the same gene she answers as a mind that never learned");
                // sharing a prefix with the lesson, she resumes from it: never a cold replay. (On the single
                // Core an attention-only mind keeps the tail she held past the lesson and reuses it too; a
                // hybrid resumes at her served prefix and prefills the rest under her genes.)
                assert!(!shares || hers.facts.reused_tokens > 0, "she reuses her served prefix when she has one");
            }
            if let Err(kept) = std::fs::remove_dir_all(&gene_dir) {
                eprintln!("the served gene stays at {}: {kept}", gene_dir.display());
            }
        }

        /// What a shadow learns: whose, how many steps (each must say Learned), and where its generation
        /// is saved, when it is.
        struct Learns<'a> {
            mind: uuid::Uuid,
            steps: usize,
            into: Option<&'a std::path::Path>,
        }

        /// A fresh shadow seated, learned and saved as `learns` says, then released.
        async fn learned(port: &dyn LearnPort, heard: &mut tokio::sync::mpsc::UnboundedReceiver<Arc<Said>>, learns: Learns<'_>) {
            let Learns { mind, steps, into } = learns;
            let seat = SeatId(uuid::Uuid::new_v4());
            let fresh = crate::inference::llama_server::FreshShadow { name: format!("shadow-{}", seat.0), rank: 8, alpha: 16.0, targets: vec!["attn_q".into(), "attn_v".into()], top_layers: 0, seed: 42 };
            let optimizer = llama::Optimizer::AdamW { learning_rate: 5e-3, beta1: 0.9, beta2: 0.999, eps: 1e-8, weight_decay: 0.0 };
            assert!(port.tell(Learning::Measure(Measure { seat, mind: continuum_persona_engine::persona::PersonaId(mind), shadow: ShadowSource::Fresh(fresh), optimizer, sized_for: lesson() })).is_ok());
            assert!(matches!(next(heard).await, Said::Seat { state: SeatState::Measured { .. }, .. }));
            assert!(port.tell(Learning::Load { seat }).is_ok());
            assert!(matches!(next(heard).await, Said::Seat { state: SeatState::Loaded { .. }, .. }));
            assert!(port.tell(Learning::Seat(Seat { seat, lease: Box::new(Counted::default()) })).is_ok());
            assert!(matches!(next(heard).await, Said::Seat { state: SeatState::Seated { .. }, .. }));
            for _ in 0..steps {
                let step = StepId(uuid::Uuid::new_v4());
                assert!(port.tell(Learning::Learn(crate::inference::learning::Learn { seat, step, lesson: lesson(), admission: Box::new(Open) })).is_ok());
                match next(heard).await {
                    Said::Step { outcome: StepOutcome::Learned { .. }, .. } => {}
                    other => panic!("expected Learned, heard {other:?}"),
                }
            }
            if let Some(into) = into {
                assert!(port.tell(Learning::Checkpoint { seat, into: into.to_path_buf(), binding: "served".into() }).is_ok());
                assert!(matches!(next(heard).await, Said::Generation { outcome: GenerationOutcome::Saved { .. }, .. }), "the gene is saved");
            }
            assert!(port.tell(Learning::Release { seat }).is_ok());
            assert!(matches!(next(heard).await, Said::Seat { state: SeatState::Released, .. }));
        }
    }

    // what this catches (the M5, 2026-11-26: every act tokenized her whole prompt, then her whole view
    // again, 1,974 + 1,691 ms over 39 acts): her view held inside her prompt tokenized once with the
    // rest after it must give exactly the tokens the whole prompt does, and her view's end, or her next
    // act's prefix stops matching what she holds.
    #[test]
    #[ignore = "requires qwen2.5-0.5b-instruct under ~/.continuum/genome/models; run explicitly"]
    fn a_held_view_tokenizes_once_with_the_rest_as_the_whole_prompt_does() {
        let path = dirs::home_dir().expect("test: a home").join(".continuum/genome/models/qwen2.5-0.5b-instruct/qwen2.5-0.5b-instruct-q4_k_m.gguf"); // expect: the test's own precondition
        if !path.exists() {
            eprintln!("SKIPPED: {} not present", path.display());
            return;
        }
        let backend = LlamaCppBackend::load(LlamaCppConfig { model_path: path, ..Default::default() }).expect("test: the 0.5B loads"); // expect: the test's own precondition
        let generation = "<|im_start|>assistant\n";
        let prompt = format!("<|im_start|>system\nYou are Benchy, a resident.<|im_end|>\n<|im_start|>user\nRead core/src/lib.rs and say what it owns. 9b: ünïcode — fine.<|im_end|>\n{generation}");
        let view = &prompt[..prompt.len() - generation.len()];
        let whole = backend.model.tokenize(&prompt, true, true).expect("test: the prompt tokenizes"); // expect: the test's own precondition
        let held_view = backend.model.tokenize(view, true, true).expect("test: the view tokenizes"); // expect: the test's own precondition
        let (tokens, view_end) = crate::inference::backends::persona_scheduler::prepare(&backend.model, &prompt, Some(view)).expect("test: prepare"); // expect: the test's own precondition
        assert_eq!(tokens, whole, "tokenized in two halves at the generation prompt, the same tokens as whole");
        assert_eq!(view_end, Some(held_view.len()), "her view ends where her view's own tokens do");
        let apart = String::from(view);
        let (tokens_apart, end_apart) = crate::inference::backends::persona_scheduler::prepare(&backend.model, &prompt, Some(&apart)).expect("test: prepare"); // expect: the test's own precondition
        assert_eq!((tokens_apart, end_apart), (whole, Some(held_view.len())), "a view rendered apart takes the other path to the same answer");
    }
