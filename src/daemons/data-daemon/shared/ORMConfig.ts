/**
 * ORM Configuration - Feature flags for incremental migration
 *
 * Controls which backend handles each collection:
 * - 'typescript': DataDaemon (current, safe)
 * - 'rust': Rust ConnectionManager via IPC
 * - 'shadow': Execute both, compare, return TypeScript result
 *
 * ⚠️  NO FALLBACKS POLICY ⚠️
 * Backend selection is DETERMINISTIC. There is NO fallback logic.
 * If config says 'rust' and Rust fails, the operation FAILS.
 * If config says 'typescript', TypeScript handles it. Period.
 * NEVER add "try X, catch, use Y" logic anywhere in the ORM.
 *
 * ⚠️  COLLECTIONS ARE TYPED ⚠️
 * All collection names come from generated-collection-constants.ts
 * which is derived from entity definitions. You CANNOT use a random string.
 */

import { COLLECTIONS, type CollectionName } from '../../../shared/generated-collection-constants';

export type ORMBackend = 'typescript' | 'rust' | 'shadow';
export type ShadowMode = 'read' | 'write' | 'both';

export interface ORMCollectionConfig {
  /** Which backend handles this collection */
  backend: ORMBackend;
  /** For shadow mode: which operations to shadow */
  shadowMode?: ShadowMode;
  /** Log all operations for this collection */
  logOperations?: boolean;
  /** Log slow operations (> threshold ms) */
  slowThresholdMs?: number;
}

/**
 * Per-collection configuration
 * Phase 4 Complete: All collections now route to Rust DataModule
 *
 * Keys MUST be CollectionName values from generated constants.
 * TypeScript will error if you try to use an invalid collection name.
 */
const COLLECTION_CONFIG: Partial<Record<CollectionName, ORMCollectionConfig>> = {
  // Core entities - now on Rust
  [COLLECTIONS.USERS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.CHAT_MESSAGES]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.MEMORIES]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.ROOMS]: { backend: 'rust', logOperations: false },

  // User state
  [COLLECTIONS.USER_STATES]: { backend: 'rust', logOperations: false },

  // Skill entities
  [COLLECTIONS.SKILLS]: { backend: 'rust', logOperations: false },

  // Canvas/collaboration
  [COLLECTIONS.CANVAS_STROKES]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.WALL_DOCUMENTS]: { backend: 'rust', logOperations: false },

  // Tasks collection
  [COLLECTIONS.TASKS]: { backend: 'rust', logOperations: false },

  // Training entities
  [COLLECTIONS.TRAINING_DATASETS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.TRAINING_EXAMPLES]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.TRAINING_SESSIONS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.TRAINING_CHECKPOINTS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.TRAINING_LOGS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.TRAINING_METRICS]: { backend: 'rust', logOperations: false },

  // Fine-tuning
  [COLLECTIONS.FINE_TUNING_JOBS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.FINE_TUNING_DATASETS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.FINE_TUNED_MODELS]: { backend: 'rust', logOperations: false },

  // Cognition logging
  [COLLECTIONS.COGNITION_STATE_SNAPSHOTS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.COGNITION_PLAN_RECORDS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.COGNITION_PLAN_STEP_EXECUTIONS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.COGNITION_SELF_STATE_UPDATES]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.COGNITION_MEMORY_OPERATIONS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.COGNITION_PLAN_REPLANS]: { backend: 'rust', logOperations: false },

  // Tool/adapter logging
  [COLLECTIONS.TOOL_EXECUTION_LOGS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.ADAPTER_DECISION_LOGS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.ADAPTER_REASONING_LOGS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.RESPONSE_GENERATION_LOGS]: { backend: 'rust', logOperations: false },

  // Persona RAG contexts
  [COLLECTIONS.PERSONA_RAG_CONTEXTS]: { backend: 'rust', logOperations: false },

  // Timeline
  [COLLECTIONS.TIMELINE_EVENTS]: { backend: 'rust', logOperations: false },

  // Activities
  [COLLECTIONS.ACTIVITIES]: { backend: 'rust', logOperations: false },

  // Handles
  [COLLECTIONS.HANDLES]: { backend: 'rust', logOperations: false },

  // Voting
  [COLLECTIONS.FILE_VOTE_PROPOSALS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.DECISION_PROPOSALS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.DECISIONS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.COORDINATION_DECISIONS]: { backend: 'rust', logOperations: false },

  // Pinned items
  [COLLECTIONS.PINNED_ITEMS]: { backend: 'rust', logOperations: false },

  // Recipes
  [COLLECTIONS.RECIPES]: { backend: 'rust', logOperations: false },

  // System config
  [COLLECTIONS.SYSTEM_CONFIG]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.SYSTEM_CHECKPOINTS]: { backend: 'rust', logOperations: false },

  // Feedback
  [COLLECTIONS.FEEDBACK_PATTERNS]: { backend: 'rust', logOperations: false },

  // Social
  [COLLECTIONS.SOCIAL_CREDENTIALS]: { backend: 'rust', logOperations: false },

  // Calls
  [COLLECTIONS.CALLS]: { backend: 'rust', logOperations: false },

  // Webhook events
  [COLLECTIONS.WEBHOOK_EVENTS]: { backend: 'rust', logOperations: false },

  // AI generations
  [COLLECTIONS.AI_GENERATIONS]: { backend: 'rust', logOperations: false },

  // Genome
  [COLLECTIONS.GENOMES]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.GENOME_LAYERS]: { backend: 'rust', logOperations: false },

  // Code index
  [COLLECTIONS.CODE_INDEX]: { backend: 'rust', logOperations: false },

  // Test/dataset executions
  [COLLECTIONS.TEST_EXECUTIONS]: { backend: 'rust', logOperations: false },
  [COLLECTIONS.DATASET_EXECUTIONS]: { backend: 'rust', logOperations: false },
};

