/**
 * Health Daemon - Server Implementation
 * 
 * Server-specific health daemon that handles health checks and system monitoring.
 */

import { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { HealthDaemon } from '../shared/HealthDaemon';

export class HealthDaemonServer extends HealthDaemon {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Server-specific initialization
   */
  protected async initialize(): Promise<void> {
    await super.initialize();
    console.log(`💓 ${this.toString()}: Server health daemon ready`);
  }
}