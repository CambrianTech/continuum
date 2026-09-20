/**
 * Health Daemon - Server Implementation
 *
 * Server-specific health daemon that handles health checks and system monitoring.
 */

import { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { HealthDaemon } from '../shared/HealthDaemon';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';

export class HealthDaemonServer extends HealthDaemon {

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize standardized logging (daemons/ subdirectory)
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  /**
   * Server-specific initialization
   */
  protected async initialize(): Promise<void> {
    await super.initialize();
    this.log.info(`💓 ${this.toString()}: Server health daemon ready`);
  }
}