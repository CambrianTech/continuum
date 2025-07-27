/**
 * Session Daemon - Server Implementation
 * 
 * Server-specific session daemon that handles session identity management.
 * Follows the sparse override pattern - minimal server-specific logic.
 */

import { JTAGContext } from '@shared/JTAGTypes';
import { JTAGRouter } from '@shared/JTAGRouter';
import { SessionDaemon } from '@sessionShared/SessionDaemon';

export class SessionDaemonServer extends SessionDaemon {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Server-specific initialization
   */
  protected async initialize(): Promise<void> {
    await super.initialize();
    console.log(`🏷️ ${this.toString()}: Server session daemon ready`);
  }
}