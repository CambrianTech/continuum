/**
 * Server Configuration Accessor - SERVER-ONLY
 *
 * ⚠️ THIS IS THE ONLY FILE THAT ACCESSES config.env/process.env ⚠️
 * ⚠️ DO NOT import this file in browser code! ⚠️
 * ⚠️ DO NOT create other files that read process.env! ⚠️
 *
 * Single source of truth for ALL server configuration:
 * - Database paths (from DATABASE_DIR, DATABASE_BACKUP_DIR, etc.)
 * - Secrets/API keys (delegates to SecretManager)
 * - Any other config.env values
 *
 * Architecture:
 * - SecretManager loads config.env into memory
 * - ServerConfig provides typed accessors for specific values
 * - All server code calls ServerConfig, never process.env directly
 */

import { SecretManager } from '../secrets/SecretManager';
import { DATABASE_PATHS } from '../data/config/DatabaseConfig';
import { HTTP_PORT, WS_PORT } from '../../shared/config';

/**
 * Server Configuration - Singleton accessor for all config.env values
 */
export class ServerConfig {
  private static instance: ServerConfig | null = null;
  private secrets: SecretManager;

  private constructor() {
    this.secrets = SecretManager.getInstance();
  }

  /**
   * Get singleton instance
   */
  static getInstance(): ServerConfig {
    if (!ServerConfig.instance) {
      ServerConfig.instance = new ServerConfig();
    }
    return ServerConfig.instance;
  }

  // ===================================
  // Database Path Accessors
  // ===================================

  /**
   * Expand $HOME or ~ in path
   */
  private expandPath(path: string): string {
    const home = process.env.HOME || process.env.USERPROFILE || '~';
    return path
      .replace(/^\$HOME/, home)
      .replace(/^~/, home);
  }

  /**
   * Get main database directory path
   * Checks DATABASE_DIR environment variable, falls back to default
   */
  getDatabaseDir(): string {
    const path = process.env.DATABASE_DIR || DATABASE_PATHS.DATA_DIR;
    return this.expandPath(path);
  }

  /**
   * Get main database connection string.
   *
   * Checks DATABASE_URL first — if set to a postgres:// URL, returns that
   * (routes to PostgresAdapter in Rust). Otherwise falls back to the
   * SQLite file path at $HOME/.continuum/data/database.sqlite.
   */
  getDatabasePath(): string {
    const databaseUrl = process.env.DATABASE_URL;
    if (databaseUrl && (databaseUrl.startsWith('postgres://') || databaseUrl.startsWith('postgresql://'))) {
      return databaseUrl;
    }
    const dir = this.getDatabaseDir();
    return `${dir}/database.sqlite`;
  }

  /**
   * Get backup directory path
   * Checks DATABASE_BACKUP_DIR environment variable
   */
  getBackupDir(): string {
    const path = process.env.DATABASE_BACKUP_DIR || DATABASE_PATHS.BACKUP_DIR;
    return this.expandPath(path);
  }

  /**
   * Get archive directory path
   * Checks DATABASE_ARCHIVE_DIR environment variable
   */
  getArchiveDir(): string {
    const path = process.env.DATABASE_ARCHIVE_DIR || DATABASE_PATHS.ARCHIVE_DIR;
    return this.expandPath(path);
  }

  /**
   * Get datasets directory path
   * Checks DATASETS_DIR environment variable
   */
  getDatasetsDir(): string {
    return process.env.DATASETS_DIR || DATABASE_PATHS.DATASETS_DIR;
  }

  /**
   * Get logs directory path
   */
  getLogsDir(): string {
    return DATABASE_PATHS.LOGS_DIR;
  }

  /**
   * Get signals directory path
   */
  getSignalsDir(): string {
    return DATABASE_PATHS.SIGNALS_DIR;
  }

  /**
   * Get sessions directory path
   */
  getSessionsDir(): string {
    return DATABASE_PATHS.SESSIONS_DIR;
  }

  // ===================================
  // Network Port Accessors
  // ===================================

  /**
   * Get HTTP server port
   * Returns build-time constant from shared/config.ts (generated from config.env)
   */
  getHttpPort(): number {
    return HTTP_PORT;
  }

  /**
   * Get WebSocket server port
   * Returns build-time constant from shared/config.ts (generated from config.env)
   */
  getWsPort(): number {
    return WS_PORT;
  }

  // ===================================
  // Secrets Accessors (delegates to SecretManager)
  // ===================================

  /**
   * Get secret value (API keys, etc.)
   */
  getSecret(key: string, requestedBy = 'unknown'): string | undefined {
    return this.secrets.get(key, requestedBy);
  }

  /**
   * Require secret value (throws if missing)
   */
  requireSecret(key: string, requestedBy = 'unknown'): string {
    return this.secrets.require(key, requestedBy);
  }

  /**
   * Check if secret exists
   */
  hasSecret(key: string): boolean {
    return this.secrets.has(key);
  }

  // ===================================
  // Generic Config Accessors
  // ===================================

  /**
   * Get any config value from environment
   * For values not covered by typed accessors above
   */
  get(key: string, defaultValue?: string): string | undefined {
    return process.env[key] || defaultValue;
  }

  /**
   * Get config value as number
   */
  getNumber(key: string, defaultValue: number): number {
    const value = process.env[key];
    if (!value) return defaultValue;
    const parsed = parseInt(value, 10);
    return isNaN(parsed) ? defaultValue : parsed;
  }

  /**
   * Get config value as boolean
   */
  getBoolean(key: string, defaultValue: boolean): boolean {
    const value = process.env[key];
    if (!value) return defaultValue;
    return value.toLowerCase() === 'true' || value === '1';
  }

  // ===================================
  // Data Daemon Configuration
  // ===================================

  /**
   * Get data daemon type: 'sqlite' (TypeScript) or 'rust' (Rust worker)
   * Reads from DATA_DAEMON_TYPE in config.env
   */
  getDataDaemonType(): 'sqlite' | 'rust' {
    // Check SecretManager first (loads from ~/.continuum/config.env)
    const value = this.secrets.get('DATA_DAEMON_TYPE', 'ServerConfig')?.toLowerCase();
    if (value === 'rust') return 'rust';
    return 'sqlite';  // Default to TypeScript SQLite adapter
  }
}

// ===================================
// Convenience Functions (single import)
// ===================================

/**
 * Get server config instance
 */
export function getServerConfig(): ServerConfig {
  return ServerConfig.getInstance();
}

/**
 * Get database directory path
 */
export function getDatabaseDir(): string {
  return ServerConfig.getInstance().getDatabaseDir();
}

/**
 * Get database file path
 */
export function getDatabasePath(): string {
  return ServerConfig.getInstance().getDatabasePath();
}

/**
 * Get backup directory path
 */
export function getBackupDir(): string {
  return ServerConfig.getInstance().getBackupDir();
}

/**
 * Get archive directory path
 */
export function getArchiveDir(): string {
  return ServerConfig.getInstance().getArchiveDir();
}

/**
 * Get datasets directory path
 */
export function getDatasetsDir(): string {
  return ServerConfig.getInstance().getDatasetsDir();
}
