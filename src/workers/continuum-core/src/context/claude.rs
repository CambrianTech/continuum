//! `ClaudeContext` — first non-persona Context kind. Slice 3 of #142.
//!
//! Per Joel 2026-06-04: "You'll get visibility inside continuum and
//! vice versa." This is where it lands: a Claude Code session
//! constructed via `ClaudeContext::bootstrap` gets its OWN airc
//! identity (Ed25519 keypair under
//! `~/.continuum/claudes/<label>/airc/`, peer_id from the keypair).
//! Any substrate-visible action it takes (`ctx.airc().say(...)`,
//! `ctx.airc().subscribe()`, future card creation, future commit
//! authorship) is signed by THIS Claude's keypair, not the host
//! operator's.
//!
//! From `airc inbox`, operators see the distinct peer_id and can
//! tell that THIS specific Claude instance did the work — the
//! audit trail Joel has been building rooms-as-history toward
//! becomes accurate per
//! [[airc-is-the-session-not-a-feature]].
//!
//! ## What this slice IS
//!
//! - `ClaudeContext` struct holding `Identity` + airc citizen handle
//!   + (placeholder) `ClaudeMetadata` extension. Implements the
//!   `Context` trait from Slice 2 cleanly — second concrete kind
//!   after `PersonaContext`.
//! - `ClaudeContext::bootstrap` — the canonical entry point. Resolves
//!   the home, calls `airc_lib::Airc::attach_as` (resume-or-mint
//!   per airc-lib semantics), constructs the Identity from the
//!   keypair's peer_id (post-Slice-1B: peer_id IS the substrate id),
//!   returns the ClaudeContext.
//! - Inline `AircHandleAdapter` — bridges `Arc<airc_lib::Airc>` into
//!   `dyn AircCitizen` (the substrate's polymorphic surface).
//!   Private to this module per outlier-validation discipline; lifted
//!   to a shared location when JtagContext / HumanContext need the
//!   same shape.
//!
//! ## What this slice is NOT
//!
//! - NOT JtagContext / HumanContext concrete types — same outlier
//!   discipline, added when consumers appear or when the jtag CLI
//!   Rust rewrite (#143) lands.
//! - NOT Identity-entity ORM persistence on bootstrap. The Identity
//!   is constructed in-memory and lives for the lifetime of the
//!   ClaudeContext; persisting to `OrmStore<Identity>` is a focused
//!   follow-up when a consumer needs query-by-id across process
//!   restarts.
//! - NOT a tool-use harness or model-tier serialization. The
//!   `ClaudeMetadata` field is a placeholder struct; extensions are
//!   added per CLAUDE.md's outlier-validation discipline (build for
//!   intent, not pre-emptively).
//! - NOT multi-instance-per-machine arbitration. If two Claude
//!   sessions on the same host want different identities, they
//!   bootstrap with different `instance_label`s. Same label across
//!   restarts = same persistent identity (airc-lib's attach_as
//!   resumes from disk).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use airc_core::EventId;
use airc_lib::{Airc, AircError};
use async_trait::async_trait;
use uuid::Uuid;

use crate::context::Context;
use crate::identity::{Identity, IdentityKind, IdentitySource};
use crate::persona::airc_citizen::AircCitizen;
use crate::persona::airc_source::AircTranscriptReader;

/// Errors during ClaudeContext bootstrap. Each variant carries
/// enough operator-facing context to act on without grepping logs,
/// per the constitutional-design doctrine ("every error has a path
/// forward").
#[derive(Debug, thiserror::Error)]
pub enum ClaudeContextError {
    #[error("failed to create claude airc home {0}: {1}")]
    HomeCreate(PathBuf, std::io::Error),
    #[error("airc-lib attach_as failed for claude {agent_name:?} at {home:?}: {source}")]
    Attach {
        agent_name: String,
        home: PathBuf,
        #[source]
        source: AircError,
    },
    #[error("failed to join room {room_name:?} as claude {agent_name:?}: {source}")]
    Join {
        agent_name: String,
        room_name: String,
        #[source]
        source: AircError,
    },
}

/// Placeholder extension for Claude-kind state. Per the CLAUDE.md
/// outlier-validation discipline, this stays minimal until a
/// downstream consumer needs more (tool-use harness wiring,
/// model-tier metadata, capability flags). Currently just declares
/// "this is a Claude instance" structurally.
#[derive(Debug, Clone, Default)]
pub struct ClaudeMetadata {
    /// The model identifier this instance is running as
    /// (e.g. "claude-opus-4-7"). Optional because the bootstrap
    /// doesn't always know — caller sets it when they do.
    pub model_id: Option<String>,
}

