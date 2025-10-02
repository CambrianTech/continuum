/**
 * User Daemon - Browser Implementation
 *
 * Browser-side stub. User lifecycle management happens server-side.
 * Browser just needs to provide the daemon interface for consistency.
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { UserDaemon } from '../shared/UserDaemon';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

export class UserDaemonBrowser extends UserDaemon {
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Browser doesn't manage persona clients
   */
  protected async ensurePersonaClients(): Promise<void> {
    // No-op: server manages persona clients
  }

  /**
   * Browser doesn't manage user state
   */
  protected async ensureUserHasState(userId: UUID): Promise<boolean> {
    // No-op: server manages user state
    return true;
  }

  /**
   * No monitoring loops in browser
   */
  protected startMonitoringLoops(): void {
    // No-op: server handles monitoring
  }

  /**
   * No monitoring loops to stop
   */
  protected stopMonitoringLoops(): void {
    // No-op
  }
}