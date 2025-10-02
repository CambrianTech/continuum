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

export class UserCreateServerCommand extends UserCreateCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('user-create', context, subpath, commander);
  }

  async execute(params: UserCreateParams): Promise<UserCreateResult> {
    try {
      console.log(`🆕 UserCreate: Creating ${params.type} user "${params.displayName}"`);

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

      // Factory creates user via appropriate subclass
      const user = await UserFactory.create(params, this.context, this.commander.router);

      console.log(`✅ UserCreate: Created ${params.type} user ${user.entity.id}`);

      return createUserCreateResult(params, {
        success: true,
        user: user.entity
      });

    } catch (error) {
      console.error(`❌ UserCreate: Failed to create user:`, error);
      return createUserCreateResult(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
