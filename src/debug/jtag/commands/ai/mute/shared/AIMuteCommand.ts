/**
 * AI Mute Command - Shared Base Implementation
 *
 * Mute or unmute an AI persona from acting in the system.
 * Enforces democratic governance with permission checks and veto power.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIMuteParams, AIMuteResult } from './AIMuteTypes';

/**
 * Abstract base for AIMute commands
 */
export abstract class AIMuteCommand extends CommandBase<AIMuteParams, AIMuteResult> {
  static readonly commandName = 'ai/mute';

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai-mute', context, subpath, commander);
  }

  /**
   * Natural environment: server (needs database and permission checks)
   */
  protected static get naturalEnvironment(): 'browser' | 'server' | 'auto' {
    return 'server';
  }

  /**
   * Entry point - subclasses implement environment-specific logic
   */
  async execute(params: AIMuteParams): Promise<AIMuteResult> {
    return this.executeMute(params);
  }

  /**
   * Abstract method for mute operation
   * Server implementation handles the permission checks and database writes
   */
  protected abstract executeMute(params: AIMuteParams): Promise<AIMuteResult>;
}
