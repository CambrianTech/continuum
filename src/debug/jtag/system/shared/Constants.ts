/**
 * System Constants - SINGLE SOURCE OF TRUTH
 *
 * ⚠️ CRITICAL: ALL system constants MUST be defined here
 * ⚠️ NEVER hardcode paths, values, or magic strings anywhere else
 * ⚠️ Import from this file ONLY - no duplication allowed
 *
 * This file exists because scattered constants create technical debt.
 * Every hardcoded value is a future bug waiting to happen.
 */

/**
 * File System Paths - ALL paths in ONE place
 *
 * Priority for configurable paths:
 * 1. Explicit parameter (user provides full path)
 * 2. Environment variable (user overrides via process.env)
 * 3. These constants (our defaults)
 */
export const PATHS = {
  /** Base continuum directory */
  CONTINUUM: '.continuum',

  /** JTAG data storage */
  JTAG_DATA: '.continuum/jtag/data',
  JTAG_BACKUPS: '.continuum/jtag/backups',

  /** Main SQLite database */
  SQLITE_DB: '.continuum/jtag/data/database.sqlite',

  /** Training datasets directory - users can override with DATASETS_DIR env var for external drives */
  DATASETS: '.continuum/datasets',
  DATASETS_PARSED: '.continuum/datasets/parsed',
  DATASETS_PREPARED: '.continuum/datasets/prepared',

  /** Logs */
  LOGS: '.continuum/logs',

  /** Screenshots */
  SCREENSHOTS: '.continuum/screenshots',

  /** Media processing output */
  MEDIA_OUTPUT: '.continuum/media',
  MEDIA_TEMP: '.continuum/media/temp',

  /** Legacy (for migration) */
  LEGACY_DB: '.continuum/database/continuum.db'
} as const;

/**
 * Environment Variables - standard names we check
 */
export const ENV_VARS = {
  DATASETS_DIR: 'DATASETS_DIR',
  REPO_PATH: 'REPO_PATH',
  SENTINEL_PATH: 'SENTINEL_PATH'
} as const;

/**
 * Collection Names - ALL entity collections
 */
export const COLLECTIONS = {
  USERS: 'users',
  USER_STATES: 'user_states',
  ROOMS: 'rooms',
  CHAT_MESSAGES: 'chat_messages',
  ARTIFACTS: 'artifacts',
  SESSIONS: 'sessions',
  TASKS: 'tasks',
  TRAINING_EXAMPLES: 'training_examples',
  TRAINING_SESSIONS: 'training_sessions',
  TRAINING_CHECKPOINTS: 'training_checkpoints',
  TRAINING_DATASETS: 'training_datasets'
} as const;

/**
 * Database Configuration
 */
export const DB_CONFIG = {
  DEFAULT_LIMIT: 100,
  MAX_BATCH_SIZE: 500,
  CONNECTION_TIMEOUT: 10000,
  ENABLE_PERFORMANCE_LOGGING: false
} as const;

/**
 * Helper: Get path with env var fallback
 * Usage: getPathWithEnvFallback('DATASETS_DIR', PATHS.DATASETS)
 */
export function getPathWithEnvFallback(envVarName: keyof typeof ENV_VARS, defaultPath: string): string {
  return process.env[ENV_VARS[envVarName]] || defaultPath;
}

/**
 * Helper: Resolve path priority (param > env > constant)
 */
export function resolvePath(explicitPath: string | undefined, envVarName: keyof typeof ENV_VARS, defaultPath: string): string {
  return explicitPath ?? getPathWithEnvFallback(envVarName, defaultPath);
}

// Re-export for backward compatibility (will be deprecated)
export { PATHS as DATABASE_PATHS };
export type CollectionName = typeof COLLECTIONS[keyof typeof COLLECTIONS];
