//! The canonical model architecture struct — ONE source of truth for a
//! model's dimensions, read authoritatively from its artifact and passed
//! around, never re-derived from the model's name.
//!
//! # Why this exists
//!
//! A model's architecture dimensions (hidden size, FFN size, head counts,
//! layer count, vocab) are properties OF a specific model. They live in the
//! artifact: a GGUF's header keys, or an HF `config.json`. They are NOT
//! inferable from the model's name — `"…-32b"` in an id is a label a human
//! typed, not a contract, and a compacted / re-quantized / renamed model
//! breaks any name→dims table the moment it deviates from the convention it
//! was hand-fit to.
//!
//! So this struct is hydrated ONCE from the artifact ([`from_gguf`] /
//! [`from_config_json`]), hung on the [`Model`](super::types::Model) row, and
//! threaded to every consumer (plasticity compaction planning, the GGUF
//! writer, KV-budget estimation). When the served model is swapped on the fly
//! — an up/downgrade to fit problem difficulty or available VRAM — the row is
//! re-hydrated from the NEW artifact and every consumer follows, because
//! nobody guesses from the name: there is no stale guess to half-apply.
//!
//! # Failure discipline
//!
//! Required dimensions that are absent from the artifact are a hard error
//! naming the missing key — never a guessed default (same posture as
//! [`crate::inference_capability::gguf_loader`]). The only "derived" values
//! are `head_dim` and `num_kv_heads` when the artifact legitimately omits
//! them: those omissions carry a spec-defined MEANING (head_dim =
//! hidden_size / num_attention_heads; absent KV-head count = multi-head
//! attention, i.e. kv == q), so computing them is reading the spec, not
//! sniffing the name.

use candle_core::quantized::gguf_file;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A model's architecture dimensions — the canonical, artifact-sourced
/// model-specific struct. Constructed only via [`from_gguf`] /
/// [`from_config_json`] (authoritative) or [`new`] (from already-validated
/// dims); never from the model's name.
///
/// [`from_gguf`]: ModelArchConfig::from_gguf
/// [`from_config_json`]: ModelArchConfig::from_config_json
/// [`new`]: ModelArchConfig::new
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelArchConfig {
    pub num_layers: usize,
    pub hidden_size: usize,
    pub num_attention_heads: usize,
    pub num_kv_heads: usize,
    pub head_dim: usize,
    /// FFN hidden dimension.
    pub intermediate_size: usize,
    pub vocab_size: usize,
    /// Trained context window (positions). A model-specific fact that lives in
    /// the same artifact as the dims — the compaction planner budgets KV cache
    /// against it and the GGUF writer stamps it into the compacted header, so
    /// it rides the same carrier instead of being hardcoded per demo model.
    pub context_length: usize,
    /// GQA ratio: `num_attention_heads / num_kv_heads`. Cached at
    /// construction so consumers never recompute (and never divide by zero).
    pub gqa_ratio: usize,
}

