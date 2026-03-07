/**
 * Database Configuration Constants - SERVER-ONLY
 *
 * ⚠️ DO NOT import this file in browser code!
 * ⚠️ These paths should never be exposed to the browser
 *
 * Single source of truth for database paths and configuration
 * Paths can be overridden via config.env (loaded by SecretManager)
 */

import { PATHS } from '../../shared/Constants';

/**
 * Database paths and connection strings - SERVER-ONLY configuration
 *
 * ROUTING: Main database is Postgres (getDatabasePath() → DATABASE_URL env or default).
 * Per-persona data (memories, embeddings) uses SQLite longterm.db files.
 *
 * Override via config.env:
 *   DATABASE_URL     — Primary Postgres connection (postgres://user@host/db)
 *   DATABASE_DIR     — Data directory ($HOME/.continuum/data)
 *
 * NOTE: These are COMPILE-TIME constants for fallback only.
 * Runtime paths come from ServerConfig which checks config.env first.
 */
export const DATABASE_PATHS = {
  /** Default Postgres connection (system Postgres, database 'continuum') */
  POSTGRES: 'postgres://joel@localhost:5432/continuum',

  /** Main database directory (server-only) - SINGULAR DEFAULT */
  DATA_DIR: '$HOME/.continuum/data',

  /** Backup directory path (server-only) */
  BACKUP_DIR: '$HOME/.continuum/data/backups',

  /** Archive directory path (server-only) */
  ARCHIVE_DIR: '$HOME/.continuum/data/archive',

  /** Datasets directory path for training data */
  DATASETS_DIR: PATHS.DATASETS,

  /**
   * Runtime paths (logs, signals, sessions) are NOT defined here.
   * Server code MUST use SystemPaths.logs.root, SystemPaths.sessions.root, etc.
   * These paths route through the composite factory ($REPO for runtime, $HOME for persistent).
   */
} as const;

/**
 * Database filenames - centralized naming
 * NOTE: Main database is Postgres. SQLite is ONLY used for per-persona longterm.db.
 */
export const DATABASE_FILES = {
  /** Per-persona SQLite database filename (memories, embeddings) */
  PERSONA_LONGTERM: 'longterm.db',
} as const;

/**
 * Database configuration settings (SERVER-ONLY)
 */
export const DATABASE_CONFIG = {
  /** Default query limit for data operations */
  DEFAULT_LIMIT: 100,

  /** Maximum batch size for bulk operations */
  MAX_BATCH_SIZE: 500,

  /** Database connection timeout in milliseconds */
  CONNECTION_TIMEOUT: 10000,

  /** Enable database performance logging */
  ENABLE_PERFORMANCE_LOGGING: false
} as const;

// Legacy alias for backward compatibility
export const DB_CONFIG = DATABASE_CONFIG;

// Re-export COLLECTIONS from Constants.ts (don't duplicate)
export { COLLECTIONS } from '../../shared/Constants';
export type { CollectionName } from '../../shared/Constants';

/**
 * ⚠️ DO NOT ADD ACCESSOR FUNCTIONS HERE ⚠️
 *
 * For runtime path accessors that check config.env, use:
 * import { getDatabasePath, getBackupDir, etc. } from '../../config/ServerConfig';
 *
 * ServerConfig is the ONLY file that reads config.env/process.env
 */