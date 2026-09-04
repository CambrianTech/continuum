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

use candle_core::quantized::gguf_file::{Content, Value};

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

/// `{arch}.attention.head_count_kv` in its PER-LAYER array form — the
/// artifact's own declaration of per-layer KV variance. `Some(vec)` only when
/// the key is an ARRAY (hybrid recurrent models — kimi-linear, kimi-k3,
/// jamba — where zeros mark recurrent layers with no per-token KV); `None`
/// for the scalar form (uniform models) or a missing key. Callers must not
/// fabricate a uniform vec from the scalar: scalar-vs-array IS the signal.
/// A zero in this array is also the honest, name-free marker of a GDN/SSM
/// hybrid whose fused ops cannot span CPU/GPU buffers (5090 issue 3, #238).
pub fn attention_head_count_kv_per_layer(ct: &Content, arch: &str) -> Option<Vec<u32>> {
    match ct
        .metadata
        .get(&format!("{arch}.attention.head_count_kv"))?
    {
        Value::Array(items) => items.iter().map(|v| v.to_u32().ok()).collect(),
        _ => None,
    }
}

/// `{arch}.attention.head_count_kv` in its SCALAR form — uniform models.
/// `None` for the per-layer array form (see
/// [`attention_head_count_kv_per_layer`]) or a missing key.
pub fn attention_head_count_kv_scalar(ct: &Content, arch: &str) -> Option<u32> {
    match ct
        .metadata
        .get(&format!("{arch}.attention.head_count_kv"))?
    {
        Value::Array(_) => None,
        v => v.to_u32().ok(),
    }
}

/// `{arch}.attention.head_count` — the query-head count. Needed only as the
/// divisor when deriving head_dim from `embedding_length` (exporters that
/// omit `attention.key_length`).
pub fn attention_head_count(ct: &Content, arch: &str) -> Option<u32> {
    ct.metadata
        .get(&format!("{arch}.attention.head_count"))
        .and_then(|v| v.to_u32().ok())
}

/// `{arch}.attention.key_length` — the per-head key dimension, written by
/// exporters whose head_dim ≠ embedding/heads (GQA with widened heads, MLA).
pub fn attention_key_length(ct: &Content, arch: &str) -> Option<u32> {
    ct.metadata
        .get(&format!("{arch}.attention.key_length"))
        .and_then(|v| v.to_u32().ok())
}

/// `{arch}.attention.value_length` — per-head value dimension; absent means
/// "same as key_length" for every exporter observed.
pub fn attention_value_length(ct: &Content, arch: &str) -> Option<u32> {
    ct.metadata
        .get(&format!("{arch}.attention.value_length"))
        .and_then(|v| v.to_u32().ok())
}

/// `{arch}.embedding_length` — the model width; head_dim fallback divisor.
pub fn embedding_length(ct: &Content, arch: &str) -> Option<u32> {
    ct.metadata
        .get(&format!("{arch}.embedding_length"))
        .and_then(|v| v.to_u32().ok())
}

