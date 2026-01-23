/**
 * Live Join Command - Base class for joining live sessions
 *
 * Joins or creates a live session for a room.
 * Handles participant tracking and session management.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { LiveJoinParams, LiveJoinResult } from './LiveJoinTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class LiveJoinCommand extends CommandBase<LiveJoinParams, LiveJoinResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/live/join', context, subpath, commander);
  }

  /**
   * Execute the join command
   */
  async execute(params: JTAGPayload): Promise<LiveJoinResult> {
    return this.executeJoin(params as LiveJoinParams);
  }

  /**
   * Subclass must implement the actual join logic
   */
  protected abstract executeJoin(params: LiveJoinParams): Promise<LiveJoinResult>;
}
