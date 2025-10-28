/**
 * User Create Server Command
 *
 * Server-side user creation using UserFactory
 * ARCHITECTURE-RULES.md compliance:
 * - Server code can import server-only dependencies
 * - Uses UserFactory for type-specific creation
 */

import { UserCreateCommand } from '../shared/UserCreateCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { UserCreateParams, UserCreateResult } from '../shared/UserCreateTypes';
import { createUserCreateResult } from '../shared/UserCreateTypes';
import { UserFactory } from '../../../../system/user/shared/UserFactory';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../../system/data/config/DatabaseConfig';

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
        const existingResult = await DataDaemon.query<UserEntity>({
          collection: COLLECTIONS.USERS,
          filter: { uniqueId: params.uniqueId }
        });

        if (existingResult.success && existingResult.data && existingResult.data.length > 0) {
          console.log(`⚠️ User with uniqueId="${params.uniqueId}" already exists, returning existing user`);

          // DataDaemon.query returns StorageResult<DataRecord<UserEntity>[]>
          // DataRecord has structure: { id, collection, data: UserEntity, metadata }
          // So existingResult.data[0].data is the actual UserEntity
          const existingUser = existingResult.data[0].data;

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
