/**
 * User Create Server Command
 *
 * Server-side user creation using UserFactory
 * ARCHITECTURE-RULES.md compliance:
 * - Server code can import server-only dependencies
 * - Uses UserFactory for type-specific creation
 */

import { UserCreateCommand } from '../shared/UserCreateCommand';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { UserCreateParams, UserCreateResult } from '../shared/UserCreateTypes';
import { createUserCreateResult } from '../shared/UserCreateTypes';
import { UserFactory } from '../../../../system/user/shared/UserFactory';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import { COLLECTIONS } from '../../../../system/data/config/DatabaseConfig';
import type { DataListParams, DataListResult } from '../../../data/list/shared/DataListTypes';
import { createDataListParams } from '../../../data/list/shared/DataListTypes';

export class UserCreateServerCommand extends UserCreateCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('user-create', context, subpath, commander);
  }

  async execute(params: UserCreateParams): Promise<UserCreateResult> {
    try {
      // Validate parameters
      if (!params.type) {
        return createUserCreateResult(params, {
          success: false,
          error: 'User type is required'
        });
      }

      if (!params.displayName) {
        return createUserCreateResult(params, {
          success: false,
          error: 'Display name is required'
        });
      }

      // Check if user with this uniqueId already exists (prevent duplicates on re-seed)
      if (params.uniqueId) {
        // Use proper command interface, not direct DataDaemon access
        const listCommand = this.commander.commands.get(DATA_COMMANDS.LIST);
        if (!listCommand) {
          throw new Error('data/list command not available');
        }

        // Create properly typed params with context and sessionId
        const listParams = createDataListParams<UserEntity>(
          this.context,
          params.sessionId,
          {
            collection: COLLECTIONS.USERS,
            filter: { uniqueId: params.uniqueId },
            limit: 1
          }
        );

        const existingResult = await listCommand.execute(listParams) as DataListResult<UserEntity>;

        if (existingResult.success && existingResult.items && existingResult.items.length > 0) {
          console.log(`⚠️ User with uniqueId="${params.uniqueId}" already exists, returning existing user`);

          // data/list command returns items array with UserEntity objects directly
          const existingUser = existingResult.items[0];

          return createUserCreateResult(params, {
            success: true,
            user: existingUser
          });
        }
      }

      // Factory creates user via appropriate subclass
      const user = await UserFactory.create(params, this.context, this.commander.router);

      return createUserCreateResult(params, {
        success: true,
        user: user.entity
      });

    } catch (error) {
      return createUserCreateResult(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
