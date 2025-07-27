/**
 * Session Daemon - Browser Implementation
 * 
 * Browser-specific session daemon that handles session identity management.
 * Follows the sparse override pattern - minimal browser-specific logic.
 */

import { JTAGContext } from '@shared/JTAGTypes';
import { JTAGRouter } from '@shared/JTAGRouter';
import { SessionDaemon } from '../shared/SessionDaemon';

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
}