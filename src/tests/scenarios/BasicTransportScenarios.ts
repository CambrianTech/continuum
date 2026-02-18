/**
 * Basic Transport Test Scenarios
 * 
 * Concrete test implementations to understand architecture issues.
 * Tests first, then fix what breaks.
 */

import { TransportTestScenario, TestCategory, type TransportMetrics } from '../framework/TransportTestFramework';
import type { JTAGTransport } from '../../system/transports/shared/TransportTypes';
import type { JTAGMessage } from '../../system/core/types/JTAGTypes';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

/**
 * Connection Test Metrics
 */
export interface ConnectionMetrics extends TransportMetrics {
  readonly connectionTime: number;
  readonly connectionSuccess: boolean;
  readonly disconnectionTime?: number;
}

/**
 * Message Test Metrics  
 */
export interface MessageMetrics extends TransportMetrics {
  readonly messagesSent: number;
  readonly messagesReceived: number;
  readonly averageLatency: number;
  readonly successRate: number;
}

/**
 * Test 1: Basic Connection Scenario
 */
export class BasicConnectionScenario extends TransportTestScenario<{}, ConnectionMetrics> {
  constructor() {
    super('Basic Connection', TestCategory.CONNECTION, {}, 15000);
  }

  async execute(transport: JTAGTransport) {
    const startTime = Date.now();
    
    try {
      // Test connection
      const connectStart = Date.now();
      if (typeof (transport as any).initialize === 'function') {
        await (transport as any).initialize();
      }
      const connectionTime = Date.now() - connectStart;
      
      // Verify connected state
      const connectionSuccess = transport.isConnected();
      
      // Test disconnection
      const disconnectStart = Date.now();
      await transport.disconnect();
      const disconnectionTime = Date.now() - disconnectStart;
      
      const metrics: ConnectionMetrics = {
        connectionTime,
        connectionSuccess,
        disconnectionTime,
        connectionsEstablished: connectionSuccess ? 1 : 0
      };
      
      const duration = Date.now() - startTime;
      return this.createResult(connectionSuccess, duration, metrics);
      
    } catch (error: any) {
      const duration = Date.now() - startTime;
      const metrics: ConnectionMetrics = {
        connectionTime: duration,
        connectionSuccess: false,
        connectionsEstablished: 0
      };
      
      return this.createResult(false, duration, metrics, error.message);
    }
  }
}

/**
 * Test 2: Message Send/Receive Scenario
 */
export class MessagePassingScenario extends TransportTestScenario<{ messageCount: number }, MessageMetrics> {
  constructor(messageCount = 5) {
    super('Message Passing', TestCategory.MESSAGE_PASSING, { messageCount }, 20000);
  }

  async execute(transport: JTAGTransport) {
    const startTime = Date.now();
    
    try {
      // Initialize transport using the actual working interface
      if (typeof (transport as any).initialize === 'function') {
        await (transport as any).initialize();
      }
      
      if (!transport.isConnected()) {
        throw new Error('Transport failed to connect');
      }
      
      const messageCount = this.config.messageCount;
      const receivedMessages: JTAGMessage[] = [];
      const latencies: number[] = [];
      
      // Set up message handler using the actual working interface
      if (transport.setMessageHandler) {
        transport.setMessageHandler((message) => {
          const receiveTime = Date.now();
          receivedMessages.push(message);
          
          // Calculate latency if timestamp is available
          if (message.timestamp) {
            const sendTime = new Date(message.timestamp).getTime();
            latencies.push(receiveTime - sendTime);
          }
        });
      }
      
      // Send messages
      const sendPromises = [];
      for (let i = 0; i < messageCount; i++) {
        const message: JTAGMessage = {
          id: generateUUID(),
          command: 'test-message',
          payload: { sequence: i, content: `Test message ${i}` },
          timestamp: new Date().toISOString()
        };
        
        sendPromises.push(transport.send(message));
      }
      
      const sendResults = await Promise.all(sendPromises);
      const successfulSends = sendResults.filter(r => r.success).length;
      
      // Wait for messages to arrive
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      const averageLatency = latencies.length > 0 
        ? latencies.reduce((a, b) => a + b, 0) / latencies.length 
        : 0;
      
      const successRate = messageCount > 0 ? (receivedMessages.length / messageCount) * 100 : 0;
      
      const metrics: MessageMetrics = {
        messagesSent: successfulSends,
        messagesReceived: receivedMessages.length,
        averageLatency,
        successRate,
        bytesTransferred: this.estimateBytes(messageCount)
      };
      
      const duration = Date.now() - startTime;
      const success = successfulSends === messageCount && receivedMessages.length >= messageCount * 0.8;
      
      // Add debug information for failed message passing
      const debugInfo = {
        expectedMessages: messageCount,
        sentSuccessfully: successfulSends,
        messagesReceived: receivedMessages.length,
        receivedMessages: receivedMessages.map(m => ({ command: m.command, id: m.id })),
        handlerConfigured: !!transport.setMessageHandler
      };
      
      const errorMessage = success ? undefined : 
        `Message passing failed: sent ${successfulSends}/${messageCount}, received ${receivedMessages.length}/${messageCount}. Debug: ${JSON.stringify(debugInfo)}`;
      
      return this.createResult(success, duration, metrics, errorMessage);
      
    } catch (error: any) {
      const duration = Date.now() - startTime;
      const metrics: MessageMetrics = {
        messagesSent: 0,
        messagesReceived: 0,
        averageLatency: 0,
        successRate: 0
      };
      
      return this.createResult(false, duration, metrics, error.message);
    }
  }
  
