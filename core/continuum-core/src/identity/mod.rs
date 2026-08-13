//! `Identity` — the substrate's universal actor identity, ORM-backed.
//!
//! Per [[airc-is-the-session-not-a-feature]] (Joel 2026-06-04):
//! "airc is CORE LEVEL. this is the session etc." + "Each UNIQUE
//! identity per persona, per you per me. Not shared." + "This is
//! like Android context and must be fixed."
//!
//! ## What this IS
//!
//! Identity is the ORM-backed actor identity that EVERY citizen on
//! the substrate carries — personas, Claude Code sessions, Codex
//! sessions, Joel-at-a-terminal, jtag CLI invocations, future web
//! users. Same shape, same table, foreign-keyable from every other
//! entity (engrams, work cards, captures, the lot).
//!
//! Per the Android-Context analogue: Identity is what makes
//! `&ctx` mean "this specific actor instance" rather than "the
//! process's global airc handle." There is no global. Code that
//! acts on the substrate either takes a `&Context` (which carries
//! an `Identity`) or it's a pure function.
//!
//! ## Why this is an Entity (not a struct + JSON file)
//!
//! Per [[no-sql-everything-through-orm-entities]] EVERY storable
//! datum goes through the ORM + entity registration. The prior
//! shape — `PersonaInstanceInfo` struct persisted as
//! `~/.continuum/personas/<name>/seed.json` — predates the
//! `#[derive(Entity)]` macro that #1519 / task #166 landed. With
//! the derive macro available, hand-rolled JSON-on-disk is
//! technical debt: untyped, non-queryable, no FK, no schema
//! migration.
//!
//! Identity uses Pattern A of the derive macro (canonical) —
//! `#[entity(primary_key)] id: Uuid` pulls in BaseEntity columns
//! (`id`, `createdAt`, `updatedAt`, `version`) automatically.
//!
//! ## Why `id == airc peer_id`
//!
//! Per [[persona-identity-derives-from-source-id]]: a persona's
//! peer_id IS its substrate identity. Same applies to every actor
//! kind — the airc Ed25519 keypair is what makes you YOU on the
//! substrate. So Identity's `id` is the peer_id directly, not a
//! separate continuum-side surrogate. Other entities that need to
//! reference an actor FK on `Identity.id` and route via airc
//! automatically.
//!
//! ## Out of scope for this slice
//!
//! - `Context` struct wrapping Identity + services + captures
//!   (Slice 2 of task #142)
//! - Bootstrap paths per IdentityKind (fresh Claude Code session
//!   minting its own Identity row + airc home; jtag CLI invocation
//!   minting ephemeral; etc. — Slice 3)
//! - `&ctx` ubiquitous refactor across substrate APIs (Slice 4)
//! - Migration of `PersonaInstanceInfo` callers to read from
//!   Identity table (Slice 1B, follow-up PR)

use continuum_orm_derive::Entity;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// The substrate's ONE canonical actor-identity type: `airc_core::PeerId`.
///
/// Every actor on the substrate — persona, agent session, human, jtag, web —
/// is identified by this single type. Per the module doctrine above ("your
/// airc cryptographic identity IS your substrate identity ... the peer_id
/// directly, not a separate continuum-side surrogate"), the canonical type is
/// airc's OWN `PeerId`: a `#[serde(transparent)]` UUIDv4 newtype
/// (`Copy + Hash + Eq + Display`, `from_uuid`/`as_uuid`) whose wire shape is the
/// bare hyphenated string — identical to what a bare `Uuid` serializes to. We
/// re-export it here so the whole crate imports identity from ONE home.
///
/// ## Do NOT define another `*Id(Uuid)` newtype for an actor
///
/// `genome::working_set::PersonaId` and `genome::recall::PeerId` were two
/// continuum-side re-inventions of exactly this concept; both were collapsed
/// onto this canonical type (2026-06-30, [[identity-one-canonical-newtype-not-bare-uuid]]).
/// A third re-invention re-opens the same drift — a persona IS a peer, so its
/// identity IS a `PeerId`; the field NAME (`persona:`, `node:`) carries the role.
///
/// For a ts-rs wire struct, type the field `PeerId` and annotate
/// `#[ts(type = "string")]` — airc's `PeerId` carries no `TS` derive by design
/// (it serializes transparently to the same string a `Uuid` would), so the
/// generated TS shape is unchanged.
pub use airc_core::PeerId;

