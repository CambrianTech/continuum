/**
 * User List Command - Shared Types
 *
 * List users in the system with filtering by type, status, and capabilities.
 * Essential for AI discovery — find other personas, check who's online, discover collaboration partners.
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { UserType, UserStatus } from '@system/data/entities/UserEntity';

/**
 * Lightweight user summary for discovery — no sensitive fields, no full entity.
 */
export interface UserSummary {
  id: UUID;
  uniqueId: string;
  displayName: string;
  type: UserType;
  status: UserStatus;
  shortDescription?: string;
  lastActiveAt: string;   // ISO date string
  provider?: string;      // AI provider (anthropic, openai, groq, etc.)
  model?: string;         // Model ID
  intelligenceLevel?: number;
  capabilities: {
    autoResponds: boolean;
    canTrain: boolean;
    canSendMessages: boolean;
  };
}

/**
 * User List Command Parameters
 */
export interface UserListParams extends CommandParams {
  type?: string;
  status?: string;
  capability?: string;
  provider?: string;
  limit?: number;
}

/**
 * User List Command Result
 */
export interface UserListResult extends CommandResult {
  success: boolean;
  users: UserSummary[];
  total: number;
  error?: string;
}

/**
 * Smart User List-specific inheritance from params
 */
export const createUserListResultFromParams = (
  params: UserListParams,
  differences: Omit<UserListResult, 'context' | 'sessionId' | 'userId'>
): UserListResult => transformPayload(params, differences);

/**
 * User List — Type-safe command executor
 *
 * Usage:
 *   import { UserList } from '...shared/UserListTypes';
 *   const result = await UserList.execute({ type: 'persona', status: 'online' });
 */
export const UserList = {
  execute(params: CommandInput<UserListParams>): Promise<UserListResult> {
    return Commands.execute<UserListParams, UserListResult>('user/list', params as Partial<UserListParams>);
  },
  commandName: 'user/list' as const,
} as const;
