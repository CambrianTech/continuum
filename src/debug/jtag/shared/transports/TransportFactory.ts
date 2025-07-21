/**
 * JTAG Transport Factory
 * Creates and manages all transport implementations with iterator pattern for testing
 */

import { JTAGTransport, JTAG_TRANSPORT, JTAGTransportType } from '../JTAGTypes';
import { BaseJTAGTransport } from './BaseTransport';

// Import all transport implementations
import { JTAGWebSocketTransportImpl } from './WebSocketTransport';
import { JTAGHTTPTransportImpl } from './HTTPTransport';
import { JTAGRESTTransportImpl } from './RESTTransport';
import { JTAGMCPTransportImpl } from './MCPTransport';
import { JTAGPollingTransportImpl } from './PollingTransport';
import { JTAGSSETransportImpl } from './SSETransport';
import { JTAGContinuumTransportImpl } from './ContinuumTransport';

export interface TransportDefinition {
  type: JTAGTransportType;
  name: string;
  description: string;
  factory: () => BaseJTAGTransport;
  testable: boolean;
}

export class JTAGTransportFactory {
  private static transportDefinitions: Map<JTAGTransportType, TransportDefinition> = new Map([
    [JTAG_TRANSPORT.WEBSOCKET, {
      type: JTAG_TRANSPORT.WEBSOCKET,
      name: 'WebSocket Transport',
      description: 'Real-time bidirectional WebSocket communication',
      factory: () => new JTAGWebSocketTransportImpl(),
      testable: true
    }],
    [JTAG_TRANSPORT.HTTP, {
      type: JTAG_TRANSPORT.HTTP,
      name: 'HTTP Transport',
      description: 'Request-response HTTP communication',
      factory: () => new JTAGHTTPTransportImpl(),
      testable: true
    }],
    [JTAG_TRANSPORT.REST, {
      type: JTAG_TRANSPORT.REST,
      name: 'REST API Transport',
      description: 'RESTful API communication with standard endpoints',
      factory: () => new JTAGRESTTransportImpl(),
      testable: true
    }],
    [JTAG_TRANSPORT.MCP, {
      type: JTAG_TRANSPORT.MCP,
      name: 'MCP Transport',
      description: 'Model Context Protocol for AI-safe communication',
      factory: () => new JTAGMCPTransportImpl(),
      testable: true
    }],
    [JTAG_TRANSPORT.POLLING, {
      type: JTAG_TRANSPORT.POLLING,
      name: 'HTTP Polling Transport',
      description: 'Long-polling HTTP for real-time-like communication',
      factory: () => new JTAGPollingTransportImpl(),
      testable: true
    }],
    [JTAG_TRANSPORT.SSE, {
      type: JTAG_TRANSPORT.SSE,
      name: 'Server-Sent Events Transport',
      description: 'One-way server-to-client streaming',
      factory: () => new JTAGSSETransportImpl(),
      testable: true
    }],
    [JTAG_TRANSPORT.CONTINUUM_WS, {
      type: JTAG_TRANSPORT.CONTINUUM_WS,
      name: 'Continuum WebSocket Transport',
      description: 'Integration with Continuum daemon system',
      factory: () => new JTAGContinuumTransportImpl(),
      testable: false // Requires full Continuum system
    }]
  ]);

  // Factory methods
  static createTransport(type: JTAGTransportType): BaseJTAGTransport {
    const definition = this.transportDefinitions.get(type);
    if (!definition) {
      throw new Error(`Unknown transport type: ${type}`);
    }
    return definition.factory();
  }

  static getAvailableTransports(): JTAGTransportType[] {
    return Array.from(this.transportDefinitions.keys());
  }

  static getTestableTransports(): JTAGTransportType[] {
    return Array.from(this.transportDefinitions.entries())
      .filter(([, def]) => def.testable)
      .map(([type]) => type);
  }

  static getTransportDefinition(type: JTAGTransportType): TransportDefinition | undefined {
    return this.transportDefinitions.get(type);
  }

  // Iterator for testing all transports
  static *createAllTransports(): Generator<{transport: BaseJTAGTransport, definition: TransportDefinition}> {
    for (const [type, definition] of this.transportDefinitions.entries()) {
      yield {
        transport: definition.factory(),
        definition
      };
    }
  }

  // Iterator for testing only testable transports
  static *createTestableTransports(): Generator<{transport: BaseJTAGTransport, definition: TransportDefinition}> {
    for (const [type, definition] of this.transportDefinitions.entries()) {
      if (definition.testable) {
        yield {
          transport: definition.factory(),
          definition
        };
      }
    }
  }

  // Utility methods for testing
  static async runTransportTest<T>(
    testFn: (transport: BaseJTAGTransport, definition: TransportDefinition) => Promise<T>,
    transportTypes?: JTAGTransportType[]
  ): Promise<Map<JTAGTransportType, {result: T, error?: string}>> {
    const results = new Map<JTAGTransportType, {result: T, error?: string}>();
    const typesToTest = transportTypes || this.getTestableTransports();

    for (const type of typesToTest) {
      const definition = this.transportDefinitions.get(type)!;
      const transport = definition.factory();
      
      try {
        transport.enableTestMode();
        const result = await testFn(transport, definition);
        results.set(type, { result });
        transport.disableTestMode();
      } catch (error: any) {
        results.set(type, { result: null as T, error: error.message });
        transport.disableTestMode();
      } finally {
        await transport.disconnect();
      }
    }

    return results;
  }

  // Auto-detection logic
  static detectBestTransport(): JTAGTransportType {
    // Browser environment
    if (typeof window !== 'undefined') {
      if (typeof WebSocket !== 'undefined') {
        return JTAG_TRANSPORT.WEBSOCKET;
      }
      if (typeof EventSource !== 'undefined') {
        return JTAG_TRANSPORT.SSE;
      }
      return JTAG_TRANSPORT.HTTP;
    }
    
    // Node.js environment
    if (typeof require !== 'undefined') {
      // Check if in Continuum environment
      try {
        require('../../../integrations/websocket/core/DaemonConnector');
        return JTAG_TRANSPORT.CONTINUUM_WS;
      } catch {
        // Fall back to WebSocket or HTTP
        try {
          require('ws');
          return JTAG_TRANSPORT.WEBSOCKET;
        } catch {
          return JTAG_TRANSPORT.HTTP;
        }
      }
    }

    // Default fallback
    return JTAG_TRANSPORT.HTTP;
  }
}