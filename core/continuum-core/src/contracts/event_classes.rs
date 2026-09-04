//! The 8 contract event class names + their payload types.
//!
//! Roadmap item L1-6 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
//! Spec: GRID-BUS-ARCHITECTURE §4.4 + MULTI-PEER-COMMANDS §7.
//!
//! These are the on-the-wire event class names that `declare_contract_event_classes`
//! registers with the L1-1 `EventClassRegistry` at startup. Once declared,
//! `Events.emit('contract:proposed', payload)` (TS side) or
//! `event_class_registry().resolve_channel('contract:proposed', payload)`
//! (Rust side) route the event onto the appropriate airc channel.
//!
//! ## Chain shape
//!
//! ```text
//!   contract:proposed   — proposer publishes terms + signs
//!         │
//!         ▼
//!   contract:bid        — interested executor publishes their bid, signs
//!         │
//!         ▼
//!   contract:accepted   — proposer picks one bid, signs the acceptance
//!         │
//!         ▼
//!   contract:executing  — executor signs "started work" (optional, observability)
//!         │
//!         ▼
//!   contract:delivered  — executor signs the delivered artifact + alloy_hash
//!         │
//!         ▼
//!   contract:verified   — proposer (or auditor) signs verification result
//!         │
//!         ▼
//!   contract:paid       — payer signs the settlement (zero-LP household = OK)
//!         │
//!         ▼ (only when a participant disputes)
//!   contract:disputed   — any signer can file with reason + sig
//! ```
//!
//! Every event carries the same `contract_id` so the airc cursor replay
//! can stitch the chain together from a single-channel scan.

use crate::events::EventClassConfig;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ─── Event class names (constants — string-typed, used as keys into L1-1) ──

pub const EVENT_CONTRACT_PROPOSED: &str = "contract:proposed";
pub const EVENT_CONTRACT_BID: &str = "contract:bid";
pub const EVENT_CONTRACT_ACCEPTED: &str = "contract:accepted";
pub const EVENT_CONTRACT_EXECUTING: &str = "contract:executing";
pub const EVENT_CONTRACT_DELIVERED: &str = "contract:delivered";
pub const EVENT_CONTRACT_VERIFIED: &str = "contract:verified";
pub const EVENT_CONTRACT_PAID: &str = "contract:paid";
pub const EVENT_CONTRACT_DISPUTED: &str = "contract:disputed";

/// All 8 names in canonical order. Used by `declare_contract_event_classes`
/// to batch-register and by tests to verify completeness.
pub const ALL_CONTRACT_EVENT_NAMES: &[&str] = &[
    EVENT_CONTRACT_PROPOSED,
    EVENT_CONTRACT_BID,
    EVENT_CONTRACT_ACCEPTED,
    EVENT_CONTRACT_EXECUTING,
    EVENT_CONTRACT_DELIVERED,
    EVENT_CONTRACT_VERIFIED,
    EVENT_CONTRACT_PAID,
    EVENT_CONTRACT_DISPUTED,
];

/// Wire-format schema version for the contract event chain. Bump when
/// any payload shape changes incompatibly; subscribers honor the
/// L1-1 `onUnknownSchema: Fail` default, so a bump that isn't rolled
/// out to all peers will trip a visible error rather than silently
/// drop events.
pub const CONTRACT_SCHEMA_VERSION: &str = "v1";

// ─── Payload types ────────────────────────────────────────────────────────
//
// Each payload carries `contract_id` (string — chain-correlation key)
// plus its event-specific fields. The payload is what
// `signing::canonical_hash` runs over to produce the bytes that get
// signed; the signature lives in the surrounding `SignedContractEvent`
// envelope (see `envelope.rs`).

/// `contract:proposed` — initiator publishes a contract for bidding.
///
/// `alloy_hash` references the substance of what's being contracted —
/// matches the proof-contract layer in
/// `docs/grid/FORGE-ALLOY-PROOF-CONTRACTS.md`. For pre-alloy use cases
/// (e.g. a `ping` dispatch with no proof bundle) the hash references
/// a synthetic "ping contract" alloy with no proof suite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/contracts/ContractProposedPayload.ts"
)]
pub struct ContractProposedPayload {
    pub contract_id: String,
    pub proposer_id: String,
    /// SHA-256 reference to the alloy bundle describing the work.
    /// Hex-encoded for human readability + ts-rs `string` mapping.
    pub alloy_hash: String,
    /// Currency/escrow terms. Zero-cost ("household") tier = empty
    /// `bid_currency` + zero `max_bid`.
    pub bid_currency: String,
    /// `#[ts(type = "number")]`: the JSON wire carries a number; ts-rs's default
    /// `bigint` for 64-bit ints breaks `JSON.stringify` (throws) and numeric
    /// fixtures. Escrow amounts never approach 2^53.
    #[ts(type = "number")]
    pub max_bid: u64,
    /// Expiry (Unix ms). After this point the proposal is dead even
    /// if no `:accepted` was ever emitted. `number` per the `max_bid` note —
    /// Unix-ms timestamps stay far below 2^53.
    #[ts(type = "number")]
    pub expiry_unix_ms: i64,
    /// Required executor capability tag — matches the L1-4
    /// `presence:peer-manifest` capability index format.
    pub required_capability: String,
}

