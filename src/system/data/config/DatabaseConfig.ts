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
 * ROUTING: ServerConfig.getDatabasePath() checks DATABASE_URL first:
 *   - postgres:// or postgresql:// → PostgresAdapter (async pool, MVCC)
 *   - File path or unset → SqliteAdapter (WAL mode)
 *
 * Override via config.env:
 *   DATABASE_URL     — Primary connection string (postgres://user@host/db)
 *   DATABASE_DIR     — SQLite fallback directory ($HOME/.continuum/data)
 *
 * NOTE: These are COMPILE-TIME constants for fallback only.
 * Runtime paths come from ServerConfig which checks config.env first.
 */
export const DATABASE_PATHS = {
  /** Default Postgres connection (dedicated cluster at ~/.continuum/data/postgres/, port 5433) */
  POSTGRES: 'postgres://joel@localhost:5433/continuum',

  /** SQLite fallback path (used when DATABASE_URL is not set) */
  SQLITE: '$HOME/.continuum/data/database.sqlite',

  /** Main database directory (server-only) - SINGULAR DEFAULT */
  DATA_DIR: '$HOME/.continuum/data',

  /** Backup directory path (server-only) */
  BACKUP_DIR: '$HOME/.continuum/data/backups',

  /** Archive directory path (server-only) */
  ARCHIVE_DIR: '$HOME/.continuum/data/archive',

  /** Datasets directory path for training data */
  DATASETS_DIR: PATHS.DATASETS,

  /** Infrastructure log files directory (SQL, tools, system) */
  LOGS_DIR: '.continuum/jtag/logs',

  /** Signal files directory (system ready indicators) */
  SIGNALS_DIR: '.continuum/jtag/signals',

  /** Sessions directory */
  SESSIONS_DIR: '.continuum/jtag/sessions',

  /** Legacy database path (for migration reference) */
  LEGACY: '.continuum/database/continuum.db'
} as const;

/**
 * Database filenames - centralized naming
 */
export const DATABASE_FILES = {
  /** Main SQLite database filename */
  SQLITE_FILENAME: 'database.sqlite'
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