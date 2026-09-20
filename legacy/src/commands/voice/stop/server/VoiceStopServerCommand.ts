/**
 * Voice Stop Command - Server Implementation
 *
 * Stop an active voice chat session
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { VoiceStopParams, VoiceStopResult } from '../shared/VoiceStopTypes';
import { createVoiceStopResultFromParams } from '../shared/VoiceStopTypes';
import { VoiceSessionManager } from '../../shared/VoiceSessionManager';

export class VoiceStopServerCommand extends CommandBase<VoiceStopParams, VoiceStopResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/stop', context, subpath, commander);
  }

  async execute(params: VoiceStopParams): Promise<VoiceStopResult> {
    console.log('🎤 SERVER: Stopping voice session', params);

    let handle = params.handle;

    // If no handle provided, try to find user's current session
    if (!handle && params.sessionId) {
      const userSessions = VoiceSessionManager.getSessionsForUser(params.sessionId);
      if (userSessions.length > 0) {
        // Get most recent session
        handle = userSessions[userSessions.length - 1].handle;
      }
    }

    // Validate we have a handle to stop
    if (!handle) {
      throw new ValidationError(
        'handle',
        `No active voice session found. Either provide a handle or ensure you have an active session.`
      );
    }

    // End the session
    const result = VoiceSessionManager.endSession(handle);

    if (!result) {
      return createVoiceStopResultFromParams(params, {
        success: false,
        stopped: false,
        handle,
        duration: 0,
      });
    }

    console.log(`🎤 Voice session stopped: ${handle.substring(0, 8)}... (${result.duration}s)`);

    return createVoiceStopResultFromParams(params, {
      success: true,
      stopped: true,
      handle,
      duration: result.duration,
    });
  }
}
