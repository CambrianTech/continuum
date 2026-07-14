//! `forge/publish` request — the validated, transport-agnostic description of a
//! layer about to enter the shared market (#99 L4, slice 2a).
//!
//! This is the ML-BOUNDARY gate: a trained LoRA leaving the machine for HuggingFace
//! (or, later, a grid peer) is exactly the "out of ML" edge where validation
//! matters most (Joel: "test and validate better… important for anything going
//! into or out of ML"). So the *building + validating* of a publish is its own
//! pure, fully-tested module — no network, no custodian, no HF SDK. The Owner-gated
//! `forge/publish` command constructs a [`PublishRequest`] here (which cannot exist
//! unless it passed every gate), then hands it to whatever transport uploads it.
//!
//! Gates, all enforced at construction:
//!  - **lift gate** — only a layer that beat its baseline may publish
//!    ([`super::publish_tags::passes_publish_lift_gate`]); "only smarter layers
//!    propagate" (REPO-GENOME-AND-COURSES §16) applied at the market boundary.
//!  - **repo id** — a well-formed `namespace/name` HF id (so we never POST a
//!    malformed target).
//!  - **gene present** — a real local gguf-lora path to upload (never publish a
//!    phantom).
//!  - **standardized tags** — the `continuum:*` facet tags the market filters on,
//!    built from the layer's metadata.

use std::path::{Path, PathBuf};

use super::publish_tags::{continuum_tags, passes_publish_lift_gate, PublishTagInput};

/// Why a publish was refused — LOUD and specific, never a silent drop
/// ([[fallbacks-are-illegal-fail-loud]]). The Owner-invoked command surfaces the
/// exact cause so a human knows precisely what to fix.
#[derive(Debug, Clone, PartialEq)]
pub enum PublishError {
    /// The layer did not beat its baseline (held-out lift ≤ 0) — refused so a
    /// regressing/overfit layer never enters the catalog peers cosine-search.
    LiftGate { lift_pct: f64 },
    /// The HF repo id is not a well-formed `namespace/name`.
    InvalidRepoId { repo_id: String, reason: String },
    /// The gguf-lora gene file to upload is missing/empty.
    MissingGene { detail: String },
    /// A required card field is empty (base model / trait).
    MissingField { field: &'static str },
    /// The upload transport (HF, grid peer, …) failed AFTER validation passed —
    /// network, auth, or a non-2xx from the destination. Distinct from the
    /// validation refusals: this was a good, gated request that couldn't be
    /// delivered (a caller may retry / try another publisher), not a bad request.
    Transport { transport: String, detail: String },
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LiftGate { lift_pct } => write!(
                f,
                "publish refused: held-out lift {lift_pct:+.2} ≤ 0 — only layers that beat \
                 their baseline may enter the market"
            ),
            Self::InvalidRepoId { repo_id, reason } => {
                write!(f, "publish refused: invalid repo id '{repo_id}' — {reason}")
            }
            Self::MissingGene { detail } => {
                write!(f, "publish refused: no gguf-lora to upload — {detail}")
            }
            Self::MissingField { field } => {
                write!(f, "publish refused: required field '{field}' is empty")
            }
            Self::Transport { transport, detail } => {
                write!(f, "publish via {transport} failed: {detail}")
            }
        }
    }
}
impl std::error::Error for PublishError {}

/// A validated HuggingFace repo id (`namespace/name`) — a newtype so an unchecked
/// string can never reach the upload transport. Both segments must be non-empty
/// and use only `[A-Za-z0-9._-]` (HF's allowed set), with no path traversal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HfRepoId(String);

