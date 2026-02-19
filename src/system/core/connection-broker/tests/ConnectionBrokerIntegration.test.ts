/**
 * Connection Broker Integration Tests - Multi-client scenarios and port conflict resolution
 * 
 * These tests validate the Connection Broker's ability to handle real-world scenarios:
 * - Multiple clients connecting simultaneously
 * - Port conflict detection and resolution
 * - Connection reuse optimization
 * - Server lifecycle management
 * - Load balancing and server selection
 * - Error recovery and fallback strategies
 */

import { describe, it, expect, beforeEach, afterEach, jest } from '@jest/globals';
import { ConnectionBroker } from '../shared/ConnectionBroker';
import { JTAGClient } from '../../client/shared/JTAGClient';
import { generateUUID } from '../../types/CrossPlatformUUID';
import type { 
  ConnectionParams,
  IConnectionBroker,
  ServerRegistryEntry,
  ConnectionResult
} from '../shared/ConnectionBrokerTypes';
import type { ITransportFactory } from '../../../transports/shared/ITransportFactory';
import type { JTAGTransport, TransportConfig } from '../../../transports/shared/TransportTypes';
import type { JTAGContext } from '../../types/JTAGTypes';

// Enhanced mock transport factory for integration testing
class IntegrationMockTransportFactory implements ITransportFactory {
  private transports: Map<string, JTAGTransport> = new Map();
  private portUsage: Map<number, number> = new Map(); // port -> usage count
  private connectionDelay = 0; // Simulate connection time
  private failureRate = 0; // Simulate connection failures
  
  constructor(private testId: string) {}
  
  setConnectionDelay(ms: number): void {
    this.connectionDelay = ms;
  }
  
  setFailureRate(rate: number): void {
    this.failureRate = rate; // 0-1, probability of failure
  }
  
  async createTransport(environment: string, config: TransportConfig): Promise<JTAGTransport> {
    // Simulate connection delay
    if (this.connectionDelay > 0) {
      await new Promise(resolve => setTimeout(resolve, this.connectionDelay));
    }
    
    // Simulate connection failures
    if (Math.random() < this.failureRate) {
      throw new Error(`Simulated connection failure (${this.failureRate * 100}% failure rate)`);
    }
    
    const transportKey = `${environment}-${config.protocol}-${config.role}-${config.serverPort}`;
    
    // Track port usage for realistic port conflict simulation
    if (config.serverPort && config.role === 'server') {
      const currentUsage = this.portUsage.get(config.serverPort) || 0;
      this.portUsage.set(config.serverPort, currentUsage + 1);
      
      // Simulate port conflicts for high usage
      if (currentUsage > 0 && Math.random() < 0.3) {
        throw new Error(`Port ${config.serverPort} already in use`);
      }
    }
    
    const transport: JTAGTransport = {
      name: `MockTransport-${transportKey}`,
      send: jest.fn().mockResolvedValue({ success: true, timestamp: new Date().toISOString() }),
      disconnect: jest.fn().mockImplementation(async () => {
        // Release port when disconnecting
        if (config.serverPort && config.role === 'server') {
          const currentUsage = this.portUsage.get(config.serverPort) || 0;
          this.portUsage.set(config.serverPort, Math.max(0, currentUsage - 1));
        }
      }),
      isConnected: jest.fn().mockReturnValue(true),
      getConnectionInfo: jest.fn().mockReturnValue({
        protocol: config.protocol,
        isConnected: true,
        connectionTime: Date.now(),
        port: config.serverPort
      })
    };
    
    this.transports.set(transportKey, transport);
    return transport;
  }
  
  async createWebSocketTransport(environment: string, config: TransportConfig): Promise<JTAGTransport> {
    return this.createTransport(environment, config);
  }
  
  getPortUsage(port: number): number {
    return this.portUsage.get(port) || 0;
  }
  
  getAllTransports(): JTAGTransport[] {
    return Array.from(this.transports.values());
  }
  
  reset(): void {
    this.transports.clear();
    this.portUsage.clear();
    this.connectionDelay = 0;
    this.failureRate = 0;
  }
}

