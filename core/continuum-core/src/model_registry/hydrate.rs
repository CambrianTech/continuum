//! Hydrate `Model` rows from their artifact's own authoritative metadata.
//!
//! The catalog (`catalog.rs`) is hand-authored, and hand-authoring is for
//! the RESIDUE no query can supply: cost per 1k, measured tokens/sec, and
//! curated capability overrides. Everything else about a local model —
//! its architecture, its trained context window, its chat template — is a
//! QUERYABLE fact that lives in the GGUF's own header. No human should
//! type those. This module reads them back from the artifact at registry
//! load time and fills the catalog row's absent fields.
//!
//! **Precedence (the law).** A catalog field that is explicitly set is an
//! OVERRIDE — it always wins. The catalog is where a human encodes the
//! deliberate correction the artifact's metadata gets wrong (e.g. the
//! rope-scaled 262144 context window the GGUF's stated `context_length`
//! understates). Hydration only fills a field left at its absent sentinel:
//!
//! | field             | absent sentinel    |
//! |-------------------|--------------------|
//! | `arch`            | [`Arch::Unknown`]  |
//! | `context_window`  | `0`                |
//! | `chat_template`   | `None`             |
//!
//! A fully-specified catalog row hydrates to a no-op (early return before
//! the GGUF is even opened) — so every current row is unaffected. The
//! mechanism activates for rows that deliberately omit a queryable field,
//! and for discovery-built rows (drop a GGUF → the model appears) where
//! the artifact is the only source.
//!
//! Hydration is OPPORTUNISTIC enrichment, not a load precondition: if the
//! GGUF isn't on disk yet (model not downloaded), absent fields stay at
//! their sentinel — the catalog row's hand-authored values, if any, still
//! stand. The only error this surfaces is a GGUF that IS present but
//! unparseable (a genuinely broken artifact the row depends on).

use super::types::{Arch, Model};
use candle_core::quantized::gguf_file;
use std::path::Path;

/// The subset of GGUF header facts the registry hydrates from. Each is
/// independent — a missing single key leaves that field `None` rather
/// than failing the whole read, because hydration only consults a key
/// when the catalog left the corresponding field absent.
struct GgufFacts {
    architecture: Option<String>,
    context_length: Option<u32>,
    chat_template: Option<String>,
}

/// Fill a model's absent queryable fields from its resolved local GGUF.
///
/// No-op (no file I/O) when the catalog already supplies every queryable
/// field, or when no local GGUF has been resolved. Errors only when the
/// GGUF is present but cannot be opened/parsed — a broken artifact the
/// row depends on, which is a fail-loud condition, not a thing to guess.
pub fn hydrate_model_from_gguf(model: &mut Model) -> Result<(), String> {
    let needs_arch = model.arch == Arch::Unknown;
    let needs_context = model.context_window == 0;
    let needs_template = model.chat_template.is_none();
    if !needs_arch && !needs_context && !needs_template {
        return Ok(());
    }

    // Catalog left a queryable field absent but there's no local artifact
    // to fill it from (e.g. a cloud model, or a local model not yet
    // downloaded). Cloud hydration flows from `/v1/models`, not here; an
    // undownloaded local model keeps whatever the catalog gave it.
    let Some(path) = model.gguf_local_path.clone() else {
        return Ok(());
    };

    let facts = read_gguf_facts(&path)
        .map_err(|e| format!("hydrating `{}` from {}: {e}", model.id, path.display()))?;

    if needs_arch {
        if let Some(arch) = &facts.architecture {
            model.arch = arch_from_gguf_string(arch);
        }
    }
    if needs_context {
        if let Some(ctx) = facts.context_length {
            model.context_window = ctx;
        }
    }
    if needs_template {
        if let Some(template) = facts.chat_template {
            model.chat_template = Some(template);
        }
    }

    Ok(())
}

/// Translate a GGUF `general.architecture` string into the registry's
/// closed [`Arch`] vocabulary. This is the ONE place the artifact's own
/// architecture naming meets our enum — call sites dispatch on the enum,
/// never on this string. Unrecognized architectures map to
/// [`Arch::Unknown`] (capability-routed, never arch-dispatched).
pub fn arch_from_gguf_string(arch: &str) -> Arch {
    match arch.to_lowercase().as_str() {
        "qwen2" => Arch::Qwen2,
        "qwen3" | "qwen3moe" => Arch::Qwen3,
        "qwen35" | "qwen3.5" => Arch::Qwen35,
        "llama" => Arch::Llama,
        "deepseek" | "deepseek2" => Arch::Deepseek,
        "mistral" | "mixtral" => Arch::Mistral,
        _ => Arch::Unknown,
    }
}

