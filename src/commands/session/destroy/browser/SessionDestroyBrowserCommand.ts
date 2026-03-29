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
    
    // Delegate to server — pass userId through if present, empty string otherwise
    // Server resolves real identity from session. Never default to SYSTEM_SCOPES.SYSTEM (all-zeros).
    const commandParams = { ...params, userId: (params as any).userId || '' } as CommandParams;
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