/// The artifact's OWN f16 KV bytes/token — Σ over layers of
/// `kv_heads × (key_len + value_len) × 2 bytes` — from the header's declared
/// geometry, no family knowledge, no size heuristic.
///
/// Why this exists (measured 2026-09-01): the planner's `weights/80_000`
/// KV-rate heuristic read a 35B fine-grained MoE at ~244 KB/token where the
/// artifact's real geometry is ~61 KB f16 (~30 KB served at q8_0, confirmed
/// against llama-server's own prompt-cache entry sizes). Expert weights add
/// ZERO KV, so any weights-scaled guess over-charges MoE models ~4×; the
/// inflation starved the host prompt cache to one mind's worth (686 MiB) AND
/// capped the lane grant below resident demand — one wrong constant, both
/// symptoms. The header knows; ask the header.
///
/// Per-layer `head_count_kv` arrays (hybrid recurrent models) are summed with
/// zeros counting zero — recurrent layers hold no per-token KV, which is
/// exactly what their zero declares.
pub fn kv_bytes_per_token_f16(ct: &Content, arch: &str) -> Option<u64> {
    let key_len = attention_key_length(ct, arch)
        .or_else(|| {
            let width = embedding_length(ct, arch)?;
            let heads = attention_head_count(ct, arch)?;
            (heads > 0).then(|| width / heads)
        })? as u64;
    let value_len = attention_value_length(ct, arch).map(|v| v as u64).unwrap_or(key_len); // unwrap_or: absent value_length means symmetric heads — every observed exporter's convention, not a guess over a present-but-unreadable key
    let per_head = (key_len + value_len).saturating_mul(2); // f16 = 2 bytes/element
    let total_kv_heads: u64 = match attention_head_count_kv_per_layer(ct, arch) {
        Some(per_layer) => per_layer.into_iter().map(|h| h as u64).sum(),
        None => {
            let uniform = attention_head_count_kv_scalar(ct, arch)? as u64;
            uniform.saturating_mul(block_count(ct, arch)? as u64)
        }
    };
    let rate = total_kv_heads.saturating_mul(per_head);
    (rate > 0).then_some(rate)
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

/// `{arch}.leading_dense_block_count` — the number of LEADING transformer
/// blocks that are dense (no routed experts) in an otherwise-MoE model.
/// DeepSeek-family models run their first few layers dense; Qwen3-MoE runs
/// every layer routed and simply omits the key. `None` therefore means "the
/// artifact declares no dense lead" — the arch's own self-description, not
/// a guessed default. MoE layer count = `block_count − leading_dense`.
pub fn leading_dense_block_count(ct: &Content, arch: &str) -> Option<u32> {
    ct.metadata
        .get(&format!("{arch}.leading_dense_block_count"))
        .and_then(|v| v.to_u32().ok())
}

/// `{arch}.expert_shared_count` — always-active shared experts per MoE
/// layer (DeepSeek/GLM style). Shared experts are RESIDENT weights: they
/// run for every token, so they belong with the trunk, never in the routed
/// expert cache or its working-set arithmetic. Absent (Qwen3-MoE, K3-class
/// routed-only layouts) means zero shared experts by the artifact's own
/// declaration.
pub fn expert_shared_count(ct: &Content, arch: &str) -> Option<u32> {
    ct.metadata
        .get(&format!("{arch}.expert_shared_count"))
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

    // what this catches: the KV rate comes from the artifact's DECLARED
    // geometry, never a weights-scaled guess (2026-09-01: the guess read a
    // 35B MoE at ~4× its real rate — experts add weights, zero KV — which
    // starved --cache-ram to one mind and under-granted lanes). Three shapes:
    // uniform scalar GQA, per-layer array with recurrent zeros (each zero
    // contributes zero KV), and the key_length-absent fallback (head_dim =
    // embedding/heads). Missing geometry = None, never a fabricated rate.
    #[test]
    fn kv_rate_reads_declared_geometry_for_all_three_header_shapes() {
        // Uniform GQA: 48 layers × 4 kv-heads × (128+128) dims × 2 bytes.
        let uniform = content_with(vec![
            ("qwen3.block_count", Value::U32(48)),
            ("qwen3.attention.head_count_kv", Value::U32(4)),
            ("qwen3.attention.key_length", Value::U32(128)),
        ]);
        assert_eq!(
            kv_bytes_per_token_f16(&uniform, "qwen3"),
            Some(48 * 4 * (128 + 128) * 2)
        );
        // Per-layer array with recurrent zeros: only the 2 attention layers
        // (8 heads each) hold KV; the zeros are SSM layers and count nothing.
        let hybrid = content_with(vec![
            ("kimi.block_count", Value::U32(4)),
            (
                "kimi.attention.head_count_kv",
                Value::Array(vec![
                    Value::U32(0),
                    Value::U32(8),
                    Value::U32(0),
                    Value::U32(8),
                ]),
            ),
            ("kimi.attention.key_length", Value::U32(64)),
        ]);
        assert_eq!(
            kv_bytes_per_token_f16(&hybrid, "kimi"),
            Some(16 * (64 + 64) * 2)
        );
        // key_length absent → head_dim = embedding / head_count.
        let derived = content_with(vec![
            ("llama.block_count", Value::U32(2)),
            ("llama.attention.head_count_kv", Value::U32(2)),
            ("llama.attention.head_count", Value::U32(32)),
            ("llama.embedding_length", Value::U32(4096)),
        ]);
        assert_eq!(
            kv_bytes_per_token_f16(&derived, "llama"),
            Some(2 * 2 * (128 + 128) * 2)
        );
        // No geometry at all → None (the weights heuristic stands upstream).
        let bare = content_with(vec![("x.block_count", Value::U32(10))]);
        assert_eq!(kv_bytes_per_token_f16(&bare, "x"), None);
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
