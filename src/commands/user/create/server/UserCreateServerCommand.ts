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
import { Events } from '../../../../system/core/shared/Events';
import { DATA_EVENTS } from '../../../../system/core/shared/EventConstants';

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
        const listParams = createDataListParams(
          this.context,
          params.sessionId,
          {
            dbHandle: 'default' as const,
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

          // ON RECREATE: re-emit data:users:created so listeners (UserDaemon)
          // re-spin runtime instances. Without this, PersonaLifecycleManager
          // calls user/create on every boot for already-seeded personas, gets
          // existing-user-found, the create path silently returns success, and
          // UserDaemon's data:users:created subscription never fires — so no
          // PersonaUser instance is constructed, no .initialize() runs, no
          // chat subscriptions wire, and personas sit dead in the DB while
          // PersonaLifecycleManager logs "✅ activated."
          //
          // Empirical regression on Linux/CUDA Carl recreate (2026-04-24):
          // probe message stored cleanly via ORM, data:chat_messages:created
          // fired, ZERO persona handlers triggered. Logs showed
          // "🎭 Allocator returned 4 persona(s)" + "✅ 4 activated" but no
          // "📢 Subscribing to chat events for N room(s)" — because the chat
          // subscription path runs in PersonaUser.initialize() which only
          // runs from UserDaemon.handleUserCreated.
          //
          // Re-emitting on existing-user-found makes the recreate path
          // identical to the fresh-create path from UserDaemon's POV. Other
          // listeners (RoomMembershipDaemon auto-add) are idempotent
          // because membership checks gate on already-member.
          Events.emit(DATA_EVENTS.USERS.CREATED, existingUser);

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
