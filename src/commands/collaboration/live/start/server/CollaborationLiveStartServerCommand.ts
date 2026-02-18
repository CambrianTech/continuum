/**
 * Collaboration Live Start Command - Server Implementation
 *
 * Start a live session with selected participants.
 * Creates or finds the DM room for the participant set, then joins the live session.
 * Like Discord's group call - select users, click call.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import { COMMANDS } from '@shared/generated-command-constants';
import type { CollaborationLiveStartParams, CollaborationLiveStartResult } from '../shared/CollaborationLiveStartTypes';
import { createCollaborationLiveStartResultFromParams } from '../shared/CollaborationLiveStartTypes';
import type { DmParams, DmResult } from '../../../dm/shared/DmTypes';
import type { LiveJoinParams, LiveJoinResult } from '../../join/shared/LiveJoinTypes';

export class CollaborationLiveStartServerCommand extends CommandBase<CollaborationLiveStartParams, CollaborationLiveStartResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/live/start', context, subpath, commander);
  }

  async execute(params: CollaborationLiveStartParams): Promise<CollaborationLiveStartResult> {
    // Validate required parameters
    if (!params.participants) {
      throw new Error('Missing required parameter: participants');
    }

    // Normalize participants to array
    const participantList = Array.isArray(params.participants)
      ? params.participants
      : [params.participants];

    if (participantList.length === 0) {
      throw new Error('At least one participant is required');
    }

    // Step 1: Create or find the DM room for this participant set
    const dmResult = await Commands.execute<DmParams, DmResult>(
      COMMANDS.COLLABORATION_DM,
      {
        participants: participantList,
        name: params.name,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!dmResult.success || !dmResult.room) {
      throw new Error(`Failed to create/find room: ${dmResult.message || 'Unknown error'}`);
    }

    // Step 2: Join the live session for this room
    const joinResult = await Commands.execute<LiveJoinParams, LiveJoinResult>(
      COMMANDS.COLLABORATION_LIVE_JOIN,
      {
        entityId: dmResult.room.id,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!joinResult.success || !joinResult.session) {
      throw new Error(`Failed to join live session: ${joinResult.message || 'Unknown error'}`);
    }

    // Return combined result
    return createCollaborationLiveStartResultFromParams(params, {
      success: true,
      message: dmResult.existed
        ? `Joined live session in existing room: ${dmResult.room.displayName}`
        : `Created room and started live session: ${dmResult.room.displayName}`,
      roomId: dmResult.roomId,
      liveSessionId: joinResult.sessionId,
      room: dmResult.room,
      session: joinResult.session,
      existed: dmResult.existed,
      participants: joinResult.participants
    });
  }
}
