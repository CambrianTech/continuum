//! Model-architecture probe for safe `n_seq_max > 1` (multi-seq
//! continuous batching) in `LlamaCppAdapter`.
//!
//! Joel (2026-05-31): "Key is low latency. It's everything especially
//! in video chat. And not stupid models." The prior-attempt failure
//! mode that this probe defends against is enabling multi-seq
//! continuous batching on an architecture that llama.cpp's Metal
//! graph aborts on — exactly the qwen3.5 / Gated-Delta-Net abort that
//! historically forced `n_seq_max = 1` everywhere.
//!
//! ### What the probe answers
//!
//! Given a GGUF's `general.architecture` string, return a typed
//! verdict:
//!
//! - `SafeForMultiSeq` — standard transformer (Llama, Qwen-2.5,
//!   Gemma-2, Mistral, …) — caller may set `n_seq_max > 1`.
//! - `SingleSeqOnly` — recurrent / state-space / hybrid
//!   architecture that llama.cpp's batched decode aborts on
//!   (qwen3, mamba, rwkv, jamba, …). Caller MUST stay at
//!   `n_seq_max = 1`; the adapter's load path enforces this as
//!   defense in depth.
//! - `Unknown` — architecture not in the curated list. Default to
//!   single-seq (the safe choice). When new architectures land,
//!   add them to the SAFE or UNSAFE list.
//!
//! ### Defense in depth
//!
//! Per the realistic-lane build plan, the adapter's `load()` calls
//! `probe_gguf_batching_safety()` and clamps `n_seq_max` to 1 if
//! the verdict is `SingleSeqOnly` (regardless of what the caller
//! configured). This is the substrate's safety net — coordinator
//! wiring can blindly pass `lane_budgets.max_concurrency` through
//! and the probe handles model-family safety.
//!
//! ### Doctrine alignment
//!
//! - [[inference-scarcity-economics]] §"prior attempt was rather
//!   shitty" — repeating the qwen3.5 abort is the exact failure
//!   mode this probe rules out by construction.
//! - [[observability-is-half-the-architecture]] — when the probe
//!   clamps, it emits a `tracing::warn` line so the operator sees
//!   the safety net firing, not silent quality loss.
//! - [[commands-are-kernel-level-and-compose]] — the probe is a
//!   pure classification function; the adapter is the only
//!   consumer. No command-level visibility (callers ask for N;
//!   the substrate decides).

use std::path::Path;

/// Verdict on whether a GGUF model can be safely served with
/// `n_seq_max > 1` (continuous batching across concurrent sequences
/// inside one shared `Context`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchingSafety {
    /// Architecture is a standard transformer that llama.cpp's
    /// batched decode multiplexes cleanly. Safe to set
    /// `n_seq_max > 1`.
    SafeForMultiSeq { arch: String },
    /// Architecture has a recurrent / state-space / Gated-Delta-Net
    /// layer that llama.cpp's Metal (and sometimes CUDA / CPU)
    /// graph aborts on with multi-seq batches. MUST stay
    /// `n_seq_max = 1`.
    SingleSeqOnly { arch: String, reason: String },
    /// Unknown architecture — defaults to single-seq (the safe
    /// choice). The arch string is preserved so operators can audit
    /// + extend the SAFE / UNSAFE lists.
    Unknown { arch: String },
}

impl BatchingSafety {
    /// True iff the verdict allows the caller's `n_seq_max > 1`.
    pub fn safe_for_multi_seq(&self) -> bool {
        matches!(self, BatchingSafety::SafeForMultiSeq { .. })
    }

    /// The original architecture string the probe classified, for
    /// telemetry + audit + extending the curated lists.
    pub fn arch(&self) -> &str {
        match self {
            BatchingSafety::SafeForMultiSeq { arch } => arch,
            BatchingSafety::SingleSeqOnly { arch, .. } => arch,
            BatchingSafety::Unknown { arch } => arch,
        }
    }

    /// Clamp a requested `n_seq_max` to the safe value per this
    /// verdict. SafeForMultiSeq returns `requested` unchanged;
    /// SingleSeqOnly / Unknown clamp to 1.
    pub fn clamp_n_seq_max(&self, requested: u32) -> u32 {
        if self.safe_for_multi_seq() {
            requested.max(1)
        } else {
            1
        }
    }
}

/// Standard transformer architectures known to multiplex cleanly
/// through llama.cpp's continuous-batching path. Strings match the
/// `general.architecture` GGUF metadata (lowercased before lookup).
///
/// Adding to this list: confirm the family ships through standard
/// attention (NOT recurrent / state-space / hybrid) AND that the
/// batched-decode path returns clean per-sequence finish reasons.
/// Test on at least one host class before expanding.
const SAFE_ARCHITECTURES: &[&str] = &[
    "llama",
    "llama2",
    "llama3",
    "llama4",
    "qwen",
    "qwen2",
    "qwen2.5",
    "qwen2moe",
    "qwen2_moe",
    "qwen2_vl",
    "gemma",
    "gemma2",
    "gemma3",
    "mistral",
    "mistral2",
    "mixtral",
    "phi",
    "phi2",
    "phi3",
    "phi3.5",
    "phimoe",
    "falcon",
    "bloom",
    "gpt2",
    "gptj",
    "gptneox",
    "starcoder",
    "starcoder2",
    "stablelm",
    "minicpm",
    "minicpm3",
    "olmo",
    "olmo2",
    "deepseek",
    "deepseek2",
    "deepseek3",
    "command-r",
    "commandr",
    "dbrx",
    "internlm2",
];

