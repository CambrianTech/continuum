/**
 * Session Destroy Client Command
 * 
 * Browser implementation that routes session destruction to server via remoteExecute.
 */

import type { JTAGContext, CommandParams } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { SYSTEM_SCOPES } from '../../../../system/core/types/SystemScopes';
import { SessionDestroyCommand } from '../shared/SessionDestroyCommand';
import { type DestroySessionParams, type DestroySessionResult, type SessionErrorResponse } from '../../../../daemons/session-daemon/shared/SessionTypes';

export class SessionDestroyClientCommand extends SessionDestroyCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Browser session destruction - delegate to server via remoteExecute pattern
   */
  protected async routeToSessionDaemon(params: DestroySessionParams): Promise<DestroySessionResult | SessionErrorResponse> {
    console.log(`🧹 BROWSER: Routing session destroy to server`);
    
    try {
      // Use remoteExecute to delegate to server command
      // DestroySessionParams (daemon-level) doesn't extend CommandParams — bridge with userId
      const commandParams = { ...params, userId: SYSTEM_SCOPES.SYSTEM } as CommandParams;
      const result = await this.remoteExecute(
        commandParams,
        'destroy' // subpath matches this command
      ) as DestroySessionResult;
      
      return result;
    } catch (error) {
      console.error(`❌ BROWSER: Failed to route session destroy to server:`, error);
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        timestamp: new Date().toISOString(),
        operation: 'destroy',
        error: error instanceof Error ? error.message : 'Remote execution failed'
      } as SessionErrorResponse;
    }
  }

  /**
   * Get environment label for logging
   */
  protected getEnvironmentLabel(): string {
    return 'BROWSER';
  }
}