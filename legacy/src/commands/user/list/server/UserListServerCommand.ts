/**
 * User List Command - Server Implementation
 *
 * Queries the user database with filtering by type, status, provider, and capabilities.
 * Returns lightweight UserSummary objects — safe for AI discovery.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { UserListParams, UserListResult, UserSummary } from '../shared/UserListTypes';
import { createUserListResultFromParams } from '../shared/UserListTypes';
import { ORM } from '@daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '@system/data/config/DatabaseConfig';
import type { UserEntity, UserType, UserStatus } from '@system/data/entities/UserEntity';

export class UserListServerCommand extends CommandBase<UserListParams, UserListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('user/list', context, subpath, commander);
  }

  async execute(params: UserListParams): Promise<UserListResult> {
    const limit = params.limit ?? 50;

    // Build ORM filter from params (type/status are DB-level filters)
    const filter: Record<string, string> = {};
    if (params.type) filter.type = params.type;
    if (params.status) filter.status = params.status;

    // Query WITHOUT limit — post-query filters (provider, capability) may discard
    // rows, so we need the full filtered set to report accurate total and avoid
    // missing matches that fall outside an arbitrary DB-level limit.
    const result = await ORM.query<UserEntity>({
      collection: COLLECTIONS.USERS,
      filter,
      sort: [{ field: 'lastActiveAt', direction: 'desc' }],
    }, 'default');

    if (!result.success || !result.data) {
      return createUserListResultFromParams(params, {
        success: false,
        users: [],
        total: 0,
        error: 'Failed to query users',
      });
    }

    // Hydrate and apply post-query filters (provider, capability)
    let users: UserSummary[] = result.data.map(row => {
      const entity = { ...row.data, id: row.id } as UserEntity;
      return this.toSummary(entity);
    });

    // Post-query filter: provider (stored inside modelConfig JSON)
    if (params.provider) {
      const providerLower = params.provider.toLowerCase();
      users = users.filter(u => u.provider?.toLowerCase() === providerLower);
    }

    // Post-query filter: capability
    if (params.capability) {
      const cap = params.capability as keyof UserSummary['capabilities'];
      users = users.filter(u => {
        const capabilities = u.capabilities as Record<string, boolean>;
        return capabilities[cap] === true;
      });
    }

    // total reflects full post-filtered count; slice to requested limit for response
    const total = users.length;
    const pagedUsers = users.slice(0, limit);

    return createUserListResultFromParams(params, {
      success: true,
      users: pagedUsers,
      total,
    });
  }

  private toSummary(entity: UserEntity): UserSummary {
    return {
      id: entity.id,
      uniqueId: entity.uniqueId,
      displayName: entity.displayName,
      type: entity.type,
      status: entity.status,
      shortDescription: entity.shortDescription,
      lastActiveAt: entity.lastActiveAt instanceof Date
        ? entity.lastActiveAt.toISOString()
        : String(entity.lastActiveAt),
      provider: entity.modelConfig?.provider,
      model: entity.modelConfig?.model,
      intelligenceLevel: entity.intelligenceLevel,
      capabilities: {
        autoResponds: entity.capabilities?.autoResponds ?? false,
        canTrain: entity.capabilities?.canTrain ?? false,
        canSendMessages: entity.capabilities?.canSendMessages ?? false,
      },
    };
  }
}