/// Recurrent / state-space / hybrid architectures known to abort or
/// produce garbage when llama.cpp decodes them with `n_seq_max > 1`.
/// Pair each arch with a human-readable reason (surfaced in logs +
/// the typed `SingleSeqOnly.reason` field).
///
/// Adding to this list: when a new recurrent / SSM family appears
/// and the batched-decode path is known broken, add the arch string
/// + a one-line reason. The substrate's safety net is only as good
/// as this list — keep it current.
const UNSAFE_ARCHITECTURES: &[(&str, &str)] = &[
    (
        "qwen3",
        "Gated-Delta-Net recurrent layer; llama.cpp Metal graph aborts on multi-seq batches",
    ),
    (
        "qwen3moe",
        "Gated-Delta-Net recurrent layer; llama.cpp Metal graph aborts on multi-seq batches",
    ),
    (
        "qwen3_moe",
        "Gated-Delta-Net recurrent layer; llama.cpp Metal graph aborts on multi-seq batches",
    ),
    (
        "mamba",
        "State-space recurrent (SSM); llama.cpp's multi-seq path not supported",
    ),
    (
        "mamba2",
        "State-space recurrent (SSM); llama.cpp's multi-seq path not supported",
    ),
    (
        "rwkv",
        "Recurrent attention-free; llama.cpp's multi-seq path not supported",
    ),
    (
        "rwkv6",
        "Recurrent attention-free; llama.cpp's multi-seq path not supported",
    ),
    (
        "rwkv7",
        "Recurrent attention-free; llama.cpp's multi-seq path not supported",
    ),
    (
        "jamba",
        "Hybrid Mamba+Transformer; llama.cpp's multi-seq path not supported",
    ),
    (
        "griffin",
        "Hybrid recurrent (Google Griffin); llama.cpp's multi-seq path not supported",
    ),
    (
        "recurrentgemma",
        "Recurrent variant of Gemma; llama.cpp's multi-seq path not supported",
    ),
    (
        "falcon_mamba",
        "Hybrid Falcon+Mamba; llama.cpp's multi-seq path not supported",
    ),
    (
        "falconmamba",
        "Hybrid Falcon+Mamba; llama.cpp's multi-seq path not supported",
    ),
];

/// Classify a `general.architecture` string from GGUF metadata.
/// Pure function — no I/O. The caller (the adapter's load path)
/// reads the metadata via `read_gguf_metadata` and passes the
/// architecture string here.
pub fn classify_architecture(arch: &str) -> BatchingSafety {
    let arch_lc = arch.to_ascii_lowercase();

    // UNSAFE wins over SAFE if both somehow match — defense in depth.
    for (unsafe_arch, reason) in UNSAFE_ARCHITECTURES {
        if arch_lc == *unsafe_arch {
            return BatchingSafety::SingleSeqOnly {
                arch: arch.to_string(),
                reason: (*reason).to_string(),
            };
        }
    }

    for safe_arch in SAFE_ARCHITECTURES {
        if arch_lc == *safe_arch {
            return BatchingSafety::SafeForMultiSeq {
                arch: arch.to_string(),
            };
        }
    }

    BatchingSafety::Unknown {
        arch: arch.to_string(),
    }
}

