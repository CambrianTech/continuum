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

  /** Datasets directory path for training data */
  DATASETS_DIR: '.continuum/datasets',

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
  TASKS: 'tasks',
  PINNED_ITEMS: 'pinned_items',
  COORDINATION_DECISIONS: 'coordination_decisions',

  // Room Wall System
  WALL_DOCUMENTS: 'wall_documents',

  // Cognition Observability
  COGNITION_STATE_SNAPSHOTS: 'cognition_state_snapshots',
  COGNITION_PLAN_RECORDS: 'cognition_plan_records',

  // Memory System (Phase 2)
  MEMORIES: 'memories'
} as const;

export type CollectionName = typeof COLLECTIONS[keyof typeof COLLECTIONS];