/// `contract:bid` — an executor's offer to take on a proposed contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/contracts/ContractBidPayload.ts"
)]
pub struct ContractBidPayload {
    pub contract_id: String,
    pub bidder_id: String,
    /// `number` per the `ContractProposedPayload::max_bid` note — JSON wire
    /// carries a number, `bigint` breaks `JSON.stringify`, bids stay < 2^53.
    #[ts(type = "number")]
    pub bid_amount: u64,
    /// Bidder's promised SLA (max latency in ms). Proposer uses this
    /// in the bid-selection policy (lower latency + lower bid wins,
    /// per the policy engine).
    pub max_latency_ms: u32,
    /// Bidder's expiry — how long this bid is honored if accepted.
    /// `number` per the `max_bid` note — Unix-ms timestamps stay below 2^53.
    #[ts(type = "number")]
    pub bid_expiry_unix_ms: i64,
}

/// `contract:accepted` — proposer's signed selection of one bidder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/contracts/ContractAcceptedPayload.ts"
)]
pub struct ContractAcceptedPayload {
    pub contract_id: String,
    pub proposer_id: String,
    pub accepted_bidder_id: String,
    /// Hash of the accepted bid envelope — pins exactly which bid was
    /// taken (defense against bid-rewrite attacks where two bids share
    /// a contract_id).
    pub accepted_bid_hash: String,
}

/// `contract:executing` — executor's signed "work started" beacon.
/// Optional event (the chain stays valid without it) but used by the
/// router daemon to mark a routing slot as in-use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/contracts/ContractExecutingPayload.ts"
)]
pub struct ContractExecutingPayload {
    pub contract_id: String,
    pub executor_id: String,
    /// `number` per the `max_bid` note — Unix-ms timestamps stay below 2^53,
    /// and `bigint` would break `JSON.stringify` on the wire.
    #[ts(type = "number")]
    pub started_at_unix_ms: i64,
}

/// `contract:delivered` — executor's signed assertion that the work is
/// done. Carries the alloy_hash of the actual artifact (which the
/// proposer compares against the originally-proposed alloy_hash to
/// detect bait-and-switch).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/contracts/ContractDeliveredPayload.ts"
)]
pub struct ContractDeliveredPayload {
    pub contract_id: String,
    pub executor_id: String,
    /// Hash of the delivered artifact (may differ from the proposed
    /// alloy_hash if the executor produced a SPECIFIC output that
    /// satisfies the proposed CONTRACT).
    pub delivered_alloy_hash: String,
    /// Optional location pointer (URL, IPFS CID, etc.) for fetching
    /// the artifact bytes. The hash is the canonical reference; this
    /// is convenience.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub artifact_url: Option<String>,
}

/// `contract:verified` — proposer (or auditor) signs the verification
/// verdict. Carries the result of running the alloy proof suite
/// against the delivered artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/contracts/ContractVerifiedPayload.ts"
)]
pub struct ContractVerifiedPayload {
    pub contract_id: String,
    pub verifier_id: String,
    /// `passed: true` ⇒ proof suite ran clean; `false` ⇒ at least one
    /// TDD assertion failed or a VDD metric was outside the tolerance
    /// band. Verifier signs either way — disputes happen via
    /// `contract:disputed`, not by withholding `:verified`.
    pub passed: bool,
    /// Concise reason string for the verdict — full details belong in
    /// a separate report referenced by alloy_hash.
    pub verdict_reason: String,
}

/// `contract:paid` — payer's signed settlement record. For the
/// zero-cost household tier this is still emitted (audit completeness)
/// with `amount: 0`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/contracts/ContractPaidPayload.ts"
)]
pub struct ContractPaidPayload {
    pub contract_id: String,
    pub payer_id: String,
    pub payee_id: String,
    /// `number` per the `ContractProposedPayload::max_bid` note — the JSON wire
    /// carries a number, `bigint` breaks `JSON.stringify`, and settlement
    /// amounts never approach 2^53 (household tier settles `amount: 0`).
    #[ts(type = "number")]
    pub amount: u64,
    pub currency: String,
    /// Optional settlement reference (chain tx hash, internal ledger
    /// entry id, etc.). Not load-bearing for replay; just provenance.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub settlement_ref: Option<String>,
}

