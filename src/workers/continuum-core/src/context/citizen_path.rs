//! Symmetric citizen-home path resolver. Slice 4 of #142.
//!
//! Per Joel 2026-06-04 ("formulaic naming of directories for all
//! users") + [[airc-is-the-session-not-a-feature]] + the
//! every-actor-is-the-same-kind-of-citizen doctrine: every
//! substrate citizen's airc home lives under one root, keyed on
//! kind + (provider for Agent) + label.
//!
//! ## Layout
//!
//! ```text
//! <continuum_root>/citizens/<kind_slug>/[<provider_slug>/]<label>/airc/
//! ```
//!
//! - `<continuum_root>` typically `~/.continuum`
//! - `<kind_slug>` lowercase plural — `personas`, `agents`, `humans`,
//!   `jtags`, `webs`
//! - `<provider_slug>` ONLY for Agent kinds — `claude`, `codex`,
//!   `gemini`, `hermes`, future. Always lowercase per Joel's
//!   directive.
//! - `<label>` the human-readable instance identifier (Maya, Niko,
//!   instance label, host handle, jtag invocation id, tab id)
//! - `airc/` the kind-agnostic airc subdirectory (`identity.key`,
//!   airc state DB)
//!
//! ## Why this lives in `context/`
//!
//! The path is the surface every Context-implementing kind uses at
//! bootstrap. Concretely: `PersonaAircRuntime`, `AgentContext`,
//! future `JtagContext` / `HumanContext` / `WebContext` all call
//! `citizen_home_path(...)` to resolve their on-disk home. ONE
//! helper, one symmetry-enforcing function, called from every
//! kind's bootstrap.

use std::path::{Path, PathBuf};

use crate::identity::IdentityKind;

/// Return the lowercase plural slug for a kind, per Joel's directive
/// ("Use always lowercase").
pub fn kind_slug(kind: IdentityKind) -> &'static str {
    match kind {
        IdentityKind::Persona => "personas",
        IdentityKind::Agent => "agents",
        IdentityKind::Human => "humans",
        IdentityKind::Jtag => "jtags",
        IdentityKind::Web => "webs",
    }
}

/// Resolve the canonical airc home directory for a citizen.
///
/// Layout:
/// - Non-Agent kinds:
///   `<continuum_root>/citizens/<kind_slug>/<label>/airc/`
/// - Agent kind:
///   `<continuum_root>/citizens/agents/<provider>/<label>/airc/`
///
/// `provider` is REQUIRED when `kind == Agent` and ignored otherwise.
/// Callers passing `None` with `Agent` get a path that includes an
/// `unknown/` segment — that's deliberate, the substrate doesn't
/// invent a provider. Per [[no-fallbacks-ever]] the agent_provider
/// is operator data, not substrate-defaulted.
pub fn citizen_home_path(
    continuum_root: &Path,
    kind: IdentityKind,
    provider: Option<&str>,
    label: &str,
) -> PathBuf {
    let kind_dir = kind_slug(kind);
    let mut path = continuum_root.join("citizens").join(kind_dir);
    if matches!(kind, IdentityKind::Agent) {
        path = path.join(provider.unwrap_or("unknown"));
    }
    path.join(label).join("airc")
}

/// Pre-Slice-4 layouts, kept for migration detection (Slice 4 hard-
/// errors on these per [[no-fallbacks-ever]]).
///
/// - Personas: `<continuum_root>/personas/<label>/airc/`
/// - Claude (the only Agent-equivalent pre-refactor):
///   `<continuum_root>/claudes/<label>/airc/`
pub fn legacy_home_path(
    continuum_root: &Path,
    kind: IdentityKind,
    label: &str,
) -> Option<PathBuf> {
    match kind {
        IdentityKind::Persona => {
            Some(continuum_root.join("personas").join(label).join("airc"))
        }
        IdentityKind::Agent => {
            // Pre-Slice-4 there was only Claude under `claudes/`.
            // Codex/Gemini/etc. didn't have layouts to migrate; they
            // only exist post-Slice-4.
            Some(continuum_root.join("claudes").join(label).join("airc"))
        }
        IdentityKind::Human | IdentityKind::Jtag | IdentityKind::Web => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn persona_path_is_symmetric() {
        let root = PathBuf::from("/r");
        let path = citizen_home_path(&root, IdentityKind::Persona, None, "maya");
        assert_eq!(path, PathBuf::from("/r/citizens/personas/maya/airc"));
    }

    #[test]
    fn agent_path_carries_provider_segment() {
        let root = PathBuf::from("/r");
        let path = citizen_home_path(
            &root,
            IdentityKind::Agent,
            Some("claude"),
            "default",
        );
        assert_eq!(
            path,
            PathBuf::from("/r/citizens/agents/claude/default/airc")
        );

        let codex_path = citizen_home_path(
            &root,
            IdentityKind::Agent,
            Some("codex"),
            "default",
        );
        assert_eq!(
            codex_path,
            PathBuf::from("/r/citizens/agents/codex/default/airc")
        );

        // Same provider + same label across kinds: provider is the
        // discriminator. Different providers DON'T collide.
        let gemini_path = citizen_home_path(
            &root,
            IdentityKind::Agent,
            Some("gemini"),
            "default",
        );
        assert_ne!(path, gemini_path);
    }

    #[test]
    fn human_jtag_web_paths_skip_provider_segment() {
        let root = PathBuf::from("/r");
        assert_eq!(
            citizen_home_path(&root, IdentityKind::Human, None, "joel-laptop"),
            PathBuf::from("/r/citizens/humans/joel-laptop/airc")
        );
        assert_eq!(
            citizen_home_path(&root, IdentityKind::Jtag, None, "inv-001"),
            PathBuf::from("/r/citizens/jtags/inv-001/airc")
        );
        assert_eq!(
            citizen_home_path(&root, IdentityKind::Web, None, "tab-42"),
            PathBuf::from("/r/citizens/webs/tab-42/airc")
        );
    }

    #[test]
    fn legacy_paths_match_pre_slice_4_layout() {
        let root = PathBuf::from("/r");
        assert_eq!(
            legacy_home_path(&root, IdentityKind::Persona, "maya"),
            Some(PathBuf::from("/r/personas/maya/airc"))
        );
        assert_eq!(
            legacy_home_path(&root, IdentityKind::Agent, "default"),
            Some(PathBuf::from("/r/claudes/default/airc"))
        );
        // Non-persona-non-Agent kinds had no pre-Slice-4 layout.
        assert_eq!(legacy_home_path(&root, IdentityKind::Human, "x"), None);
        assert_eq!(legacy_home_path(&root, IdentityKind::Jtag, "x"), None);
        assert_eq!(legacy_home_path(&root, IdentityKind::Web, "x"), None);
    }
}
