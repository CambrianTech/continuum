/**
 * Database Configuration Constants
 *
 * Single source of truth for database paths and configuration
 */

/**
 * Database file paths - centralized configuration
 */
export const DATABASE_PATHS = {
  /** Main SQLite database file path */
  SQLITE: '.continuum/jtag/data/database.sqlite',

  /** Data directory base path */
  DATA_DIR: '.continuum/jtag/data',

  /** Backup directory path */
  BACKUP_DIR: '.continuum/jtag/backups',

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
 * Database configuration settings
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

/**
 * Collection names - centralized collection naming
 */
export const COLLECTIONS = {
  USERS: 'users',
  USER_STATES: 'user_states',
  ROOMS: 'rooms',
  CHAT_MESSAGES: 'chat_messages',
  ARTIFACTS: 'artifacts',
  SESSIONS: 'sessions',
  TASKS: 'tasks'
} as const;

export type CollectionName = typeof COLLECTIONS[keyof typeof COLLECTIONS];