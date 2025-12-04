/**
 * IUserStorage - Interface for user database operations
 *
 * Abstracts database operations for all user types
 * Implementations: SQLite (PersonaUser, HumanUser), Memory (AgentUser, tests)
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

export interface IUserStorage {
  /**
   * Store a value in user's database
   * @param key - Key to store under
   * @param value - Value to store (will be JSON serialized)
   */
  store<T>(key: string, value: T): Promise<{ success: boolean; error?: string }>;

  /**
   * Read a value from user's database
   * @param key - Key to read
   * @returns Value or null if not found
   */
  read<T>(key: string): Promise<{ success: boolean; data?: T; error?: string }>;

  /**
   * Delete a value from user's database
   * @param key - Key to delete
   */
  delete(key: string): Promise<{ success: boolean; error?: string }>;

  /**
   * List all keys in user's database
   */
  listKeys(): Promise<{ success: boolean; keys?: string[]; error?: string }>;

  /**
   * Check if storage is available
   */
  isAvailable(): Promise<boolean>;

  /**
   * Get database path (if applicable)
   */
  getDatabasePath(): string | undefined;

  /**
   * Close database connection
   */
  close(): Promise<void>;
}
