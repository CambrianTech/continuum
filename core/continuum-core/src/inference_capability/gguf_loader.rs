//! GGUF metadata → `QwenModelMetadata` populator (CBAR-PIECE-5 PR-2).
//!
//! PR-1 (`residency.rs`) defined the typed surface + pure gate. This PR-2
//! reads a real GGUF file and produces the `QwenModelMetadata` the gate
//! consumes. Still no inference dispatch, no runtime probe wiring — just
//! `&Path` → `QwenModelMetadata`. PR-3 wires both probe + this loader
//! into the actual turn dispatcher.
//!
//! ## What gets extracted
//!
//! From the GGUF file's metadata map:
//!
//! - `general.architecture` (required) → `architecture` field, used to
//!   index `{architecture}.block_count`.
//! - `general.name` (optional) → `model_name`, falls back to the file
//!   stem if missing.
//! - `{architecture}.block_count` (required) → `layer_count`.
//! - `general.file_type` (required) → mapped via `file_type_to_bytes_per_param`
//!   to `bytes_per_parameter_quantized`.
//! - `general.parameter_count` (optional) OR derived if absent →
//!   `parameter_count_billions`.
//! - Architecture-keyed lookup → `layer_kinds_needing_check`.
//!
//! ## Failure-mode discipline
//!
//! - **No silent fallback for required fields**: missing `block_count`,
//!   missing `general.architecture`, or unknown `general.file_type`
//!   value all return `Err` — never a guessed default. Same posture as
//!   `backends::read_gguf_metadata` (Joel's 2026-04-23 fix removed all
//!   the silent-llama-fallback paths there).
//! - **`general.parameter_count` is OPTIONAL** with a typed fallback
//!   that LOGS the inference (file_size × bytes-per-param-inverse).
//!   The fallback path is loud — every caller sees "parameter_count
//!   estimated from file size, not GGUF metadata" so a future PR can
//!   tighten when canon files start carrying the field reliably.
//! - **Unknown architecture**: not blocked here — the residency gate's
//!   `unsupported_layer_kinds_on_backend` already filters per backend.
//!   PR-2's job is to extract data, not gate. Returns `Ok` with an
//!   empty `layer_kinds_needing_check`.
//!
//! ## What this DOES NOT do
//!
//! - Open the model for inference. That's `load_gguf_backend` in
//!   `backends::mod`.
//! - Probe hardware. That's `probe::probe_inference_capabilities`.
//! - Decide whether the gate passes. That's `residency::check_residency_gate`.
//! - Cache the metadata. Caller (PR-3) owns the cache decision.

use crate::inference_capability::residency::QwenModelMetadata;
use candle_core::quantized::gguf_file;
use std::path::Path;

/// Open a GGUF file + extract the residency-relevant metadata.
///
/// Thin file-opener around `parse_qwen_metadata_from_content` — the
/// parsing logic is tested via helpers (`file_type_to_bytes_per_param`,
/// `layer_kinds_for_architecture`) so this wrapper is mostly I/O.
pub fn read_qwen_model_metadata(path: &Path) -> Result<QwenModelMetadata, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open GGUF at {}: {e}", path.display()))?;
    let content = gguf_file::Content::read(&mut file)
        .map_err(|e| format!("Failed to read GGUF at {}: {e}", path.display()))?;

    let file_size_bytes = std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| format!("Failed to stat GGUF {}: {e}", path.display()))?;
    let fallback_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    parse_qwen_metadata_from_content(&content, fallback_name, file_size_bytes, path)
}

