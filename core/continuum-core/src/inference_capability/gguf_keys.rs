//! Canonical GGUF header-key readers — the ONE place each well-known GGUF
//! metadata key is coerced from the artifact.
//!
//! Before this module, `general.architecture`, the `context_length`
//! fallback chain, `general.parameter_count`, and friends were hand-read
//! with `md.get("...").and_then(|v| v.to_string().ok()).cloned()` in six
//! different files (`model_registry::hydrate`, `model_registry::arch_config`,
//! `gguf_loader`, `inference::backends::mod`, ...). That is the compression
//! anti-pattern: one logical decision ("how do I read key X from a GGUF")
//! smeared across many places, free to drift. It DID drift — the
//! architecture-specific-`context_length`-then-`llama.context_length`
//! fallback that older exporters need was present in `hydrate` and
//! `backends` but silently ABSENT in `arch_config`, so a GGUF carrying only
//! `llama.context_length` hydrated its Model row fine yet failed dimension
//! extraction.
//!
//! Each function here reads exactly one canonical key (or key policy) and
//! returns `Option` — a MISSING or wrong-typed key is `None`, never a
//! guessed default. Callers own the required-vs-optional decision: the
//! registry-row hydrator leaves an absent field for the catalog to fill; a
//! dimension/residency reader wraps the `Option` in its own `.ok_or_else`
//! with a precise "missing required key" error. The coercion mechanics live
//! here; the fatality policy stays with the consumer.
//!
//! This module carries NO model-family knowledge — no name table, no size
//! guess. It reads the artifact's own self-description. Translating the
//! architecture STRING into the registry's closed `Arch` enum is a separate
//! decision that lives with the enum (`model_registry::hydrate::
//! arch_from_gguf_string`); this module only surfaces the raw string.

use candle_core::quantized::gguf_file::Content;

/// `general.architecture` — the model's own declared architecture string
/// (e.g. `"qwen3"`, `"llama"`). Required for correctness by most readers
/// (it keys every `{arch}.*` dimension), but returned as `Option` so each
/// caller applies its own fatality policy. Absent only for a broken export.
pub fn architecture(ct: &Content) -> Option<String> {
    ct.metadata
        .get("general.architecture")
        .and_then(|v| v.to_string().ok())
        .cloned()
}

/// `general.name` — the human-facing model name. Optional everywhere:
/// callers that need a name fall back to the file stem, since the name is
/// display-only and never gates correctness.
pub fn general_name(ct: &Content) -> Option<String> {
    ct.metadata
        .get("general.name")
        .and_then(|v| v.to_string().ok())
        .cloned()
}

/// The model's trained context window, in tokens.
///
/// The ONE fallback policy, defined once: prefer the architecture-specific
/// `{arch}.context_length`, then the historical `llama.context_length` that
/// some older exporters wrote regardless of the actual architecture. A
/// reader that has this key under only the `llama.*` name must still resolve
/// it — encoding that here is the whole reason this function exists.
pub fn context_length(ct: &Content, arch: &str) -> Option<u32> {
    ct.metadata
        .get(&format!("{arch}.context_length"))
        .or_else(|| ct.metadata.get("llama.context_length"))
        .and_then(|v| v.to_u32().ok())
}

/// `{arch}.block_count` — the transformer layer count. Keyed under the
/// model's own architecture; required by dimension + residency readers,
/// surfaced as `Option` so they own the "missing → refuse" error.
pub fn block_count(ct: &Content, arch: &str) -> Option<u32> {
    ct.metadata
        .get(&format!("{arch}.block_count"))
        .and_then(|v| v.to_u32().ok())
}

/// `{arch}.expert_count` — the total number of routed experts in an MoE
/// model (e.g. 896 for a K3-class model, 128 for a Qwen3-MoE). `None` on a
/// dense model that never wrote the key — the caller's cue to treat the
/// artifact as non-MoE and skip expert paging entirely. Keyed under the
/// model's own architecture, like every other dimension; carries no
/// family knowledge.
pub fn expert_count(ct: &Content, arch: &str) -> Option<u32> {
    ct.metadata
        .get(&format!("{arch}.expert_count"))
        .and_then(|v| v.to_u32().ok())
}

/// `{arch}.expert_used_count` — the number of experts activated per token
/// (top-k routing; e.g. 16-of-896 for a K3-class model, 8-of-128 for a
/// Qwen3-MoE). Present only on MoE artifacts; `None` on a dense model.
/// Distinct from `expert_count`: this is the ACTIVE set size per forward
/// pass — the number that sets the paging working-set floor, since at least
/// this many experts must be resident to serve one token. The full
/// `expert_count` is what the splitter carves onto the Frozen tier.
pub fn expert_used_count(ct: &Content, arch: &str) -> Option<u32> {
    ct.metadata
        .get(&format!("{arch}.expert_used_count"))
        .and_then(|v| v.to_u32().ok())
}

/// `general.parameter_count` — the model's own declared total parameter
/// count. A `u64` in the spec (a 7B model already overflows `u32`). Absent
/// for exporters that omit it; a caller that needs a number either takes a
/// catalog override or derives loudly from file size — never silently here.
pub fn parameter_count(ct: &Content) -> Option<u64> {
    ct.metadata
        .get("general.parameter_count")
        .and_then(|v| v.to_u64().ok())
}

