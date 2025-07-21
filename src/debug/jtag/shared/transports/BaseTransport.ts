/**
 * Base Transport Class
 * Abstract base for all JTAG transports with built-in testability
 */

import { JTAGTransport, JTAGConfig, JTAGWebSocketMessage, JTAGTransportResponse, JTAG_STATUS, JTAGTransportType, JTAGConnectionState, JTAGStatus } from '../JTAGTypes';

export abstract class BaseJTAGTransport implements JTAGTransport {
  public readonly name: string;
  protected connected = false;
  protected statusHandler?: (status: JTAGStatus, details?: any) => void;
  protected messageHandler?: (message: JTAGWebSocketMessage) => void;
  protected disconnectHandler?: () => void;
  protected connectionId: string;
  protected lastActivity = Date.now();

  // Test support properties
  protected _testMode = false;
  protected _mockResponses: Map<string, any> = new Map();
  protected _statusEvents: Array<{status: JTAGStatus, details?: any, timestamp: number}> = [];

  constructor(name: string) {
    this.name = name;
    this.connectionId = `${name}_${Date.now().toString(36)}_${Math.random().toString(36).substr(2, 6)}`;
  }

  // Abstract methods each transport must implement
  abstract getTransportType(): JTAGTransportType;
  abstract initialize(config: JTAGConfig): Promise<boolean>;
  abstract send<T>(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse<T>>;

  // Common interface methods
  isConnected(): boolean {
    return this.connected;
  }

  async disconnect(): Promise<void> {
    this.connected = false;
    this.emitStatus(JTAG_STATUS.DISCONNECTED, { reason: 'manual_disconnect' });
  }

  onMessage(handler: (message: JTAGWebSocketMessage) => void): void {
    this.messageHandler = handler;
  }

  onDisconnect(handler: () => void): void {
    this.disconnectHandler = handler;
  }

  onStatusChange(handler: (status: JTAGStatus, details?: any) => void): void {
    this.statusHandler = handler;
  }

  getConnectionState(): JTAGConnectionState {
    return {
      connected: this.connected,
      connectionId: this.connectionId,
      endpoint: this.getEndpoint(),
      protocol: this.getProtocol(),
      lastActivity: this.lastActivity,
      metadata: this.getMetadata()
    };
  }

  // Protected helper methods
  protected emitStatus(status: JTAGStatus, details?: any): void {
    this.lastActivity = Date.now();
    
    // Store for testing
    this._statusEvents.push({ status, details, timestamp: this.lastActivity });
    
    // Emit to handler
    this.statusHandler?.(status, details);
  }

  protected updateActivity(): void {
    this.lastActivity = Date.now();
  }

  // Abstract methods for connection info
  protected abstract getEndpoint(): string;
  protected abstract getProtocol(): string;
  protected abstract getMetadata(): Record<string, any>;

  // TEST SUPPORT METHODS
  enableTestMode(): void {
    this._testMode = true;
  }

  disableTestMode(): void {
    this._testMode = false;
    this._mockResponses.clear();
    this._statusEvents = [];
  }

  setMockResponse(key: string, response: any): void {
    this._mockResponses.set(key, response);
  }

  getStatusEvents(): Array<{status: JTAGStatus, details?: any, timestamp: number}> {
    return [...this._statusEvents];
  }

  getLastStatusEvent(): {status: JTAGStatus, details?: any, timestamp: number} | null {
    return this._statusEvents[this._statusEvents.length - 1] || null;
  }

  clearStatusEvents(): void {
    this._statusEvents = [];
  }

  // Test assertion helpers
  waitForStatus(status: JTAGStatus, timeoutMs = 5000): Promise<{status: JTAGStatus, details?: any}> {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`Timeout waiting for status: ${status}`));
      }, timeoutMs);

      const checkStatus = () => {
        const event = this._statusEvents.find(e => e.status === status);
        if (event) {
          clearTimeout(timeout);
          resolve(event);
        } else {
          setTimeout(checkStatus, 50);
        }
      };

      checkStatus();
    });
  }

  hasStatus(status: JTAGStatus): boolean {
    return this._statusEvents.some(e => e.status === status);
  }

  getStatusCount(status: JTAGStatus): number {
    return this._statusEvents.filter(e => e.status === status).length;
  }
}