impl HfRepoId {
    pub fn parse(raw: &str) -> Result<Self, PublishError> {
        let raw = raw.trim();
        let bad = |reason: &str| PublishError::InvalidRepoId {
            repo_id: raw.to_string(),
            reason: reason.to_string(),
        };
        let (ns, name) = raw.split_once('/').ok_or_else(|| bad("expected 'namespace/name'"))?;
        if name.contains('/') {
            return Err(bad("more than one '/' — expected exactly 'namespace/name'"));
        }
        for (seg, label) in [(ns, "namespace"), (name, "name")] {
            if seg.is_empty() {
                return Err(bad(&format!("{label} segment is empty")));
            }
            if !seg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            {
                return Err(bad(&format!(
                    "{label} '{seg}' has characters outside [A-Za-z0-9._-]"
                )));
            }
            if seg == "." || seg == ".." {
                return Err(bad(&format!("{label} may not be '.' or '..'")));
            }
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Everything the transport needs to publish ONE layer — and it can only be
/// constructed via [`PublishRequest::build`], which enforces every gate. If you
/// hold a `PublishRequest`, it is publishable by construction.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishRequest {
    /// Validated target repo (`namespace/name`).
    pub repo_id: HfRepoId,
    /// Local gguf-lora gene file to upload.
    pub gene_path: PathBuf,
    /// Standardized `continuum:*` + HF tags (the market's facet filter).
    pub tags: Vec<String>,
    /// Held-out lift (%) that cleared the gate — recorded on the card as the
    /// quality provenance.
    pub lift_pct: f64,
    // --- Card metadata (denormalized so the transport can render the model card
    //     without re-fetching anything; all validated already). ---
    /// Base model the layer was forged against.
    pub base_model: String,
    /// Trait/role the layer specializes (the card's "role").
    pub trait_kind: String,
    /// Persona the layer trains for, if any.
    pub persona_name: Option<String>,
    /// Training score (0–100), if measured.
    pub score: Option<i64>,
    /// Training epochs, if known.
    pub epochs: Option<i64>,
    /// LoRA rank, if known.
    pub rank: Option<i64>,
}

/// Inputs to [`PublishRequest::build`] — the raw facts a completed forge run knows
/// about the layer. Kept separate from the validated request so the caller can't
/// skip validation.
#[derive(Debug, Clone, Default)]
pub struct PublishInputs {
    pub repo_id: String,
    pub gene_path: PathBuf,
    pub base_model: String,
    pub trait_kind: String,
    pub persona_name: Option<String>,
    pub project_type: Option<String>,
    pub score: Option<i64>,
    pub epochs: Option<i64>,
    pub rank: Option<i64>,
    /// Held-out lift as a fraction (e.g. 0.051 = +5.1pts). The gate is `> 0`.
    pub lift: f64,
}

impl PublishRequest {
    /// Build a validated request — the ONLY constructor. Enforces, in order: the
    /// lift gate, a well-formed repo id, a present gene file, and the required card
    /// fields. Returns the exact [`PublishError`] on the first failure.
    ///
    /// `gene_exists` is injected (rather than hitting the filesystem here) so this
    /// stays a pure, deterministically-testable function; the command passes a real
    /// `|p| p.exists()`.
    pub fn build(
        inputs: &PublishInputs,
        gene_exists: impl Fn(&Path) -> bool,
    ) -> Result<Self, PublishError> {
        // 1. Lift gate first — cheapest, and the whole point.
        if !passes_publish_lift_gate(inputs.lift) {
            return Err(PublishError::LiftGate {
                lift_pct: inputs.lift * 100.0,
            });
        }
        // 2. Required card fields.
        if inputs.base_model.trim().is_empty() {
            return Err(PublishError::MissingField { field: "base_model" });
        }
        if inputs.trait_kind.trim().is_empty() {
            return Err(PublishError::MissingField { field: "trait_kind" });
        }
        // 3. Repo id.
        let repo_id = HfRepoId::parse(&inputs.repo_id)?;
        // 4. Gene present.
        if inputs.gene_path.as_os_str().is_empty() {
            return Err(PublishError::MissingGene {
                detail: "empty gene path".to_string(),
            });
        }
        if !gene_exists(&inputs.gene_path) {
            return Err(PublishError::MissingGene {
                detail: format!("no file at {}", inputs.gene_path.display()),
            });
        }
        // 5. Tags (slice-1 core).
        let tags = continuum_tags(&PublishTagInput {
            base_model: Some(inputs.base_model.clone()),
            trait_type: Some(inputs.trait_kind.clone()),
            score: inputs.score,
            epochs: inputs.epochs,
            persona_name: inputs.persona_name.clone(),
            project_type: inputs.project_type.clone(),
            rank: inputs.rank,
        });

        Ok(Self {
            repo_id,
            gene_path: inputs.gene_path.clone(),
            tags,
            lift_pct: inputs.lift * 100.0,
            base_model: inputs.base_model.clone(),
            trait_kind: inputs.trait_kind.clone(),
            persona_name: inputs.persona_name.clone(),
            score: inputs.score,
            epochs: inputs.epochs,
            rank: inputs.rank,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_inputs() -> PublishInputs {
        PublishInputs {
            repo_id: "continuum-ai/devstral-code-asha".to_string(),
            gene_path: PathBuf::from("/genome/asha/code/adapters.gguf"),
            base_model: "unsloth/Devstral-Small-2507-GGUF".to_string(),
            trait_kind: "code".to_string(),
            persona_name: Some("Asha".to_string()),
            project_type: Some("continuum".to_string()),
            score: Some(87),
            epochs: Some(3),
            rank: Some(16),
            lift: 0.051,
        }
    }

    // what this catches: a fully-valid layer builds a request with the right repo,
    // gene, lift, and market tags — the happy path the transport relies on.
    #[test]
    fn valid_layer_builds_a_publishable_request() {
        let req = PublishRequest::build(&good_inputs(), |_| true).expect("should build");
        assert_eq!(req.repo_id.as_str(), "continuum-ai/devstral-code-asha");
        assert!((req.lift_pct - 5.1).abs() < 1e-9);
        assert!(req.tags.contains(&"continuum:role=code".to_string()));
        assert!(req.tags.contains(&"continuum:base=devstral-small-2507-gguf".to_string()));
    }

    // what this catches: the market boundary must REFUSE a layer that didn't beat
    // baseline — the core anti-corner-cut of #99.
    #[test]
    fn non_positive_lift_is_refused() {
        for lift in [0.0, -0.02] {
            let mut i = good_inputs();
            i.lift = lift;
            assert!(matches!(
                PublishRequest::build(&i, |_| true),
                Err(PublishError::LiftGate { .. })
            ));
        }
    }

    // what this catches: malformed HF targets never reach the network.
    #[test]
    fn malformed_repo_ids_are_refused() {
        for bad in ["noSlash", "too/many/slashes", "/name", "ns/", "ns/na me", "ns/../x"] {
            let mut i = good_inputs();
            i.repo_id = bad.to_string();
            assert!(
                matches!(
                    PublishRequest::build(&i, |_| true),
                    Err(PublishError::InvalidRepoId { .. })
                ),
                "'{bad}' should be rejected"
            );
        }
        // a normal one parses.
        assert!(HfRepoId::parse("continuum-ai/qwen3-coder-30b").is_ok());
    }

    // what this catches: never publish a phantom — the gene file must exist.
    #[test]
    fn missing_gene_is_refused() {
        assert!(matches!(
            PublishRequest::build(&good_inputs(), |_| false),
            Err(PublishError::MissingGene { .. })
        ));
        let mut empty = good_inputs();
        empty.gene_path = PathBuf::new();
        assert!(matches!(
            PublishRequest::build(&empty, |_| true),
            Err(PublishError::MissingGene { .. })
        ));
    }

    // what this catches: the card's identifying fields can't be blank.
    #[test]
    fn missing_required_fields_are_refused() {
        let mut no_base = good_inputs();
        no_base.base_model = "  ".to_string();
        assert_eq!(
            PublishRequest::build(&no_base, |_| true),
            Err(PublishError::MissingField { field: "base_model" })
        );
        let mut no_trait = good_inputs();
        no_trait.trait_kind = String::new();
        assert_eq!(
            PublishRequest::build(&no_trait, |_| true),
            Err(PublishError::MissingField { field: "trait_kind" })
        );
    }
}
