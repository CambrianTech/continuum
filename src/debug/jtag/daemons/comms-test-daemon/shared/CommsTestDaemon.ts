/**
 * Comms Test Daemon - THROWAWAY Testing Only
 *
 * Now testing Rust adapter integration with concurrent databases!
 *
 * Step 1: ✅ Get daemon running in the system
 * Step 2: 🔄 Add Rust adapter integration with database testing
 * Step 3: Delete after testing complete
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { type BaseResponsePayload } from '../../../system/core/types/ResponseTypes';

export interface DatabaseTestParams {
  dbCount: number;
  testDir: string;
  operations: number;
}

export interface DatabaseTestResult {
  success: boolean;
  databases: {
    handle: string;
    path: string;
    operations: number;
    duration: number;
    success: boolean;
    error?: string;
  }[];
  totalDuration: number;
  totalOperations: number;
}

/**
 * Comms test daemon - now with database testing!
 */
export abstract class CommsTestDaemon extends DaemonBase {
  public readonly subpath: string = 'comms-test';

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('comms-test-daemon', context, router);
  }

  /**
   * Initialize daemon
   */
  protected async initialize(): Promise<void> {
    console.log(`🧪 ${this.toString()}: Comms test daemon initialized`);
  }

  /**
   * Handle incoming messages
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    // For now, just return success - we're using direct method calls
    return {
      context: message.payload.context,
      sessionId: message.payload.sessionId,
      success: true,
      timestamp: new Date().toISOString()
    };
  }

  /**
   * Test method: Simple echo
   */
  async testEcho(message: string): Promise<{ success: boolean; echo: string }> {
    return {
      success: true,
      echo: `ECHO: ${message}`
    };
  }

  /**
   * Test method: Database operations
   * Abstract - server implementation will use RustAdapter
   */
  abstract testDatabase(params: DatabaseTestParams): Promise<DatabaseTestResult>;

  /**
   * Cleanup
   */
  override async shutdown(): Promise<void> {
    console.log(`🧪 ${this.toString()}: Shutting down comms test daemon...`);
    await super.shutdown();
  }
}
