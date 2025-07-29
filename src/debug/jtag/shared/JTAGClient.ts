/**
 * JTAG Client - Remote connection to JTAG System
 * 
 * Minimal client implementation that extends JTAGBase.
 * Will connect to remote JTAG System via transport in next step.
 */

import { JTAGBase } from './JTAGBase';
import type { JTAGContext } from './JTAGTypes';

export class JTAGClient extends JTAGBase {
  private clientSessionId: string;

  constructor(context: JTAGContext) {
    super('jtag-client', context);
    this.clientSessionId = context.uuid;
  }

  /**
   * Get current session ID
   */
  getSessionId(): string {
    return this.clientSessionId;
  }

  /**
   * Implementation of abstract method from JTAGBase
   * For now, returns empty interface - will add transport in next step
   */
  protected getCommandsInterface(): Record<string, Function> {
    // Minimal implementation - will add transport forwarding in step 5
    return {};
  }

  /**
   * Static factory method for easy client creation
   */
  static async connect(): Promise<JTAGClient> {
    const context: JTAGContext = {
      uuid: `client-${Date.now()}`,
      environment: 'server' // CLI runs in server environment but acts as client
    };
    
    const client = new JTAGClient(context);
    return client;
  }
}