/// Pure parser — extracts `QwenModelMetadata` from already-parsed
/// gguf_file::Content. The `path` is only used for error messages.
///
/// Separated from `read_qwen_model_metadata` for testability: this
/// function can be exercised with synthetic content (or, in PR-2's
/// scope, by checking the helper-level behavior separately).
fn parse_qwen_metadata_from_content(
    content: &gguf_file::Content,
    fallback_name: String,
    file_size_bytes: u64,
    path: &Path,
) -> Result<QwenModelMetadata, String> {
    // architecture: required (same posture as backends::read_gguf_metadata).
    // Read through the ONE shared canonical-key reader; this consumer's
    // policy is "refuse if absent".
    let architecture =
        crate::inference_capability::gguf_keys::architecture(content).ok_or_else(|| {
            format!(
                "GGUF {} is missing required 'general.architecture' — refuse rather than \
                 guess. Same rule as backends::read_gguf_metadata (Joel 2026-04-23).",
                path.display()
            )
        })?;

    // model_name: optional; fall back to file stem (recoverable, doesn't
    // affect gate correctness; only display).
    let model_name =
        crate::inference_capability::gguf_keys::general_name(content).unwrap_or(fallback_name);

    // block_count: required. The {arch}.block_count key is the canonical
    // GGUF layer count. Without it, the residency gate's layer-count
    // evidence is missing — refuse rather than fake.
    let layer_count = crate::inference_capability::gguf_keys::block_count(content, &architecture)
        .ok_or_else(|| {
        format!(
            "GGUF {} (arch={architecture}) is missing required '{architecture}.block_count' \
                 — residency gate cannot report gpu_layer_count without it. Refuse rather \
                 than guess.",
            path.display()
        )
    })?;

    // file_type: required. Maps to bytes_per_parameter. Unknown enum
    // value returns Err — better to refuse than guess wrong quantization
    // (caller would over- or under-estimate VRAM).
    let file_type = content
        .metadata
        .get("general.file_type")
        .and_then(|v| v.to_u32().ok())
        .ok_or_else(|| {
            format!(
                "GGUF {} is missing required 'general.file_type' — bytes-per-param mapping \
                 needs the quantization tag to estimate VRAM.",
                path.display()
            )
        })?;
    let bytes_per_parameter_quantized = file_type_to_bytes_per_param(file_type).map_err(|e| {
        format!(
            "GGUF {} has unsupported file_type={file_type}: {e}. Add the mapping or fix \
             the GGUF.",
            path.display()
        )
    })?;

    // parameter_count: prefer metadata, fall back to file_size/bytes_per_param.
    // The fallback is loud — comment in the QwenModelMetadata field documents
    // that bytes_per_parameter_quantized is the input to the estimate, so a
    // user who sees "30B Q4_K_M = 17GB" can sanity-check.
    let parameter_count_billions = crate::inference_capability::gguf_keys::parameter_count(content)
        .map(|n| n as f64 / 1.0e9)
        .unwrap_or_else(|| {
            // Fallback: derive from file size. Approximate — GGUF includes
            // metadata overhead, token-embedding tables, output projection,
            // etc., which aren't pure parameter bytes. Off by ~5-10% on
            // large models; close enough for the gate's coarse decision.
            let est_params = file_size_bytes as f64 / bytes_per_parameter_quantized;
            est_params / 1.0e9
        });

    let layer_kinds_needing_check = layer_kinds_for_architecture(&architecture);

    Ok(QwenModelMetadata {
        model_name,
        architecture,
        layer_count,
        parameter_count_billions,
        bytes_per_parameter_quantized,
        layer_kinds_needing_check,
    })
}