/// What a CALLER writes when it means "that persona" — a full UUID, an 8-char
/// short-id, or a name (`"Asha"`). Deliberately NOT an identity.
///
/// ## Why this is a separate type from [`PeerId`]
///
/// Both were `String`, so nothing stopped an unresolved reference being stored,
/// compared, or handed to a subsystem as though it were an identity — and that is
/// not hypothetical. `PersonaWorkspaceRegistry::resolve_persona` exists precisely
/// to close "the loose-`String` id boundary … the defect class that fed a dead id
/// to a doomed eval" (its own words), and when this type was introduced it had
/// **one** production caller against 55 `persona_id: String` fields. The check was
/// right and almost nothing called it — the nastiest shape a check can have.
///
/// A name is not an identity: it is ambiguous (two personas can share one), it is
/// mutable, and it is only meaningful against a live roster. So the two roles get
/// two types, and the ONLY bridge between them is resolution:
///
/// ```ignore
/// let id: PeerId = registry.resolve_persona(&params.persona)?;  // the one door
/// ```
///
/// Params carry a `PersonaRef`. Everything downstream carries a [`PeerId`]. Passing
/// an unresolved reference where an identity belongs is now a type error rather
/// than a runtime surprise three subsystems away.
///
/// Wire shape is unchanged — `#[serde(transparent)]` over the string a caller
/// already sends, so no client, recipe, or stored payload has to change.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS, schemars::JsonSchema,
)]
#[ts(export, export_to = "../../../protocol/typescript/identity/PersonaRef.ts")]
#[serde(transparent)]
#[schemars(description = "A persona reference: full UUID, 8-char short-id, or name")]
pub struct PersonaRef(pub String);

impl PersonaRef {
    pub fn new(reference: impl Into<String>) -> Self {
        Self(reference.into())
    }

    /// The raw text, for the resolver and for error messages. Deliberately the ONLY
    /// accessor — there is no `as_peer_id()`, because a reference is not an identity
    /// until a roster says which one it is.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PersonaRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for PersonaRef {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for PersonaRef {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// A resolved identity is always a legal reference to itself — the direction that
/// is safe. The reverse has no `From` on purpose: it requires a roster.
impl From<PeerId> for PersonaRef {
    fn from(id: PeerId) -> Self {
        Self(id.to_string())
    }
}

/// What kind of actor this identity belongs to. The substrate
/// treats every kind symmetrically — same Identity entity, same
/// ORM table, same airc-peer routing — but the kind tag lets
/// downstream code branch when the actor type matters (e.g.,
/// cognition pipeline runs for `Persona` but not for `Jtag`).
///
/// Per [[airc-is-the-session-not-a-feature]] every value here is a
/// first-class substrate citizen; none is "second-class" or "for
/// internal use."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/identity/IdentityKind.ts")]
pub enum IdentityKind {
    /// An autonomous persona — has a name, cognition pipeline,
    /// engrams, optional LoRA genome. Bootstraps via
    /// `PersonaIdentityProvider` at substrate start.
    Persona,
    /// An external AI agent session — Claude Code instance, Codex
    /// session, Gemini, Hermes, OpenClaw, future provider. The
    /// SPECIFIC provider is carried in `Identity::agent_provider`
    /// so adding a new agent provider (GPT-5, Claude-5, whatever)
    /// doesn't churn this enum. Per the Slice-1 reviewer's
    /// extensibility concern + [[organization-purity-as-we-migrate]]
    /// + Joel 2026-06-04 ("Codex and Gemini etc. Use always
    /// lowercase").
    Agent,
    /// A human at a terminal / IDE — one row per active session
    /// (one Joel-at-laptop, another Joel-at-iMac). Bootstrapped
    /// via human-presence detection (login, IDE attach) or
    /// explicit `airc init`.
    Human,
    /// A jtag CLI invocation. Can be ephemeral (mint on each
    /// `jtag X` call, retire on exit) or long-lived (one identity
    /// per user, persisted across invocations) — TBD by Slice 5.
    Jtag,
    /// A browser tab / web user. One row per tab session.
    Web,
}

/// How this identity came into being — resumed from prior state
/// or minted fresh. Telemetry-honest per
/// [[substrate-is-a-good-citizen-on-the-host]] so operators can
/// see at a glance whether the substrate is rehydrating prior
/// citizens or spawning new ones.
///
/// Renamed from `PersonaIdentitySource` to `IdentitySource` per
/// the universal-kind shape — same enum now applies to every
/// `IdentityKind`, not just `Persona`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/identity/IdentitySource.ts")]
pub enum IdentitySource {
    /// Rehydrated from a prior session — keypair loaded from
    /// `home_path/identity.key`, ORM row already existed.
    ResumedFromDisk,
    /// Freshly minted — new keypair generated, new home_path
    /// carved out, new ORM row inserted.
    FreshlyMinted,
}

/// The substrate's universal actor identity, ORM-backed.
///
/// One row per active actor instance. Foreign-keyable from every
/// other entity that needs to record "which citizen did this" —
/// engram authorship, work-card claims, capture-sink scopes,
/// audit trails.
///
/// Pattern A of `#[derive(Entity)]` per #1519: `id` is both the
/// primary key AND the airc peer_id. The macro pulls in BaseEntity
/// columns (`createdAt`, `updatedAt`, `version`) automatically;
/// the struct only declares the kind-specific fields.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Entity)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/identity/Identity.ts")]
#[entity(collection = "identities")]
pub struct Identity {
    /// Primary key AND airc peer_id. The substrate makes no
    /// distinction — your airc cryptographic identity IS your
    /// substrate identity. Other entities FK on this.
    #[ts(type = "string")]
    #[entity(primary_key)]
    pub id: Uuid,

