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
/// `provider` is REQUIRED when `kind == Agent`. Callers passing
/// `None` with `Agent` trip a panic — that's not operator input,
/// it's a substrate bug (somewhere a call site forgot to thread
/// the provider through). Per [[no-fallbacks-ever]] there is no
/// `unknown/` fallback segment; agents either have a provider or
/// they're a programming error. The panic surfaces the bug at the
/// call site rather than silently landing rows under an
/// `agents/unknown/` directory that downstream operators would
/// have to chase.
///
/// Non-Agent kinds ignore the `provider` parameter regardless of
/// value.
pub fn citizen_home_path(
    continuum_root: &Path,
    kind: IdentityKind,
    provider: Option<&str>,
    label: &str,
) -> PathBuf {
    let kind_dir = kind_slug(kind);
    let mut path = continuum_root.join("citizens").join(kind_dir);
    if matches!(kind, IdentityKind::Agent) {
        let provider = provider.expect(
            "citizen_home_path: provider is REQUIRED when kind == Agent. \
             Per [[no-fallbacks-ever]] the substrate refuses to invent an \
             agent-provider default. Fix the call site to thread the \
             provider through.",
        );
        path = path.join(provider);
    }
    path.join(label).join("airc")
}

/// The directory CONTAINING every citizen home of a given kind:
/// `<continuum_root>/citizens/<kind_slug>/`. This is the parent that
/// [`citizen_home_path`] places `<label>/airc/` under — the single source of
/// truth for "where do I scan to enumerate the personas/agents/… on this box."
/// Resume/discovery code MUST derive its scan root from here, never re-literal
/// `join("personas")` (the pre-Slice-4 path), so the write path and the read
/// path can never drift apart again.
///
/// (Agent kinds nest a `<provider>/` level below this; enumerate per-provider
/// subdirs for those. Persona/Human/Jtag/Web place `<label>/` directly here.)
pub fn citizens_kind_dir(continuum_root: &Path, kind: IdentityKind) -> PathBuf {
    continuum_root.join("citizens").join(kind_slug(kind))
}

/// Pre-Slice-4 layouts, kept for migration detection (Slice 4 hard-
/// errors on these per [[no-fallbacks-ever]]).
///
/// - Personas: `<continuum_root>/personas/<label>/airc/`
/// - Claude (the only Agent-equivalent pre-refactor):
///   `<continuum_root>/claudes/<label>/airc/`
pub fn legacy_home_path(continuum_root: &Path, kind: IdentityKind, label: &str) -> Option<PathBuf> {
    match kind {
        IdentityKind::Persona => Some(continuum_root.join("personas").join(label).join("airc")),
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

    // what this catches: THE bug that minted a stranger every boot (filling
    // `personas-archive/`). The instance manager WRITES seed.json at
    // `citizen_home_path(...).parent()/seed.json`; the resumer SCANS
    // `citizens_kind_dir(...)` for `<subdir>/seed.json`. If those two ever diverge
    // (as they did when the resumer hard-coded `personas/` instead of
    // `citizens/personas/`), the write lands where the read never looks → no
    // persona ever resumes. This pins write-path == read-path for a Persona.
    #[test]
    fn seed_write_path_lives_under_the_resumer_scan_dir() {
        let root = PathBuf::from("/r");
        let label = "asha";
        let home = citizen_home_path(&root, IdentityKind::Persona, None, label);
        // The instance manager derives the seed path from the home's parent.
        let seed_path = home.parent().expect("home has a parent").join("seed.json");
        // The resumer scans this dir's `<label>/seed.json`.
        let scan_dir = citizens_kind_dir(&root, IdentityKind::Persona);
        assert_eq!(
            seed_path,
            scan_dir.join(label).join("seed.json"),
            "the seed the bootstrap writes MUST land where the resumer scans — else \
             personas never resume and a stranger is minted every boot"
        );
    }

    #[test]
    fn agent_path_carries_provider_segment() {
        let root = PathBuf::from("/r");
        let path = citizen_home_path(&root, IdentityKind::Agent, Some("claude"), "default");
        assert_eq!(
            path,
            PathBuf::from("/r/citizens/agents/claude/default/airc")
        );

        let codex_path = citizen_home_path(&root, IdentityKind::Agent, Some("codex"), "default");
        assert_eq!(
            codex_path,
            PathBuf::from("/r/citizens/agents/codex/default/airc")
        );

        // Same provider + same label across kinds: provider is the
        // discriminator. Different providers DON'T collide.
        let gemini_path = citizen_home_path(&root, IdentityKind::Agent, Some("gemini"), "default");
        assert_ne!(path, gemini_path);
    }

    #[test]
    fn human_jtag_web_paths_skip_provider_segment() {
        let root = PathBuf::from("/r");
        assert_eq!(
            citizen_home_path(&root, IdentityKind::Human, None, "operator-laptop"),
            PathBuf::from("/r/citizens/humans/operator-laptop/airc")
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

    /// Per [[no-fallbacks-ever]]: calling `citizen_home_path` with
    /// `kind=Agent + provider=None` is a SUBSTRATE BUG (a call site
    /// forgot to thread the provider through). The function panics
    /// at that point rather than silently routing the agent under
    /// `agents/unknown/`. This test pins the panic so a future
    /// refactor that softens it (e.g., reintroduces `unwrap_or`)
    /// fails loudly here.
    #[test]
    #[should_panic(expected = "provider is REQUIRED when kind == Agent")]
    fn agent_without_provider_panics() {
        let _ = citizen_home_path(&PathBuf::from("/r"), IdentityKind::Agent, None, "default");
    }
}