impl ModelArchConfig {
    /// Construct from already-validated dimensions, computing `gqa_ratio`.
    ///
    /// Errors if `num_kv_heads` is zero (would divide by zero) or does not
    /// divide `num_attention_heads` (not a valid GQA grouping) — a corrupt
    /// dimension set is a fail-loud condition, not something to round.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        num_layers: usize,
        hidden_size: usize,
        num_attention_heads: usize,
        num_kv_heads: usize,
        head_dim: usize,
        intermediate_size: usize,
        vocab_size: usize,
        context_length: usize,
    ) -> Result<Self, String> {
        if num_kv_heads == 0 {
            return Err("model arch: num_kv_heads is 0".to_string());
        }
        if num_attention_heads % num_kv_heads != 0 {
            return Err(format!(
                "model arch: num_attention_heads ({num_attention_heads}) is not a multiple of \
                 num_kv_heads ({num_kv_heads}) — invalid GQA grouping"
            ));
        }
        Ok(Self {
            num_layers,
            hidden_size,
            num_attention_heads,
            num_kv_heads,
            head_dim,
            intermediate_size,
            vocab_size,
            context_length,
            gqa_ratio: num_attention_heads / num_kv_heads,
        })
    }

    /// Read the architecture dimensions authoritatively from a GGUF header.
    ///
    /// Keys are architecture-scoped (`{arch}.embedding_length`, …) where
    /// `{arch}` is the GGUF's own `general.architecture` string — we read the
    /// artifact's declaration, we do not map its name to anything. A missing
    /// REQUIRED key is a hard error naming the key; `head_dim` and
    /// `vocab_size` fall back only to spec-defined derivations (see below),
    /// never to a hardcoded number.
    pub fn from_gguf(path: &Path) -> Result<Self, String> {
        let mut file =
            std::fs::File::open(path).map_err(|e| format!("open GGUF {}: {e}", path.display()))?;
        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| format!("read GGUF {}: {e}", path.display()))?;
        let md = &content.metadata;

        // `general.architecture` via the ONE shared reader; this consumer's
        // policy is "required" so it wraps the shared `Option` in its own
        // refuse-error.
        let arch = crate::inference_capability::gguf_keys::architecture(&content)
            .ok_or_else(|| format!("GGUF {} missing `general.architecture`", path.display()))?;

        // Required scalar dimension keyed under the model's own architecture.
        let req = |key: &str| -> Result<usize, String> {
            let v = md
                .get(key)
                .ok_or_else(|| format!("GGUF {} missing required key `{key}`", path.display()))?;
            v.to_u32()
                .map(|n| n as usize)
                .map_err(|e| format!("GGUF {} key `{key}` not a u32: {e}", path.display()))
        };

        let hidden_size = req(&format!("{arch}.embedding_length"))?;
        let intermediate_size = req(&format!("{arch}.feed_forward_length"))?;
        let num_attention_heads = req(&format!("{arch}.attention.head_count"))?;
        let num_kv_heads = req(&format!("{arch}.attention.head_count_kv"))?;
        let num_layers = req(&format!("{arch}.block_count"))?;
        // context_length through the shared reader so this path inherits the
        // `{arch}.context_length` → `llama.context_length` fallback the other
        // readers already have (previously ABSENT here — a GGUF carrying only
        // the historical llama.* key failed dimension extraction while its
        // Model row hydrated fine). Required for the KV-cache budget, so the
        // absent case is a refuse-error.
        let context_length = crate::inference_capability::gguf_keys::context_length(&content, &arch)
            .ok_or_else(|| {
                format!(
                    "GGUF {} missing required context_length (tried `{arch}.context_length` \
                         and `llama.context_length`)",
                    path.display()
                )
            })? as usize;

        // head_dim: explicit `{arch}.attention.key_length` if present, else the
        // spec-defined derivation hidden_size / num_attention_heads (llama.cpp
        // omits the key precisely when it equals that quotient).
        let head_dim = match md.get(&format!("{arch}.attention.key_length")) {
            Some(v) => v.to_u32().map(|n| n as usize).map_err(|e| {
                format!(
                    "GGUF {} key `{arch}.attention.key_length` not a u32: {e}",
                    path.display()
                )
            })?,
            None => {
                if num_attention_heads == 0 {
                    return Err(format!(
                        "GGUF {} has 0 attention heads — cannot derive head_dim",
                        path.display()
                    ));
                }
                hidden_size / num_attention_heads
            }
        };

        // vocab_size: explicit `{arch}.vocab_size` if present, else the length
        // of the tokenizer's token array (the vocab IS that array). One of the
        // two must exist — a GGUF with neither cannot be served.
        let vocab_size = match md.get(&format!("{arch}.vocab_size")) {
            Some(v) => v.to_u32().map(|n| n as usize).map_err(|e| {
                format!(
                    "GGUF {} key `{arch}.vocab_size` not a u32: {e}",
                    path.display()
                )
            })?,
            None => {
                let tokens = md.get("tokenizer.ggml.tokens").ok_or_else(|| {
                    format!(
                        "GGUF {} missing both `{arch}.vocab_size` and `tokenizer.ggml.tokens`",
                        path.display()
                    )
                })?;
                tokens
                    .to_vec()
                    .map_err(|e| {
                        format!(
                            "GGUF {} `tokenizer.ggml.tokens` not an array: {e}",
                            path.display()
                        )
                    })?
                    .len()
            }
        };

        Self::new(
            num_layers,
            hidden_size,
            num_attention_heads,
            num_kv_heads,
            head_dim,
            intermediate_size,
            vocab_size,
            context_length,
        )
    }

    /// Read the architecture dimensions authoritatively from an HF
    /// `config.json` in `dir` (the safetensors form used by training/convert).
    ///
    /// Field names are the HF standard (`hidden_size`, `num_hidden_layers`,
    /// …). A missing required field is a hard error naming the field;
    /// `num_key_value_heads` and `head_dim` fall back only to their HF
    /// spec-defined derivations (MHA → kv == q; head_dim = hidden / heads).
    pub fn from_config_json(dir: &Path) -> Result<Self, String> {
        let path = dir.join("config.json");
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;

        let req = |key: &str| -> Result<usize, String> {
            json.get(key)
                .and_then(|x| x.as_u64())
                .map(|n| n as usize)
                .ok_or_else(|| format!("config.json {} missing required `{key}`", path.display()))
        };

        let hidden_size = req("hidden_size")?;
        let intermediate_size = req("intermediate_size")?;
        let num_attention_heads = req("num_attention_heads")?;
        let num_layers = req("num_hidden_layers")?;
        let vocab_size = req("vocab_size")?;
        let context_length = req("max_position_embeddings")?;

        // Absent `num_key_value_heads` MEANS multi-head attention (kv == q) in
        // the HF config spec — a defined semantic, not a guess.
        let num_kv_heads = json
            .get("num_key_value_heads")
            .and_then(|x| x.as_u64())
            .map(|n| n as usize)
            .unwrap_or(num_attention_heads);

        // Absent `head_dim` MEANS hidden_size / num_attention_heads (HF spec).
        let head_dim = match json.get("head_dim").and_then(|x| x.as_u64()) {
            Some(n) => n as usize,
            None => {
                if num_attention_heads == 0 {
                    return Err(format!(
                        "config.json {} has 0 attention heads — cannot derive head_dim",
                        path.display()
                    ));
                }
                hidden_size / num_attention_heads
            }
        };

        Self::new(
            num_layers,
            hidden_size,
            num_attention_heads,
            num_kv_heads,
            head_dim,
            intermediate_size,
            vocab_size,
            context_length,
        )
    }

    /// Read the architecture dimensions authoritatively from a model artifact,
    /// dispatching on its on-disk shape: a `.gguf` file → [`from_gguf`]; a
    /// directory containing `config.json` (the safetensors form) →
    /// [`from_config_json`]. Any other shape is a hard error naming what was
    /// found — we never guess dims from the path string.
    ///
    /// [`from_gguf`]: ModelArchConfig::from_gguf
    /// [`from_config_json`]: ModelArchConfig::from_config_json
    pub fn from_artifact(path: &Path) -> Result<Self, String> {
        if path.is_dir() {
            if path.join("config.json").is_file() {
                return Self::from_config_json(path);
            }
            return Err(format!(
                "model artifact dir {} has no config.json — cannot source arch dims",
                path.display()
            ));
        }
        if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
            return Self::from_gguf(path);
        }
        Err(format!(
            "model artifact {} is neither a .gguf file nor a directory with config.json",
            path.display()
        ))
    }

    /// Per-layer parameter count for attention (Q + K + V + O projections),
    /// under a hypothetical retained head count (used by the compaction
    /// planner to price a pruned layer).
    pub fn attention_params_per_layer(&self, q_heads: usize, kv_heads: usize) -> usize {
        let q_params = q_heads * self.head_dim * self.hidden_size; // Q proj
        let k_params = kv_heads * self.head_dim * self.hidden_size; // K proj
        let v_params = kv_heads * self.head_dim * self.hidden_size; // V proj
        let o_params = self.hidden_size * q_heads * self.head_dim; // O proj
        q_params + k_params + v_params + o_params
    }

    /// Per-layer parameter count for MLP (gate + up + down).
    pub fn mlp_params_per_layer(&self) -> usize {
        // gate: [hidden, intermediate], up: [hidden, intermediate], down: [intermediate, hidden]
        3 * self.hidden_size * self.intermediate_size
    }

    /// Embedding + LM head parameter count.
    pub fn embedding_params(&self) -> usize {
        // embed_tokens + lm_head (may be tied, but budget for both)
        2 * self.vocab_size * self.hidden_size
    }

    /// Norm + bias params (small, always F32).
    pub fn norm_params(&self) -> usize {
        // 2 norms per layer (attn_norm, ffn_norm) + 1 final norm, each hidden_size
        (2 * self.num_layers + 1) * self.hidden_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: `new` must reject dimension sets that would divide by
    // zero or form an invalid GQA grouping (fail loud), and must compute the
    // gqa_ratio for valid ones — the invariant every consumer relies on when it
    // reads the carrier instead of re-deriving from the name.
    #[test]
    fn new_validates_gqa_grouping_and_computes_ratio() {
        // Valid GQA: 40 query heads over 8 KV heads → ratio 5.
        let cfg = ModelArchConfig::new(64, 5120, 40, 8, 128, 27648, 152064, 32768).unwrap();
        assert_eq!(cfg.gqa_ratio, 5);
        assert_eq!(cfg.context_length, 32768);

        // Valid MHA: kv == q → ratio 1.
        let mha = ModelArchConfig::new(28, 3072, 24, 24, 128, 8192, 128256, 8192).unwrap();
        assert_eq!(mha.gqa_ratio, 1);

        // Zero KV heads → error, never a divide-by-zero.
        assert!(ModelArchConfig::new(1, 8, 4, 0, 2, 16, 100, 2048).is_err());

        // Non-dividing grouping (7 query heads over 2 KV) → error.
        assert!(ModelArchConfig::new(1, 8, 7, 2, 2, 16, 100, 2048).is_err());
    }

    // what this catches: a missing artifact / missing required key must fail
    // loud naming the problem — never silently yield a guessed default. This is
    // the whole point of sourcing dims from the artifact instead of the name.
    #[test]
    fn from_gguf_fails_loud_on_missing_artifact() {
        let err = ModelArchConfig::from_gguf(Path::new("/nonexistent/model.gguf")).unwrap_err();
        assert!(
            err.contains("open GGUF"),
            "should name the open failure: {err}"
        );
    }

    // what this catches: config.json reader parses the HF-standard fields and
    // applies the spec-defined MHA derivation when num_key_value_heads is
    // absent (kv == q) — reading the spec, not guessing.
    #[test]
    fn from_config_json_reads_fields_and_derives_mha() {
        let dir =
            std::env::temp_dir().join(format!("continuum_arch_config_mha_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.json"),
            r#"{
                "hidden_size": 2048,
                "intermediate_size": 5632,
                "num_attention_heads": 16,
                "num_hidden_layers": 24,
                "vocab_size": 32000,
                "max_position_embeddings": 4096
            }"#,
        )
        .unwrap();

        let cfg = ModelArchConfig::from_config_json(&dir).unwrap();
        assert_eq!(cfg.hidden_size, 2048);
        assert_eq!(cfg.num_attention_heads, 16);
        // num_key_value_heads absent → MHA (kv == q).
        assert_eq!(cfg.num_kv_heads, 16);
        assert_eq!(cfg.gqa_ratio, 1);
        // head_dim absent → hidden_size / heads = 2048 / 16 = 128.
        assert_eq!(cfg.head_dim, 128);
        assert_eq!(cfg.context_length, 4096);

        std::fs::remove_dir_all(&dir).ok();
    }

    // what this catches: a config.json missing a required field fails loud
    // naming that field, rather than defaulting it.
    #[test]
    fn from_config_json_fails_loud_on_missing_required_field() {
        let dir = std::env::temp_dir().join(format!(
            "continuum_arch_config_missing_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), r#"{ "hidden_size": 2048 }"#).unwrap();

        let err = ModelArchConfig::from_config_json(&dir).unwrap_err();
        assert!(
            err.contains("missing required") && err.contains("intermediate_size"),
            "should name the first missing required field: {err}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
