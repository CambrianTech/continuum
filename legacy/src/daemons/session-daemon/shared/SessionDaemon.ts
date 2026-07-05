/**
 * Session Daemon - Identity Service Only
 * 
 * Microarchitecture approach: Create session UUID + track basic lifecycle
 * Single responsibility: Session identity management and basic metadata
 * Size: ~100-150 lines maximum per microarchitecture principles
 * 
 * What it does NOT do:
 * - Directory creation → DirectoryDaemon
 * - File/artifact management → ArtifactDaemon  
 * - WebSocket handling → Router
 * - Browser processes → BrowserDaemon
 * - Message routing → RouterDaemon
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { SessionResponse } from './SessionTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * Session Daemon - Identity Service Only
 * 
 * Core responsibility: Create session UUID + track basic lifecycle
 * Does NOT handle directories, files, WebSockets, browsers, routing, etc.
 * 
 */
export abstract class SessionDaemon extends DaemonBase {
  public readonly subpath: string = 'session-daemon';

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('session-daemon', context, router);
  }

  /**
   * Initialize session daemon
   */
  protected async initialize(): Promise<void> {
    console.log(`🏷️ ${this.toString()}: Session daemon initialized - identity service ready`);
  }


  /**
   * Process incoming session messages - base implementation forwards to server
   * SessionDaemonServer overrides this to provide actual session management
   */
  protected async processMessage(message: JTAGMessage): Promise<SessionResponse> {
    console.log(`📨 ${this.toString()}: Forwarding session message to server`);
    
    // Base implementation forwards to server via router.postMessage
    if (this.context.environment !== 'server') {
       return await this.executeRemote(message, 'server') as SessionResponse;
    }
    
    // Server environment should override this method
    throw new Error(`SessionDaemonServer must override handleMessage for actual session management`);
  }
}
