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
//! | field              | absent sentinel    |
//! |--------------------|--------------------|
//! | `arch`             | [`Arch::Unknown`]  |
//! | `context_window`   | `0`                |
//! | `chat_template`    | `None`             |
//! | `parameter_count`  | `0`                |
//!
//! A fully-specified catalog row hydrates to a no-op (early return before
//! the GGUF is even opened) — so every current row is unaffected. The
//! mechanism activates for rows that deliberately omit a queryable field,
//! and for discovery-built rows (drop a GGUF → the model appears) where
//! the artifact is the only source.
//!
//! **Sensory capabilities are ADDITIVE, not sentinel-filled.** A local
//! multimodal model carries a second artifact — the mmproj (multimodal
//! projector) GGUF — whose own `clip.has_vision_encoder` /
//! `clip.has_audio_encoder` header keys are the authoritative statement of
//! which raw modalities the model ingests natively. This is what LiveKit
//! media routing keys off: a model that `has(Vision)`/`has(AudioInput)`
//! consumes a camera/mic track directly; a model that does not gets the
//! sensory bridge (STT / TTS / vision-describe / object-detect → text).
//! When an mmproj is present and readable, hydration INSERTS the encoders
//! it declares into the capability set — it never removes a hand-authored
//! capability (the catalog stays the offline-truth override, so the
//! curated rows work in CI without the projector file on disk). The win is
//! the dropped-in projector model that named no caps: it still becomes
//! correctly sighted/hearing from its own artifact, never from a name guess.
//!
//! Hydration is OPPORTUNISTIC enrichment, not a load precondition: if the
//! GGUF isn't on disk yet (model not downloaded), absent fields stay at
//! their sentinel — the catalog row's hand-authored values, if any, still
//! stand. The only error this surfaces is a GGUF that IS present but
//! unparseable (a genuinely broken artifact the row depends on).

use super::types::{Arch, Capability, Model};
use crate::inference_capability::gguf_keys;
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
    /// `general.parameter_count` — the model's own declared total parameter
    /// count. `u64` because a 7B+ model overflows `u32`. This is the ONLY
    /// authoritative source for the size fact: the alternative (parsing
    /// `"4b"`/`"7b"` out of the model name) is exactly the string-sniff the
    /// registry exists to kill. Absent (`None`) when the exporter omitted
    /// the key — the row then keeps the sentinel `0`, an honest "unknown".
    parameter_count: Option<u64>,
}

/// Fill a model's absent queryable fields from its resolved local GGUF.
///
/// No-op (no file I/O) when the catalog already supplies every queryable
/// field, or when no local GGUF has been resolved. Errors only when the
/// GGUF is present but cannot be opened/parsed — a broken artifact the
/// row depends on, which is a fail-loud condition, not a thing to guess.
pub fn hydrate_model_from_gguf(model: &mut Model) -> Result<(), String> {
    // Sensory caps live on their own artifact (the mmproj) with their own
    // gate, independent of the main-GGUF fields below — run it first so a
    // fully-specified-but-projector-bearing row still gets its modalities.
    hydrate_sensory_caps_from_mmproj(model)?;

    let needs_arch = model.arch == Arch::Unknown;
    let needs_context = model.context_window == 0;
    let needs_template = model.chat_template.is_none();
    let needs_param_count = model.parameter_count == 0;
    if !needs_arch && !needs_context && !needs_template && !needs_param_count {
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
    if needs_param_count {
        if let Some(count) = facts.parameter_count {
            model.parameter_count = count;
        }
    }

    Ok(())
}

/// Insert the native input modalities the model's multimodal projector
/// declares, from the projector's own header — the authoritative source for
/// "does this model see / hear raw pixels / raw audio". Additive: it never
/// removes a hand-authored capability (the catalog override stands), it only
/// fills what a projector-bearing model didn't spell out. No-op when there's
/// no projector path (text-only model) or the projector isn't on disk yet
/// (opportunistic, same contract as the main GGUF read). A projector that IS
/// present but unparseable is a broken artifact the row points at — fail loud.
fn hydrate_sensory_caps_from_mmproj(model: &mut Model) -> Result<(), String> {
    let Some(path) = model.mmproj_local_path.clone() else {
        return Ok(());
    };
    if !path.exists() {
        return Ok(());
    }
    let encoders = read_projector_encoders(&path).map_err(|e| {
        format!(
            "hydrating sensory caps for `{}` from projector {}: {e}",
            model.id,
            path.display()
        )
    })?;
    apply_projector_encoders(model, encoders);
    Ok(())
}

/// Which raw-input encoders a multimodal projector carries. Each maps to the
/// [`Capability`] the model gains: a vision encoder ⇒ [`Capability::Vision`],
/// an audio encoder ⇒ [`Capability::AudioInput`]. Output modalities
/// ([`Capability::AudioOutput`]) are NOT a projector fact — a projector is an
/// input adapter — so they stay catalog-authored until validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProjectorEncoders {
    has_vision: bool,
    has_audio: bool,
}