    /// What kind of actor this identity belongs to. Indexed so
    /// queries like "show me all live Persona identities" or
    /// "count active Claude sessions" stay O(log N).
    #[entity(indexed)]
    pub kind: IdentityKind,

    /// Human-readable label. For personas this is the persona
    /// name (Maya, Niko, ...). For Claude/Codex sessions it's a
    /// session identifier (e.g., "Claude-Opus-4.7-2026-06-04T..."
    /// — runtime decides shape). For humans it's the operator's
    /// chosen handle. Indexed for query-by-name workflows
    /// (`airc whois <name>` and similar).
    #[entity(indexed)]
    pub agent_name: String,

    /// Absolute path to this identity's airc home dir on disk.
    /// Carries the keypair (`identity.key`) and the airc state DB.
    /// String (not PathBuf) because PathBuf doesn't have native
    /// SQLite affinity; convert at the use site.
    pub home_path: String,

    /// The room this identity defaults to subscribing to at
    /// bootstrap. For personas it's the spawned-into room
    /// (continuum's default_room). For Claude/Codex it's the
    /// room their work coordinates in. Indexed for "show me
    /// every identity in room X" queries.
    #[ts(type = "string")]
    #[entity(indexed)]
    pub default_room: Uuid,

    /// Whether this identity was rehydrated from disk or minted
    /// fresh during this bootstrap. Indexed for telemetry —
    /// "what fraction of citizens are fresh this hour?" is a
    /// meaningful operator question.
    #[entity(indexed)]
    pub source: IdentitySource,

    /// For `IdentityKind::Agent` rows, names the SPECIFIC external
    /// AI provider — "claude", "codex", "gemini", "hermes",
    /// "openclaw", future. Lowercase by convention (Joel 2026-06-04
    /// "Use always lowercase"). `None` for non-Agent kinds.
    ///
    /// Why a `String` and not a sub-enum: extensibility. Adding a
    /// new provider should NOT churn the IdentityKind enum or
    /// require enum-as-JSON migration. The substrate routes
    /// agent-provider-specific logic through this string at the
    /// few sites that need it (tool-use harness, model-tier
    /// metadata).
    ///
    /// The directory layout for Agent kinds carries the provider:
    /// `citizens/agents/<provider>/<label>/airc/`. Non-Agent kinds
    /// use `citizens/<kind>/<label>/airc/` (no provider segment).
    #[entity(indexed)]
    #[ts(optional)]
    pub agent_provider: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::{OrmEntity, OrmStore};
    use std::sync::Arc;