// Test utilities
function createTestContext(environment: 'browser' | 'server' = 'server'): JTAGContext {
  return {
    uuid: generateUUID(),
    environment
  };
}

function createConnectionParams(overrides: Partial<ConnectionParams> = {}): ConnectionParams {
  return {
    protocols: ['websocket'],
    mode: 'shared',
    targetEnvironment: 'server',
    sessionId: generateUUID(),
    context: createTestContext(),
    timeoutMs: 5000,
    enableFallback: true,
    maxRetries: 3,
    ...overrides
  };
}

async function createMultipleConnections(
  broker: IConnectionBroker,
  count: number,
  paramOverrides: Partial<ConnectionParams> = {}
): Promise<ConnectionResult[]> {
  const promises: Promise<ConnectionResult>[] = [];
  
  for (let i = 0; i < count; i++) {
    const params = createConnectionParams({
      sessionId: generateUUID(), // Unique session for each connection
      ...paramOverrides
    });
    promises.push(broker.connect(params));
  }
  
  return Promise.all(promises);
}

describe('Connection Broker Integration Tests', () => {
  let broker: IConnectionBroker;
  let mockFactory: IntegrationMockTransportFactory;
  const testId = generateUUID();

  beforeEach(() => {
    mockFactory = new IntegrationMockTransportFactory(testId);
    broker = new ConnectionBroker({
      portPool: {
        startPort: 9100, // Use different range to avoid conflicts with other tests
        endPort: 9110,
        reservedPorts: [],
        allocationStrategy: 'sequential'
      },
      registry: {
        persistInterval: 1000,
        cleanupInterval: 2000,
        maxEntryAge: 10000
      },
      timeouts: {
        connection: 3000,
        retryDelay: 100,
        maxTotal: 10000
      }
    }, mockFactory);
  });

  afterEach(async () => {
    if (broker instanceof ConnectionBroker) {
      await broker.shutdown();
    }
    mockFactory.reset();
  });

  describe('Multi-Client Connection Scenarios', () => {
    it('should handle multiple clients connecting simultaneously', async () => {
      const connectionCount = 5;
      
      const results = await createMultipleConnections(broker, connectionCount, {
        mode: 'isolated' // Each client gets its own server
      });
      
      expect(results).toHaveLength(connectionCount);
      
      // Verify all connections succeeded
      results.forEach((result, index) => {
        expect(result).toBeDefined();
        expect(result.transport).toBeDefined();
        expect(result.server.port).toBeGreaterThanOrEqual(9100);
        expect(result.server.port).toBeLessThanOrEqual(9110);
        expect(result.strategy).toBe('created_new');
      });
      
      // Verify unique ports assigned
      const assignedPorts = results.map(r => r.server.port);
      const uniquePorts = new Set(assignedPorts);
      expect(uniquePorts.size).toBe(connectionCount); // All ports should be unique
    });

    it('should reuse servers when clients request shared connections', async () => {
      // Create initial shared connection
      const initialConnection = await broker.connect(createConnectionParams({
        mode: 'shared'
      }));
      
      // Mark the server as ready for reuse
      const state = await broker.getRegistryState();
      const server = state.servers.find(s => s.guid === initialConnection.server.guid);
      if (server) {
        (server as any).status = 'ready';
      }
      
      // Create additional shared connections
      const additionalConnections = await createMultipleConnections(broker, 3, {
        mode: 'shared',
        targetEnvironment: 'server' // Same environment as initial
      });
      
      // All additional connections should reuse the same server
      additionalConnections.forEach(result => {
        expect(result.strategy).toBe('reused_existing');
        expect(result.server.port).toBe(initialConnection.server.port);
      });
      
      // Verify connection count tracking
      const finalState = await broker.getRegistryState();
      const finalServer = finalState.servers.find(s => s.guid === initialConnection.server.guid);
      expect(finalServer?.connectionCount).toBe(4); // 1 initial + 3 additional
    });

    it('should handle mixed connection modes intelligently', async () => {
      const connections = await Promise.all([
        // Isolated connection - gets its own server
        broker.connect(createConnectionParams({ mode: 'isolated' })),
        
        // Shared connection - gets its own server initially
        broker.connect(createConnectionParams({ mode: 'shared' })),
        
        // Preferred connection - should reuse shared server
        broker.connect(createConnectionParams({ mode: 'preferred' })),
        
        // Another isolated - gets new server
        broker.connect(createConnectionParams({ mode: 'isolated' }))
      ]);
      
      const strategies = connections.map(c => c.strategy);
      
      expect(strategies[0]).toBe('created_new'); // isolated
      expect(strategies[1]).toBe('created_new'); // shared (first)
      expect(strategies[2]).toBe('reused_existing'); // preferred (reuses shared)
      expect(strategies[3]).toBe('created_new'); // isolated (new)
      
      // Verify port allocation
      const ports = connections.map(c => c.server.port);
      expect(new Set(ports).size).toBe(3); // 3 unique servers
    });
  });

  describe('Port Conflict Resolution', () => {
    it('should handle port allocation exhaustion gracefully', async () => {
      // Configure broker with very limited port range
      const limitedBroker = new ConnectionBroker({
        portPool: {
          startPort: 9200,
          endPort: 9202, // Only 3 ports available
          reservedPorts: [],
          allocationStrategy: 'sequential'
        }
      }, mockFactory);
      
      try {
        // Create connections up to the limit
        const successfulConnections = await createMultipleConnections(limitedBroker, 3, {
          mode: 'isolated'
        });
        
        expect(successfulConnections).toHaveLength(3);
        
        // Next connection should fail due to port exhaustion
        await expect(limitedBroker.connect(createConnectionParams({ mode: 'isolated' })))
          .rejects.toThrow(/No available ports/);
          
      } finally {
        await limitedBroker.shutdown();
      }
    });

    it('should fall back to alternative protocols when primary fails', async () => {
      // Configure factory to fail for UDP multicast
      mockFactory.setFailureRate(0.5); // 50% failure rate
      
      const connections: ConnectionResult[] = [];
      const attempts = 10;
      
      // Attempt multiple connections with fallback enabled
      for (let i = 0; i < attempts; i++) {
        try {
          const result = await broker.connect(createConnectionParams({
            protocols: ['udp-multicast', 'websocket'], // UDP will fail, fallback to websocket
            enableFallback: true,
            maxRetries: 3
          }));
          connections.push(result);
        } catch (error) {
          // Some failures are expected due to random failure rate
          console.log(`Connection ${i} failed:`, error);
        }
      }
      
      // At least some connections should succeed via fallback
      expect(connections.length).toBeGreaterThan(0);
      
      // Successful connections should indicate fallback was used (when it occurred)
      const fallbackConnections = connections.filter(c => c.metadata.usedFallback);
      console.log(`${fallbackConnections.length}/${connections.length} used fallback protocol`);
    });

    it('should handle server startup failures and retry', async () => {
      // Configure factory to fail initially, then succeed
      let attemptCount = 0;
      const originalCreate = mockFactory.createTransport.bind(mockFactory);
      
      mockFactory.createTransport = jest.fn().mockImplementation((env, config) => {
        attemptCount++;
        if (attemptCount <= 2 && config.role === 'server') {
          throw new Error(`Server startup failure (attempt ${attemptCount})`);
        }
        return originalCreate(env, config);
      });
      
      const result = await broker.connect(createConnectionParams({
        maxRetries: 5
      }));
      
      expect(result).toBeDefined();
      expect(result.metadata.retryAttempts).toBeGreaterThan(0);
      expect(attemptCount).toBeGreaterThan(2);
    });
  });

  describe('Load Balancing and Server Selection', () => {
    it('should distribute load across multiple available servers', async () => {
      // Create multiple servers manually
      const serverPorts = [9300, 9301, 9302];
      const serverGuids: string[] = [];
      
      for (const port of serverPorts) {
        const guid = await broker.registerServer({
          name: `test-server-${port}`,
          port,
          protocol: 'websocket',
          environment: 'server',
          processId: process.pid
        });
        serverGuids.push(guid);
        
        // Mark as ready
        const state = await broker.getRegistryState();
        const server = state.servers.find(s => s.guid === guid);
        if (server) {
          (server as any).status = 'ready';
        }
      }
      
      // Create multiple client connections
      const connections = await createMultipleConnections(broker, 6, {
        mode: 'shared' // Should distribute across available servers
      });
      
      // Verify connections are distributed
      const serverUsage = new Map<number, number>();
      connections.forEach(conn => {
        const port = conn.server.port;
        serverUsage.set(port, (serverUsage.get(port) || 0) + 1);
      });
      
      console.log('Server usage distribution:', Object.fromEntries(serverUsage));
      
      // Each server should have at least one connection
      expect(serverUsage.size).toBeGreaterThan(1);
    });

    it('should prefer servers with lower connection counts for load balancing', async () => {
      // Create a server with existing load
      const highLoadServerGuid = await broker.registerServer({
        name: 'high-load-server',
        port: 9400,
        protocol: 'websocket',
        environment: 'server',
        processId: process.pid
      });
      
      // Create a server with no load
      const lowLoadServerGuid = await broker.registerServer({
        name: 'low-load-server',
        port: 9401,
        protocol: 'websocket',
        environment: 'server',
        processId: process.pid
      });
      
      // Simulate high load on first server
      const state = await broker.getRegistryState();
      const highLoadServer = state.servers.find(s => s.guid === highLoadServerGuid);
      const lowLoadServer = state.servers.find(s => s.guid === lowLoadServerGuid);
      
      if (highLoadServer && lowLoadServer) {
        (highLoadServer as any).status = 'ready';
        (highLoadServer as any).connectionCount = 5; // High load
        (lowLoadServer as any).status = 'ready';
        (lowLoadServer as any).connectionCount = 0; // No load
      }
      
      // New connection should prefer the low-load server
      const connection = await broker.connect(createConnectionParams({
        mode: 'shared'
      }));
      
      expect(connection.server.port).toBe(9401); // Should connect to low-load server
      expect(connection.strategy).toBe('reused_existing');
    });
  });

  describe('Error Recovery and Resilience', () => {
    it('should recover from transport factory errors', async () => {
      let errorCount = 0;
      const maxErrors = 2;
      
      const originalCreate = mockFactory.createTransport.bind(mockFactory);
      mockFactory.createTransport = jest.fn().mockImplementation((env, config) => {
        errorCount++;
        if (errorCount <= maxErrors) {
          throw new Error(`Transport factory error ${errorCount}`);
        }
        return originalCreate(env, config);
      });
      
      const result = await broker.connect(createConnectionParams({
        maxRetries: 5,
        enableFallback: true
      }));
      
      expect(result).toBeDefined();
      expect(result.metadata.retryAttempts).toBe(maxErrors);
      expect(errorCount).toBe(maxErrors + 1); // Final successful attempt
    });

    it('should handle server registry corruption gracefully', async () => {
      // Create valid connection
      const connection1 = await broker.connect(createConnectionParams());
      expect(connection1).toBeDefined();
      
      // Simulate registry corruption by manually corrupting server state
      const state = await broker.getRegistryState();
      state.servers.forEach(server => {
        (server as any).status = 'error'; // Mark all servers as errored
      });
      
      // New connection should still succeed by creating new server
      const connection2 = await broker.connect(createConnectionParams({
        mode: 'isolated' // Force new server creation
      }));
      
      expect(connection2).toBeDefined();
      expect(connection2.strategy).toBe('created_new');
      expect(connection2.server.port).not.toBe(connection1.server.port);
    });

    it('should clean up failed servers automatically', async () => {
      // Create connection that will fail
      const guid = await broker.registerServer({
        name: 'failing-server',
        port: 9500,
        protocol: 'websocket',
        environment: 'server',
        processId: process.pid
      });
      
      // Mark server as error state
      const state = await broker.getRegistryState();
      const server = state.servers.find(s => s.guid === guid);
      if (server) {
        (server as any).status = 'error';
      }
      
      expect(state.servers).toHaveLength(1);
      
      // Trigger cleanup
      await broker.cleanup();
      
      // Failed server should be removed
      const cleanedState = await broker.getRegistryState();
      expect(cleanedState.servers).toHaveLength(0);
    });
  });

  describe('Performance and Statistics', () => {
    it('should track connection statistics accurately', async () => {
      // Create various types of connections
      const isolatedConnections = await createMultipleConnections(broker, 2, { mode: 'isolated' });
      const sharedConnections = await createMultipleConnections(broker, 3, { mode: 'shared' });
      
      const stats = (await broker.getRegistryState()).statistics;
      
      expect(stats.totalConnections).toBe(5);
      expect(stats.successfulConnections).toBe(5);
      expect(stats.avgConnectionTime).toBeGreaterThan(0);
      expect(stats.portUtilization.allocated).toBeGreaterThan(0);
      expect(stats.portUtilization.available).toBeLessThan(stats.portUtilization.total);
      
      // Connection reuse rate should reflect shared connections
      // At least 2 of the shared connections should have reused servers
      expect(stats.reuseRate).toBeGreaterThanOrEqual(0.2); // At least 20% reuse
    });

    it('should handle high-concurrency connection requests', async () => {
      const concurrentConnections = 20;
      const startTime = Date.now();
      
      // Create many connections concurrently
      const results = await createMultipleConnections(broker, concurrentConnections, {
        mode: 'preferred' // Mix of reuse and new creation
      });
      
      const endTime = Date.now();
      const totalTime = endTime - startTime;
      
      expect(results).toHaveLength(concurrentConnections);
      
      // All connections should complete within reasonable time
      expect(totalTime).toBeLessThan(10000); // 10 seconds max
      
      // Verify connection strategies are distributed appropriately
      const strategies = results.map(r => r.strategy);
      const newConnections = strategies.filter(s => s === 'created_new').length;
      const reusedConnections = strategies.filter(s => s === 'reused_existing').length;
      
      console.log(`High-concurrency test: ${newConnections} new, ${reusedConnections} reused connections`);
      
      // Should have both new and reused connections
      expect(newConnections).toBeGreaterThan(0);
      expect(reusedConnections).toBeGreaterThan(0);
    });
  });

  describe('Real-World Scenarios', () => {
    it('should simulate npm start + npm test port conflict scenario', async () => {
      // Simulate npm start - creates server on preferred port
      const npmStartConnection = await broker.connect(createConnectionParams({
        mode: 'shared',
        server: { port: 9001, name: 'npm-start-server' }, // Preferred port
        metadata: { source: 'npm-start' }
      }));
      
      expect(npmStartConnection.server.port).toBe(9001);
      expect(npmStartConnection.strategy).toBe('created_new');
      
      // Simulate npm test - should reuse existing server if compatible
      const npmTestConnection = await broker.connect(createConnectionParams({
        mode: 'preferred',
        server: { port: 9001, name: 'npm-test-client' },
        metadata: { source: 'npm-test' }
      }));
      
      // Should reuse the npm start server
      expect(npmTestConnection.strategy).toBe('reused_existing');
      expect(npmTestConnection.server.port).toBe(9001);
      expect(npmTestConnection.server.guid).toBe(npmStartConnection.server.guid);
      
      console.log('✅ Port conflict simulation: npm test successfully reused npm start server');
    });

    it('should handle development workflow with frequent reconnections', async () => {
      // Simulate development cycle: connect, disconnect, reconnect
      const sessionId = generateUUID();
      
      for (let cycle = 0; cycle < 5; cycle++) {
        console.log(`Development cycle ${cycle + 1}`);
        
        // Connect
        const connection = await broker.connect(createConnectionParams({
          sessionId, // Same session across reconnections
          mode: 'preferred',
          metadata: { cycle }
        }));
        
        expect(connection).toBeDefined();
        
        // Simulate some work time
        await new Promise(resolve => setTimeout(resolve, 50));
        
        // Disconnect
        await connection.transport.disconnect();
        
        // Update server connection count
        const state = await broker.getRegistryState();
        const server = state.servers.find(s => s.guid === connection.server.guid);
        if (server && server.connectionCount > 0) {
          (server as any).connectionCount--;
        }
      }
      
      // Final state should show proper cleanup
      const finalState = await broker.getRegistryState();
      console.log(`Final servers: ${finalState.servers.length}`);
    });
  });
});