/**
 * JTAG Client - Remote connection to JTAG System
 * 
 * Minimal client implementation that extends JTAGBase.
 * Will connect to remote JTAG System via transport in next step.
 */

import { generateUUID, type UUID} from './CrossPlatformUUID';
import { JTAGBase, type CommandsInterface } from './JTAGBase';
import type { JTAGContext } from './JTAGTypes';

export class JTAGClient extends JTAGBase {

  constructor(context: JTAGContext) {
    super('jtag-client', context);
  }

  /**
   * Get current session ID
   */
  get sessionId(): UUID {
    return this.context.uuid;
  }

  /**
   * Implementation of abstract method from JTAGBase
   * For now, returns empty interface - will add transport in next step
   */
  protected getCommandsInterface(): CommandsInterface {
    // Minimal implementation - will add transport forwarding in step 5
    return new Map();
  }

  /**
   * Static factory method for easy client creation
   */
  static async connect(): Promise<JTAGClient> {
    const context: JTAGContext = {
      uuid: generateUUID(),
      environment: 'server' // CLI runs in server environment but acts as client
    };
    
    const client = new JTAGClient(context);
    
    return client;
  }
}