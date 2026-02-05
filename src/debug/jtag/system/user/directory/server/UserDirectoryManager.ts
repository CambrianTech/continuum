/**
 * UserDirectoryManager - Unified directory structure for all users
 *
 * Manages the .continuum/users/{userId}/ directory structure
 * Works identically for HumanUser and PersonaUser - no special cases
 *
 * Directory structure:
 * .continuum/users/{userId}/
 *   ├── data/
 *   │   └── longterm.db  (UserStateEntity + memories + everything)
 *   ├── logs/
 *   │   └── activity.log
 *   └── config/
 *       └── preferences.json
 */

import * as path from 'path';
import * as fs from 'fs';
import type { UUID } from '../../../core/types/CrossPlatformUUID';

export interface UserDirectoryPaths {
  root: string;              // .continuum/users/{userId}
  data: string;              // .continuum/users/{userId}/data
  logs: string;              // .continuum/users/{userId}/logs
  config: string;            // .continuum/users/{userId}/config
  database: string;          // .continuum/users/{userId}/data/longterm.db
  activityLog: string;       // .continuum/users/{userId}/logs/activity.log
  preferences: string;       // .continuum/users/{userId}/config/preferences.json
}

export class UserDirectoryManager {
  private readonly baseDir: string;

  /**
   * @param baseDir - Base directory for all users (default: .continuum/users)
   */
  constructor(baseDir: string = '.continuum/users') {
    this.baseDir = baseDir;
  }

  /**
   * Get all paths for a user.
   * Note: Persona directories now use uniqueId (human-readable), not UUID.
   * The legacy UUID fallback checks `.continuum/personas/{userId}` for backward compat.
   */
  getPaths(userId: UUID): UserDirectoryPaths {
    let root = path.join(this.baseDir, userId);

    // Check if new path exists
    if (!fs.existsSync(root)) {
      // Fall back to legacy persona path if it exists (may be UUID-named from old code)
      const legacyPath = path.join('.continuum/personas', userId);
      if (fs.existsSync(legacyPath)) {
        root = legacyPath;
      }
      // else: use new path (will be created on ensureDirectories)
    }

    const data = path.join(root, 'data');
    const logs = path.join(root, 'logs');
    const config = path.join(root, 'config');

    return {
      root,
      data,
      logs,
      config,
      database: path.join(data, 'longterm.db'),
      activityLog: path.join(logs, 'activity.log'),
      preferences: path.join(config, 'preferences.json')
    };
  }

  /**
   * Ensure user directory structure exists
   * Creates all subdirectories if they don't exist
   */
  async ensureDirectories(userId: UUID): Promise<void> {
    const paths = this.getPaths(userId);

    // Create all directories
    await fs.promises.mkdir(paths.root, { recursive: true });
    await fs.promises.mkdir(paths.data, { recursive: true });
    await fs.promises.mkdir(paths.logs, { recursive: true });
    await fs.promises.mkdir(paths.config, { recursive: true });
  }

  /**
   * Check if user directory exists
   */
  async exists(userId: UUID): Promise<boolean> {
    const paths = this.getPaths(userId);
    try {
      await fs.promises.access(paths.root, fs.constants.F_OK);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Get database path for a user
   * Most common operation - shorthand for getPaths(userId).database
   */
  getDatabasePath(userId: UUID): string {
    return this.getPaths(userId).database;
  }

  /**
   * Get activity log path for a user
   */
  getActivityLogPath(userId: UUID): string {
    return this.getPaths(userId).activityLog;
  }

  /**
   * Get preferences path for a user
   */
  getPreferencesPath(userId: UUID): string {
    return this.getPaths(userId).preferences;
  }

  /**
   * List all user IDs with directories
   */
  async listUsers(): Promise<UUID[]> {
    try {
      const entries = await fs.promises.readdir(this.baseDir, { withFileTypes: true });
      return entries
        .filter(entry => entry.isDirectory())
        .map(entry => entry.name as UUID);
    } catch {
      return [];
    }
  }

  /**
   * Delete user directory (careful!)
   * Only use for cleanup/testing
   */
  async deleteUserDirectory(userId: UUID): Promise<void> {
    const paths = this.getPaths(userId);
    await fs.promises.rm(paths.root, { recursive: true, force: true });
  }

  /**
   * Get disk usage for a user
   */
  async getUserDiskUsage(userId: UUID): Promise<{ bytes: number; humanReadable: string }> {
    const paths = this.getPaths(userId);

    try {
      const stats = await fs.promises.stat(paths.database);
      const bytes = stats.size;

      // Convert to human readable
      const units = ['B', 'KB', 'MB', 'GB'];
      let size = bytes;
      let unitIndex = 0;

      while (size >= 1024 && unitIndex < units.length - 1) {
        size /= 1024;
        unitIndex++;
      }

      const humanReadable = `${size.toFixed(2)} ${units[unitIndex]}`;

      return { bytes, humanReadable };
    } catch {
      return { bytes: 0, humanReadable: '0 B' };
    }
  }
}
