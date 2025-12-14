/**
 * Governance Daemon Server - Server Implementation
 *
 * Server-specific governance daemon that handles automated governance workflows.
 */

import { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { GovernanceDaemon } from '../shared/GovernanceDaemon';
import { Logger } from '../../../system/core/logging/Logger';

export class GovernanceDaemonServer extends GovernanceDaemon {

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
    this.log.info(`⚖️ ${this.toString()}: Server governance daemon ready`);
  }
}
