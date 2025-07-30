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
import { TransportFactory } from '@systemTransports';
import type { TransportConfig, JTAGTransport } from '@systemTransports';

/**
 * JTAGClient connection options
 */
export interface JTAGClientConnectOptions {
  readonly targetEnvironment?: 'server' | 'browser';
  readonly transportType?: 'websocket' | 'http';
  readonly serverPort?: number;
  readonly serverUrl?: string;
  readonly timeout?: number;
  readonly enableFallback?: boolean;
  readonly maxRetries?: number;
  readonly retryDelay?: number;
}

export class JTAGClient extends JTAGBase {
  protected systemTransport?: JTAGTransport;
  protected commandsInterface?: CommandsInterface;

  public readonly sessionId: UUID;

  // Initialize sessionId in constructor
  constructor(context: JTAGContext) {
    super('jtag-client', context);
    this.sessionId = context.uuid;
  }


  protected override async initialize(options?: JTAGClientConnectOptions): Promise<void> {
    const transportConfig: TransportConfig = { 
      preferred: options?.transportType ?? 'websocket',
      role: 'client', // JTAGClient always creates client transports (connectors)
      fallback: options?.enableFallback ?? true,
      serverPort: options?.serverPort ?? 9001, // Default WebSocket port
      serverUrl: options?.serverUrl ?? `ws://localhost:${options?.serverPort ?? 9001}`,
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
   * Static factory method for easy client creation with robust retry logic for slow server startup
   */
  static async connect(options?: JTAGClientConnectOptions): Promise<JTAGClient> {
    const maxRetries = options?.maxRetries ?? 120; // 120 retries = ~60 seconds with 500ms delay
    const retryDelay = options?.retryDelay ?? 500; // 500ms between retries
    
    const context: JTAGContext = {
      uuid: generateUUID(),
      environment: options?.targetEnvironment ?? 'server' // CLI runs in server environment by default
    };
    
    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      try {
        const client = new JTAGClient(context);
        await client.initialize(options);
        
        console.log(`✅ JTAGClient: Connected using ${options?.transportType ?? 'websocket'} transport on port ${options?.serverPort ?? 9001}`);
        return client;
        
      } catch (error) {
        if (attempt === maxRetries) {
          console.error(`❌ JTAGClient: Failed to connect after ${maxRetries} attempts (${maxRetries * retryDelay / 1000}s)`);
          throw error;
        }
        
        if (attempt === 1 || attempt % 10 === 0) { // Show progress every 10th attempt
          console.log(`⏳ JTAGClient: Connection attempt ${attempt}/${maxRetries}, server may still be starting...`);
        }
        
        await new Promise(resolve => setTimeout(resolve, retryDelay));
      }
    }
    
    throw new Error('Connection failed after all retry attempts');
  }
}