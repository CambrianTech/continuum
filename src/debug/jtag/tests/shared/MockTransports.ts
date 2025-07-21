/**
 * Mock Transport Implementations for Testing
 * 
 * Provides controllable, predictable transport behaviors for isolated testing
 * of JTAG business logic without network dependencies.
 */

import type { 
  JTAGTransport, 
  JTAGWebSocketMessage, 
  JTAGTransportResponse,
  JTAGConfig 
} from '../../shared/JTAGTypes';

/**
 * Mock Transport - Always Succeeds
 * Perfect for testing business logic without network concerns
 */
export class MockSuccessTransport implements JTAGTransport {
  name = 'mock-success';
  private initialized = false;
  private messages: JTAGWebSocketMessage[] = [];

  async initialize(config: JTAGConfig): Promise<boolean> {
    this.initialized = true;
    return true;
  }

  async send<T>(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse<T>> {
    this.messages.push(message);
    
    return {
      success: true,
      data: { received: true, messageId: message.messageId } as T,
      timestamp: new Date().toISOString(),
      messageId: message.messageId,
      transportMeta: {
        transport: this.name,
        duration: 1, // Instant
        retries: 0
      }
    };
  }

  isConnected(): boolean {
    return this.initialized;
  }

  async disconnect(): Promise<void> {
    this.initialized = false;
  }

  // Test helpers
  getMessages(): JTAGWebSocketMessage[] {
    return [...this.messages];
  }

  getLastMessage(): JTAGWebSocketMessage | null {
    return this.messages[this.messages.length - 1] || null;
  }

  clearMessages(): void {
    this.messages = [];
  }

  getMessageCount(): number {
    return this.messages.length;
  }
}

/**
 * Mock Transport - Always Fails
 * Perfect for testing error handling and fallback scenarios
 */
export class MockFailureTransport implements JTAGTransport {
  name = 'mock-failure';
  private failureReason: string;

  constructor(failureReason = 'Mock transport failure') {
    this.failureReason = failureReason;
  }

  async initialize(config: JTAGConfig): Promise<boolean> {
    return false; // Always fails to initialize
  }

  async send<T>(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse<T>> {
    return {
      success: false,
      error: this.failureReason,
      timestamp: new Date().toISOString(),
      messageId: message.messageId,
      transportMeta: {
        transport: this.name,
        duration: 0,
        retries: 0
      }
    };
  }

  isConnected(): boolean {
    return false;
  }

  async disconnect(): Promise<void> {
    // Already disconnected
  }
}

/**
 * Mock Transport - Controllable Behavior
 * Allows fine-grained control over transport behavior for specific test scenarios
 */
export class MockControllableTransport implements JTAGTransport {
  name = 'mock-controllable';
  private shouldSucceed = true;
  private shouldConnect = true;
  private delay = 0;
  private messages: JTAGWebSocketMessage[] = [];
  private responses: any[] = [];

  async initialize(config: JTAGConfig): Promise<boolean> {
    await this.sleep(this.delay);
    return this.shouldConnect;
  }

