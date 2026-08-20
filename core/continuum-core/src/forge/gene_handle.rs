//! `forge::gene_handle` — the **node-aware** gene handle (Contract C, Pass 4).
//!
//! Continuum holds **handles, not bytes** (FORGE-CUSTODIAN-CONTRACT.md §4): a
//! produced gene is referenced by a handle the *producer records*, never guessed
//! from disk (the PEFT `adapter_config.json:base_model_name_or_path` is an HF id
//! that never string-matches the served continuum registry id — see
//! `adapter_manifest`). [`TrainedAdapter`](super::adapter_manifest::TrainedAdapter)
//! is that record *locally*. [`GeneHandle`] is its **grid extension**: the same
//! honest gene→served-model record, plus enough to locate, attribute, and
//! trust-gate the gene when it crosses node boundaries.
//!
//! On the grid the gene bytes stay on the forge node under its `save_directory`;
//! what crosses back to a requester is a `GeneHandle`. To page it in, the
//! requester either leases inference from the holding node (text-only,
//! `[[compute-lease-boundary]]`) or fetches the bytes into its own serving node
//! gated by `trust_scope` (the genome-market exchange, §6). The handle carries
//! exactly enough to make that choice — and nothing about the bytes themselves.
//!
//! ## Why these field types (the #17 reconciliation, in the forge context)
//! §4 sketched `locator: HandleRef`, `provenance: AlloyHash`, `trust_scope:
//! TrustTier`. Reconciled against what actually exists in tree:
//! - **locator** is NOT a [`HandleRef`](crate::runtime::cell_shapes::HandleRef):
//!   that type is a *state-correlation envelope* (owner/id/type_tag) for live
//!   stateful sequences, the wrong semantics for a gene at rest. Nor is it a
//!   [`CommandUri`](crate::routing::command_uri::CommandUri): that addresses a
//!   *command* (its `path` is a command path like `data/list`), not bytes on a
//!   filesystem. A gene locator is its own honest concept — byte custody is
//!   `{which node, what path}` — so [`GeneLocator`] is a structured enum, not a
//!   borrowed handle and not a stringly-typed URI. `Node`'s peer maps onto
//!   GRID-ADDRESSING-AND-ROUTING when Pass 6 wires transport; the structured form
//!   stays the source of truth (structured > stringly-typed, round-trips clean).
//! - **provenance** is [`AlloyHash`] — a newtype over the `sha256:…` content hash
//!   the forge-alloy spec already uses as a `String` on
//!   [`ForgeArtifact`](super::artifact::ForgeArtifact), wrapped so it can't be
//!   confused with any other string (mirrors `PeerId(Uuid)`). `None` until the
//!   gene is alloy-attested.
//! - **trust_scope** is [`TrustLevel`] (the GridTrustAuthPolicy boundary enum —
//!   Blocked/Provisional/Trusted/Owner), reused verbatim; there is no separate
//!   `TrustTier` type.
//!
//! This pass introduces the TYPE and its honest projection from the local
//! manifest record. It does NOT wire grid transport (Pass 6) or the endpoint
//! table (Pass 5) — those consume `GeneHandle`; here we make it exist and prove
//! it round-trips the wire (it must, to cross back to a requester).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

use super::adapter_manifest::TrainedAdapter;
use crate::identity::PeerId;
use crate::modules::grid::node::TrustLevel;

/// Content-addressed provenance of a gene — the forge-alloy `sha256:…` hash of
/// the populated artifact (FORGE-ALLOY-SPEC: *what it is, how it was made*). A
/// newtype over the `String` form so the type system catches it being swapped
/// with any other id (mirrors [`PeerId`]). Transparent on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/AlloyHash.ts",
    type = "string"
)]
pub struct AlloyHash(pub String);

impl AlloyHash {
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where a gene's bytes physically live — byte custody, structured.
///
/// `Local` is the degenerate single-node case (today's `TrainedAdapter.path`):
/// the bytes are on THIS node's filesystem, no remote fetch. `Node` carries the
/// holding peer + its custody-relative path, so a requester can decide between a
/// compute-lease and a trust-gated byte fetch. The locator deliberately does NOT
/// duplicate `node` as a sibling field on [`GeneHandle`] (a top-level `node`
/// would have to lie — point at self — for a local gene); ask [`GeneHandle::node`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/forge/GeneLocator.ts"
)]
#[serde(tag = "where", rename_all = "lowercase")]
pub enum GeneLocator {
    /// Bytes on this node's filesystem — no remote fetch needed.
    Local { path: PathBuf },
    /// Bytes held by a remote forge node, at a custody-relative path.
    Node {
        // Canonical airc PeerId (serde-transparent Uuid, no ts-rs derive) → string.
        #[ts(type = "string")]
        node: PeerId,
        path: String,
    },
}

