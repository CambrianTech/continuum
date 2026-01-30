/**
 * Central Nervous System Types
 *
 * Pure TypeScript types for CNS configuration and orchestration.
 * No implementation logic - just types.
 */

import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import type { PersonaInbox, QueueItem } from '../PersonaInbox';
import type { PersonaStateManager } from '../PersonaState';
import type { RustCognitionBridge } from '../RustCognitionBridge';

/**
 * Pre-computed fast-path decision from Rust's serviceCycleFull.
 * Eliminates a separate fastPathDecision IPC round-trip.
 */
export interface FastPathDecision {
  should_respond: boolean;
  confidence: number;
  reason: string;
  decision_time_ms: number;
  fast_path_used: boolean;
}

/**
 * Configuration for PersonaCentralNervousSystem
 *
 * All scheduling is delegated to Rust. TS handles execution.
 */
export interface CNSConfig {
  // Core modules
  readonly inbox: PersonaInbox;
  readonly personaState: PersonaStateManager;

  // Rust cognition bridge (required — all scheduling delegates to Rust)
  readonly rustBridge: RustCognitionBridge;

  // Persona reference (for delegating chat handling)
  readonly personaId: UUID;
  readonly personaName: string;
  readonly uniqueId: string;  // Format: {name}-{shortId} for log paths

  // Callback for delegating to PersonaUser (avoids circular dependency)
  // decision is the pre-computed fast-path decision from Rust's serviceCycleFull
  readonly handleChatMessage: (item: QueueItem, decision?: FastPathDecision) => Promise<void>;

  // Configuration
  readonly allowBackgroundThreads: boolean;
  readonly maxBackgroundThreads?: number;
}

/**
 * Capability detection for CNS tier selection
 */
export interface ModelCapabilities {
  readonly 'advanced-reasoning'?: boolean;
  readonly 'meta-cognition'?: boolean;
  readonly 'long-context'?: boolean;
  readonly 'moderate-reasoning'?: boolean;
  readonly 'pattern-recognition'?: boolean;
  readonly 'fast-inference'?: boolean;
  readonly 'template-responses'?: boolean;
}

/**
 * CNS tier levels (for factory selection)
 */
export enum CNSTier {
  DETERMINISTIC = 'deterministic',  // Simple models, fixed rules
  HEURISTIC = 'heuristic',          // Mid-tier models, adaptive rules
  NEURAL = 'neural'                 // Frontier models, learned weights
}
