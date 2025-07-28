/**
 * Session Daemon - Browser Implementation
 * 
 * Browser-specific session daemon that handles session identity management.
 * TEMPORARY: Direct sessionStorage access until ArtifactoryDaemon is created.
 */

import { JTAGContext, JTAGMessageFactory } from '@shared/JTAGTypes';
import { JTAGRouter } from '@shared/JTAGRouter';
import { SessionDaemon } from '@daemonsSessionDaemon/shared/SessionDaemon';
import { type UUID } from '@shared/CrossPlatformUUID';

export class SessionDaemonBrowser extends SessionDaemon {
  private static readonly SESSION_STORAGE_KEY = 'jtag_session_id';
  
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
   * Override: Check existing session in sessionStorage (browser-specific)
   */
  protected override async checkExistingSession(): Promise<UUID | null> {
    if (typeof sessionStorage === 'undefined') {
      return null;
    }
    
    const sessionId = sessionStorage.getItem(SessionDaemonBrowser.SESSION_STORAGE_KEY);
    return sessionId as UUID | null;
  }

  /**
   * Override: Store session in sessionStorage (browser-specific)
   */
  protected override async storeSession(sessionId: UUID): Promise<void> {
    if (typeof sessionStorage !== 'undefined') {
      sessionStorage.setItem(SessionDaemonBrowser.SESSION_STORAGE_KEY, sessionId);
      console.log(`💾 ${this.toString()}: Stored session in sessionStorage: ${sessionId}`);
    }
  }

  /**
   * Override: Clear session from sessionStorage (browser-specific)
   */
  protected override async clearSession(): Promise<void> {
    if (typeof sessionStorage !== 'undefined') {
      sessionStorage.removeItem(SessionDaemonBrowser.SESSION_STORAGE_KEY);
      console.log(`🗑️ ${this.toString()}: Cleared session from sessionStorage`);
    }
  }
}