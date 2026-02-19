/**
 * Session Destroy Browser Command
 * 
 * Browser implementation that routes session destruction to local session daemon.
 */

import type { JTAGContext, CommandParams } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { SYSTEM_SCOPES } from '../../../../system/core/types/SystemScopes';
import { SessionDestroyCommand } from '../shared/SessionDestroyCommand';
import { type DestroySessionParams, type DestroySessionResult, type SessionErrorResponse } from '../../../../daemons/session-daemon/shared/SessionTypes';

export class SessionDestroyBrowserCommand extends SessionDestroyCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Browser delegates to server (like screenshot pattern)
   */
  protected async routeToSessionDaemon(params: DestroySessionParams): Promise<DestroySessionResult | SessionErrorResponse> {
    console.log(`🧹 BROWSER: Session destruction needs server → delegating to server`);
    
    // Use the same pattern as screenshot: delegate to server via remoteExecute
    // DestroySessionParams (daemon-level) doesn't extend CommandParams — bridge with userId
    const commandParams = { ...params, userId: SYSTEM_SCOPES.SYSTEM } as CommandParams;
    const result = await this.remoteExecute(commandParams);
    return result as DestroySessionResult | SessionErrorResponse;
  }

  /**
   * Get environment label for logging
   */
  protected getEnvironmentLabel(): string {
    return 'BROWSER';
  }
}