/// The node-aware gene handle — `TrainedAdapter` plus what it takes to locate,
/// attribute, and trust-gate the gene across the grid. Crosses the wire back to a
/// requester; the bytes never do (FORGE-CUSTODIAN-CONTRACT.md §4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/forge/GeneHandle.ts")]
pub struct GeneHandle {
    /// The gene's name — how a page-in request and logs refer to it (same role as
    /// `TrainedAdapter.alias`).
    pub alias: String,
    /// The CONTINUUM base model id this gene was trained for — the association the
    /// serving daemon filters on (NOT the PEFT HF id; see `adapter_manifest`).
    pub base_model_id: String,
    /// Where the bytes live (local fs, or a remote node + custody path).
    pub locator: GeneLocator,
    /// What it is / how it was made — the forge-alloy attestation. `None` until
    /// the gene is alloy-attested (a locally-forged gene predates its alloy).
    #[serde(default)]
    pub provenance: Option<AlloyHash>,
    /// The GridTrustAuthPolicy boundary this gene may cross. A locally-forged gene
    /// is `Owner` (own node); a gene admitted from the grid carries the trust it
    /// was admitted at — the hard gate Pass 5/6 route on before any byte transfer.
    pub trust_scope: TrustLevel,
}

impl GeneHandle {
    /// Which node holds the bytes — `None` for a [`GeneLocator::Local`] gene (it's
    /// here), `Some(peer)` for a [`GeneLocator::Node`] gene. The accessor that
    /// keeps `node` out of the top-level struct where it would have to lie.
    pub fn node(&self) -> Option<PeerId> {
        match &self.locator {
            GeneLocator::Local { .. } => None,
            GeneLocator::Node { node, .. } => Some(*node),
        }
    }

    /// Is the gene resident on this node (no remote fetch / lease needed)?
    pub fn is_local(&self) -> bool {
        matches!(self.locator, GeneLocator::Local { .. })
    }
}

impl TrainedAdapter {
    /// Project this local manifest record into a node-aware [`GeneHandle`]: the
    /// bytes are local ([`GeneLocator::Local`]), the gene is `Owner`-scoped (it
    /// was forged on this node), and it carries no alloy provenance yet (`None` —
    /// a locally-forged gene predates its attestation). This is the honest local
    /// degenerate case; Pass 5/6 mint `Node`-located, grid-trust-scoped handles.
    pub fn as_gene_handle(&self) -> GeneHandle {
        GeneHandle {
            alias: self.alias.clone(),
            base_model_id: self.base_model_id.clone(),
            locator: GeneLocator::Local {
                path: self.path.clone(),
            },
            provenance: None,
            trust_scope: TrustLevel::Owner,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // what this catches: the local projection is honest — a manifest record
    // becomes a Local-located, Owner-scoped, un-attested handle. If the locator
    // ever defaulted to Node or the scope to anything less than Owner, a node
    // would mis-advertise its own gene's custody/trust on the grid.
    #[test]
    fn trained_adapter_projects_to_local_owner_handle() {
        let ta = TrainedAdapter {
            alias: "coder-v1".into(),
            path: PathBuf::from("/genes/coder.gguf"),
            base_model_id: "continuum-ai/qwen3.5-4b-code-forged-GGUF".into(),
        };
        let h = ta.as_gene_handle();
        assert_eq!(h.alias, "coder-v1");
        assert_eq!(h.base_model_id, ta.base_model_id);
        assert!(h.is_local(), "a manifest gene lives on this node");
        assert_eq!(h.node(), None, "local gene has no holding peer");
        assert_eq!(h.provenance, None, "locally-forged gene predates its alloy");
        assert_eq!(h.trust_scope, TrustLevel::Owner, "own gene is Owner-scoped");
        assert_eq!(
            h.locator,
            GeneLocator::Local {
                path: PathBuf::from("/genes/coder.gguf")
            }
        );
    }

    // what this catches: a GeneHandle round-trips JSON — it MUST, because the
    // whole point is that it crosses the wire back to a requester while the bytes
    // stay node-local. A field that failed to (de)serialize would silently drop on
    // the grid hop. Covers both locator variants + Some/None provenance.
    #[test]
    fn gene_handle_round_trips_both_locators() {
        let local = GeneHandle {
            alias: "coder".into(),
            base_model_id: "b".into(),
            locator: GeneLocator::Local {
                path: PathBuf::from("/genes/coder.gguf"),
            },
            provenance: None,
            trust_scope: TrustLevel::Owner,
        };
        let back: GeneHandle =
            serde_json::from_value(serde_json::to_value(&local).unwrap()).unwrap();
        assert_eq!(local, back);

        let peer = PeerId::from_uuid(Uuid::from_u128(0x42));
        let remote = GeneHandle {
            alias: "vision".into(),
            base_model_id: "b2".into(),
            locator: GeneLocator::Node {
                node: peer,
                path: "gguf-lora/vision.gguf".into(),
            },
            provenance: Some(AlloyHash::new("sha256:deadbeef")),
            trust_scope: TrustLevel::Trusted,
        };
        let back: GeneHandle =
            serde_json::from_value(serde_json::to_value(&remote).unwrap()).unwrap();
        assert_eq!(remote, back);
        assert_eq!(
            back.node(),
            Some(peer),
            "remote gene names its holding peer"
        );
        assert!(!back.is_local());
    }

    // what this catches: an OLDER producer that omits `provenance` (predating the
    // field) still deserializes — provenance is additive/optional, so a manifest
    // or grid message without it reads as un-attested, never a hard parse failure.
    #[test]
    fn missing_provenance_deserializes_as_unattested() {
        let json = serde_json::json!({
            "alias": "coder",
            "base_model_id": "b",
            "locator": { "where": "local", "path": "/genes/coder.gguf" },
            "trust_scope": "owner"
        });
        let h: GeneHandle = serde_json::from_value(json).unwrap();
        assert_eq!(h.provenance, None);
        assert!(h.is_local());
    }
}