/// `tokenizer.chat_template` — the Jinja chat template baked into the
/// artifact. Optional: an absent template means the caller supplies one.
pub fn chat_template(ct: &Content) -> Option<String> {
    ct.metadata
        .get("tokenizer.chat_template")
        .and_then(|v| v.to_string().ok())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::quantized::gguf_file::{self, Value};
    use std::io::Cursor;

    /// Assemble an in-memory GGUF from a metadata list and read it back into a
    /// `Content` — the only way to exercise the readers against real
    /// candle-parsed metadata without a fixture file on disk. No tensors: a
    /// metadata-only GGUF is enough to test key readers.
    fn content_with(md: Vec<(&str, Value)>) -> Content {
        let refs: Vec<(&str, &Value)> = md.iter().map(|(k, v)| (*k, v)).collect();
        let mut buf = Cursor::new(Vec::new());
        gguf_file::write(&mut buf, &refs, &[]).unwrap();
        buf.set_position(0);
        Content::read(&mut buf).unwrap()
    }

    // what this catches: the architecture string is read from the artifact's
    // own `general.architecture`, and a broken export missing it yields None
    // (the caller's cue to refuse) — not a guessed "llama".
    #[test]
    fn architecture_reads_the_key_and_absent_is_none() {
        let ct = content_with(vec![(
            "general.architecture",
            Value::String("qwen3".to_string()),
        )]);
        assert_eq!(architecture(&ct).as_deref(), Some("qwen3"));

        let empty = content_with(vec![]);
        assert_eq!(architecture(&empty), None);
    }

    // what this catches: THE reason this module exists — the context_length
    // fallback policy is defined once. A GGUF carrying only the historical
    // `llama.context_length` (no `{arch}.context_length`) must still resolve,
    // so every reader inherits the fallback instead of arch_config silently
    // lacking it.
    #[test]
    fn context_length_falls_back_to_the_llama_key() {
        // Architecture-specific key wins when present.
        let specific = content_with(vec![
            ("qwen3.context_length", Value::U32(262144)),
            ("llama.context_length", Value::U32(4096)),
        ]);
        assert_eq!(context_length(&specific, "qwen3"), Some(262144));

        // Only the historical llama.* key present → still resolves.
        let legacy = content_with(vec![("llama.context_length", Value::U32(32768))]);
        assert_eq!(context_length(&legacy, "qwen3"), Some(32768));

        // Neither present → None (caller refuses).
        let neither = content_with(vec![]);
        assert_eq!(context_length(&neither, "qwen3"), None);
    }

    // what this catches: parameter_count is read wide (u64) so a 7B+ count
    // doesn't truncate, and an omitting exporter yields None rather than 0 —
    // 0 is the registry's "absent, hydrate me" sentinel, so a real 0 here
    // would be indistinguishable from missing.
    #[test]
    fn parameter_count_is_u64_and_absent_is_none() {
        let ct = content_with(vec![(
            "general.parameter_count",
            Value::U64(30_000_000_000),
        )]);
        assert_eq!(parameter_count(&ct), Some(30_000_000_000));

        let empty = content_with(vec![]);
        assert_eq!(parameter_count(&empty), None);
    }

    // what this catches: block_count is keyed under the model's OWN
    // architecture, not a hardcoded family — a qwen3 GGUF's layers live under
    // `qwen3.block_count`, a llama GGUF's under `llama.block_count`, and the
    // reader composes the key from the passed arch with zero name knowledge.
    #[test]
    fn block_count_is_keyed_under_the_passed_architecture() {
        let ct = content_with(vec![("qwen3.block_count", Value::U32(64))]);
        assert_eq!(block_count(&ct, "qwen3"), Some(64));
        assert_eq!(block_count(&ct, "llama"), None, "wrong arch key → None");
    }

    // what this catches: the MoE layout the Seam-1 paging splitter reads — how
    // many experts to carve onto the Frozen tier (expert_count) and how many
    // must stay resident per token (expert_used_count, the working-set floor).
    // Both keyed under the model's OWN arch (a K3-class 896/16 lives under the
    // K3 arch string, a Qwen3-MoE's under its own), and BOTH absent on a dense
    // model → None, which is the splitter's cue that there is nothing to page.
    #[test]
    fn expert_layout_is_keyed_under_arch_and_dense_is_none() {
        let moe = content_with(vec![
            ("qwen3moe.expert_count", Value::U32(896)),
            ("qwen3moe.expert_used_count", Value::U32(16)),
        ]);
        assert_eq!(expert_count(&moe, "qwen3moe"), Some(896));
        assert_eq!(expert_used_count(&moe, "qwen3moe"), Some(16));
        assert_eq!(expert_count(&moe, "llama"), None, "wrong arch key → None");

        // A dense model wrote neither key → None, never a guessed 0/1 — the
        // splitter must distinguish "not MoE" from "MoE with zero experts".
        let dense = content_with(vec![("llama.block_count", Value::U32(32))]);
        assert_eq!(expert_count(&dense, "llama"), None);
        assert_eq!(expert_used_count(&dense, "llama"), None);
    }
}