/// `contract:disputed` — any signer can file. Replay reproduces every
/// disputed contract for auditor review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/contracts/ContractDisputedPayload.ts"
)]
pub struct ContractDisputedPayload {
    pub contract_id: String,
    pub disputer_id: String,
    pub reason: String,
    /// Optional reference to the specific prior event being disputed
    /// (e.g. the verified-hash if the disputer claims wrong verdict).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub disputed_event_hash: Option<String>,
}

// ─── SDK EventSpec registrations (sdk_codegen) ─────────────────────────────
//
// The codegen view of the contract events: class + ts-rs payload, so the
// generated EventMap + typed EventApi (api.onContractProposed/emitContractPaid)
// flow from the same Rust declaration as the runtime EventClassConfig below.
// `declare_event_spec!(Ident, CLASS_CONST, PayloadType)` is local sugar — one
// line per event, registered into the auto-discovered event registry.
macro_rules! declare_event_spec {
    ($ident:ident, $class:expr, $payload:ty) => {
        pub struct $ident;
        impl crate::sdk_codegen::EventSpec for $ident {
            const CLASS: &'static str = $class;
            type Payload = $payload;
        }
        crate::register_event!($ident);
    };
}

declare_event_spec!(
    ContractProposedEvent,
    EVENT_CONTRACT_PROPOSED,
    ContractProposedPayload
);
declare_event_spec!(ContractBidEvent, EVENT_CONTRACT_BID, ContractBidPayload);
declare_event_spec!(
    ContractAcceptedEvent,
    EVENT_CONTRACT_ACCEPTED,
    ContractAcceptedPayload
);
declare_event_spec!(
    ContractExecutingEvent,
    EVENT_CONTRACT_EXECUTING,
    ContractExecutingPayload
);
declare_event_spec!(
    ContractDeliveredEvent,
    EVENT_CONTRACT_DELIVERED,
    ContractDeliveredPayload
);
declare_event_spec!(
    ContractVerifiedEvent,
    EVENT_CONTRACT_VERIFIED,
    ContractVerifiedPayload
);
declare_event_spec!(ContractPaidEvent, EVENT_CONTRACT_PAID, ContractPaidPayload);
declare_event_spec!(
    ContractDisputedEvent,
    EVENT_CONTRACT_DISPUTED,
    ContractDisputedPayload
);

// ─── EventClass registration helper ───────────────────────────────────────

/// Register all 8 contract event classes with the L1-1 registry.
///
/// Idempotent: safe to call from multiple init paths; conflicting
/// re-declarations throw per the L1-1 contract-integrity rule.
///
/// Channel choice: all 8 use `Global` — contract events are
/// mesh-visible by design (the trust substrate REQUIRES that everyone
/// can audit-replay the chain). Future tiered contracts (private to a
/// circle, e.g. trusted-orgs) could shift to a private channel via a
/// separate event-class declaration; that's an L4-Phase-C decision,
/// not L1-6.
pub fn declare_contract_event_classes() -> Result<usize, String> {
    use crate::events::declare_event_class;
    use crate::events::EventClassChannelStrategy;

    let mut declared = 0;
    for name in ALL_CONTRACT_EVENT_NAMES {
        let cfg = EventClassConfig {
            broadcast: true,
            channel: Some(EventClassChannelStrategy::Global),
            schema_version: CONTRACT_SCHEMA_VERSION.to_string(),
            on_unknown_schema: None, // defaults to Fail
            description: Some(format!("L1-6 contract event chain — {name}")),
        };
        declare_event_class(name, &cfg)
            .map_err(|e| format!("L1-6: failed to declare event class '{name}': {e}"))?;
        declared += 1;
    }
    Ok(declared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::lookup_event_class;

    #[test]
    fn all_8_names_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for name in ALL_CONTRACT_EVENT_NAMES {
            assert!(seen.insert(*name), "duplicate name: {name}");
        }
        assert_eq!(seen.len(), 8);
    }

    #[test]
    fn all_names_use_contract_prefix() {
        for name in ALL_CONTRACT_EVENT_NAMES {
            assert!(name.starts_with("contract:"), "bad name: {name}");
        }
    }

    #[test]
    fn declare_registers_all_eight() {
        // Note: registry is process-global — if another test in this
        // crate already declared with the same names + same config,
        // declare_contract_event_classes is idempotent and still passes.
        let count = declare_contract_event_classes().expect("declare must succeed");
        assert_eq!(count, 8);

        for name in ALL_CONTRACT_EVENT_NAMES {
            let cfg = lookup_event_class(name)
                .unwrap_or_else(|| panic!("class '{name}' was declared but lookup returned None"));
            assert!(cfg.broadcast, "{name} must be broadcast");
            assert_eq!(cfg.schema_version, CONTRACT_SCHEMA_VERSION);
        }
    }
}
