// IPC Types - Generated from Rust (single source of truth)
// Re-run: cargo test --package continuum-core --lib export_bindings

export type { InboxMessageRequest } from './InboxMessageRequest';

// Re-export cognition types used in IPC responses
export type {
  CognitionDecision,
  PriorityScore,
  PriorityFactors,
} from '../persona';

// Re-export persona state for IPC responses
export type { PersonaState, Mood } from '../persona';