  async send<T>(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse<T>> {
    this.messages.push(message);
    await this.sleep(this.delay);

    if (!this.shouldSucceed) {
      return {
        success: false,
        error: 'Controllable transport set to fail',
        timestamp: new Date().toISOString(),
        messageId: message.messageId,
        transportMeta: {
          transport: this.name,
          duration: this.delay,
          retries: 0
        }
      };
    }

    const responseData = this.responses.length > 0 
      ? this.responses.shift() 
      : { received: true, messageId: message.messageId };

    return {
      success: true,
      data: responseData as T,
      timestamp: new Date().toISOString(),
      messageId: message.messageId,
      transportMeta: {
        transport: this.name,
        duration: this.delay,
        retries: 0
      }
    };
  }

  isConnected(): boolean {
    return this.shouldConnect;
  }

  async disconnect(): Promise<void> {
    this.shouldConnect = false;
  }

  // Control methods for testing
  setSuccess(success: boolean): void {
    this.shouldSucceed = success;
  }

  setConnected(connected: boolean): void {
    this.shouldConnect = connected;
  }

  setDelay(delayMs: number): void {
    this.delay = delayMs;
  }

  queueResponse(response: any): void {
    this.responses.push(response);
  }

  getMessages(): JTAGWebSocketMessage[] {
    return [...this.messages];
  }

  clearMessages(): void {
    this.messages = [];
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

/**
 * Mock Transport - Network Simulation
 * Simulates real network conditions (latency, intermittent failures, etc.)
 */
export class MockNetworkTransport implements JTAGTransport {
  name = 'mock-network';
  private latencyMs: number;
  private dropRate: number; // 0-1, probability of dropping messages
  private messages: JTAGWebSocketMessage[] = [];
  private connected = false;

  constructor(latencyMs = 100, dropRate = 0.1) {
    this.latencyMs = latencyMs;
    this.dropRate = dropRate;
  }

  async initialize(config: JTAGConfig): Promise<boolean> {
    // Simulate connection time
    await this.sleep(this.latencyMs);
    
    // Sometimes connection fails
    const connectionSuccess = Math.random() > 0.1; // 90% success rate
    this.connected = connectionSuccess;
    return connectionSuccess;
  }

  async send<T>(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse<T>> {
    const startTime = Date.now();
    this.messages.push(message);

    // Simulate network latency
    await this.sleep(this.latencyMs + Math.random() * this.latencyMs);

    // Simulate message drops
    if (Math.random() < this.dropRate) {
      return {
        success: false,
        error: 'Message dropped (network simulation)',
        timestamp: new Date().toISOString(),
        messageId: message.messageId,
        transportMeta: {
          transport: this.name,
          duration: Date.now() - startTime,
          retries: 0
        }
      };
    }

    return {
      success: true,
      data: { received: true, messageId: message.messageId } as T,
      timestamp: new Date().toISOString(),
      messageId: message.messageId,
      transportMeta: {
        transport: this.name,
        duration: Date.now() - startTime,
        retries: 0
      }
    };
  }

  isConnected(): boolean {
    return this.connected;
  }

  async disconnect(): Promise<void> {
    this.connected = false;
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  getMessages(): JTAGWebSocketMessage[] {
    return [...this.messages];
  }

  // Simulate network events
  simulateDisconnection(): void {
    this.connected = false;
  }

  simulateReconnection(): void {
    this.connected = true;
  }
}

/**
 * Mock Transport Factory for Testing
 */
export class MockTransportFactory {
  static createSuccessTransport(): MockSuccessTransport {
    return new MockSuccessTransport();
  }

  static createFailureTransport(reason?: string): MockFailureTransport {
    return new MockFailureTransport(reason);
  }

  static createControllableTransport(): MockControllableTransport {
    return new MockControllableTransport();
  }

  static createNetworkSimulation(latencyMs = 100, dropRate = 0.1): MockNetworkTransport {
    return new MockNetworkTransport(latencyMs, dropRate);
  }
}

/**
 * Test Utilities for Transport Testing
 */
export class TransportTestUtils {
  static async waitForMessages(transport: MockSuccessTransport | MockControllableTransport, count: number, timeoutMs = 5000): Promise<boolean> {
    const startTime = Date.now();
    
    while (Date.now() - startTime < timeoutMs) {
      if (transport.getMessageCount() >= count) {
        return true;
      }
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    
    return false;
  }

  static createJTAGMessage(type: 'log' | 'screenshot' | 'exec' = 'log'): JTAGWebSocketMessage {
    return {
      type,
      payload: {
        timestamp: new Date().toISOString(),
        context: 'test' as any,
        component: 'TEST',
        message: 'Test message',
        type: 'log'
      },
      timestamp: new Date().toISOString(),
      messageId: `test-${Date.now()}-${Math.random()}`
    };
  }

  static assertTransportBehavior(transport: any, expectedMessages: number, expectedConnected: boolean): void {
    const actualMessages = transport.getMessages ? transport.getMessages().length : 0;
    const actualConnected = transport.isConnected();

    if (actualMessages !== expectedMessages) {
      throw new Error(`Expected ${expectedMessages} messages, got ${actualMessages}`);
    }

    if (actualConnected !== expectedConnected) {
      throw new Error(`Expected connected: ${expectedConnected}, got: ${actualConnected}`);
    }
  }
}