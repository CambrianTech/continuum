/**
 * Session Create Browser Command
 * 
 * Browser implementation that routes session creation to local session daemon.
 */

import type { JTAGContext, CommandParams } from '../../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { SessionCreateCommand } from '../shared/SessionCreateCommand';
import { type CreateSessionParams, type CreateSessionResult, type SessionErrorResponse } from '../../../../daemons/session-daemon/shared/SessionTypes';

export class SessionCreateBrowserCommand extends SessionCreateCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Browser delegates to server (like screenshot pattern)
   */
  protected async routeToSessionDaemon(params: CreateSessionParams): Promise<CreateSessionResult | SessionErrorResponse> {
    console.log(`🏷️ BROWSER: Session creation needs server → delegating to server`);
    
    // Delegate to server via remoteExecute
    // CRITICAL: Do NOT default userId to SYSTEM_SCOPES.SYSTEM — that's all-zeros.
    // Server resolves real identity from connectionContext.identity.deviceId.
    // If userId is undefined, keep it undefined — server handles resolution.
    const commandParams = { ...params } as CommandParams;
    const result = await this.remoteExecute(commandParams);
    return result as CreateSessionResult | SessionErrorResponse;
  }

  /**
   * Get environment label for logging
   */
  protected getEnvironmentLabel(): string {
    return 'BROWSER';
  }
}