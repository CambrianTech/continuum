//! `forge/publish` tag + gate core — the pure, network-free heart of #99 (L4).
//!
//! Ported from the legacy `hf-publish.py` (`build_tags` / `normalize_tag_value`)
//! so the Rust core owns the market-publish path end to end. This module is
//! deliberately PURE (no I/O, no HF, no ACL): it computes the standardized
//! `continuum:*` tag set a layer carries into the shared catalog, and the
//! lift-gate that decides whether a layer is even publishable. The Owner-gated
//! `forge/publish` command (ACL already reserves it Owner-only, because network
//! publish is a consent-gated action — [[consent-gates-on-actions-never-caps-on-
//! cognition]]) calls these, then hands off to the custodian's `push_to_hub`.
//!
//! Why tags matter to the market: they are the coarse, exact-match facet filter
//! that runs BEFORE cosine (base-model compatibility is a HARD gate, not a soft
//! rank — a Qwen LoRA can't apply to Llama). `continuum:base=…` /
//! `continuum:role=…` narrow the candidate set; cosine over the card then ranks
//! within it. See docs/architecture/GENOME-FOUNDRY-SENTINEL.md Part 7 + Part 10.

/// Tag schema version — MUST match `CONTINUUM_TAG_SCHEMA_VERSION` in the legacy
/// `AdapterPublishSchema.ts` and `hf-publish.py` so tags produced by either path
/// are cross-readable in the shared catalog.
pub const CONTINUUM_TAG_SCHEMA_VERSION: u32 = 1;

/// The subset of a layer's manifest that drives its published tags. All optional
/// — a field absent (or empty / zero where the legacy script used a truthy check)
/// simply omits its tag, exactly as `hf-publish.py` did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublishTagInput {
    /// Base model the layer was forged against (e.g. `unsloth/Devstral-Small-2507-GGUF`).
    pub base_model: Option<String>,
    /// Trait / role the layer specializes (the `traitType` — e.g. `code`).
    pub trait_type: Option<String>,
    /// Training performance score (integer). `Some(0)` IS published (the legacy
    /// script only skipped `None`); a real score of 0 is a fact worth recording.
    pub score: Option<i64>,
    /// Training epochs. Skipped when absent or zero (legacy truthy check).
    pub epochs: Option<i64>,
    /// Persona the layer trains for.
    pub persona_name: Option<String>,
    /// Project / directory type this layer is scoped to (repo-scope facet).
    pub project_type: Option<String>,
    /// LoRA rank. Skipped when absent or zero (legacy truthy check).
    pub rank: Option<i64>,
}

/// Normalize a tag value to lowercase kebab-case — a faithful port of
/// `hf-publish.py::normalize_tag_value` (which itself matches
/// `AdapterPublishSchema.ts::normalizeTagValue`). Regex-free so it carries no dep:
/// `"Sprite Artist"` → `"sprite-artist"`, `"sprite_artist"` → `"sprite-artist"`,
/// `"camelCase"` → `"camel-case"`.
pub fn normalize_tag_value(value: &str) -> String {
    // 1. camelCase → kebab: insert '-' between a lower/digit and an UPPER.
    let mut split = String::with_capacity(value.len() + 8);
    let mut prev: Option<char> = None;
    for c in value.chars() {
        if let Some(p) = prev {
            if (p.is_ascii_lowercase() || p.is_ascii_digit()) && c.is_ascii_uppercase() {
                split.push('-');
            }
        }
        split.push(c);
        prev = Some(c);
    }
    // 2. '_'/whitespace → '-', 3. lowercase, 4. collapse repeats, 5. trim '-'.
    let mut out = String::with_capacity(split.len());
    let mut last_dash = false;
    for c in split.chars() {
        if c == '_' || c.is_whitespace() || c == '-' {
            if !last_dash {
                out.push('-');
                last_dash = true;
            }
        } else {
            last_dash = false;
            out.extend(c.to_lowercase());
        }
    }
    out.trim_matches('-').to_string()
}

/// Strip the org prefix from a model id, then normalize — port of
/// `hf-publish.py::normalize_base_model`. `"unsloth/Devstral-Small-2507"` →
/// `"devstral-small-2507"`.
pub fn normalize_base_model(model_id: &str) -> String {
    let name = model_id.rsplit('/').next().unwrap_or(model_id);
    normalize_tag_value(name)
}