/// Read the registry-relevant header keys from a GGUF, each independently.
/// Opening/parsing failure is an error (broken artifact); a missing
/// individual key is just `None` (the catalog field, if set, stands).
fn read_gguf_facts(path: &Path) -> Result<GgufFacts, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("open GGUF: {e}"))?;
    let content = gguf_file::Content::read(&mut file).map_err(|e| format!("read GGUF: {e}"))?;

    let architecture = content
        .metadata
        .get("general.architecture")
        .and_then(|v| v.to_string().ok())
        .cloned();

    // Architecture-specific key first, then the historical `llama.*` key
    // some exporters wrote regardless of architecture.
    let context_length = architecture
        .as_ref()
        .and_then(|arch| content.metadata.get(&format!("{arch}.context_length")))
        .or_else(|| content.metadata.get("llama.context_length"))
        .and_then(|v| v.to_u32().ok());

    let chat_template = content
        .metadata
        .get("tokenizer.chat_template")
        .and_then(|v| v.to_string().ok())
        .cloned();

    Ok(GgufFacts {
        architecture,
        context_length,
        chat_template,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::types::Capability;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn model_with(arch: Arch, context_window: u32, chat_template: Option<&str>) -> Model {
        Model {
            id: "test/model".into(),
            name: None,
            provider: "llamacpp-local".into(),
            arch,
            context_window,
            max_output_tokens: 4096,
            tokens_per_second: 30.0,
            capabilities: BTreeSet::from([Capability::TextGeneration]),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: None,
            gguf_local_path: None,
            mmproj_local_path: None,
            chat_template: chat_template.map(str::to_string),
            multi_party_strategy: Default::default(),
            stop_sequences: Vec::new(),
        }
    }

    // what this catches: the GGUF-arch vocabulary → Arch enum mapping. A
    // regression here would silently route a model through the wrong arch
    // dispatch (or drop it to Unknown), which is exactly the stringly-typed
    // bug the enum exists to prevent.
    #[test]
    fn arch_string_maps_to_enum_and_unknown_is_the_escape_hatch() {
        assert_eq!(arch_from_gguf_string("qwen2"), Arch::Qwen2);
        assert_eq!(arch_from_gguf_string("Qwen2"), Arch::Qwen2);
        assert_eq!(arch_from_gguf_string("qwen3moe"), Arch::Qwen3);
        assert_eq!(arch_from_gguf_string("llama"), Arch::Llama);
        assert_eq!(arch_from_gguf_string("deepseek2"), Arch::Deepseek);
        assert_eq!(arch_from_gguf_string("mixtral"), Arch::Mistral);
        assert_eq!(arch_from_gguf_string("some-future-arch"), Arch::Unknown);
    }

    // what this catches: a fully-specified catalog row must hydrate to a
    // pure no-op — it must NOT touch the filesystem (the gguf path here is
    // bogus; if hydration tried to read it, this would error). Proves the
    // explicit-catalog-value-wins precedence and zero-regression claim.
    #[test]
    fn fully_specified_row_is_a_noop_even_with_a_bogus_gguf_path() {
        let mut m = model_with(Arch::Qwen35, 262144, Some("{{ template }}"));
        m.gguf_local_path = Some(PathBuf::from("/nonexistent/broken.gguf"));
        hydrate_model_from_gguf(&mut m).expect("no-op must not read the file");
        assert_eq!(m.arch, Arch::Qwen35);
        assert_eq!(m.context_window, 262144);
        assert_eq!(m.chat_template.as_deref(), Some("{{ template }}"));
    }

    // what this catches: a row with absent queryable fields but NO resolved
    // GGUF (undownloaded local model / cloud model) must not error — absent
    // fields stay at their sentinel for cloud/`/v1/models` hydration or a
    // later download to fill. Hydration is opportunistic, not a precondition.
    #[test]
    fn absent_fields_without_a_gguf_are_left_for_later_not_an_error() {
        let mut m = model_with(Arch::Unknown, 0, None);
        assert!(m.gguf_local_path.is_none());
        hydrate_model_from_gguf(&mut m).expect("missing artifact is not fatal here");
        assert_eq!(m.arch, Arch::Unknown);
        assert_eq!(m.context_window, 0);
        assert!(m.chat_template.is_none());
    }

    // what this catches: a present-but-unparseable GGUF that the row DEPENDS
    // on (it left arch absent) is a fail-loud broken artifact, named with the
    // model id and path — not silently swallowed.
    #[test]
    fn broken_gguf_a_row_depends_on_fails_loud() {
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("broken.gguf");
        std::fs::write(&bogus, b"not a real gguf").unwrap();
        let mut m = model_with(Arch::Unknown, 0, None);
        m.gguf_local_path = Some(bogus);
        let err = hydrate_model_from_gguf(&mut m).expect_err("broken artifact must surface");
        assert!(err.contains("test/model"), "error names the model: {err}");
    }
}