    /// Smoke test that the derive macro generated a valid OrmEntity
    /// impl: schema parses, collection name matches `#[entity(collection)]`,
    /// every declared field (serde camelCase-renamed) is present in
    /// the schema, BaseEntity columns are auto-injected.
    #[test]
    fn identity_schema_is_derived() {
        let schema = Identity::collection_schema();
        assert_eq!(schema.collection, "identities");
        let field_names: Vec<&str> =
            schema.fields.iter().map(|f| f.name.as_str()).collect();
        // BaseEntity columns auto-injected by the derive when
        // `#[entity(primary_key)]` is on `id: Uuid`.
        assert!(field_names.contains(&"id"), "id missing");
        assert!(field_names.contains(&"createdAt"), "createdAt missing");
        assert!(field_names.contains(&"updatedAt"), "updatedAt missing");
        // Declared fields, camelCased via #[serde(rename_all = "camelCase")].
        assert!(field_names.contains(&"kind"), "kind missing");
        assert!(field_names.contains(&"agentName"), "agentName missing");
        assert!(field_names.contains(&"homePath"), "homePath missing");
        assert!(field_names.contains(&"defaultRoom"), "defaultRoom missing");
        assert!(field_names.contains(&"source"), "source missing");
        assert!(field_names.contains(&"agentProvider"), "agentProvider missing");
    }

    /// Identity round-trips through OrmStore: save, find-by-id,
    /// find-all. Proves the entity layer works for substrate's
    /// universal actor identity, the foundation of task #142's
    /// BaseUser/Context hierarchy.
    ///
    /// Query-by-kind / query-by-room (the predicate-pushdown API)
    /// is exercised when the query-builder layer lands; for now
    /// `find_all` + manual filter is the canonical read path.
    #[tokio::test]
    async fn identity_round_trips_through_orm() {
        let (adapter, _tmp) = crate::orm::store::fresh_adapter().await;
        let store: OrmStore<Identity> = OrmStore::new(Arc::clone(&adapter))
            .await
            .expect("new store");

        let shared_room = Uuid::new_v4();
        let alice = Identity {
            id: Uuid::new_v4(),
            kind: IdentityKind::Persona,
            agent_name: "Maya".to_string(),
            home_path: "/tmp/test/maya/airc".to_string(),
            default_room: shared_room,
            source: IdentitySource::FreshlyMinted,
            agent_provider: None,
        };
        let bob = Identity {
            id: Uuid::new_v4(),
            kind: IdentityKind::Agent,
            agent_name: "claude-session-X".to_string(),
            home_path: "/tmp/test/claude-x/airc".to_string(),
            default_room: shared_room,
            source: IdentitySource::FreshlyMinted,
            agent_provider: Some("claude".to_string()),
        };

        store.save(alice.id, &alice).await.expect("save alice");
        store.save(bob.id, &bob).await.expect("save bob");

        let loaded_alice = store
            .find_by_id(alice.id)
            .await
            .expect("find alice")
            .expect("alice exists");
        assert_eq!(loaded_alice.agent_name, "Maya");
        assert_eq!(loaded_alice.kind, IdentityKind::Persona);
        assert_eq!(loaded_alice.id, alice.id);
        assert_eq!(loaded_alice.default_room, shared_room);
        assert_eq!(loaded_alice.agent_provider, None);

        let all = store.find_all().await.expect("find_all");
        assert_eq!(all.len(), 2, "both identities present");

        // Manual filter by kind — exercises the round-trip end to
        // end without depending on a query-builder API. When the
        // predicate-pushdown layer lands, this becomes a single
        // filter_eq call; until then this proves the data is there
        // and decodable.
        let personas: Vec<_> = all.iter().filter(|(_, i)| i.kind == IdentityKind::Persona).collect();
        assert_eq!(personas.len(), 1);
        assert_eq!(personas[0].1.agent_name, "Maya");

        let agents: Vec<_> = all.iter().filter(|(_, i)| i.kind == IdentityKind::Agent).collect();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].1.agent_name, "claude-session-X");
        assert_eq!(agents[0].1.agent_provider.as_deref(), Some("claude"));
    }
}
