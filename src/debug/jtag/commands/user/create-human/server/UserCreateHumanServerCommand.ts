/**
 * User Create Human Command - Server Implementation
 *
 * Creates HumanUser instances via UserDaemonServer for chat participation
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { UserCreateHumanParams, UserCreateHumanResult } from '../shared/UserCreateHumanTypes';
import { createUserCreateHumanResultFromParams } from '../shared/UserCreateHumanTypes';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';
import { HumanUser } from '../../../../domain/user/HumanUser';
import { COLLECTIONS } from '../../../../system/data/core/FieldMapping';
import { ISOString, UserId, CitizenId, SessionId } from '../../../../system/data/domains/CoreTypes';
import type { UserData } from '../../../../system/data/domains/User';
import type { DataCreateParams, DataCreateResult } from '../../../data/create/shared/DataCreateTypes';
import { USER_EVENTS } from '../../../../system/events/user/UserEventConstants';
import type { UserCreatedEventData } from '../../../../system/events/user/UserEventTypes';
import { EVENT_SCOPES } from '../../../../system/events/shared/EventSystemConstants';
import type { EventBridgePayload } from '../../../../system/events/shared/EventSystemTypes';
import { JTAGMessageFactory } from '../../../../system/core/types/JTAGTypes';
import { JTAG_ENDPOINTS } from '../../../../system/core/router/shared/JTAGEndpoints';

export class UserCreateHumanServerCommand extends CommandBase<UserCreateHumanParams, UserCreateHumanResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('user-create-human', context, subpath, commander);
  }

  async execute(params: UserCreateHumanParams): Promise<UserCreateHumanResult> {
    console.debug(`👤 USER SERVER: Creating HumanUser "${params.displayName}" for session ${params.sessionId}`);

    try {
      // Create new HumanUser domain object
      const userId = generateUUID();
      const humanUser = HumanUser.createNew(params.displayName, params.sessionId);

      // Serialize to UserData for UI persistence
      const userData: UserData = {
        // BaseEntity fields (required by UserData interface)
        id: userId,
        createdAt: ISOString(humanUser.createdAt),
        updatedAt: ISOString(new Date().toISOString()),
        version: 1,

        // UserData specific fields
        userId: UserId(userId),
        citizenId: CitizenId(userId), // Same for now
        type: 'human' as const,

        profile: {
          displayName: humanUser.displayName,
          avatar: '👤',
          joinedAt: ISOString(humanUser.createdAt)
        },

        capabilities: {
          canSendMessages: true,
          canReceiveMessages: true,
          canCreateRooms: true,
          canInviteOthers: true,
          canModerate: false,
          autoResponds: false,
          providesContext: false,
          canTrain: false,
          canAccessPersonas: true
        },

        preferences: {
          theme: 'auto' as const,
          language: 'en',
          timezone: 'UTC',
          notifications: {
            mentions: true,
            directMessages: true,
            roomUpdates: false
          },
          privacy: {
            showOnlineStatus: true,
            allowDirectMessages: true,
            shareActivity: false
          }
        },

        status: humanUser.isOnline ? 'online' as const : 'offline' as const,
        lastActiveAt: ISOString(humanUser.lastActiveAt),
        sessionsActive: [SessionId(params.sessionId)]
      };

      // Store using server-side command delegation
      const createResult = await this.remoteExecute<DataCreateParams, DataCreateResult>(
        {
          collection: COLLECTIONS.USERS,
          data: userData,
          id: userId,
          context: this.context,
          sessionId: params.sessionId
        },
        'data/create',
        'server' // Explicitly target server environment
      );

      if (!createResult.success) {
        throw new Error(createResult.error || 'Failed to create user in database');
      }

      console.log(`✅ USER SERVER: Created HumanUser ${humanUser.displayName} (${userId})`);

      // Emit USER_CREATED event (following chat message pattern)
      await this.emitUserCreatedEvent(humanUser, userData, userId);

      return createUserCreateHumanResultFromParams(params, {
        success: true,
        userId,
        userData
      });

    } catch (error) {
      const errorMessage = `Failed to create human user: ${error instanceof Error ? error.message : String(error)}`;
      console.error(`❌ USER SERVER: ${errorMessage}`);

      return createUserCreateHumanResultFromParams(params, {
        success: false,
        error: errorMessage
      });
    }
  }

  /**
   * Emit USER_CREATED event for real-time widget updates
   * Follows the same pattern as chat message events
   */
  private async emitUserCreatedEvent(humanUser: HumanUser, userData: UserData, userId: string): Promise<void> {
    try {
      const eventPayload: EventBridgePayload = {
        context: this.context,
        sessionId: userData.sessionsActive[0], // Use first active session
        type: 'event-bridge',
        scope: {
          type: EVENT_SCOPES.GLOBAL, // User events are global (not room-scoped like chat)
          id: 'users',
          sessionId: userData.sessionsActive[0]
        },
        eventName: USER_EVENTS.USER_CREATED,
        data: {
          eventType: 'user:user-created',
          userId,
          user: humanUser,
          userData,
          timestamp: new Date().toISOString()
        } as UserCreatedEventData,
        originSessionId: userData.sessionsActive[0] as string,
        originContextUUID: this.context.uuid,
        timestamp: new Date().toISOString()
      };

      // Create event message using JTAG message factory
      const eventMessage = JTAGMessageFactory.createEvent(
        this.context,
        'user-create-human',
        JTAG_ENDPOINTS.EVENTS.BRIDGE,
        eventPayload
      );

      // Route event through Router (handles cross-context distribution)
      const result = await this.commander.router.postMessage(eventMessage);
      console.log(`📨 SERVER-EVENT: Emitted USER_CREATED for user ${humanUser.displayName} (${userId})`, result);

    } catch (error) {
      console.error(`❌ USER-EVENT-EMISSION-FAILED-${Date.now()}: Failed to emit user created event:`, error);
      // Don't fail the entire operation if event emission fails
    }
  }

}