#![cfg(test)]
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // what this catches: the lazy recall embedder is safe to CONSTRUCT at boot —
    // no GPU/gateway probe fires until the first `embed` — and reports a stable
    // "resolving" identity + dim 0 until then, so nothing on the boot path can
    // wedge the IPC socket bind (the concurrency-guide non-negotiable this exists
    // to honor). Resolution itself needs the model registry, exercised live.
    #[test]
    fn lazy_recall_embedder_is_boot_safe_before_resolution() {
        let e = LazyRecallEmbedder::new();
        assert_eq!(e.id(), "recall-embedder(resolving)");
        assert_eq!(e.dim(), 0);
        assert!(e.unrelated_null().is_none());
    }

    // what this catches: the persistent snapshot survives a "restart" — vectors
    // written by snapshot_to load back byte-identical via load_from, so cached
    // content never re-embeds (nor re-fights the serving lane for VRAM) across a
    // core restart. A missing file warms as Ok(0), never an error (first boot).
    #[test]
    fn embedding_cache_snapshot_round_trips() {
        let src = EmbeddingCache::new();
        let ka = EmbeddingCache::key("qwen3-embedding-0.6b", "the deploy went red");
        let kb = EmbeddingCache::key("qwen3-embedding-0.6b", "a heron in the shallows");
        let va = vec![0.1f32, -0.2, 0.3, 0.4];
        let vb = vec![1.0f32, 2.0, 3.0];
        src.map.insert(ka, va.clone());
        src.map.insert(kb, vb.clone());

        let path = std::env::temp_dir().join("continuum-embcache-roundtrip.bin");
        let _ = std::fs::remove_file(&path);
        assert_eq!(src.snapshot_to(&path).unwrap(), 2);

        // A fresh cache (the "restarted" process) warms from the snapshot.
        let dst = EmbeddingCache::new();
        assert_eq!(dst.load_from(&path).unwrap(), 2);
        assert_eq!(dst.map.get(&ka).map(|e| e.clone()), Some(va));
        assert_eq!(dst.map.get(&kb).map(|e| e.clone()), Some(vb));

        // Missing file (first boot) → Ok(0), not an error.
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            dst.load_from(&path).unwrap(),
            0,
            "missing snapshot warms as Ok(0)"
        );
    }

    // what this catches: the cosine of identical text is ~1; orthogonal
    // (no shared vocabulary) is ~0. The relevance primitive is sane.
    #[tokio::test]
    async fn identical_text_is_maximally_similar() {
        let e = LexicalEmbedder::new();
        let a = e
            .embed("the deploy pipeline went red after the migration")
            .await;
        let same = e
            .embed("the deploy pipeline went red after the migration")
            .await;
        assert!(cosine_similarity(&a, &same) > 0.999);
    }

    // what this catches: topical relevance — a query about the rollout plan is
    // MORE similar to the rollout memory than to an unrelated lunch memory, even
    // though both are equally "old". This is relevance, not recency.
    #[tokio::test]
    async fn relevant_text_outscores_unrelated_text() {
        let e = LexicalEmbedder::new();
        let query = e
            .embed("what was our rollout plan for the auth flow?")
            .await;
        let relevant = e
            .embed("we will ship the auth flow behind a feature flag and ramp the rollout to 10%")
            .await;
        let unrelated = e
            .embed("lunch is at noon, someone booked the corner table")
            .await;
        let rel = cosine_similarity(&query, &relevant);
        let unrel = cosine_similarity(&query, &unrelated);
        assert!(
            rel > unrel,
            "relevant memory must score higher: relevant={rel:.3} unrelated={unrel:.3}"
        );
        assert!(rel > 0.0);
    }

    // what this catches: a zero/empty embedding or length mismatch yields 0.0,
    // never a panic or NaN — recall must degrade to "no signal", not crash.
    #[test]
    fn degrades_safely() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0]), 0.0);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
        // Non-finite components → 0.0 ("no signal"), NEVER NaN — a NaN here
        // would poison the arbiter sort (every partial_cmp becomes Equal).
        assert_eq!(cosine_similarity(&[f32::NAN, 1.0], &[1.0, 1.0]), 0.0);
        assert_eq!(cosine_similarity(&[f32::INFINITY, 1.0], &[1.0, 1.0]), 0.0);
        assert!(cosine_similarity(&[f32::NAN, 1.0], &[1.0, 1.0]).is_finite());
    }

    // what this catches: embeddings are REPRODUCIBLE across calls (FNV, not the
    // randomized DefaultHasher) — required for replay/record-recreate to work.
    #[tokio::test]
    async fn embeddings_are_reproducible() {
        let e = LexicalEmbedder::new();
        assert_eq!(
            e.embed("reproducible vectors").await,
            e.embed("reproducible vectors").await
        );
    }

    /// An embedder that counts how many times it actually computed — to prove
    /// the cache prevents recomputation.
    struct CountingEmbedder {
        id: &'static str,
        calls: AtomicUsize,
        batches: AtomicUsize,
    }
    impl CountingEmbedder {
        fn new(id: &'static str) -> Self {
            Self {
                id,
                calls: AtomicUsize::new(0),
                batches: AtomicUsize::new(0),
            }
        }
    }
    #[async_trait]
    impl EmbeddingProvider for CountingEmbedder {
        fn id(&self) -> &str {
            self.id
        }
        fn dim(&self) -> usize {
            4
        }
        async fn embed(&self, text: &str) -> Vec<f32> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            // Deterministic per-content vector (content length spread over dims).
            let n = text.len() as f32;
            vec![n, n + 1.0, n + 2.0, n + 3.0]
        }
        async fn embed_many(&self, texts: &[String]) -> Vec<Vec<f32>> {
            self.batches.fetch_add(1, Ordering::SeqCst);
            texts.iter().map(|t| {
                let n = t.len() as f32;
                vec![n, n + 1.0, n + 2.0, n + 3.0]
            }).collect()
        }
    }

    // what this catches (card d157d382, the 5090 2026-10-10): a gene signature minted text by
    // text, one embed call (and one lease) per training example, ~75 min of a 113-example
    // dispatch. Through the cache, a mint of N texts is ONE batched inner call, cached texts
    // never reach the inner embedder, and the result is in order.
    #[tokio::test]
    async fn a_signature_mint_embeds_its_corpus_in_one_batch_through_the_cache() {
        let inner = Arc::new(CountingEmbedder::new("counting"));
        let cached = CachingEmbeddingProvider::with_cache(inner.clone(), Arc::new(EmbeddingCache::new()));
        let warm = cached.embed("already seen").await;
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(cached);
        let texts: Vec<String> = ["already seen", "a", "bb", "ccc"].iter().map(|s| s.to_string()).collect();
        let corpus = crate::genome::corpus_ref::CorpusRef {
            name: "test-corpus".into(),
            content_hash: "sha256:0000000000000000000000000000000000000000000000000000000000000000".into(),
            size_bytes: 4,
            source_url: None,
        };
        crate::genome::signature::GeneSignature::mint(&texts, corpus, &embedder, 1_000)
            .await
            .expect("test: mint");
        assert_eq!(inner.batches.load(Ordering::SeqCst), 1, "the corpus is embedded in one batched call");
        assert_eq!(inner.calls.load(Ordering::SeqCst), 1, "only the warm-up went one by one");
        let again = embedder.embed_many(&texts).await;
        assert_eq!(inner.batches.load(Ordering::SeqCst), 1, "a second pass is all cache hits");
        assert_eq!(again[0], warm, "in order, the cached vector first");
        assert_eq!(again[3], vec![3.0, 4.0, 5.0, 6.0]);
    }

    // what this catches: THE OPTIMIZATION — the same content is embedded ONCE;
    // the second call (any persona) is a cache hit, the inner embedder is NOT
    // re-invoked. This is the compute-once-per-content / 14-personas-reuse-one win.
    #[tokio::test]
    async fn cache_computes_once_and_reuses() {
        let inner = Arc::new(CountingEmbedder::new("counting"));
        let cache = Arc::new(EmbeddingCache::new());
        let cached = CachingEmbeddingProvider::with_cache(inner.clone(), cache);

        let a = cached.embed("the deploy went red").await;
        let b = cached.embed("the deploy went red").await; // any persona, same content
        assert_eq!(a, b, "cache returns the identical vector");
        assert_eq!(
            inner.calls.load(Ordering::SeqCst),
            1,
            "inner embedder computed ONCE despite two embed calls"
        );
    }

    // what this catches: distinct content is genuinely recomputed (the cache
    // isn't returning a stale vector for new text).
    #[tokio::test]
    async fn cache_recomputes_distinct_content() {
        let inner = Arc::new(CountingEmbedder::new("counting"));
        let cache = Arc::new(EmbeddingCache::new());
        let cached = CachingEmbeddingProvider::with_cache(inner.clone(), cache);
        let _ = cached.embed("first message").await;
        let _ = cached.embed("a different message").await;
        assert_eq!(inner.calls.load(Ordering::SeqCst), 2);
    }

    /// An embedder that fails the first time (empty = "no signal", the down-embedder
    /// sentinel) then succeeds — to prove a transient failure is never cached.
    struct FlakyEmbedder {
        calls: AtomicUsize,
    }
    #[async_trait]
    impl EmbeddingProvider for FlakyEmbedder {
        fn id(&self) -> &str {
            "flaky"
        }
        fn dim(&self) -> usize {
            4
        }
        async fn embed(&self, _text: &str) -> Vec<f32> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Vec::new() // first call: embedder "down" → no-signal sentinel
            } else {
                vec![1.0, 2.0, 3.0, 4.0] // recovered
            }
        }
    }

    // what this catches: CACHE POISONING. A failed embed (empty no-signal vector)
    // must NOT be cached — otherwise a transient embedder hiccup becomes a permanent
    // zero-relevance verdict for that content across every persona, never retried.
    // Regression here = one bad embed silently blinding recall to a memory forever.
    #[tokio::test]
    async fn failed_embed_is_not_cached_and_retries() {
        let inner = Arc::new(FlakyEmbedder {
            calls: AtomicUsize::new(0),
        });
        let cache = Arc::new(EmbeddingCache::new());
        let cached = CachingEmbeddingProvider::with_cache(inner.clone(), cache);

        let first = cached.embed("the embedder is down right now").await;
        assert!(first.is_empty(), "first embed failed → no-signal sentinel");

        // Same content again: because the failure was NOT cached, the inner embedder
        // is re-invoked (now recovered) instead of returning the poisoned empty.
        let second = cached.embed("the embedder is down right now").await;
        assert_eq!(
            second,
            vec![1.0, 2.0, 3.0, 4.0],
            "retry recomputed a real vector — the failure was not cached"
        );
        assert_eq!(
            inner.calls.load(Ordering::SeqCst),
            2,
            "inner embedder was retried, proving the empty was never cached"
        );
    }

    // what this catches: the probe gate — only a non-empty, non-zero, all-finite
    // vector counts as "neural is live". Empty / all-zero / NaN all mean "no
    // signal" → fall back to lexical. Regression here = embedding every memory
    // into a zero vector (recall returns nothing) instead of using lexical.
    #[test]
    fn probe_gate_distinguishes_usable_from_no_signal() {
        assert!(probe_indicates_usable(&[0.0, 0.1, 0.0]));
        assert!(!probe_indicates_usable(&[]), "empty = no signal");
        assert!(!probe_indicates_usable(&[0.0, 0.0]), "all-zero = no signal");
        assert!(!probe_indicates_usable(&[f32::NAN, 1.0]), "NaN = no signal");
        assert!(
            !probe_indicates_usable(&[f32::INFINITY, 1.0]),
            "Inf = no signal"
        );
    }

    /// Minimal adapter whose embeddings are configurable, to drive the resolver
    /// without a network. `embeds` = the vector create_embedding returns (empty
    /// simulates "model not loaded"); `supports` = the capability flag.
    struct FakeEmbedAdapter {
        supports: bool,
        embeds: Vec<f32>,
    }
    #[async_trait]
    impl AIProviderAdapter for FakeEmbedAdapter {
        fn provider_id(&self) -> &str {
            "fake"
        }
        fn name(&self) -> &str {
            "fake"
        }
        fn default_model(&self) -> &str {
            "fake-model"
        }
        fn capabilities(&self) -> crate::ai::adapter::AdapterCapabilities {
            let mut capabilities = std::collections::BTreeSet::new();
            if self.supports {
                capabilities.insert(crate::model_registry::Capability::Embedding);
            }
            crate::ai::adapter::AdapterCapabilities {
                capabilities,
                ..Default::default()
            }
        }
        async fn generate_text(
            &self,
            _request: crate::ai::types::TextGenerationRequest,
        ) -> Result<crate::ai::types::TextGenerationResponse, String> {
            Err("not used".into())
        }
        async fn create_embedding(
            &self,
            _request: crate::ai::types::EmbeddingRequest,
        ) -> Result<crate::ai::types::EmbeddingResponse, String> {
            Ok(crate::ai::types::EmbeddingResponse {
                embeddings: vec![self.embeds.clone()],
                model: "fake-model".into(),
                provider: "fake".into(),
                usage: crate::ai::types::UsageMetrics {
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    estimated_cost: None,
                },
                response_time_ms: 0,
            })
        }
    }

    // what this catches: when the adapter serves real embeddings, the live path
    // resolves to the NEURAL embedder (semantic recall), not lexical. id() of a
    // neural-backed provider is the model slug.
    #[tokio::test]
    async fn resolver_picks_neural_when_embed_model_serves() {
        let adapter = Arc::new(FakeEmbedAdapter {
            supports: true,
            embeds: vec![0.1, 0.2, 0.3],
        });
        let embedder = resolve_recall_embedder(adapter).await;
        assert_eq!(
            embedder.id(),
            CANONICAL_EMBED_MODEL,
            "neural embedder's id is the model slug"
        );
    }

    // what this catches: THE NO-SIGNAL GUARD — an adapter that claims embedding
    // support but returns an empty vector (model not loaded) must fall back to
    // lexical, NOT embed everything into nothing.
    #[tokio::test]
    async fn resolver_falls_back_to_lexical_when_probe_empty() {
        let adapter = Arc::new(FakeEmbedAdapter {
            supports: true,
            embeds: vec![],
        });
        let embedder = resolve_recall_embedder(adapter).await;
        assert_eq!(embedder.id(), "lexical-fnv-tf", "must fall back to lexical");
    }

    // what this catches: an adapter with no embedding capability skips the probe
    // entirely and uses lexical — no wasted round-trip, always a working embedder.
    #[tokio::test]
    async fn resolver_uses_lexical_when_adapter_lacks_embeddings() {
        let adapter = Arc::new(FakeEmbedAdapter {
            supports: false,
            embeds: vec![0.1],
        });
        let embedder = resolve_recall_embedder(adapter).await;
        assert_eq!(embedder.id(), "lexical-fnv-tf");
    }

    // what this catches: vectors from DIFFERENT embedding spaces (provider ids)
    // never collide in the cache — keying includes provider_id, so a lexical
    // vector is never served to a neural query (cosine across spaces is nonsense).
    #[tokio::test]
    async fn cache_keys_by_embedding_space() {
        let cache = Arc::new(EmbeddingCache::new());
        let lexical = CachingEmbeddingProvider::with_cache(
            Arc::new(CountingEmbedder::new("space-a")),
            cache.clone(),
        );
        let neural = CachingEmbeddingProvider::with_cache(
            Arc::new(CountingEmbedder::new("space-b")),
            cache.clone(),
        );
        let _ = lexical.embed("same text").await;
        let _ = neural.embed("same text").await;
        assert_eq!(
            cache.len(),
            2,
            "same text in two embedding spaces = two distinct cache entries"
        );
    }

    // what this catches: TRANSPORT IS INVISIBLE TO THE CACHE — "identity is the
    // model, transport is the policy" (IntelMac/BigMama contract). Two DIFFERENT
    // provider impls (e.g. local NeuralEmbeddingProvider + cross-grid
    // GridEmbeddingProvider) that serve the SAME model return the SAME
    // provider_id, so a vector computed by one is reused by the other — same
    // model, same space, same cache key. The grid round-trip never fires for
    // content the local path already embedded.
    #[tokio::test]
    async fn cache_shared_across_transports_with_same_model_slug() {
        let cache = Arc::new(EmbeddingCache::new());
        // Two distinct impls, SAME model slug — stand-ins for Neural (local) and
        // Grid (cross-grid), both serving qwen3-embedding-0.6b.
        let local = Arc::new(CountingEmbedder::new("qwen3-embedding-0.6b"));
        let grid = Arc::new(CountingEmbedder::new("qwen3-embedding-0.6b"));
        let neural = CachingEmbeddingProvider::with_cache(local.clone(), cache.clone());
        let grid_provider = CachingEmbeddingProvider::with_cache(grid.clone(), cache.clone());

        let a = neural.embed("the deploy went red").await; // local path computes + caches
        let b = grid_provider.embed("the deploy went red").await; // grid path hits the SAME entry
        assert_eq!(a, b, "same model slug → same cache entry across transports");
        assert_eq!(local.calls.load(Ordering::SeqCst), 1, "local computed once");
        assert_eq!(
            grid.calls.load(Ordering::SeqCst),
            0,
            "grid round-trip never fired — the local-cached vector was reused"
        );
        assert_eq!(
            cache.len(),
            1,
            "one entry: identity is the model, not the transport"
        );
    }

    // what this catches: the in-process neural embedder actually PRODUCES semantic
    // vectors — runtime proof the resolve_recall_embedder neural path works end to
    // end (registry lookup → LlamaCppAdapter → backend.embed → cosine), not just
    // that it compiles. Similar text must rank above unrelated text; a lexical or
    // broken embedder fails this. Exercises the real production path:
    // local_embed_adapter() → try_neural_embedder().
    // #[ignore]: needs the Qwen3-Embedding-0.6B GGUF on disk; CI without it skips.
    // Run: cargo test -p continuum-core --features metal,accelerate,test-fixtures \
    //   --lib cognition::embedding::tests::in_process_embedder_ranks_semantic_similarity \
    //   -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn in_process_embedder_ranks_semantic_similarity() {
        // local_embed_adapter() reads the process-wide registry; seed it (idempotent
        // — ignore "already initialized" when run alongside other tests).
        let _ = crate::model_registry::init_global();

        let (adapter, model_id) =
            local_embed_adapter().expect("embed GGUF must be on disk for this real-model test");
        eprintln!("[embed-test] loaded in-process embed adapter: {model_id}");

        let embedder = try_neural_embedder(adapter, CANONICAL_EMBED_MODEL)
            .await
            .expect("neural embedder probe must succeed with a real embed model on disk");

        let anchor = embedder
            .embed("the deployment failed with a compile error")
            .await;
        let similar = embedder
            .embed("the build broke because the code did not compile")
            .await;
        let different = embedder
            .embed("she watered the tomato plants in the garden")
            .await;

        assert!(
            !anchor.is_empty(),
            "real embed must return a non-empty vector"
        );
        eprintln!(
            "[embed-test] dim={} (canonical={})",
            anchor.len(),
            CANONICAL_EMBED_DIM
        );

        let sim = cosine_similarity(&anchor, &similar);
        let dif = cosine_similarity(&anchor, &different);
        eprintln!("[embed-test] cosine(similar)={sim:.4}  cosine(different)={dif:.4}");

        assert!(
            sim > dif,
            "semantically similar text must rank above unrelated text: {sim} !> {dif}"
        );
        assert!(sim > 0.5, "similar pair should be clearly related: {sim}");
    }

    // what this catches: the rung-1 load-probe-fail storm (BigMama's Windows
    // node, 2026-08-02 — 102 llama_context creations × ~1200 MiB, zero IPC).
    // A failed shared resolve must consume exactly ONE attempt and every
    // subsequent resolve inside the cooldown must return fast WITHOUT
    // re-attempting a model load. Runs with no model registry installed, so
    // rung 1 fails at `local_embed_adapter` — the cheap flavor of the same
    // failure path the storm rode.
    #[tokio::test]
    async fn failed_rung1_resolve_does_not_reattempt_within_cooldown() {
        let attempts_before = rung1_cell().lock().await.attempts;
        let a = shared_in_process_embedder(CANONICAL_EMBED_MODEL).await;
        let b = shared_in_process_embedder(CANONICAL_EMBED_MODEL).await;
        let c = shared_in_process_embedder(CANONICAL_EMBED_MODEL).await;
        let attempts_after = rung1_cell().lock().await.attempts;
        // In a bare test process there is no registry → all resolves fail…
        assert!(a.is_none() && b.is_none() && c.is_none());
        // …and the cooldown means three resolves cost at most ONE attempt.
        // (≤ rather than == : another test in the parallel suite may have
        // burned the process-global attempt first — the invariant is "no
        // storm", not "this test owns the counter".)
        assert!(
            attempts_after - attempts_before <= 1,
            "three back-to-back failed resolves must not attempt more than one load \
             (got {} new attempts)",
            attempts_after - attempts_before
        );
    }

    // what this catches (regression for the 2026-08-08 civilization-wide stall):
    // CachingEmbeddingProvider's overflow eviction self-deadlocked — the
    // `iter()` shard READ guard lived for the whole `if let` block (scrutinee
    // temporary lifetime), so `remove` parked forever on the same shard's WRITE
    // lock. Reachable ONLY when the cache is at EMBEDDING_CACHE_MAX, so it
    // slept until production filled 20k entries — then every novel embed hung
    // its task, recall embeds inside every turn, and all four citizens went
    // silent (sampled live: 100% parked in lock_exclusive_slow). This test
    // fills a PRIVATE cache to the cap and embeds one novel text under a
    // timeout: pre-fix it deadlocks (timeout fires), post-fix it evicts and
    // returns in microseconds.
    #[tokio::test]
    async fn full_cache_eviction_does_not_self_deadlock() {
        struct Tiny;
        #[async_trait::async_trait]
        impl EmbeddingProvider for Tiny {
            fn id(&self) -> &str {
                "tiny-test-embedder"
            }
            fn dim(&self) -> usize {
                2
            }
            async fn embed(&self, _text: &str) -> Vec<f32> {
                vec![0.5, -0.5]
            }
        }
        let cache = Arc::new(EmbeddingCache::new());
        for i in 0..EMBEDDING_CACHE_MAX {
            cache.map.insert(i as u64, vec![0.0, 0.0]);
        }
        let provider = CachingEmbeddingProvider::with_cache(Arc::new(Tiny), cache.clone());
        let v = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            provider.embed("a text the full cache has never seen"),
        )
        .await
        .expect("full-cache embed must not deadlock on its own eviction");
        assert_eq!(v, vec![0.5, -0.5]);
        assert!(
            cache.map.len() <= EMBEDDING_CACHE_MAX,
            "eviction kept the cache bounded"
        );
    }
