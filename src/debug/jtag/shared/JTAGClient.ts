// ISSUES: 0 open, last updated 2025-07-29 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * JTAGClient - Client for interacting with the JTAG system
 * 
  * Provides methods to connect to the JTAG system, send commands, and receive responses.
 * 
 * ISSUES: (look for TODOs)
 * - Implemment the initalize method to set up the transport and commands interface.

 * CORE ARCHITECTURE:

 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Message routing logic and subscriber management
 * - Integration tests: Cross-context transport reliability
 * - Performance tests: Message throughput under load
 * - Failure tests: Network partition and recovery scenarios
 * 
 * ARCHITECTURAL INSIGHTS:
  * - Provides a unified client for calling the JTAGSystem via a transport (such as WebSocket)
 */


import { generateUUID, type UUID} from './CrossPlatformUUID';
import { JTAGBase, type CommandsInterface } from './JTAGBase';
import type { JTAGContext } from './JTAGTypes';
import { TransportFactory } from '@system/transports/shared/TransportFactory';
import type { TransportConfig, JTAGTransport } from '@system/transports/shared/TransportTypes';

export class JTAGClient extends JTAGBase {
  protected systemTransport?: JTAGTransport;
  protected commandsInterface?: CommandsInterface;

  public readonly sessionId: UUID;

  // Initialize sessionId in constructor
  constructor(context: JTAGContext) {
    super('jtag-client', context);
    this.sessionId = context.uuid;
  }


  protected override async initialize(): Promise<void> {
    const transportConfig: TransportConfig = { 
      preferred: 'websocket', 
      fallback: true,
      eventSystem: this.eventManager.events,
      sessionId: this.sessionId // Pass sessionId for client handshake
    };

    this.systemTransport = await TransportFactory.createTransport(this.context.environment, transportConfig);

    // Create remote commands interface that routes through transport
    await this.setupRemoteCommandsInterface();
  }

  /**
   * Set up commands interface that routes commands through transport to remote JTAGSystem
   */
  private async setupRemoteCommandsInterface(): Promise<void> {
    if (!this.systemTransport) {
      throw new Error('Transport not available for remote commands setup');
    }

    // Create a commands interface that proxies commands through the transport
    this.commandsInterface = new Map();
    
    // For now, create placeholder commands that will be populated when we discover
    // available commands from the remote system
    // TODO: Implement command discovery from remote JTAGSystem
    console.log(`📡 JTAGClient: Remote commands interface setup completed`);
  }
  
  protected getCommandsInterface(): CommandsInterface {
    return this.commandsInterface ?? new Map();
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
    await client.initialize(); // Actually initialize the client
    
    return client;
  }
}