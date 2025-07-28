/**
 * Session Daemon - Server Implementation
 * 
 * Server-specific session daemon that handles session identity management.
 * Follows the sparse override pattern - minimal server-specific logic.
 */

import { JTAGContext } from '@shared/JTAGTypes';
import { JTAGRouter } from '@shared/JTAGRouter';
import { SessionDaemon } from '@daemonsSessionDaemon/shared/SessionDaemon';
import { generateUUID, type UUID } from '@shared/CrossPlatformUUID';

export class SessionDaemonServer extends SessionDaemon {
  private activeSession: UUID | null = null;
  
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

  /**
   * Server checks memory for existing session
   */
  protected override async checkExistingSession(): Promise<UUID | null> {
    console.log(`🔍 ${this.toString()}: Checking for existing session in server memory`);
    return this.activeSession;
  }

  /**
   * Server stores session in memory
   */
  protected override async storeSession(sessionId: UUID): Promise<void> {
    console.log(`💾 ${this.toString()}: Storing session in server memory: ${sessionId}`);
    this.activeSession = sessionId;
  }

  /**
   * Server clears session from memory
   */
  protected override async clearSession(): Promise<void> {
    console.log(`🗑️ ${this.toString()}: Clearing session from server memory`);
    this.activeSession = null;
  }
}