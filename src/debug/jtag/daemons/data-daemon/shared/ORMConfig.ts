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
 */

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
 */
const COLLECTION_CONFIG: Record<string, ORMCollectionConfig> = {
  // Core entities - now on Rust
  'users': { backend: 'rust', logOperations: false },
  'chatMessages': { backend: 'rust', logOperations: false },
  'chat_messages': { backend: 'rust', logOperations: false },
  'memories': { backend: 'rust', logOperations: false },
  'rooms': { backend: 'rust', logOperations: false },
  'room_memberships': { backend: 'rust', logOperations: false },

  // Persona entities
  'persona_states': { backend: 'rust', logOperations: false },
  'persona_skills': { backend: 'rust', logOperations: false },
  'persona_tasks': { backend: 'rust', logOperations: false },

  // Session/state entities
  'sessions': { backend: 'rust', logOperations: false },
  'user_states': { backend: 'rust', logOperations: false },

  // Training entities
  'training_samples': { backend: 'rust', logOperations: false },
  'training_runs': { backend: 'rust', logOperations: false },

  // Skill entities
  'skills': { backend: 'rust', logOperations: false },
  'skill_activations': { backend: 'rust', logOperations: false },

  // Canvas/collaboration
  'canvas_strokes': { backend: 'rust', logOperations: false },
  'wall_documents': { backend: 'rust', logOperations: false },

  // Tasks collection
  'tasks': { backend: 'rust', logOperations: false },
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
 *
 * Phase 4 Status:
 * - ORMRustClient IPC wired to Rust continuum-core DataModule
 * - SqlNamingConverter removed from ORM layer (Rust handles naming)
 * - Filter format conversion added ($eq → eq, $gt → gt, etc.)
 * - Store/update operations still failing when enabled
 *
 * Set to false to enable Rust backend. Currently true (TypeScript) pending debug.
 */
export const FORCE_TYPESCRIPT_BACKEND = true;

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

  return COLLECTION_CONFIG[collection] ?? DEFAULT_CONFIG;
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
export function setCollectionBackend(collection: string, backend: ORMBackend): void {
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
    status[collection] = FORCE_TYPESCRIPT_BACKEND ? 'typescript' : config.backend;
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
  const config = COLLECTION_CONFIG[collection] ?? DEFAULT_CONFIG;
  return config.backend;
}

/**
 * Assert that a collection is using the expected backend.
 * Use this to validate your assumptions at runtime.
 * Throws if the backend doesn't match expectations.
 */
export function assertBackend(collection: string, expected: ORMBackend): void {
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
