#![cfg(test)]
    use super::*;

    /// what this catches: the `ffn_moe_topk-<il>` name parse — the cheap per-node filter
    /// that turns llama.cpp's graph-callback tensor name into a layer index. A regression
    /// (matching the wrong node, or mis-parsing the suffix) either drops the PGO signal
    /// silently or mis-attributes experts to the wrong layer. The FFI callback is a thin
    /// wrapper over this pure fn.
    #[test]
    fn parse_moe_topk_layer_extracts_block_index() {
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk-0"), Some(0));
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk-31"), Some(31));
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk-895"), Some(895)); // K3 has ~896
        // bare name (no per-layer suffix) → can't attribute a layer → skip
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk"), None);
        // wrong node → None (the callback fires for EVERY node; only topk carries selection)
        assert_eq!(parse_moe_topk_layer("ffn_moe_gate-3"), None);
        assert_eq!(parse_moe_topk_layer("attn_norm-2"), None);
        // malformed suffix → None, never a panic
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk-"), None);
        assert_eq!(parse_moe_topk_layer("ffn_moe_topk-abc"), None);
    }

    /// what this catches: the `ExpertObserver` sink contract — the callback hands
    /// `(layer, selected expert ids)` straight through. A `Send + Sync + Debug` mock
    /// records them; this pins the trait shape the continuum-side residency tally consumes.
    #[test]
    fn expert_observer_receives_layer_and_selected_experts() {
        use std::sync::Mutex;
        #[derive(Debug, Default)]
        struct RecordingObserver {
            seen: Mutex<Vec<(u32, Vec<i32>)>>,
        }
        impl ExpertObserver for RecordingObserver {
            fn observe(&self, layer: u32, experts: &[i32], _n_expert_used: usize) {
                self.seen.lock().unwrap().push((layer, experts.to_vec()));
            }
        }
        let obs = RecordingObserver::default();
        // Two tokens, top-2 at layer 5 (experts [3,7] then [3,1], flattened), then layer 6.
        obs.observe(5, &[3, 7, 3, 1], 2);
        obs.observe(6, &[0, 2], 2);
        let seen = obs.seen.lock().unwrap();
        assert_eq!(seen.as_slice(), &[(5, vec![3, 7, 3, 1]), (6, vec![0, 2])]);
    }

    #[test]
    fn sampler_greedy_builds_and_drops() {
        let _s = Sampler::greedy();
    }

    #[test]
    fn sampler_chain_builds_and_drops() {
        let _s = Sampler::chain()
            .top_k(40)
            .top_p(0.9, 1)
            .temp(0.8)
            .dist(42)
            .build();
    }

    #[test]
    fn batch_for_tokens_roundtrip() {
        let b = Batch::for_tokens(vec![1, 2, 3, 4]);
        assert_eq!(b.n_tokens(), 4);
    }

    #[test]
    fn batch_allocated_push_clear() {
        let mut b = Batch::allocated(8, 1);
        assert_eq!(b.n_tokens(), 0);
        b.push(42, 0, &[0], true);
        b.push(43, 1, &[0], true);
        assert_eq!(b.n_tokens(), 2);
        b.clear();
        assert_eq!(b.n_tokens(), 0);
    }