/// Fold a projector's declared encoders into the model's capability set.
/// Pure and additive — the single place the projector's input modalities
/// meet the registry's [`Capability`] vocabulary.
fn apply_projector_encoders(model: &mut Model, encoders: ProjectorEncoders) {
    if encoders.has_vision {
        model.capabilities.insert(Capability::Vision);
    }
    if encoders.has_audio {
        model.capabilities.insert(Capability::AudioInput);
    }
}

/// Read a multimodal projector GGUF's `clip.has_*_encoder` flags. Opening or
/// parsing failure is an error (broken artifact); a missing individual flag
/// is a definite `false` — the projector spec writes the flag when the
/// encoder is present, so its absence is a real "no such encoder", not an
/// unknown to guess around.
fn read_projector_encoders(path: &Path) -> Result<ProjectorEncoders, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("open projector: {e}"))?;
    let content =
        gguf_file::Content::read(&mut file).map_err(|e| format!("read projector: {e}"))?;
    let flag = |key: &str| {
        content
            .metadata
            .get(key)
            .and_then(|v| v.to_bool().ok())
            .unwrap_or(false)
    };
    Ok(ProjectorEncoders {
        has_vision: flag("clip.has_vision_encoder"),
        has_audio: flag("clip.has_audio_encoder"),
    })
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

    // Every canonical key is read through the ONE shared reader
    // ([`gguf_keys`]) so the coercion + the `context_length` fallback policy
    // can never drift from the other readers. This hydrator's role is purely
    // "absent → None, let the catalog field stand"; the fatality decision is
    // the caller's, so each fact is a bare `Option`.
    let architecture = gguf_keys::architecture(&content);
    let context_length = architecture
        .as_deref()
        .and_then(|arch| gguf_keys::context_length(&content, arch));
    let chat_template = gguf_keys::chat_template(&content);
    let parameter_count = gguf_keys::parameter_count(&content);

    Ok(GgufFacts {
        architecture,
        context_length,
        chat_template,
        parameter_count,
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
            weights_bytes: None,
            mmproj_bytes: None,
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
            hf_source: None,
            gguf_local_path: None,
            mmproj_local_path: None,
            chat_template: chat_template.map(str::to_string),
            multi_party_strategy: Default::default(),
            stop_sequences: Vec::new(),
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
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
        // Fully specified now includes the param count — otherwise the row
        // still needs hydration for that one fact and would open the file.
        m.parameter_count = 4_000_000_000;
        m.gguf_local_path = Some(PathBuf::from("/nonexistent/broken.gguf"));
        hydrate_model_from_gguf(&mut m).expect("no-op must not read the file");
        assert_eq!(m.arch, Arch::Qwen35);
        assert_eq!(m.context_window, 262144);
        assert_eq!(m.chat_template.as_deref(), Some("{{ template }}"));
        assert_eq!(m.parameter_count, 4_000_000_000);
    }

    // what this catches: `parameter_count` is wired into the needs-gate — a
    // row that specifies arch/context/template but leaves the param count at
    // its `0` sentinel is NOT a no-op; it must consult the artifact. With a
    // bogus path that means a fail-loud read, proving the field participates
    // in hydration rather than being silently skipped (which would leave the
    // size fact permanently unknown and tempt a name-substring guess). The
    // inverse — an explicit param count winning as an override — is covered
    // by the fully-specified no-op test above.
    #[test]
    fn absent_param_count_alone_still_triggers_hydration() {
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("broken.gguf");
        std::fs::write(&bogus, b"not a real gguf").unwrap();
        let mut m = model_with(Arch::Qwen35, 262144, Some("{{ template }}"));
        assert_eq!(m.parameter_count, 0, "the one absent queryable fact");
        m.gguf_local_path = Some(bogus);
        let err = hydrate_model_from_gguf(&mut m)
            .expect_err("param count alone must still drive the read");
        assert!(err.contains("test/model"), "error names the model: {err}");
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

    // what this catches: the projector→capability fold is additive. A vision
    // encoder grants Vision, an audio encoder grants AudioInput, both grant
    // both — and an existing hand-authored cap is preserved, never clobbered.
    // This is the exact fact LiveKit routing reads to send a raw track to a
    // native model vs bridging (STT/TTS/describe) for one that lacks the cap.
    #[test]
    fn projector_encoders_fold_additively_into_capabilities() {
        // vision-only projector → gains Vision, keeps its text caps
        let mut m = model_with(Arch::Qwen2, 32_768, None);
        apply_projector_encoders(
            &mut m,
            ProjectorEncoders {
                has_vision: true,
                has_audio: false,
            },
        );
        assert!(m.has(Capability::Vision));
        assert!(!m.has(Capability::AudioInput));
        assert!(m.has(Capability::TextGeneration), "existing caps preserved");

        // omni projector → gains both input modalities
        let mut omni = model_with(Arch::Qwen2, 32_768, None);
        apply_projector_encoders(
            &mut omni,
            ProjectorEncoders {
                has_vision: true,
                has_audio: true,
            },
        );
        assert!(omni.has(Capability::Vision));
        assert!(omni.has(Capability::AudioInput));

        // a projector never asserts output modality — that's not its job
        assert!(!omni.has(Capability::AudioOutput));

        // re-applying is idempotent (BTreeSet insert), and a hand-authored
        // cap the projector doesn't declare survives a re-fold
        omni.capabilities.insert(Capability::AudioOutput);
        apply_projector_encoders(
            &mut omni,
            ProjectorEncoders {
                has_vision: true,
                has_audio: true,
            },
        );
        assert!(omni.has(Capability::AudioOutput), "override not clobbered");
    }

    // what this catches: a text-only model (no projector path) must not touch
    // the filesystem for sensory caps and must gain none — the 3.5B-class model
    // that routes through the STT/TTS/describe bridge, not raw tracks.
    #[test]
    fn text_only_model_gets_no_sensory_caps_and_no_io() {
        let mut m = model_with(Arch::Qwen35, 262144, Some("{{ t }}"));
        m.parameter_count = 4_000_000_000; // fully specified: main gate is a no-op
        assert!(m.mmproj_local_path.is_none());
        hydrate_model_from_gguf(&mut m).expect("no projector, no work, no error");
        assert!(!m.has(Capability::Vision));
        assert!(!m.has(Capability::AudioInput));
    }

    // what this catches: a projector path that points at a present-but-broken
    // file is a fail-loud artifact error naming the model — not a silent skip
    // that would leave a multimodal model mysteriously blind. (An ABSENT
    // projector path is the opportunistic-skip case, covered above.)
    #[test]
    fn broken_projector_fails_loud() {
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("mmproj-broken.gguf");
        std::fs::write(&bogus, b"not a real gguf").unwrap();
        let mut m = model_with(Arch::Qwen2, 32_768, Some("{{ t }}"));
        m.parameter_count = 7_000_000_000;
        m.mmproj_local_path = Some(bogus);
        let err = hydrate_model_from_gguf(&mut m).expect_err("broken projector must surface");
        assert!(err.contains("test/model"), "error names the model: {err}");
        assert!(err.contains("projector"), "error names the artifact: {err}");
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
