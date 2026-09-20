/**
 * Session Destroy Server Command
 * 
 * Server implementation that routes session destruction to server session daemon.
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { SessionDestroyCommand } from '../shared/SessionDestroyCommand';
import { type DestroySessionParams, type DestroySessionResult, type SessionErrorResponse } from '../../../../daemons/session-daemon/shared/SessionTypes';

export class SessionDestroyServerCommand extends SessionDestroyCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Server session destruction - directly call session daemon (like screenshot pattern)
   */
  protected async routeToSessionDaemon(params: DestroySessionParams): Promise<DestroySessionResult | SessionErrorResponse> {
    // console.debug(`🧹 SERVER: Destroying session directly via session daemon`);

    // Find the SessionDaemon directly
    const sessionDaemon = this.commander.router.getSubscriber('session-daemon');
    
    if (!sessionDaemon) {
      throw new Error('SessionDaemon not available');
    }

    // Create message for session daemon
    const sessionMessage = JTAGMessageFactory.createRequest(
      this.context,
      'server/session-daemon',  
      'session-daemon/destroy',
      params,
      JTAGMessageFactory.generateCorrelationId()
    );

    // console.debug(`🔍 SERVER: Calling session daemon directly`);
    const response = await sessionDaemon.handleMessage(sessionMessage);
    // console.debug(`🔍 SERVER: Session daemon response:`, JSON.stringify(response, null, 2));
    
    return response as DestroySessionResult | SessionErrorResponse;
  }

  /**
   * Get environment label for logging
   */
  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }
}