/// Build the standardized `continuum:*` tag list for a layer — a faithful port of
/// `hf-publish.py::build_tags`. Order is stable (matches the legacy script) so a
/// catalog diff is meaningful. The base model is emitted BOTH as HF-native
/// (`base_model:<raw>`) and normalized (`continuum:base=<kebab>`) so HF's own
/// model-tree and our facet filter both resolve it.
pub fn continuum_tags(input: &PublishTagInput) -> Vec<String> {
    let mut tags = vec![
        "peft".to_string(),
        "lora".to_string(),
        "continuum".to_string(),
        format!("continuum:schema={CONTINUUM_TAG_SCHEMA_VERSION}"),
    ];

    if let Some(bm) = input.base_model.as_deref().filter(|s| !s.is_empty()) {
        tags.push(format!("base_model:{bm}"));
        tags.push(format!("continuum:base={}", normalize_base_model(bm)));
    }
    if let Some(role) = input.trait_type.as_deref().filter(|s| !s.is_empty()) {
        tags.push(format!("continuum:role={}", normalize_tag_value(role)));
    }
    // performance: present (even 0) → published; only None is skipped.
    if let Some(score) = input.score {
        tags.push(format!("continuum:score={score}"));
    }
    if let Some(epochs) = input.epochs.filter(|e| *e != 0) {
        tags.push(format!("continuum:epochs={epochs}"));
    }
    if let Some(persona) = input.persona_name.as_deref().filter(|s| !s.is_empty()) {
        tags.push(format!(
            "continuum:persona={}",
            normalize_tag_value(persona)
        ));
    }
    if let Some(pt) = input.project_type.as_deref().filter(|s| !s.is_empty()) {
        tags.push(format!(
            "continuum:project-type={}",
            normalize_tag_value(pt)
        ));
    }
    if let Some(rank) = input.rank.filter(|r| *r != 0) {
        tags.push(format!("continuum:rank={rank}"));
    }

    tags
}

/// The #99 publish quality gate: a layer is only publishable to the shared market
/// if it BEAT its baseline on the held-out gym (positive lift). This mirrors the
/// L3 adoption gate (`training_completion_sentinel`: page-in only when `lift >
/// 0`) — "only smarter layers propagate" (REPO-GENOME-AND-COURSES §16), now
/// applied at the publish boundary so a regressing or overfit layer never enters
/// the catalog other peers cosine-search. The Owner-invoked `forge/publish`
/// command calls this before any upload; a failing layer is refused LOUD, never
/// silently published ([[fallbacks-are-illegal-fail-loud]]).
pub fn passes_publish_lift_gate(lift: f64) -> bool {
    lift > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the kebab normalizer must match the legacy TS/py exactly,
    // or tags produced by the Rust path won't cross-read with existing HF layers.
    #[test]
    fn normalize_matches_legacy_cases() {
        assert_eq!(normalize_tag_value("Sprite Artist"), "sprite-artist");
        assert_eq!(normalize_tag_value("sprite_artist"), "sprite-artist");
        assert_eq!(normalize_tag_value("camelCase"), "camel-case");
        assert_eq!(normalize_tag_value("HTTPServer"), "httpserver"); // no lower→upper boundary inside caps run
        assert_eq!(normalize_tag_value("  __weird--Value__  "), "weird-value");
        assert_eq!(normalize_tag_value("code"), "code");
    }

    #[test]
    fn base_model_strips_org_then_normalizes() {
        assert_eq!(
            normalize_base_model("unsloth/Devstral-Small-2507"),
            "devstral-small-2507"
        );
        assert_eq!(
            normalize_base_model("qwen2.5-coder-14b"),
            "qwen2.5-coder-14b"
        );
    }

    // what this catches: the exact tag set + order the market's facet filter reads.
    // A drift here silently changes what `continuum:base=`/`continuum:role=` a peer
    // matches on.
    #[test]
    fn continuum_tags_full_set_in_order() {
        let input = PublishTagInput {
            base_model: Some("unsloth/Devstral-Small-2507-GGUF".to_string()),
            trait_type: Some("code".to_string()),
            score: Some(87),
            epochs: Some(3),
            persona_name: Some("Asha".to_string()),
            project_type: Some("continuum".to_string()),
            rank: Some(16),
        };
        assert_eq!(
            continuum_tags(&input),
            vec![
                "peft",
                "lora",
                "continuum",
                "continuum:schema=1",
                "base_model:unsloth/Devstral-Small-2507-GGUF",
                "continuum:base=devstral-small-2507-gguf",
                "continuum:role=code",
                "continuum:score=87",
                "continuum:epochs=3",
                "continuum:persona=asha",
                "continuum:project-type=continuum",
                "continuum:rank=16",
            ]
        );
    }

    // what this catches: legacy truthy-vs-None semantics — score 0 publishes,
    // epochs/rank 0 and empty strings are omitted (must match hf-publish.py).
    #[test]
    fn optional_fields_follow_legacy_truthiness() {
        let minimal = PublishTagInput {
            score: Some(0),                  // present → published even at 0
            epochs: Some(0),                 // zero → omitted
            rank: Some(0),                   // zero → omitted
            base_model: Some(String::new()), // empty → omitted
            ..Default::default()
        };
        let tags = continuum_tags(&minimal);
        assert!(tags.contains(&"continuum:score=0".to_string()));
        assert!(!tags.iter().any(|t| t.starts_with("continuum:epochs")));
        assert!(!tags.iter().any(|t| t.starts_with("continuum:rank")));
        assert!(!tags.iter().any(|t| t.starts_with("base_model:")));
        // Always-present base four regardless.
        assert_eq!(
            &tags[..4],
            &["peft", "lora", "continuum", "continuum:schema=1"]
        );
    }

    // what this catches: the publish gate must reject non-positive lift so a
    // regressing/overfit layer never enters the shared catalog.
    #[test]
    fn publish_gate_only_passes_positive_lift() {
        assert!(passes_publish_lift_gate(0.02));
        assert!(!passes_publish_lift_gate(0.0));
        assert!(!passes_publish_lift_gate(-0.1));
    }
}
