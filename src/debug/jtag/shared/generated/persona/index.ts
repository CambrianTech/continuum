// Persona Cognition Types - Generated from Rust (single source of truth)
// Re-run: cargo test --package continuum-core --lib export_bindings

// Core types
export type { SenderType } from './SenderType';
export type { Modality } from './Modality';
export type { Mood } from './Mood';

// Inbox items
export type { InboxMessage } from './InboxMessage';
export type { InboxTask } from './InboxTask';
export type { QueueItem } from './QueueItem';

// State management
export type { PersonaState } from './PersonaState';

// Decision types
export type { CognitionDecision } from './CognitionDecision';
export type { PriorityScore } from './PriorityScore';
export type { PriorityFactors } from './PriorityFactors';

// Channel system types
export type { ActivityDomain } from './ActivityDomain';
export type { ChannelStatus } from './ChannelStatus';
export type { ChannelRegistryStatus } from './ChannelRegistryStatus';
export type { ChannelEnqueueRequest } from './ChannelEnqueueRequest';
export type { ServiceCycleResult } from './ServiceCycleResult';
export type { ConsolidatedContext } from './ConsolidatedContext';