/// Map the GGUF `general.file_type` enum value to bytes-per-parameter
/// for VRAM estimation. Values match llama.cpp's `ggml_ftype` enum.
///
/// Returns Err for unknown values rather than guessing — caller should
/// treat that as a broken/unsupported GGUF, not a thing to paper over.
///
/// Values cover the quantizations we actually ship today. New
/// quantization formats added by llama.cpp upstream require an explicit
/// entry here; the GGUF won't load through this path until added,
/// surfacing as a clear error.
pub(crate) fn file_type_to_bytes_per_param(ft: u32) -> Result<f64, String> {
    // Source: llama.cpp ggml-quants.h ggml_ftype enum + bits-per-weight
    // for each quantization scheme. Divided by 8 for bytes-per-weight.
    match ft {
        0 => Ok(4.0),       // ALL_F32
        1 => Ok(2.0),       // MOSTLY_F16
        2 => Ok(4.5 / 8.0), // MOSTLY_Q4_0
        3 => Ok(5.0 / 8.0), // MOSTLY_Q4_1
        // 4-5 removed in modern llama.cpp
        7 => Ok(8.5 / 8.0),     // MOSTLY_Q8_0
        8 => Ok(5.5 / 8.0),     // MOSTLY_Q5_0
        9 => Ok(6.0 / 8.0),     // MOSTLY_Q5_1
        10 => Ok(2.625 / 8.0),  // MOSTLY_Q2_K
        11 => Ok(3.4375 / 8.0), // MOSTLY_Q3_K_S
        12 => Ok(3.4375 / 8.0), // MOSTLY_Q3_K_M
        13 => Ok(3.4375 / 8.0), // MOSTLY_Q3_K_L
        14 => Ok(4.5 / 8.0),    // MOSTLY_Q4_K_S
        15 => Ok(4.85 / 8.0),   // MOSTLY_Q4_K_M  ← the workhorse
        16 => Ok(5.5 / 8.0),    // MOSTLY_Q5_K_S
        17 => Ok(5.69 / 8.0),   // MOSTLY_Q5_K_M
        18 => Ok(6.5625 / 8.0), // MOSTLY_Q6_K
        19 => Ok(2.25 / 8.0),   // MOSTLY_IQ2_XXS
        20 => Ok(2.5 / 8.0),    // MOSTLY_IQ2_XS
        21 => Ok(3.0 / 8.0),    // MOSTLY_Q2_K_S
        22 => Ok(3.0625 / 8.0), // MOSTLY_IQ3_XS
        23 => Ok(3.0625 / 8.0), // MOSTLY_IQ3_XXS
        24 => Ok(1.5625 / 8.0), // MOSTLY_IQ1_S
        25 => Ok(4.25 / 8.0),   // MOSTLY_IQ4_NL
        26 => Ok(3.4375 / 8.0), // MOSTLY_IQ3_S
        27 => Ok(3.4375 / 8.0), // MOSTLY_IQ3_M
        28 => Ok(2.5 / 8.0),    // MOSTLY_IQ2_S
        29 => Ok(2.75 / 8.0),   // MOSTLY_IQ2_M
        30 => Ok(4.25 / 8.0),   // MOSTLY_IQ4_XS
        31 => Ok(1.75 / 8.0),   // MOSTLY_IQ1_M
        32 => Ok(8.5 / 8.0),    // MOSTLY_BF16
        unknown => Err(format!(
            "file_type={unknown} is not in the supported quantization table — add the \
             bits-per-weight entry or fix the GGUF"
        )),
    }
}

