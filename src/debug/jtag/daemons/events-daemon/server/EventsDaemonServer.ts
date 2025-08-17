/**
 * Events Daemon - Server Implementation
 * 
 * Handles cross-context event bridging in server environment
 */

import { EventsDaemon } from '../shared/EventsDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { EventManager } from '../../../system/events/shared/JTAGEventSystem';

export class EventsDaemonServer extends EventsDaemon {
  protected eventManager = new EventManager();

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }
}