  private estimateBytes(messageCount: number): number {
    // Rough estimate: ~200 bytes per test message
    return messageCount * 200;
  }
}

/**
 * Test 3: Transport Initialization Scenario
 */
export class InitializationScenario extends TransportTestScenario<{}, TransportMetrics> {
  constructor() {
    super('Transport Initialization', TestCategory.INITIALIZATION, {}, 10000);
  }

  async execute(transport: JTAGTransport) {
    const startTime = Date.now();
    
    try {
      // Test multiple initialization attempts
      if (typeof (transport as any).initialize === 'function') {
        await (transport as any).initialize();
      }
      
      if (!transport.isConnected()) {
        throw new Error('Transport not connected after initialization');
      }
      
      // Test double initialization (should be safe)
      if (typeof (transport as any).initialize === 'function') {
        await (transport as any).initialize();
      }
      
      const duration = Date.now() - startTime;
      const metrics: TransportMetrics = {
        connectionsEstablished: 1,
        memoryUsage: this.getMemoryUsage()
      };
      
      return this.createResult(true, duration, metrics);
      
    } catch (error: any) {
      const duration = Date.now() - startTime;
      const metrics: TransportMetrics = {
        connectionsEstablished: 0,
        errorsEncountered: 1
      };
      
      return this.createResult(false, duration, metrics, error.message);
    }
  }
  
  private getMemoryUsage(): number {
    if (typeof process !== 'undefined' && process.memoryUsage) {
      return process.memoryUsage().heapUsed;
    }
    return 0;
  }
}

/**
 * Test 4: Error Handling Scenario
 */
export class ErrorHandlingScenario extends TransportTestScenario<{}, TransportMetrics> {
  constructor() {
    super('Error Handling', TestCategory.FAULT_TOLERANCE, {}, 15000);
  }

  async execute(transport: JTAGTransport) {
    const startTime = Date.now();
    let errorsEncountered = 0;
    
    try {
      // Test sending message before initialization
      try {
        const message: JTAGMessage = {
          id: generateUUID(),
          command: 'test-before-init',
          payload: {},
          timestamp: new Date().toISOString()
        };
        
        await transport.send(message);
      } catch (error) {
        errorsEncountered++;
        // Expected error - this is good
      }
      
      // Initialize properly
      if (typeof (transport as any).initialize === 'function') {
        await (transport as any).initialize();
      }
      
      // Test sending invalid message
      try {
        const invalidMessage = null as any;
        await transport.send(invalidMessage);
      } catch (error) {
        errorsEncountered++;
        // Expected error - this is good  
      }
      
      // Test disconnect and reconnect
      await transport.disconnect();
      if (typeof (transport as any).initialize === 'function') {
        await (transport as any).initialize();
      }
      
      const duration = Date.now() - startTime;
      const metrics: TransportMetrics = {
        errorsEncountered,
        connectionsEstablished: 2 // Initial + reconnect
      };
      
      // Success if we handled errors gracefully and reconnected
      const success = transport.isConnected() && errorsEncountered >= 1;
      
      const errorMessage = success ? undefined :
        `Error handling failed: connected=${transport.isConnected()}, errorsEncountered=${errorsEncountered} (expected >=1)`;
      
      return this.createResult(success, duration, metrics, errorMessage);
      
    } catch (error: any) {
      const duration = Date.now() - startTime;
      const metrics: TransportMetrics = {
        errorsEncountered: errorsEncountered + 1,
        connectionsEstablished: 0
      };
      
      return this.createResult(false, duration, metrics, error.message);
    }
  }
}

/**
 * Export all basic scenarios
 */
export const BASIC_TRANSPORT_SCENARIOS = [
  new InitializationScenario(),
  new BasicConnectionScenario(),
  new MessagePassingScenario(3),
  new ErrorHandlingScenario()
];