/**
 * Default config for collections not explicitly listed
 * Phase 4: Default to Rust for all collections
 */
const DEFAULT_CONFIG: ORMCollectionConfig = {
  backend: 'rust',
  logOperations: false,
  slowThresholdMs: 100,
};

/**
 * GLOBAL KILL SWITCH
 * When true, ALL operations go to TypeScript regardless of collection config
 * Use this to instantly revert if anything goes wrong
 */
export const FORCE_TYPESCRIPT_BACKEND = false;

/**
 * Enable shadow mode globally (run both backends, compare results)
 * Only applies when FORCE_TYPESCRIPT_BACKEND is false
 */
export const ENABLE_SHADOW_MODE = false;

/**
 * Log all ORM operations (verbose, use for debugging)
 */
export const LOG_ALL_OPERATIONS = false;

/**
 * Get configuration for a collection
 */
export function getCollectionConfig(collection: string): ORMCollectionConfig {
  if (FORCE_TYPESCRIPT_BACKEND) {
    return { ...DEFAULT_CONFIG, backend: 'typescript' };
  }

  return COLLECTION_CONFIG[collection as CollectionName] ?? DEFAULT_CONFIG;
}

/**
 * Check if a collection should use Rust backend
 */
export function shouldUseRust(collection: string): boolean {
  if (FORCE_TYPESCRIPT_BACKEND) return false;
  const config = getCollectionConfig(collection);
  return config.backend === 'rust';
}

/**
 * Check if a collection should run in shadow mode
 */
export function shouldShadow(collection: string): boolean {
  if (FORCE_TYPESCRIPT_BACKEND) return false;
  if (ENABLE_SHADOW_MODE) return true;
  const config = getCollectionConfig(collection);
  return config.backend === 'shadow';
}

/**
 * Check if operations should be logged for a collection
 */
export function shouldLog(collection: string): boolean {
  if (LOG_ALL_OPERATIONS) return true;
  const config = getCollectionConfig(collection);
  return config.logOperations ?? false;
}

/**
 * Set collection backend at runtime (for testing/debugging)
 */
export function setCollectionBackend(collection: CollectionName, backend: ORMBackend): void {
  COLLECTION_CONFIG[collection] = {
    ...(COLLECTION_CONFIG[collection] ?? DEFAULT_CONFIG),
    backend,
  };
}

/**
 * Get current backend status for all collections
 */
export function getBackendStatus(): Record<string, ORMBackend> {
  const status: Record<string, ORMBackend> = {};
  for (const [collection, config] of Object.entries(COLLECTION_CONFIG)) {
    if (config) {
      status[collection] = FORCE_TYPESCRIPT_BACKEND ? 'typescript' : config.backend;
    }
  }
  return status;
}

/**
 * Get the EXACT backend that WILL be used for a collection.
 * No ambiguity. No fallbacks. This is what runs.
 */
export function getActiveBackend(collection: string): ORMBackend {
  if (FORCE_TYPESCRIPT_BACKEND) {
    return 'typescript';
  }
  const config = COLLECTION_CONFIG[collection as CollectionName] ?? DEFAULT_CONFIG;
  return config.backend;
}

/**
 * Assert that a collection is using the expected backend.
 * Use this to validate your assumptions at runtime.
 * Throws if the backend doesn't match expectations.
 */
export function assertBackend(collection: CollectionName, expected: ORMBackend): void {
  const actual = getActiveBackend(collection);
  if (actual !== expected) {
    throw new Error(
      `Backend mismatch for '${collection}': expected '${expected}', but config says '${actual}'. ` +
      `FORCE_TYPESCRIPT_BACKEND=${FORCE_TYPESCRIPT_BACKEND}. No fallbacks - fix your config.`
    );
  }
}

/**
 * Print current backend configuration to console.
 * Call this at startup to see EXACTLY what's configured.
 */
export function printBackendConfig(): void {
  console.log('\n=== ORM Backend Configuration ===');
  console.log(`FORCE_TYPESCRIPT_BACKEND: ${FORCE_TYPESCRIPT_BACKEND}`);
  console.log(`ENABLE_SHADOW_MODE: ${ENABLE_SHADOW_MODE}`);
  console.log('\nPer-collection backends:');

  const status = getBackendStatus();
  for (const [collection, backend] of Object.entries(status)) {
    const marker = backend === 'rust' ? '🦀' : backend === 'shadow' ? '👥' : '📘';
    console.log(`  ${marker} ${collection}: ${backend}`);
  }

  console.log('\n⚠️  NO FALLBACKS: If rust fails, it fails. No silent TypeScript bypass.');
  console.log('================================\n');
}

// Re-export for convenience
export { COLLECTIONS, type CollectionName };
