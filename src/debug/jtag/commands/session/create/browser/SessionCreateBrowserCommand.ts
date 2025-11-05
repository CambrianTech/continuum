/**
 * Session Create Browser Command
 * 
 * Browser implementation that routes session creation to local session daemon.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
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
    
    // Use the same pattern as screenshot: delegate to server via remoteExecute
    const result = await this.remoteExecute(params);
    return result as CreateSessionResult | SessionErrorResponse;
  }

  /**
   * Get environment label for logging
   */
  protected getEnvironmentLabel(): string {
    return 'BROWSER';
  }
}