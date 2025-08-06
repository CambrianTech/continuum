/**
 * Session Create Server Command
 * 
 * Server implementation that routes session creation to server session daemon.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { SessionCreateCommand } from '../shared/SessionCreateCommand';
import { type CreateSessionParams, type CreateSessionResult, type SessionErrorResponse } from '../../../../daemons/session-daemon/shared/SessionTypes';

export class SessionCreateServerCommand extends SessionCreateCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Server session creation - directly call session daemon (like screenshot pattern)
   */
  protected async routeToSessionDaemon(params: CreateSessionParams): Promise<CreateSessionResult | SessionErrorResponse> {
    console.log(`🏷️ SERVER: Creating session directly via session daemon`);

    // Route via commander's router to local session daemon
    const router = (this.commander as any).router;
    if (!router) {
      throw new Error('Router not available for session daemon communication');
    }

    // Create message for server session daemon
    const sessionMessage = JTAGMessageFactory.createRequest(
      this.context,
      `server/session-daemon`,  // Route to server session daemon
      'session-daemon/create',
      params,
      JTAGMessageFactory.generateCorrelationId()
    );

    console.log(`🔍 SERVER: Routing to server session daemon directly`);
    const response = await router.postMessage(sessionMessage);
    console.log(`🔍 SERVER: Session daemon response:`, JSON.stringify(response, null, 2));
    
    return response as CreateSessionResult | SessionErrorResponse;
  }

  /**
   * Get environment label for logging
   */
  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }
}