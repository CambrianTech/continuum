/**
 * Session Create Command - Shared Base
 * 
 * Base implementation for session create command that routes to session daemon internally.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';
import { type SessionCreateParams, type SessionCreateResult, createSessionCreateResult } from './SessionCreateTypes';
import { type CreateSessionParams, type CreateSessionResult, type SessionErrorResponse } from '../../../../daemons/session-daemon/shared/SessionTypes';

export abstract class SessionCreateCommand extends CommandBase<SessionCreateParams, SessionCreateResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('session/create', context, subpath, commander);
  }

  /**
   * Execute session create command by routing to session daemon
   */
  async execute(params: JTAGPayload): Promise<SessionCreateResult> {
    const createParams = params as SessionCreateParams;
    
    console.log(`🏷️ ${this.getEnvironmentLabel()}: Creating session - "${createParams.displayName}"`);
    console.log(`🔍 ${this.getEnvironmentLabel()}: SessionCreateCommand.execute called with:`, JSON.stringify(createParams, null, 2));

    try {
      // Convert command params to session daemon params
      const sessionParams: CreateSessionParams = {
        context: createParams.context,
        sessionId: createParams.sessionId,
        operation: 'create',
        category: createParams.category,
        displayName: createParams.displayName,
        userId: createParams.userId ?? generateUUID(),
        isShared: createParams.isShared ?? true
      };

      // Route to session daemon via router
      const result = await this.routeToSessionDaemon(sessionParams);
      
      if ('error' in result) {
        const errorResponse = result as SessionErrorResponse;
        console.error(`❌ ${this.getEnvironmentLabel()}: Session creation failed:`, errorResponse.error);
        
        return createSessionCreateResult(createParams, {
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
      
      // Extract session metadata from session daemon response
      let sessionMetadata = null;
      if (sessionResult.session) {
        // Session daemon returned proper CreateSessionResult format
        console.log(`🔍 ${this.getEnvironmentLabel()}: Found session field with data:`, sessionResult.session);
        sessionMetadata = sessionResult.session;
      } else if (sessionResult.commandResult?.session) {
        // Response is nested in commandResult (from remoteExecute pattern)
        console.log(`🔍 ${this.getEnvironmentLabel()}: Found session in commandResult:`, sessionResult.commandResult.session);
        sessionMetadata = sessionResult.commandResult.session;
      } else if (sessionResult.metadata) {
        // Fallback: Response has metadata field (old format)
        console.log(`🔍 ${this.getEnvironmentLabel()}: Using fallback metadata field conversion`);
        sessionMetadata = {
          sessionId: sessionResult.metadata.id || sessionResult.responseSessionId,
          category: sessionResult.metadata.category,
          userId: sessionResult.metadata.userId,
          displayName: sessionResult.metadata.displayName,
          created: new Date(sessionResult.metadata.created),
          isActive: sessionResult.metadata.isActive,
          lastActive: new Date(sessionResult.metadata.lastActive),
          sourceContext: createParams.context,
          isShared: createParams.isShared
        };
      } else {
        console.log(`🔍 ${this.getEnvironmentLabel()}: No session or metadata field found in response`);
        console.log(`🔍 ${this.getEnvironmentLabel()}: Available response fields:`, Object.keys(sessionResult));
      }
      
      console.log(`✅ ${this.getEnvironmentLabel()}: Session created: ${sessionMetadata?.sessionId}`);

      const finalResult = createSessionCreateResult(createParams, {
        success: sessionResult.success ?? true,
        session: sessionMetadata
      });
      
      console.log(`🔍 ${this.getEnvironmentLabel()}: Final result being returned:`, JSON.stringify(finalResult, null, 2));
      
      return finalResult;

    } catch (error) {
      console.error(`❌ ${this.getEnvironmentLabel()}: Session creation error:`, error);
      
      return createSessionCreateResult(createParams, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown session creation error'
      });
    }
  }

  /**
   * Route message to session daemon - environment-specific implementation
   */
  protected abstract routeToSessionDaemon(params: CreateSessionParams): Promise<CreateSessionResult | SessionErrorResponse>;

  /**
   * Get environment label for logging
   */
  protected abstract getEnvironmentLabel(): string;
}