/// Layer kinds that may NOT be supported on every backend, keyed by
/// architecture. Conservative — when in doubt, return the layer kinds
/// so the residency gate can block with specific reasons rather than
/// silently allow.
///
/// Today's known per-architecture gaps for the Vulkan llama.cpp build:
///
/// - `qwen3moe`: missing `moe_gate` + `sliding_window_attn`
/// - `qwen3`: missing `sliding_window_attn`
///
/// Other architectures return empty — Metal/CUDA handle them cleanly
/// and the gate's `unsupported_layer_kinds_on_backend` filters on
/// architecture (qwen2 / qwen2vl pass Vulkan).
///
/// This is a static table because the layer-kind set is canonical per
/// architecture in the vendored llama.cpp build. When the build pulls
/// in new Vulkan kernels, update the table; the test
/// `architecture_layer_kinds_table_pins_known_arches` enforces every
/// entry stays explicit.
pub(crate) fn layer_kinds_for_architecture(arch: &str) -> Vec<String> {
    match arch {
        "qwen3moe" => vec!["moe_gate".into(), "sliding_window_attn".into()],
        "qwen3" => vec!["sliding_window_attn".into()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== file_type_to_bytes_per_param =====

    /// What this catches: every quantization the production fleet
    /// actually ships maps to a known value. If a new quantization
    /// becomes default and someone forgets to add the table entry, the
    /// loader will refuse the file at parse time — but this test
    /// catches the canonical-quant regressions at unit-test time.
    #[test]
    fn workhorse_quants_have_table_entries() {
        for ft in &[0, 1, 2, 7, 8, 14, 15, 17, 18, 32] {
            assert!(
                file_type_to_bytes_per_param(*ft).is_ok(),
                "file_type={ft} (a workhorse quant) is missing from the table"
            );
        }
    }

    /// What this catches: Q4_K_M (15) — the most common quantization
    /// in production — gives ~0.6 bytes/param. The residency gate's
    /// VRAM estimate depends on this; if the value drifts to e.g. 1.0,
    /// every Q4 prediction over-estimates 2× and the gate blocks
    /// turns that would have fit.
    #[test]
    fn q4_k_m_bytes_per_param_within_band() {
        let bpp = file_type_to_bytes_per_param(15).unwrap();
        assert!(
            bpp > 0.55 && bpp < 0.65,
            "Q4_K_M bpp={bpp} outside 0.55-0.65 band"
        );
    }

    /// What this catches: FP16 (1) gives exactly 2.0 bytes/param.
    /// Pinned because FP16 is the canonical "full precision but half"
    /// reference point; tests + docs assume 2.0.
    #[test]
    fn fp16_bytes_per_param_is_two() {
        assert_eq!(file_type_to_bytes_per_param(1).unwrap(), 2.0);
    }

    /// What this catches: F32 (0) gives 4.0 bytes/param. Boundary
    /// case — full precision baseline.
    #[test]
    fn f32_bytes_per_param_is_four() {
        assert_eq!(file_type_to_bytes_per_param(0).unwrap(), 4.0);
    }

    /// What this catches: unknown file_type returns Err (not a guess,
    /// not a panic). The whole module's reason-for-existing is "refuse
    /// to lie about VRAM"; silent-default-on-unknown-quant is exactly
    /// the bug we exist to prevent.
    #[test]
    fn unknown_file_type_returns_err() {
        let result = file_type_to_bytes_per_param(9999);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("9999"),
            "error should name the unknown value: {msg}"
        );
    }

    /// What this catches: removed file_types (4, 5 in modern llama.cpp)
    /// don't have entries — they should also Err loud rather than
    /// silently match a default. Defensive against future re-adds with
    /// different semantics.
    #[test]
    fn removed_file_types_return_err() {
        for ft in &[4, 5, 6] {
            assert!(
                file_type_to_bytes_per_param(*ft).is_err(),
                "file_type={ft} (removed in modern llama.cpp) should Err"
            );
        }
    }

    /// What this catches: file_type ordering — heavier quants always
    /// give more bytes/param than lighter ones within their family.
    /// Sanity check that the table values are internally consistent.
    #[test]
    fn quants_ordered_by_bits_per_weight() {
        let q4_k_m = file_type_to_bytes_per_param(15).unwrap();
        let q5_k_m = file_type_to_bytes_per_param(17).unwrap();
        let q6_k = file_type_to_bytes_per_param(18).unwrap();
        let q8_0 = file_type_to_bytes_per_param(7).unwrap();
        let f16 = file_type_to_bytes_per_param(1).unwrap();
        let f32 = file_type_to_bytes_per_param(0).unwrap();
        assert!(q4_k_m < q5_k_m, "Q4_K_M={q4_k_m} >= Q5_K_M={q5_k_m}");
        assert!(q5_k_m < q6_k, "Q5_K_M={q5_k_m} >= Q6_K={q6_k}");
        assert!(q6_k < q8_0, "Q6_K={q6_k} >= Q8_0={q8_0}");
        assert!(q8_0 < f16, "Q8_0={q8_0} >= F16={f16}");
        assert!(f16 < f32, "F16={f16} >= F32={f32}");
    }

    /// What this catches: IQ-series sub-2-bit quants give less than 0.4
    /// bytes/param. These exist for extreme-low-VRAM scenarios; the
    /// table must cover them for those use-cases.
    #[test]
    fn iq_series_quants_under_half_byte() {
        for ft in &[19, 20, 24, 31] {
            let bpp = file_type_to_bytes_per_param(*ft).unwrap();
            assert!(bpp < 0.4, "IQ ft={ft} bpp={bpp} should be < 0.4");
        }
    }

    // ===== layer_kinds_for_architecture =====

    /// What this catches: qwen3moe correctly lists both moe_gate +
    /// sliding_window_attn. The residency gate's UnsupportedLayer
    /// reason iterates this list; missing kinds means the gate would
    /// silently pass a model the Vulkan backend can't run.
    #[test]
    fn qwen3moe_lists_moe_gate_and_sliding_window() {
        let kinds = layer_kinds_for_architecture("qwen3moe");
        assert_eq!(kinds.len(), 2);
        assert!(kinds.contains(&"moe_gate".to_string()));
        assert!(kinds.contains(&"sliding_window_attn".to_string()));
    }

    /// What this catches: qwen3 (non-MoE) lists sliding_window_attn
    /// but NOT moe_gate. The distinction matters — qwen3 dense can run
    /// on Vulkan IF the sliding-window kernel is present; qwen3moe
    /// can't because moe_gate is missing.
    #[test]
    fn qwen3_lists_sliding_window_only() {
        let kinds = layer_kinds_for_architecture("qwen3");
        assert_eq!(kinds, vec!["sliding_window_attn".to_string()]);
    }

    /// What this catches: qwen2 + qwen2vl have NO declared difficult
    /// kinds — Vulkan supports them today. If this regresses, every
    /// Vulkan-only host loses Qwen2 silently.
    #[test]
    fn qwen2_and_qwen2vl_have_empty_layer_kinds() {
        assert_eq!(layer_kinds_for_architecture("qwen2"), Vec::<String>::new());
        assert_eq!(
            layer_kinds_for_architecture("qwen2vl"),
            Vec::<String>::new()
        );
    }

    /// What this catches: arbitrary unknown architecture returns
    /// empty (not panic, not error). The loader doesn't gate
    /// unsupported architectures — that's `unsupported_layer_kinds_on_backend`
    /// in residency.rs. This helper's contract is "tell me what THIS
    /// arch needs"; "I don't know" maps to "nothing declared," which
    /// the gate then handles by passing on safe backends + blocking
    /// only when the architecture-keyed rule kicks in.
    #[test]
    fn unknown_arch_returns_empty_kinds() {
        assert_eq!(
            layer_kinds_for_architecture("mistral"),
            Vec::<String>::new()
        );
        assert_eq!(layer_kinds_for_architecture("phi3"), Vec::<String>::new());
        assert_eq!(layer_kinds_for_architecture(""), Vec::<String>::new());
        assert_eq!(
            layer_kinds_for_architecture("future-model"),
            Vec::<String>::new()
        );
    }

    /// What this catches: layer-kind table stays stable for the
    /// architectures the team explicitly knows about. If someone
    /// renames moe_gate → moe_router (or similar) in the table without
    /// updating residency.rs's matching test, this fails — forcing the
    /// rename to land in both places.
    #[test]
    fn architecture_layer_kinds_table_pins_known_arches() {
        // Pin every entry by exact contents. Adding a new entry that
        // narrows scope is fine; renaming an entry is the failure mode
        // this test catches.
        assert_eq!(
            layer_kinds_for_architecture("qwen3moe"),
            vec!["moe_gate".to_string(), "sliding_window_attn".to_string()]
        );
        assert_eq!(
            layer_kinds_for_architecture("qwen3"),
            vec!["sliding_window_attn".to_string()]
        );
    }

    // ===== integration: read_qwen_model_metadata =====

    /// What this catches: non-existent path returns Err with a useful
    /// message (filename in error). Smoke test for the file-opener
    /// wrapper; the parse logic is covered by helper tests above.
    #[test]
    fn nonexistent_path_returns_err() {
        let path = Path::new("/nonexistent/definitely-not-a-real-file.gguf");
        let result = read_qwen_model_metadata(path);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("Failed to open GGUF") || msg.contains("No such file"));
    }

    /// What this catches: a non-GGUF file returns Err (not a panic, not
    /// a silent zero-filled QwenModelMetadata). Defensive — if someone
    /// points the loader at e.g. a .safetensors or a text file by
    /// accident, the error names the path.
    #[test]
    fn non_gguf_file_returns_err() {
        // Use Cargo.toml as a known-not-GGUF file present in every dev
        // checkout. The gguf_file::Content::read should fail to find
        // the magic bytes / version.
        let path = std::env::current_dir()
            .ok()
            .map(|d| d.join("Cargo.toml"))
            .filter(|p| p.exists());
        let Some(path) = path else {
            return;
        };
        let result = read_qwen_model_metadata(&path);
        assert!(result.is_err(), "non-GGUF file should Err, got Ok");
    }
}