/// Probe a GGUF file's architecture + classify. Reads the
/// `general.architecture` metadata key (cheap — just the GGUF
/// header, no weights). Use this at adapter-load time as the
/// substrate's safety net before honoring `with_n_seq_max(N)`.
pub fn probe_gguf_batching_safety(path: &Path) -> Result<BatchingSafety, String> {
    let meta = crate::inference::backends::read_gguf_metadata(path)?;
    Ok(classify_architecture(&meta.architecture))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── safe architectures ─────────────────────────────────────

    #[test]
    fn standard_llama_classes_as_safe() {
        let v = classify_architecture("llama");
        assert!(v.safe_for_multi_seq());
        assert_eq!(v.arch(), "llama");
    }

    #[test]
    fn qwen2_classes_as_safe() {
        assert!(classify_architecture("qwen2").safe_for_multi_seq());
        assert!(classify_architecture("qwen2.5").safe_for_multi_seq());
        assert!(classify_architecture("qwen2_moe").safe_for_multi_seq());
    }

    #[test]
    fn gemma_family_classes_as_safe() {
        assert!(classify_architecture("gemma").safe_for_multi_seq());
        assert!(classify_architecture("gemma2").safe_for_multi_seq());
        assert!(classify_architecture("gemma3").safe_for_multi_seq());
    }

    #[test]
    fn safe_classification_is_case_insensitive() {
        assert!(classify_architecture("LLAMA").safe_for_multi_seq());
        assert!(classify_architecture("Qwen2.5").safe_for_multi_seq());
        assert!(classify_architecture("GeMmA2").safe_for_multi_seq());
    }

    // ── unsafe architectures ───────────────────────────────────

    #[test]
    fn qwen3_classes_as_single_seq_only_with_reason() {
        let v = classify_architecture("qwen3");
        assert!(!v.safe_for_multi_seq());
        match v {
            BatchingSafety::SingleSeqOnly { arch, reason } => {
                assert_eq!(arch, "qwen3");
                assert!(reason.contains("Gated-Delta-Net"));
            }
            other => panic!("expected SingleSeqOnly, got {other:?}"),
        }
    }

    #[test]
    fn qwen3moe_variants_class_as_single_seq() {
        assert!(!classify_architecture("qwen3moe").safe_for_multi_seq());
        assert!(!classify_architecture("qwen3_moe").safe_for_multi_seq());
        assert!(!classify_architecture("Qwen3MoE").safe_for_multi_seq());
    }

    #[test]
    fn mamba_family_classes_as_single_seq() {
        assert!(!classify_architecture("mamba").safe_for_multi_seq());
        assert!(!classify_architecture("mamba2").safe_for_multi_seq());
    }

    #[test]
    fn rwkv_family_classes_as_single_seq() {
        assert!(!classify_architecture("rwkv").safe_for_multi_seq());
        assert!(!classify_architecture("rwkv6").safe_for_multi_seq());
        assert!(!classify_architecture("rwkv7").safe_for_multi_seq());
    }

    #[test]
    fn hybrid_architectures_class_as_single_seq() {
        assert!(!classify_architecture("jamba").safe_for_multi_seq());
        assert!(!classify_architecture("griffin").safe_for_multi_seq());
        assert!(!classify_architecture("recurrentgemma").safe_for_multi_seq());
        assert!(!classify_architecture("falcon_mamba").safe_for_multi_seq());
        assert!(!classify_architecture("falconmamba").safe_for_multi_seq());
    }

    // ── unknown architectures ──────────────────────────────────

    #[test]
    fn unknown_architecture_classes_as_unknown_and_not_safe() {
        let v = classify_architecture("some-future-arch-2027");
        assert!(!v.safe_for_multi_seq());
        match v {
            BatchingSafety::Unknown { arch } => {
                assert_eq!(arch, "some-future-arch-2027");
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn empty_architecture_string_classes_as_unknown() {
        let v = classify_architecture("");
        assert!(!v.safe_for_multi_seq());
        match v {
            BatchingSafety::Unknown { arch } => assert!(arch.is_empty()),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    // ── clamp behavior ─────────────────────────────────────────

    #[test]
    fn clamp_passes_through_when_safe() {
        let v = BatchingSafety::SafeForMultiSeq {
            arch: "llama".to_string(),
        };
        assert_eq!(v.clamp_n_seq_max(4), 4);
        assert_eq!(v.clamp_n_seq_max(16), 16);
        // Zero clamps to 1 minimum.
        assert_eq!(v.clamp_n_seq_max(0), 1);
        assert_eq!(v.clamp_n_seq_max(1), 1);
    }

    #[test]
    fn clamp_forces_one_when_unsafe() {
        let v = BatchingSafety::SingleSeqOnly {
            arch: "qwen3".to_string(),
            reason: "test".to_string(),
        };
        assert_eq!(v.clamp_n_seq_max(4), 1);
        assert_eq!(v.clamp_n_seq_max(16), 1);
        assert_eq!(v.clamp_n_seq_max(0), 1);
    }

    #[test]
    fn clamp_forces_one_when_unknown() {
        let v = BatchingSafety::Unknown {
            arch: "future-thing".to_string(),
        };
        assert_eq!(v.clamp_n_seq_max(4), 1);
        assert_eq!(v.clamp_n_seq_max(16), 1);
    }

    // ── critical invariant: unsafe never reports safe ──────────

    #[test]
    fn every_unsafe_architecture_classifies_as_single_seq_only() {
        // Loop over the curated list — defense against accidentally
        // moving an arch from UNSAFE to SAFE without updating the
        // table.
        for (arch, _reason) in UNSAFE_ARCHITECTURES {
            let v = classify_architecture(arch);
            assert!(
                !v.safe_for_multi_seq(),
                "{arch} should be single-seq-only but classified as safe"
            );
        }
    }

    #[test]
    fn every_safe_architecture_classifies_as_safe_for_multi_seq() {
        for arch in SAFE_ARCHITECTURES {
            let v = classify_architecture(arch);
            assert!(
                v.safe_for_multi_seq(),
                "{arch} should be safe for multi-seq but classified as not safe"
            );
        }
    }
}
