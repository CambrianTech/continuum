/**
 * UserDaemon Types - Shared type definitions
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * User Operation Context - Similar to DataOperationContext
 */
export interface UserOperationContext {
  readonly sessionId: UUID;
  readonly timestamp: string;
  readonly source: string;
  readonly transactionId?: UUID;
}

/**
 * User Query Parameters
 */
export interface UserQueryParams {
  readonly isActive?: boolean;
  readonly citizenType?: 'human' | 'ai' | 'system';
  readonly sessionId?: UUID;
  readonly limit?: number;
  readonly orderBy?: Array<{
    readonly field: string;
    readonly direction: 'asc' | 'desc';
  }>;
}

/**
 * User Authentication Result
 */
export interface UserAuthResult {
  readonly success: boolean;
  readonly userId?: UUID;
  readonly error?: string;
}

/**
 * User Session Info
 */
export interface UserSessionInfo {
  readonly userId: UUID;
  readonly sessionId: UUID;
  readonly isActive: boolean;
  readonly lastActiveAt: string;
  readonly userAgent?: string;
}