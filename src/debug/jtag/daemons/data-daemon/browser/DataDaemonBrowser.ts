/**
 * Data Daemon Browser - Client Forwarding
 * 
 * Browser-side implementation that forwards all data operations to server
 * No local storage - all data operations go through server
 */

import { DataDaemon } from '../shared/DataDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

/**
 * Data Daemon Browser - Pure Forwarding Client
 * 
 * All operations are forwarded to server - no browser-side data storage
 */
export class DataDaemonBrowser extends DataDaemon {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Initialize browser data daemon
   */
  protected async initialize(): Promise<void> {
    console.log(`🗄️ ${this.toString()}: Browser data daemon initialized - forwarding to server`);
  }
}