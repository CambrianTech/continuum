//! L1-6 contract event chain + ed25519 signing.
//!
//! Roadmap item L1-6 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
//! Spec: GRID-BUS-ARCHITECTURE §4.4 + MULTI-PEER-COMMANDS §7.
//!
//! Three layers, native-truth-thin-SDK pattern:
//!
//!   1. `signing` — ed25519 primitives (matches `airc-protocol = "2"`).
//!      Keypair generation, sign, verify, canonical SHA-256 hashing.
//!   2. `event_classes` — the 8 contract event class names + payloads,
//!      plus `declare_contract_event_classes()` that registers them
//!      with the L1-1 `EventClassRegistry`.
//!   3. `envelope` — the `SignedContractEvent<P>` wrapper that pairs
//!      a typed payload with `event_name` + `signer_pubkey_hex` +
//!      `signature_hex`. Signature pins `(event_name, payload)`
//!      together so relabeling attacks fail verification.
//!
//! Phase A: primitives + types + declarations + unit tests.
//! Phase B: pubkey lookup against L1-4's `presence:peer-manifest`,
//! verify-on-replay handler over L1-2's `AircEventTransport`.

pub mod envelope;
pub mod event_classes;
pub mod signing;
pub mod verification;

#[cfg(test)]
mod chain_tests;

pub use envelope::SignedContractEvent;
pub use event_classes::{
    declare_contract_event_classes, ContractAcceptedPayload, ContractBidPayload,
    ContractDeliveredPayload, ContractDisputedPayload, ContractExecutingPayload,
    ContractPaidPayload, ContractProposedPayload, ContractVerifiedPayload,
    ALL_CONTRACT_EVENT_NAMES, CONTRACT_SCHEMA_VERSION, EVENT_CONTRACT_ACCEPTED, EVENT_CONTRACT_BID,
    EVENT_CONTRACT_DELIVERED, EVENT_CONTRACT_DISPUTED, EVENT_CONTRACT_EXECUTING,
    EVENT_CONTRACT_PAID, EVENT_CONTRACT_PROPOSED, EVENT_CONTRACT_VERIFIED,
};
pub use signing::{
    canonical_hash, ContractSigningKey, ContractVerifyingKey, SigningError, CANONICAL_HASH_LEN,
    PUBLIC_KEY_LEN, SIGNATURE_LEN,
};
pub use verification::{verify_contract_replay, ContractVerificationError, VerifiedContractEvent};