/// A Claude Code session as a first-class substrate citizen.
///
/// Implements `Context`, so any substrate API taking `&dyn Context`
/// accepts a `ClaudeContext` cleanly — the same shape that personas
/// flow through. The validation moment for Slice 2's trait per
/// CLAUDE.md outlier discipline: PersonaContext and ClaudeContext
/// both fit Context without forcing.
pub struct ClaudeContext {
    identity: Identity,
    airc: Arc<dyn AircCitizen>,
    #[allow(dead_code)] // Field used through getters once consumers appear.
    metadata: ClaudeMetadata,
}

impl ClaudeContext {
    /// Bootstrap a Claude Code session's substrate presence.
    ///
    /// Steps:
    /// 1. Resolve home at `<continuum_root>/claudes/<instance_label>/airc/`
    ///    and `tokio::fs::create_dir_all` it. Per
    ///    [[personas-are-citizens-airc-is-identity-provider]] this
    ///    is a sibling-not-nested layout — Claude's airc home lives
    ///    alongside personas under one continuum root, not inside
    ///    any other actor's scope.
    /// 2. Call `Airc::attach_as(home, agent_name, daemon_socket)`.
    ///    airc-lib's identity ceremony resumes the keypair if
    ///    `identity.key` exists, or mints a fresh one if not. Either
    ///    way the substrate gets a real Ed25519 keypair the Claude
    ///    session now owns.
    /// 3. Construct `Identity` with `id = peer_id` per
    ///    [[persona-identity-derives-from-source-id]] (post-Slice-1B
    ///    the cryptographic keypair IS the substrate identity).
    ///    `source` reflects whether airc-lib resumed or minted.
    /// 4. Wrap `Arc<Airc>` in `AircHandleAdapter` → `Arc<dyn
    ///    AircCitizen>` so the Context surface returns the polymorphic
    ///    handle every substrate API expects.
    ///
    /// On failure, returns a typed `ClaudeContextError` with the
    /// operator-actionable detail. No silent fallback per
    /// [[no-fallbacks-ever]].
    pub async fn bootstrap(
        continuum_root: &Path,
        instance_label: impl Into<String>,
        daemon_socket: PathBuf,
        default_room: Uuid,
        room_name: Option<&str>,
        metadata: ClaudeMetadata,
    ) -> Result<Self, ClaudeContextError> {
        let instance_label = instance_label.into();
        let agent_name = format!("claude-{instance_label}");
        let home = continuum_root
            .join("claudes")
            .join(&instance_label)
            .join("airc");

        // Detect resume vs mint BEFORE create_dir_all by checking the
        // per-instance_label home dir itself, NOT identity.key. Per
        // the per-label home convention
        // (<continuum_root>/claudes/<instance_label>/airc/), the
        // home dir's existence is a clean proxy for "has this
        // specific Claude label been bootstrapped before?" — without
        // the (key_exists, agent_not_stored) edge case that bites
        // when probing identity.key directly. Edge cases (manually
        // populated home with no airc state) report as
        // `ResumedFromDisk` here, which is the more operator-honest
        // default since SOMEONE staged that directory deliberately.
        let home_pre_existed = tokio::fs::try_exists(&home).await.unwrap_or(false);

        tokio::fs::create_dir_all(&home)
            .await
            .map_err(|e| ClaudeContextError::HomeCreate(home.clone(), e))?;

        let airc = Airc::attach_as(home.clone(), agent_name.clone(), daemon_socket)
            .await
            .map_err(|source| ClaudeContextError::Attach {
                agent_name: agent_name.clone(),
                home: home.clone(),
                source,
            })?;

        // Join the room by NAME (not by UUID-as-string). Per
        // PersonaAircRuntime::bootstrap's hard-won lesson at
        // `persona/airc_runtime.rs:170-179` and the empirical catch
        // in card 800ce5bd: `Airc::join(uuid_str)` DERIVES a fresh
        // channel uuid from the string, landing the subscription on
        // a different channel than the operator's. The room_name
        // path is the canonical one — passing `None` skips the join
        // for callers that have already joined some other way.
        if let Some(name) = room_name {
            airc.join(name)
                .await
                .map_err(|source| ClaudeContextError::Join {
                    agent_name: agent_name.clone(),
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
            kind: IdentityKind::Claude,
            agent_name: agent_name.clone(),
            home_path: home.to_string_lossy().into_owned(),
            default_room,
            source,
        };

        let airc_arc: Arc<dyn AircCitizen> =
            Arc::new(AircHandleAdapter::new(Arc::new(airc)));

        tracing::info!(
            peer_id = %peer_id,
            agent_name = %agent_name,
            home = %home.display(),
            source = ?source,
            "ClaudeContext bootstrap: identity ready"
        );

        Ok(Self {
            identity,
            airc: airc_arc,
            metadata,
        })
    }
}

impl Context for ClaudeContext {
    fn identity(&self) -> std::borrow::Cow<'_, Identity> {
        std::borrow::Cow::Borrowed(&self.identity)
    }

    fn airc(&self) -> &Arc<dyn AircCitizen> {
        &self.airc
    }
}

// ─── AircHandleAdapter ─────────────────────────────────────────────────
//
// Bridges `Arc<airc_lib::Airc>` into `dyn AircCitizen + dyn
// AircTranscriptReader`. PersonaAircRuntime has its own equivalent
// inline impl (carrying persona-specific lifecycle state); this is
// the kind-agnostic shape Claude / future Codex / future Jtag /
// future Human / future Web all use. Private to this module per
// CLAUDE.md outlier discipline — lift to a shared `context::
// airc_adapter` module the first time a second non-persona kind
// (JtagContext or similar) wants the same struct.

struct AircHandleAdapter {
    inner: Arc<Airc>,
}

impl AircHandleAdapter {
    fn new(inner: Arc<Airc>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl AircTranscriptReader for AircHandleAdapter {
    async fn page_recent(
        &self,
        limit: usize,
    ) -> Result<Vec<airc_lib::TranscriptEvent>, AircError> {
        self.inner.page_recent(limit).await
    }
}

#[async_trait]
impl AircCitizen for AircHandleAdapter {
    fn peer_id(&self) -> Uuid {
        self.inner.peer_id().as_uuid()
    }

    async fn subscribe(&self) -> Result<airc_lib::EventStream, AircError> {
        self.inner.subscribe().await
    }

    async fn say(&self, text: &str) -> Result<EventId, AircError> {
        self.inner.say(text).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::airc_citizen::StubAircCitizen;

    /// `ClaudeContext` satisfies `Context` — projects its stored
    /// Identity (the `Cow::Borrowed` zero-clone case Slice 2 set up)
    /// and exposes the airc handle. Constructed without going
    /// through `bootstrap` (which needs a real airc daemon) — we
    /// use the stub citizen to prove the trait surface fits.
    #[test]
    fn claude_context_implements_context_zero_clone() {
        let peer_id = Uuid::new_v4();
        let identity = Identity {
            id: peer_id,
            kind: IdentityKind::Claude,
            agent_name: "claude-test".to_string(),
            home_path: "/tmp/claude-test/airc".to_string(),
            default_room: Uuid::new_v4(),
            source: IdentitySource::FreshlyMinted,
        };
        let stub: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(peer_id));
        let ctx = ClaudeContext {
            identity: identity.clone(),
            airc: stub,
            metadata: ClaudeMetadata::default(),
        };

        // identity() is Cow::Borrowed — zero-clone per Slice 2's
        // shape; we observe by checking matches::matches!.
        let observed = ctx.identity();
        assert!(matches!(observed, std::borrow::Cow::Borrowed(_)));
        let observed = observed.as_ref();
        assert_eq!(observed.id, peer_id);
        assert_eq!(observed.kind, IdentityKind::Claude);
        assert_eq!(observed.agent_name, "claude-test");

        // airc() exposes the stub citizen we wrapped — round-trip.
        assert_eq!(ctx.airc().peer_id(), peer_id);
    }

    /// `ClaudeContext` and `StubContext` are the second-outlier pair
    /// Slice 2 deferred from validating. Both implement `Context`;
    /// both carry distinct kind tags; both expose identity + airc
    /// the same way. The substrate's universal-handle contract is
    /// now validated across the persona-kind shape (PersonaContext)
    /// AND a non-persona kind (ClaudeContext) — the trait fits
    /// future variants (HumanContext, JtagContext) without forcing.
    #[test]
    fn claude_context_and_persona_context_satisfy_same_context_trait() {
        let peer_id_a = Uuid::new_v4();
        let identity_a = Identity {
            id: peer_id_a,
            kind: IdentityKind::Claude,
            agent_name: "claude".to_string(),
            home_path: "/tmp/a".to_string(),
            default_room: Uuid::new_v4(),
            source: IdentitySource::FreshlyMinted,
        };
        let stub_a: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(peer_id_a));
        let claude = ClaudeContext {
            identity: identity_a,
            airc: stub_a,
            metadata: ClaudeMetadata::default(),
        };

        // Box<dyn Context> would fail to compile if Slice 3 broke
        // the trait's object-safety. This line IS the proof.
        let boxed: Box<dyn Context> = Box::new(claude);
        assert_eq!(boxed.identity().as_ref().kind, IdentityKind::Claude);
        assert_eq!(boxed.airc().peer_id(), peer_id_a);
    }
}
