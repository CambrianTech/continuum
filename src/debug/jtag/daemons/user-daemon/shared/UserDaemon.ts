/**
 * UserDaemon - Universal User Management Orchestrator
 *
 * Proper browser/server abstraction following DataDaemon pattern.
 * Manages living user objects (BaseUser hierarchy) for chat participation and LoRA training.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseUser } from '../../../domain/user/BaseUser';
import type { StorageResult } from '../../data-daemon/shared/DataStorageAdapter';
import type { UserDaemonBase } from './UserDaemonBase';
import type { UserOperationContext, UserQueryParams, UserAuthResult } from './UserDaemonTypes';

/**
 * UserDaemon - Static Interface with Auto-Context Injection
 *
 * Follows the same pattern as DataDaemon.store(), DataDaemon.query(), etc.
 * Uses sharedInstance that gets initialized with browser/server specific implementation.
 */
export class UserDaemon {
  private static sharedInstance: UserDaemonBase | undefined;
  private static context: UserOperationContext | undefined;

  /**
   * Initialize UserDaemon with environment-specific implementation
   */
  static initialize(instance: UserDaemonBase, context: UserOperationContext): void {
    UserDaemon.sharedInstance = instance;
    UserDaemon.context = context;
  }

  private static ensureInitialized(): void {
    if (!UserDaemon.sharedInstance || !UserDaemon.context) {
      throw new Error('UserDaemon not initialized - system must call UserDaemon.initialize() first');
    }
  }

  /**
   * Create Human user - type-safe, no generics
   */
  static async createHuman(
    displayName: string,
    sessionId: UUID
  ): Promise<StorageResult<BaseUser>> {
    UserDaemon.ensureInitialized();
    return UserDaemon.sharedInstance!.createHuman(displayName, sessionId, UserDaemon.context!);
  }

  /**
   * Create Agent user - type-safe, no generics
   */
  static async createAgent(
    displayName: string,
    agentType: string
  ): Promise<StorageResult<BaseUser>> {
    UserDaemon.ensureInitialized();
    return UserDaemon.sharedInstance!.createAgent(displayName, agentType, UserDaemon.context!);
  }

  /**
   * Query users with automatic context injection
   */
  static async query(params: UserQueryParams): Promise<StorageResult<BaseUser[]>> {
    UserDaemon.ensureInitialized();
    return UserDaemon.sharedInstance!.query(params, UserDaemon.context!);
  }

  /**
   * Get user by ID with automatic context injection
   */
  static async getById(userId: UUID): Promise<StorageResult<BaseUser | null>> {
    UserDaemon.ensureInitialized();
    return UserDaemon.sharedInstance!.getById(userId, UserDaemon.context!);
  }

  /**
   * Update user presence status
   */
  static async updatePresence(
    userId: UUID,
    isOnline: boolean,
    lastActiveAt?: string
  ): Promise<StorageResult<void>> {
    UserDaemon.ensureInitialized();
    const timestamp = lastActiveAt || new Date().toISOString();
    return UserDaemon.sharedInstance!.updatePresence(userId, isOnline, timestamp, UserDaemon.context!);
  }

  /**
   * Get user presence status
   */
  static async getPresence(userId: UUID): Promise<StorageResult<{ isOnline: boolean; lastActiveAt: string }>> {
    UserDaemon.ensureInitialized();
    return UserDaemon.sharedInstance!.getPresence(userId, UserDaemon.context!);
  }

  /**
   * Authenticate user by session - critical for chat and state management
   */
  static async authenticateBySession(sessionId: UUID): Promise<StorageResult<BaseUser | null>> {
    const result = await UserDaemon.query({ sessionId, limit: 1 });
    if (!result.success) {
      return { success: false, error: result.error };
    }
    return {
      success: true,
      data: result.data?.[0] ?? null
    };
  }

  /**
   * Get or create session user - ensures user exists for session
   */
  static async getOrCreateSessionUser(sessionId: UUID, displayName: string): Promise<StorageResult<BaseUser>> {
    // Try to find existing user for session
    const existingResult = await UserDaemon.authenticateBySession(sessionId);
    if (existingResult.success && existingResult.data) {
      return { success: true, data: existingResult.data };
    }

    // Create new human user for session
    return UserDaemon.createHuman(displayName, sessionId);
  }
}