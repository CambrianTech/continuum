//! `AgentContext` — substrate citizen for external AI agents.
//!
//! Slice 4 of #142 generalizes the Slice-3 `ClaudeContext` into a
//! provider-parameterized `AgentContext`. Same shape, same bootstrap
//! contract — but `provider` carries which external AI flavor this
//! session is (Claude, Codex, Gemini, Hermes, OpenClaw, future).
//! Per Joel 2026-06-04: "What about Codex and Gemini etc.? Use
//! always lowercase."
//!
//! ## Why provider as a `String` not a sub-enum
//!
//! Extensibility. Adding a new agent provider (GPT-5, Claude-5,
//! some company's bespoke agent) should NOT require a substrate
//! release that ships an enum variant. The few sites that branch
//! on provider (future tool-use harness, model-tier metadata) match
//! the string. Per the Slice-1 reviewer's IdentityKind extensibility
//! concern.
//!
//! ## Bootstrap flow
//!
//! 1. Resolve home via the symmetric helper
//!    `citizen_home_path(continuum_root, IdentityKind::Agent,
//!     Some(provider), instance_label)` →
//!    `<continuum_root>/citizens/agents/<provider>/<label>/airc/`.
//! 2. Migration check: if the legacy path exists for `provider ==
//!    "claude"` (`<continuum_root>/claudes/<label>/airc/`), hard-
//!    error with an actionable `mv` command per
//!    [[no-fallbacks-ever]].
//! 3. `tokio::fs::create_dir_all`. Capture pre-existence of the home
//!    BEFORE mkdir for honest resume/mint telemetry.
//! 4. `airc_lib::Airc::attach_as(home, agent_name, daemon_socket)`.
//! 5. Optional `Airc::join(room_name)` per the name-derives-channel
//!    discipline (see PersonaAircRuntime's hard-won lesson +
//!    PR #1524's review fix).
//! 6. Construct `Identity { id: peer_id, kind: Agent, agent_name,
//!    home_path, default_room, source, agent_provider:
//!    Some(provider) }`.
//! 7. Wrap `Arc<Airc>` in the shared `AircHandleAdapter`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use airc_lib::{Airc, AircError};
use uuid::Uuid;

use crate::context::airc_adapter::AircHandleAdapter;
use crate::context::citizen_path::{citizen_home_path, legacy_home_path};
use crate::context::Context;
use crate::identity::{Identity, IdentityKind, IdentitySource};
use crate::persona::airc_citizen::AircCitizen;

#[derive(Debug, thiserror::Error)]
pub enum AgentContextError {
    #[error(
        "legacy citizen home detected at {legacy:?} — Slice 4 of #142 moved \
         agents under `citizens/agents/<provider>/<label>/airc/`. To migrate \
         this agent's identity AND keep its peer_id stable, run:\n\
         \n  mkdir -p {new_parent:?} && mv {legacy:?} {new:?}\n\n\
         Then re-run. Per [[no-fallbacks-ever]] the substrate refuses to \
         silently use the legacy path."
    )]
    LegacyLayoutDetected {
        legacy: PathBuf,
        new: PathBuf,
        new_parent: PathBuf,
    },
    #[error("failed to create agent airc home {0}: {1}")]
    HomeCreate(PathBuf, std::io::Error),
    #[error(
        "airc-lib attach_as failed for agent provider={provider:?} \
         label={instance_label:?} at {home:?}: {source}"
    )]
    Attach {
        provider: String,
        instance_label: String,
        home: PathBuf,
        #[source]
        source: AircError,
    },
    #[error(
        "failed to join room {room_name:?} as agent provider={provider:?} \
         label={instance_label:?}: {source}"
    )]
    Join {
        provider: String,
        instance_label: String,
        room_name: String,
        #[source]
        source: AircError,
    },
}

/// Per-agent extension metadata. Provider-specific (Claude's
/// model_id is `claude-opus-4-7`; Codex's is its own identifier).
/// Stays minimal until a downstream consumer needs more (tool-use
/// harness wiring, capability flags) per CLAUDE.md outlier
/// discipline.
#[derive(Debug, Clone, Default)]
pub struct AgentMetadata {
    /// The model identifier this instance is running as
    /// (e.g. `"claude-opus-4-7"`, `"gpt-5"`, `"gemini-2.5"`).
    /// Optional because the bootstrap may not know — caller sets
    /// it when they do.
    pub model_id: Option<String>,
}

/// An external AI agent session as a first-class substrate citizen.
/// Replaces Slice 3's `ClaudeContext`; provider is now a parameter,
/// so the same struct handles Claude / Codex / Gemini / Hermes /
/// any future provider without a per-provider type.
///
/// Implements `Context` cleanly with `Cow::Borrowed(&self.identity)`
/// (Slice-2 zero-clone shape).
pub struct AgentContext {
    identity: Identity,
    airc: Arc<dyn AircCitizen>,
    #[allow(dead_code)] // Field used through getters once consumers appear.
    metadata: AgentMetadata,
}

