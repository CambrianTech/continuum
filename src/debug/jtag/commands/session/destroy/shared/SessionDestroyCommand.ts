/**
 * Session Destroy Command - Shared Base
 * 
 * Base implementation for session destroy command that routes to session daemon internally.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { type SessionDestroyParams, type SessionDestroyResult, createSessionDestroyResult } from './SessionDestroyTypes';
import { type DestroySessionParams, type DestroySessionResult, type SessionErrorResponse } from '../../../../daemons/session-daemon/shared/SessionTypes';

export abstract class SessionDestroyCommand extends CommandBase<SessionDestroyParams, SessionDestroyResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('session/destroy', context, subpath, commander);
  }

  /**
   * Execute session destroy command by routing to session daemon
   */
  async execute(params: JTAGPayload): Promise<SessionDestroyResult> {
    const destroyParams = params as SessionDestroyParams;
    
    console.log(`🧹 ${this.getEnvironmentLabel()}: Destroying session - "${destroyParams.sessionId}"`);
    console.log(`🔍 ${this.getEnvironmentLabel()}: SessionDestroyCommand.execute called with:`, JSON.stringify(destroyParams, null, 2));

    try {
      // Convert command params to session daemon params
      const sessionParams: DestroySessionParams = {
        context: destroyParams.context,
        sessionId: destroyParams.sessionId,
        operation: 'destroy',
        reason: destroyParams.reason ?? 'command_request'
      };

      // Route to session daemon via router
      const result = await this.routeToSessionDaemon(sessionParams);
      
      if ('error' in result) {
        const errorResponse = result as SessionErrorResponse;
        console.error(`❌ ${this.getEnvironmentLabel()}: Session destruction failed:`, errorResponse.error);
        
        return createSessionDestroyResult(destroyParams, {
          success: false,
          error: errorResponse.error
        });
      }
      
      // Debug: Log the raw response from session daemon
      console.log(`🔍 ${this.getEnvironmentLabel()}: Raw session daemon response:`, JSON.stringify(result, null, 2));
      
      // Handle router-wrapped response format
      let sessionResult = result as any;
      if (sessionResult.resolved && sessionResult.response) {
        // Router wrapped the response - extract the actual daemon response
        console.log(`🔍 ${this.getEnvironmentLabel()}: Router wrapped response detected, extracting response`);
        sessionResult = sessionResult.response;
      }
      
      console.log(`🔍 ${this.getEnvironmentLabel()}: Processed session result:`, JSON.stringify(sessionResult, null, 2));
      
      console.log(`✅ ${this.getEnvironmentLabel()}: Session destroyed: ${sessionResult.destroyedSessionId}`);

      const finalResult = createSessionDestroyResult(destroyParams, {
        success: sessionResult.success ?? true,
        destroyedSessionId: sessionResult.destroyedSessionId,
        timestamp: sessionResult.timestamp
      });
      
      console.log(`🔍 ${this.getEnvironmentLabel()}: Final result being returned:`, JSON.stringify(finalResult, null, 2));
      
      return finalResult;

    } catch (error) {
      console.error(`❌ ${this.getEnvironmentLabel()}: Session destruction error:`, error);
      
      return createSessionDestroyResult(destroyParams, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown session destruction error'
      });
    }
  }

  /**
   * Route message to session daemon - environment-specific implementation
   */
  protected abstract routeToSessionDaemon(params: DestroySessionParams): Promise<DestroySessionResult | SessionErrorResponse>;

  /**
   * Get environment label for logging
   */
  protected abstract getEnvironmentLabel(): string;
}