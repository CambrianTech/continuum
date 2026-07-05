/**
 * Session Daemon - Browser Implementation
 * 
 * Browser-specific session daemon that handles session identity management.
 * TEMPORARY: Direct sessionStorage access until ArtifactoryDaemon is created.
 */

import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { SessionDaemon } from '../shared/SessionDaemon';
import type { SessionResponse } from '../shared/SessionTypes';

export class SessionDaemonBrowser extends SessionDaemon {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Browser-specific initialization
   */
  protected async initialize(): Promise<void> {
    await super.initialize();
    console.log(`🏷️ ${this.toString()}: Browser session daemon ready`);
  }

  /**
   * Override processMessage for browser to properly await server response
   */
  protected async processMessage(message: JTAGMessage): Promise<SessionResponse> {
    console.log(`📨 ${this.toString()}: Forwarding session message to server`);
    
    // Forward to server and await full response with session data
    const serverOperation = message.endpoint.split('/').pop() || 'create';
    console.log(`⚡ ${this.toString()}: Executing remote operation: $server/${serverOperation}`);
    
    try {
      const result = await this.executeRemote(message, 'server') as SessionResponse;
      console.log(`🔍 ${this.toString()}: Server response received:`, JSON.stringify(result, null, 2));
      return result;
    } catch (error) {
      console.error(`❌ ${this.toString()}: Remote execution failed:`, error);
      throw error;
    }
  }
}