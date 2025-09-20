/**
 * User Create Persona Command - Server Implementation
 *
 * Creates PersonaUser instances via UserDaemonServer for AI chat participation with LoRA training
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { UserCreatePersonaParams, UserCreatePersonaResult } from '../shared/UserCreatePersonaTypes';
import { createUserCreatePersonaResultFromParams } from '../shared/UserCreatePersonaTypes';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';
import { PersonaUser } from '../../../../domain/user/PersonaUser';
import { COLLECTIONS } from '../../../../system/data/core/FieldMapping';
import { ISOString, UserId, CitizenId, SessionId } from '../../../../system/data/domains/CoreTypes';
import type { UserData } from '../../../../system/data/domains/User';
import type { AIModelConfig } from '../../../../domain/user/UserRelationships';
import type { DataCreateParams, DataCreateResult } from '../../../data/create/shared/DataCreateTypes';

export class UserCreatePersonaServerCommand extends CommandBase<UserCreatePersonaParams, UserCreatePersonaResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('user-create-persona', context, subpath, commander);
  }

  async execute(params: UserCreatePersonaParams): Promise<UserCreatePersonaResult> {
    console.debug(`🎭 USER SERVER: Creating PersonaUser "${params.displayName}" (${params.personaStyle}) for session ${params.sessionId}`);

    try {
      // Create new PersonaUser domain object
      const userId = generateUUID();

      // Default AI model config for personas if not provided
      const defaultModelConfig: AIModelConfig = {
        provider: 'anthropic',
        model: 'claude-3-haiku',
        temperature: 0.8,
        systemPrompt: params.systemPrompt || `You are a ${params.personaStyle} persona with distinct personality traits.`
      };

      const finalModelConfig = params.modelConfig || defaultModelConfig;
      const personaUser = PersonaUser.createNew(params.displayName, params.sessionId, params.personaStyle, finalModelConfig);

      // Serialize to UserData for UI persistence using DataDaemon static interface
      const userData: UserData = {
        // BaseEntity fields (required by UserData interface)
        id: userId,
        createdAt: ISOString(personaUser.createdAt),
        updatedAt: ISOString(new Date().toISOString()),
        version: 1,

        // UserData specific fields
        userId: UserId(userId),
        citizenId: CitizenId(userId), // Same for now
        type: 'ai' as const,

        profile: {
          displayName: personaUser.displayName,
          avatar: '🎭',
          joinedAt: ISOString(personaUser.createdAt)
        },

        capabilities: {
          canSendMessages: true,
          canReceiveMessages: true,
          canCreateRooms: false,
          canInviteOthers: false,
          canModerate: false,
          autoResponds: true,
          providesContext: true,
          canTrain: true,
          canAccessPersonas: false
        },

        preferences: {
          theme: 'auto' as const,
          language: 'en',
          timezone: 'UTC',
          notifications: {
            mentions: true,
            directMessages: true,
            roomUpdates: true
          },
          privacy: {
            showOnlineStatus: true,
            allowDirectMessages: true,
            shareActivity: true
          }
        },

        status: personaUser.isOnline ? 'online' as const : 'offline' as const,
        lastActiveAt: ISOString(personaUser.lastActiveAt),
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
        throw new Error(createResult.error || 'Failed to create persona user in database');
      }

      console.log(`✅ USER SERVER: Created PersonaUser ${personaUser.displayName} (${userId})`);

      return createUserCreatePersonaResultFromParams(params, {
        success: true,
        userId,
        userData
      });

    } catch (error) {
      const errorMessage = `Failed to create persona user: ${error instanceof Error ? error.message : String(error)}`;
      console.error(`❌ USER SERVER: ${errorMessage}`);

      return createUserCreatePersonaResultFromParams(params, {
        success: false,
        error: errorMessage
      });
    }
  }
}