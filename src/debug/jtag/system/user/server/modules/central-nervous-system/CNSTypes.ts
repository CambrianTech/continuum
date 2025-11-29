/**
 * Central Nervous System Types
 *
 * Pure TypeScript types for CNS configuration and orchestration.
 * No implementation logic - just types.
 */

import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import type { ICognitiveScheduler, ActivityDomain } from '../cognitive-schedulers/ICognitiveScheduler';
import type { PersonaInbox, QueueItem } from '../PersonaInbox';
import type { PersonaStateManager } from '../PersonaState';
import type { PersonaGenome } from '../PersonaGenome';

/**
 * Configuration for PersonaCentralNervousSystem
 */
export interface CNSConfig {
  // Core modules (existing)
  readonly scheduler: ICognitiveScheduler;
  readonly inbox: PersonaInbox;
  readonly personaState: PersonaStateManager;
  readonly genome: PersonaGenome;

  // Persona reference (for delegating chat handling)
  readonly personaId: UUID;
  readonly personaName: string;
  readonly uniqueId: string;  // Format: {name}-{shortId} for log paths

  // Callbacks for delegating to PersonaUser (avoids circular dependency)
  readonly handleChatMessage: (item: QueueItem) => Promise<void>;
  readonly pollTasks: () => Promise<void>;
  readonly generateSelfTasks: () => Promise<void>;

  // Domain configuration
  readonly enabledDomains: ReadonlyArray<ActivityDomain>;
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
