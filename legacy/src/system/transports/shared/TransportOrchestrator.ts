/**
 * Transport Orchestrator - Bridges pure transports with JTAG business logic
 * 
 * This class handles the JTAG-specific concerns (message serialization, correlation,
 * event integration) while using pure transports as dumb pipes underneath.
 * 
 * ARCHITECTURE:
 * - Pure transports handle only networking (dumb pipes)
 * - Orchestrator handles JTAG messaging, events, sessions
 * - Clean separation of concerns
 */

import type { JTAGMessage } from '../../core/types/JTAGTypes';
import type { EventsInterface } from '../../events';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { TransportSendResult, JTAGTransport } from './TransportTypes';
import type { PureTransport, PureTransportConfig } from './PureTransportTypes';
import { TRANSPORT_EVENTS } from './TransportEvents';

/**
 * Orchestrator configuration - contains JTAG business logic
 */
export interface TransportOrchestratorConfig {
  // JTAG business logic
  eventSystem: EventsInterface;
  sessionId: UUID;
  messageHandler?: (message: JTAGMessage) => void;
  
  // Pure transport configuration (passed through)
  transportConfig: PureTransportConfig;
}

/**
 * Transport Orchestrator - Bridges pure transports with JTAG system
 */
export class TransportOrchestrator implements JTAGTransport {
  public readonly name: string;
  
  private pureTransport: PureTransport;
  private config: TransportOrchestratorConfig;
  
  constructor(pureTransport: PureTransport, config: TransportOrchestratorConfig) {
    this.pureTransport = pureTransport;
    this.config = config;
    this.name = `jtag-orchestrated-${pureTransport.name}`;
    
    // Set up pure transport callbacks to handle JTAG business logic
    this.setupTransportCallbacks();
  }
  
  /**
   * JTAG message send - handles serialization and correlation
   */
  async send(message: JTAGMessage): Promise<TransportSendResult> {
    try {
      // Serialize JTAG message to transport data
      const serializedData = JSON.stringify(message);
      
      // Send via pure transport (dumb pipe)
      const pureResult = await this.pureTransport.send(serializedData);
      
      // Emit JTAG events
      this.config.eventSystem.emit(TRANSPORT_EVENTS.MESSAGE_SENT, {
        transportName: this.name,
        messageId: message.messageId,
        timestamp: pureResult.timestamp
      });
      
      // Convert pure result to JTAG result
      return {
        success: pureResult.success,
        timestamp: pureResult.timestamp,
        sentCount: 1
      };
      
    } catch (error) {
      this.config.eventSystem.emit(TRANSPORT_EVENTS.SEND_ERROR, {
        transportName: this.name,
        error: error instanceof Error ? error.message : String(error)
      });
      
      throw error;
    }
  }
  
  /**
   * Connection status - delegates to pure transport
   */
  isConnected(): boolean {
    return this.pureTransport.isConnected();
  }
  
  /**
   * Disconnect - delegates to pure transport
   */
  async disconnect(): Promise<void> {
    await this.pureTransport.disconnect();
    
    this.config.eventSystem.emit(TRANSPORT_EVENTS.DISCONNECTED, {
      transportName: this.name,
      timestamp: new Date().toISOString()
    });
  }
  
  /**
   * Reconnect - if supported by pure transport
   */
  async reconnect(): Promise<void> {
    if (this.pureTransport.connect) {
      await this.pureTransport.connect(this.config.transportConfig);
    } else {
      throw new Error(`Transport ${this.pureTransport.name} does not support reconnection`);
    }
  }
  
  /**
   * Set message handler - JTAG business logic
   */
  setMessageHandler(handler: (message: JTAGMessage) => void): void {
    this.config.messageHandler = handler;
  }
  
  /**
   * Setup pure transport callbacks to handle JTAG business logic
   */
  private setupTransportCallbacks(): void {
    // Handle raw data from pure transport
    if (this.pureTransport.onData) {
      this.pureTransport.onData((data: string | Uint8Array) => {
        this.handleIncomingData(data);
      });
    }
    
    // Handle connection events
    if (this.pureTransport.onConnect) {
      this.pureTransport.onConnect(() => {
        this.config.eventSystem.emit(TRANSPORT_EVENTS.CONNECTED, {
          transportName: this.name,
          timestamp: new Date().toISOString()
        });
      });
    }
    
    if (this.pureTransport.onDisconnect) {
      this.pureTransport.onDisconnect((reason?: string) => {
        this.config.eventSystem.emit(TRANSPORT_EVENTS.DISCONNECTED, {
          transportName: this.name,
          reason,
          timestamp: new Date().toISOString()
        });
      });
    }
    
    if (this.pureTransport.onError) {
      this.pureTransport.onError((error: Error) => {
        this.config.eventSystem.emit(TRANSPORT_EVENTS.ERROR, {
          transportName: this.name,
          error: error.message,
          timestamp: new Date().toISOString()
        });
      });
    }
  }
  
  /**
   * Handle incoming raw data - deserialize and route to JTAG handlers
   */
  private handleIncomingData(data: string | Uint8Array): void {
    try {
      // Convert binary data to string if needed
      const stringData = typeof data === 'string' ? data : new TextDecoder().decode(data);
      
      // Deserialize JTAG message
      const message: JTAGMessage = JSON.parse(stringData);
      
      // Emit JTAG event
      this.config.eventSystem.emit(TRANSPORT_EVENTS.MESSAGE_RECEIVED, {
        transportName: this.name,
        messageId: message.messageId,
        timestamp: new Date().toISOString()
      });
      
      // Route to JTAG message handler
      if (this.config.messageHandler) {
        this.config.messageHandler(message);
      }
      
    } catch (error) {
      this.config.eventSystem.emit(TRANSPORT_EVENTS.PARSE_ERROR, {
        transportName: this.name,
        error: error instanceof Error ? error.message : String(error),
        rawData: data.toString(),
        timestamp: new Date().toISOString()
      });
    }
  }
}