impl AgentContext {
    /// Bootstrap an external AI agent's substrate presence.
    ///
    /// `provider` names the agent flavor (e.g. `"claude"`,
    /// `"codex"`, `"gemini"`). Lowercase by convention; callers
    /// SHOULD normalize before passing. The substrate does NOT
    /// auto-lowercase — that would mask caller bugs.
    pub async fn bootstrap(
        continuum_root: &Path,
        provider: impl Into<String>,
        instance_label: impl Into<String>,
        daemon_socket: PathBuf,
        default_room: Uuid,
        room_name: Option<&str>,
        metadata: AgentMetadata,
    ) -> Result<Self, AgentContextError> {
        let provider = provider.into();
        let instance_label = instance_label.into();
        let agent_name = format!("{provider}-{instance_label}");

        // Migration check — if a legacy Slice-3 layout exists for
        // this provider+label, refuse silently using it. Per
        // [[no-fallbacks-ever]] the substrate hard-errors with the
        // exact `mv` command. Only the `"claude"` provider has a
        // legacy layout (`<root>/claudes/<label>/airc/`) since
        // Slice 3 only supported Claude.
        if provider == "claude" {
            if let Some(legacy) =
                legacy_home_path(continuum_root, IdentityKind::Agent, &instance_label)
            {
                if tokio::fs::try_exists(&legacy).await.unwrap_or(false) {
                    let new = citizen_home_path(
                        continuum_root,
                        IdentityKind::Agent,
                        Some(&provider),
                        &instance_label,
                    );
                    let new_parent = new
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| new.clone());
                    return Err(AgentContextError::LegacyLayoutDetected {
                        legacy,
                        new,
                        new_parent,
                    });
                }
            }
        }

        let home = citizen_home_path(
            continuum_root,
            IdentityKind::Agent,
            Some(&provider),
            &instance_label,
        );

        // Honest resume/mint detection: check whether the per-label
        // home dir existed BEFORE create_dir_all. Per Slice 3
        // review fix #1.
        let home_pre_existed = tokio::fs::try_exists(&home).await.unwrap_or(false);

        tokio::fs::create_dir_all(&home)
            .await
            .map_err(|e| AgentContextError::HomeCreate(home.clone(), e))?;

        let airc = Airc::attach_as(home.clone(), agent_name.clone(), daemon_socket)
            .await
            .map_err(|source| AgentContextError::Attach {
                provider: provider.clone(),
                instance_label: instance_label.clone(),
                home: home.clone(),
                source,
            })?;

        // Join by NAME (not UUID-as-string) per the recurring hazard
        // documented in PersonaAircRuntime + Slice 3 review fix #2.
        if let Some(name) = room_name {
            airc.join(name)
                .await
                .map_err(|source| AgentContextError::Join {
                    provider: provider.clone(),
                    instance_label: instance_label.clone(),
                    room_name: name.to_string(),
                    source,
                })?;
        }

        let peer_id = airc.peer_id().as_uuid();
        let source = if home_pre_existed {
            IdentitySource::ResumedFromDisk
        } else {
            IdentitySource::FreshlyMinted
        };

        let identity = Identity {
            id: peer_id,
            kind: IdentityKind::Agent,
            agent_name: agent_name.clone(),
            home_path: home.to_string_lossy().into_owned(),
            default_room,
            source,
            agent_provider: Some(provider.clone()),
        };

        let airc_arc: Arc<dyn AircCitizen> =
            Arc::new(AircHandleAdapter::new(Arc::new(airc)));

        tracing::info!(
            peer_id = %peer_id,
            agent_name = %agent_name,
            provider = %provider,
            home = %home.display(),
            source = ?source,
            "AgentContext bootstrap: identity ready"
        );

        Ok(Self {
            identity,
            airc: airc_arc,
            metadata,
        })
    }
}

impl Context for AgentContext {
    fn identity(&self) -> std::borrow::Cow<'_, Identity> {
        std::borrow::Cow::Borrowed(&self.identity)
    }

    fn airc(&self) -> &Arc<dyn AircCitizen> {
        &self.airc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::airc_citizen::StubAircCitizen;

    fn fixture_identity(provider: &str) -> Identity {
        let peer_id = Uuid::new_v4();
        Identity {
            id: peer_id,
            kind: IdentityKind::Agent,
            agent_name: format!("{provider}-test"),
            home_path: format!("/tmp/agent-test/{provider}/airc"),
            default_room: Uuid::new_v4(),
            source: IdentitySource::FreshlyMinted,
            agent_provider: Some(provider.to_string()),
        }
    }

    fn fixture_ctx(identity: Identity) -> AgentContext {
        let peer_id = identity.id;
        let stub: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(peer_id));
        AgentContext {
            identity,
            airc: stub,
            metadata: AgentMetadata::default(),
        }
    }

    #[test]
    fn agent_context_implements_context_zero_clone() {
        let identity = fixture_identity("claude");
        let id = identity.id;
        let ctx = fixture_ctx(identity);

        let observed = ctx.identity();
        assert!(matches!(observed, std::borrow::Cow::Borrowed(_)));
        let observed = observed.as_ref();
        assert_eq!(observed.id, id);
        assert_eq!(observed.kind, IdentityKind::Agent);
        assert_eq!(observed.agent_provider.as_deref(), Some("claude"));
        assert_eq!(ctx.airc().peer_id(), id);
    }

    #[test]
    fn agent_context_handles_multiple_providers_uniformly() {
        // Claude, Codex, Gemini all flow through the same
        // AgentContext shape. The substrate-level proof that the
        // generalization holds: each constructs identically, each
        // satisfies Context via dynamic dispatch.
        for provider in ["claude", "codex", "gemini", "hermes"] {
            let ctx = fixture_ctx(fixture_identity(provider));
            let boxed: Box<dyn Context> = Box::new(ctx);
            assert_eq!(boxed.identity().as_ref().kind, IdentityKind::Agent);
            assert_eq!(
                boxed.identity().as_ref().agent_provider.as_deref(),
                Some(provider)
            );
        }
    }
}
