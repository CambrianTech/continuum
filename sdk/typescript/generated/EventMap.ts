// GENERATED from the Rust event registry (core/continuum-core sdk_codegen).
// DO NOT EDIT. Source of truth: each event's EventSpec (class + ts-rs
// Payload). Regenerate after an event changes.

import type { ContractAcceptedPayload } from './wire/contracts/ContractAcceptedPayload';
import type { ContractBidPayload } from './wire/contracts/ContractBidPayload';
import type { ContractDeliveredPayload } from './wire/contracts/ContractDeliveredPayload';
import type { ContractDisputedPayload } from './wire/contracts/ContractDisputedPayload';
import type { ContractExecutingPayload } from './wire/contracts/ContractExecutingPayload';
import type { ContractPaidPayload } from './wire/contracts/ContractPaidPayload';
import type { ContractProposedPayload } from './wire/contracts/ContractProposedPayload';
import type { ContractVerifiedPayload } from './wire/contracts/ContractVerifiedPayload';

/** event class -> payload. Generated; the contract is Rust-origin. */
export interface EventMap {
  'contract:accepted': ContractAcceptedPayload;
  'contract:bid': ContractBidPayload;
  'contract:delivered': ContractDeliveredPayload;
  'contract:disputed': ContractDisputedPayload;
  'contract:executing': ContractExecutingPayload;
  'contract:paid': ContractPaidPayload;
  'contract:proposed': ContractProposedPayload;
  'contract:verified': ContractVerifiedPayload;
}

export type EventClass = keyof EventMap;
