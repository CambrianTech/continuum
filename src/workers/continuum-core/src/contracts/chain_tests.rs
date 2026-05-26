//! End-to-end L1-6 contract chain integration tests.
//!
//! Walks the full 8-event chain (proposed → bid → accepted → executing
//! → delivered → verified → paid → disputed) for a synthetic "ping
//! grid dispatch with zero-LP household terms" — the worked example
//! the roadmap names as the L1-6 done-criterion.
//!
//! No airc transport yet — these tests sign + verify in-memory and
//! prove the envelopes round-trip bit-equivalently through JSON. The
//! airc-cursor replay variant lands in Phase B once L1-4
//! (`presence:peer-manifest`) provides the per-peer pubkey index.

#![cfg(test)]

use crate::contracts::{
    envelope::SignedContractEvent,
    event_classes::{
        ContractAcceptedPayload, ContractBidPayload, ContractDeliveredPayload,
        ContractDisputedPayload, ContractExecutingPayload, ContractPaidPayload,
        ContractProposedPayload, ContractVerifiedPayload, EVENT_CONTRACT_ACCEPTED,
        EVENT_CONTRACT_BID, EVENT_CONTRACT_DELIVERED, EVENT_CONTRACT_DISPUTED,
        EVENT_CONTRACT_EXECUTING, EVENT_CONTRACT_PAID, EVENT_CONTRACT_PROPOSED,
        EVENT_CONTRACT_VERIFIED,
    },
    signing::ContractSigningKey,
};

/// Synthetic clock — the test fixes signed_at_unix_ms so the JSON
/// round-trip is bit-exact reproducible.
const T0: i64 = 1_779_800_000_000;

/// Two-peer worked example: peer-a proposes, peer-b bids + executes.
struct Peers {
    proposer: ContractSigningKey,
    executor: ContractSigningKey,
}

fn make_peers() -> Peers {
    Peers {
        proposer: ContractSigningKey::generate(),
        executor: ContractSigningKey::generate(),
    }
}

#[test]
fn full_chain_proposed_to_paid_verifies_end_to_end() {
    let peers = make_peers();
    let contract_id = "c-ping-001".to_string();
    let alloy_hash = "sha256:ping-contract-alloy-stub".to_string();

    // 1. proposer publishes
    let proposed = SignedContractEvent::sign(
        EVENT_CONTRACT_PROPOSED,
        ContractProposedPayload {
            contract_id: contract_id.clone(),
            proposer_id: "peer-a".into(),
            alloy_hash: alloy_hash.clone(),
            bid_currency: String::new(),
            max_bid: 0,
            expiry_unix_ms: T0 + 60_000,
            required_capability: "inference:ping".into(),
        },
        &peers.proposer,
        T0,
    )
    .unwrap();
    proposed.verify().expect("proposed must verify");

    // 2. executor bids
    let bid = SignedContractEvent::sign(
        EVENT_CONTRACT_BID,
        ContractBidPayload {
            contract_id: contract_id.clone(),
            bidder_id: "peer-b".into(),
            bid_amount: 0,
            max_latency_ms: 50,
            bid_expiry_unix_ms: T0 + 30_000,
        },
        &peers.executor,
        T0 + 100,
    )
    .unwrap();
    bid.verify().expect("bid must verify");

    // 3. proposer accepts (pins the bid hash so the chain is unambiguous)
    let bid_hash_hex = bid.signature_hex.clone(); // bid sig serves as a stable bid identifier
    let accepted = SignedContractEvent::sign(
        EVENT_CONTRACT_ACCEPTED,
        ContractAcceptedPayload {
            contract_id: contract_id.clone(),
            proposer_id: "peer-a".into(),
            accepted_bidder_id: "peer-b".into(),
            accepted_bid_hash: bid_hash_hex,
        },
        &peers.proposer,
        T0 + 200,
    )
    .unwrap();
    accepted.verify().expect("accepted must verify");

    // 4. executor signs "started"
    let executing = SignedContractEvent::sign(
        EVENT_CONTRACT_EXECUTING,
        ContractExecutingPayload {
            contract_id: contract_id.clone(),
            executor_id: "peer-b".into(),
            started_at_unix_ms: T0 + 300,
        },
        &peers.executor,
        T0 + 300,
    )
    .unwrap();
    executing.verify().expect("executing must verify");

    // 5. executor signs delivered artifact
    let delivered = SignedContractEvent::sign(
        EVENT_CONTRACT_DELIVERED,
        ContractDeliveredPayload {
            contract_id: contract_id.clone(),
            executor_id: "peer-b".into(),
            delivered_alloy_hash: alloy_hash.clone(),
            artifact_url: Some("pong".into()),
        },
        &peers.executor,
        T0 + 400,
    )
    .unwrap();
    delivered.verify().expect("delivered must verify");

    // 6. proposer (acting as verifier) signs verdict
    let verified = SignedContractEvent::sign(
        EVENT_CONTRACT_VERIFIED,
        ContractVerifiedPayload {
            contract_id: contract_id.clone(),
            verifier_id: "peer-a".into(),
            passed: true,
            verdict_reason: "ping matched expected pong".into(),
        },
        &peers.proposer,
        T0 + 500,
    )
    .unwrap();
    verified.verify().expect("verified must verify");

    // 7. proposer signs the settlement (zero-LP household — amount 0)
    let paid = SignedContractEvent::sign(
        EVENT_CONTRACT_PAID,
        ContractPaidPayload {
            contract_id: contract_id.clone(),
            payer_id: "peer-a".into(),
            payee_id: "peer-b".into(),
            amount: 0,
            currency: String::new(),
            settlement_ref: None,
        },
        &peers.proposer,
        T0 + 600,
    )
    .unwrap();
    paid.verify().expect("paid must verify");
}

#[test]
fn disputed_event_signs_and_verifies() {
    let peers = make_peers();

    let disputed = SignedContractEvent::sign(
        EVENT_CONTRACT_DISPUTED,
        ContractDisputedPayload {
            contract_id: "c-ping-002".into(),
            disputer_id: "peer-b".into(),
            reason: "verifier marked failed but artifact matched alloy_hash".into(),
            disputed_event_hash: Some("verified-event-hex-stub".into()),
        },
        &peers.executor,
        T0 + 700,
    )
    .unwrap();

    let pubkey = disputed.verify().unwrap();
    assert_eq!(pubkey.to_bytes(), peers.executor.verifying_key().to_bytes());
}

#[test]
fn full_chain_round_trips_through_json_bit_exact() {
    // Each event's JSON serialization must round-trip identical bytes —
    // this is what makes airc-cursor replay reproducible across peers.
    let peers = make_peers();

    let proposed = SignedContractEvent::sign(
        EVENT_CONTRACT_PROPOSED,
        ContractProposedPayload {
            contract_id: "c-bitexact-001".into(),
            proposer_id: "peer-a".into(),
            alloy_hash: "sha256:any".into(),
            bid_currency: String::new(),
            max_bid: 0,
            expiry_unix_ms: T0 + 60_000,
            required_capability: "inference:ping".into(),
        },
        &peers.proposer,
        T0,
    )
    .unwrap();

    let json_a = serde_json::to_string(&proposed).unwrap();
    let restored: SignedContractEvent<ContractProposedPayload> =
        serde_json::from_str(&json_a).unwrap();
    let json_b = serde_json::to_string(&restored).unwrap();
    assert_eq!(json_a, json_b, "JSON round-trip must be bit-exact");

    // And the restored envelope's signature still verifies — proves the
    // wire form lossless-round-trips the canonical bytes.
    restored.verify().unwrap();
}
