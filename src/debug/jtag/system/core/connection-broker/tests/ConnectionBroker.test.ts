/**
 * Connection Broker Unit Tests - Comprehensive test suite
 * 
 * Tests the core functionality of the Connection Broker including:
 * - Server registration and discovery
 * - Port allocation strategies  
 * - Connection reuse logic
 * - Error handling and edge cases
 * - Statistics tracking
 * - Configuration management
 */

import { describe, it, expect, beforeEach, afterEach, jest } from '@jest/globals';
import { ConnectionBroker } from '../shared/ConnectionBroker';
import { generateUUID } from '../../types/CrossPlatformUUID';
import type { 
  ConnectionParams,
  ServerRegistryEntry,
  ServerSelector,
  ConnectionBrokerConfig,
  IConnectionBroker
} from '../shared/ConnectionBrokerTypes';
import { ConnectionBrokerError } from '../shared/ConnectionBrokerTypes';
import type { ITransportFactory } from '../../../transports/shared/ITransportFactory';
import type { JTAGTransport, TransportConfig } from '../../../transports/shared/TransportTypes';
import type { JTAGContext } from '../../types/JTAGTypes';

// Mock transport factory for testing
class MockTransportFactory implements ITransportFactory {
  private mockTransports: Map<string, JTAGTransport> = new Map();
  
  async createTransport(environment: string, config: TransportConfig): Promise<JTAGTransport> {
    const key = `${environment}-${config.protocol}-${config.role}-${config.serverPort}`;
    
    const mockTransport: JTAGTransport = {
      send: jest.fn().mockResolvedValue({ success: true, timestamp: new Date().toISOString() }),
      disconnect: jest.fn().mockResolvedValue(undefined),
      isConnected: jest.fn().mockReturnValue(true),
      getConnectionInfo: jest.fn().mockReturnValue({
        protocol: config.protocol,
        isConnected: true,
        connectionTime: Date.now()
      })
    };
    
    this.mockTransports.set(key, mockTransport);
    return mockTransport;
  }
  
  async createWebSocketTransport(environment: string, config: TransportConfig): Promise<JTAGTransport> {
    return this.createTransport(environment, config);
  }
  
  getMockTransport(environment: string, protocol: string, role: string, port: number): JTAGTransport | undefined {
    const key = `${environment}-${protocol}-${role}-${port}`;
    return this.mockTransports.get(key);
  }
  
  clearMocks(): void {
    this.mockTransports.clear();
  }
}

// Test utilities
function createTestContext(): JTAGContext {
  return {
    uuid: generateUUID(),
    environment: 'server'
  };
}

function createTestConnectionParams(overrides: Partial<ConnectionParams> = {}): ConnectionParams {
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

function createTestServer(overrides: Partial<Omit<ServerRegistryEntry, 'guid' | 'createdAt' | 'lastActivity' | 'connectionCount' | 'status'>> = {}): Omit<ServerRegistryEntry, 'guid' | 'createdAt' | 'lastActivity' | 'connectionCount' | 'status'> {
  return {
    name: 'test-server',
    port: 9001,
    protocol: 'websocket',
    environment: 'server',
    processId: process.pid,
    capabilities: [],
    metadata: {},
    tags: [],
    ...overrides
  };
}

describe('ConnectionBroker', () => {
  let broker: IConnectionBroker;
  let mockFactory: MockTransportFactory;
  let testConfig: Partial<ConnectionBrokerConfig>;

  beforeEach(() => {
    mockFactory = new MockTransportFactory();
    testConfig = {
      portPool: {
        startPort: 9001,
        endPort: 9010,
        reservedPorts: [9005],
        allocationStrategy: 'sequential'
      },
      registry: {
        persistInterval: 1000,
        cleanupInterval: 2000,
        maxEntryAge: 10000
      },
      timeouts: {
        connection: 5000,
        retryDelay: 100,
        maxTotal: 15000
      }
    };
    
    broker = new ConnectionBroker(testConfig, mockFactory);
  });

  afterEach(async () => {
    if (broker instanceof ConnectionBroker) {
      await broker.shutdown();
    }
    mockFactory.clearMocks();
  });

  describe('Server Registration', () => {
    it('should register a new server successfully', async () => {
      const serverData = createTestServer({
        name: 'test-websocket-server',
        port: 9001,
        protocol: 'websocket'
      });

      const serverGuid = await broker.registerServer(serverData);
      
      expect(serverGuid).toBeDefined();
      expect(typeof serverGuid).toBe('string');
      
      const state = await broker.getRegistryState();
      expect(state.servers).toHaveLength(1);
      
      const registeredServer = state.servers[0];
      expect(registeredServer.guid).toBe(serverGuid);
      expect(registeredServer.name).toBe('test-websocket-server');
      expect(registeredServer.port).toBe(9001);
      expect(registeredServer.protocol).toBe('websocket');
      expect(registeredServer.status).toBe('starting');
    });

    it('should prevent port conflicts during registration', async () => {
      const server1 = createTestServer({ name: 'server-1', port: 9001 });
      const server2 = createTestServer({ name: 'server-2', port: 9001 });

      await broker.registerServer(server1);
      
      await expect(broker.registerServer(server2)).rejects.toThrow(ConnectionBrokerError);
      await expect(broker.registerServer(server2)).rejects.toThrow('Port 9001 already allocated');
    });

    it('should handle server unregistration correctly', async () => {
      const serverData = createTestServer({ port: 9002 });
      const serverGuid = await broker.registerServer(serverData);
      
      let state = await broker.getRegistryState();
      expect(state.servers).toHaveLength(1);
      expect(state.portAllocations.has(9002)).toBe(true);
      
      await broker.unregisterServer(serverGuid);
      
      state = await broker.getRegistryState();
      expect(state.servers).toHaveLength(0);
      expect(state.portAllocations.has(9002)).toBe(false);
    });

    it('should handle unregistering unknown server gracefully', async () => {
      const unknownGuid = generateUUID();
      
      // Should not throw
      await expect(broker.unregisterServer(unknownGuid)).resolves.toBeUndefined();
    });
  });

  describe('Server Discovery', () => {
    let server1Guid: string;
    let server2Guid: string;

    beforeEach(async () => {
      const server1 = createTestServer({ 
        name: 'server-1', 
        port: 9001, 
        tags: ['test', 'websocket'] 
      });
      const server2 = createTestServer({ 
        name: 'server-2', 
        port: 9002, 
        protocol: 'websocket',
        tags: ['production', 'websocket'] 
      });

      server1Guid = await broker.registerServer(server1);
      server2Guid = await broker.registerServer(server2);
      
      // Mark servers as ready
      const state = await broker.getRegistryState();
      state.servers.forEach(server => {
        if (server instanceof Object && 'status' in server) {
          (server as any).status = 'ready';
        }
      });
    });

    it('should find all servers when no selector provided', async () => {
      const servers = await broker.findServers();
      expect(servers).toHaveLength(2);
    });

    it('should find server by GUID', async () => {
      const servers = await broker.findServers({ guid: server1Guid });
      expect(servers).toHaveLength(1);
      expect(servers[0].guid).toBe(server1Guid);
      expect(servers[0].name).toBe('server-1');
    });

    it('should find server by name', async () => {
      const servers = await broker.findServers({ name: 'server-2' });
      expect(servers).toHaveLength(1);
      expect(servers[0].guid).toBe(server2Guid);
      expect(servers[0].port).toBe(9002);
    });

    it('should find server by port', async () => {
      const servers = await broker.findServers({ port: 9001 });
      expect(servers).toHaveLength(1);
      expect(servers[0].guid).toBe(server1Guid);
    });

    it('should find servers by tags', async () => {
      const servers = await broker.findServers({ tags: ['websocket'] });
      expect(servers).toHaveLength(2); // Both servers have 'websocket' tag
      
      const testServers = await broker.findServers({ tags: ['test'] });
      expect(testServers).toHaveLength(1);
      expect(testServers[0].name).toBe('server-1');
      
      const prodServers = await broker.findServers({ tags: ['production'] });
      expect(prodServers).toHaveLength(1);
      expect(prodServers[0].name).toBe('server-2');
    });

    it('should return empty array for non-matching selectors', async () => {
      const servers = await broker.findServers({ name: 'nonexistent' });
      expect(servers).toHaveLength(0);
      
      const tagServers = await broker.findServers({ tags: ['nonexistent'] });
      expect(tagServers).toHaveLength(0);
    });
  });

  describe('Connection Establishment', () => {
    it('should create new connection when no servers exist', async () => {
      const params = createTestConnectionParams({
        mode: 'shared',
        protocols: ['websocket']
      });

      const result = await broker.connect(params);
      
      expect(result).toBeDefined();
      expect(result.strategy).toBe('created_new');
      expect(result.server.port).toBeGreaterThanOrEqual(9001);
      expect(result.server.port).toBeLessThanOrEqual(9010);
      expect(result.transport).toBeDefined();
      expect(result.metadata.protocolUsed).toBe('websocket');
      expect(result.metadata.usedFallback).toBe(false);
    });

    it('should reuse existing compatible server in shared mode', async () => {
      // First, create a server
      const serverData = createTestServer({ 
        name: 'reusable-server',
        port: 9003,
        environment: 'server'
      });
      const serverGuid = await broker.registerServer(serverData);
      
      // Mark server as ready
      const state = await broker.getRegistryState();
      const server = state.servers.find(s => s.guid === serverGuid);
      if (server) {
        (server as any).status = 'ready';
      }

      const params = createTestConnectionParams({
        mode: 'shared',
        targetEnvironment: 'server',
        protocols: ['websocket']
      });

      const result = await broker.connect(params);
      
      expect(result.strategy).toBe('reused_existing');
      expect(result.server.guid).toBe(serverGuid);
      expect(result.server.connectionCount).toBe(1);
      expect(result.metadata.diagnostics?.serverReused).toBe(true);
    });

    it('should create new server in isolated mode even when compatible server exists', async () => {
      // Register existing server
      const existingServer = createTestServer({ port: 9003 });
      await broker.registerServer(existingServer);

      const params = createTestConnectionParams({
        mode: 'isolated',
        protocols: ['websocket']
      });

      const result = await broker.connect(params);
      
      expect(result.strategy).toBe('created_new');
      expect(result.server.port).not.toBe(9003); // Should allocate different port
    });

    it('should fail in required mode when no compatible server exists', async () => {
      const params = createTestConnectionParams({
        mode: 'required',
        protocols: ['websocket']
      });

      await expect(broker.connect(params)).rejects.toThrow(ConnectionBrokerError);
    });

    it('should apply fallback protocol when primary fails', async () => {
      const params = createTestConnectionParams({
        protocols: ['udp-multicast', 'websocket'], // Primary will "fail", fallback to websocket
        mode: 'isolated',
        enableFallback: true
      });

      // Mock the first protocol to fail
      const originalCreate = mockFactory.createTransport;
      mockFactory.createTransport = jest.fn().mockImplementation((env, config) => {
        if (config.protocol === 'udp-multicast') {
          throw new Error('UDP multicast not available');
        }
        return originalCreate.call(mockFactory, env, config);
      });

      const result = await broker.connect(params);
      
      expect(result.strategy).toBe('fallback_protocol');
      expect(result.metadata.protocolUsed).toBe('websocket');
      expect(result.metadata.usedFallback).toBe(true);
    });
  });

  describe('Port Allocation', () => {
    it('should allocate ports sequentially by default', async () => {
      const params1 = createTestConnectionParams({ mode: 'isolated' });
      const params2 = createTestConnectionParams({ mode: 'isolated' });
      const params3 = createTestConnectionParams({ mode: 'isolated' });

      const result1 = await broker.connect(params1);
      const result2 = await broker.connect(params2);
      const result3 = await broker.connect(params3);

      expect(result1.server.port).toBe(9001);
      expect(result2.server.port).toBe(9002);
      expect(result3.server.port).toBe(9003);
    });

    it('should skip reserved ports during allocation', async () => {
      // Port 9005 is reserved in test config
      const params: ConnectionParams[] = [];
      for (let i = 0; i < 6; i++) {
        params.push(createTestConnectionParams({ mode: 'isolated' }));
      }

      const results = await Promise.all(params.map(p => broker.connect(p)));
      const ports = results.map(r => r.server.port);

      expect(ports).toEqual([9001, 9002, 9003, 9004, 9006, 9007]);
      expect(ports).not.toContain(9005); // Reserved port should be skipped
    });

    it('should throw error when no ports available', async () => {
      // Try to allocate more ports than available (9001-9010, minus 9005 reserved = 9 available)
      const promises: Promise<any>[] = [];
      for (let i = 0; i < 11; i++) { // Request more than available
        promises.push(broker.connect(createTestConnectionParams({ mode: 'isolated' })));
      }

      // First 9 should succeed, 10th and 11th should fail
      const results = await Promise.allSettled(promises);
      
      const successful = results.filter(r => r.status === 'fulfilled').length;
      const failed = results.filter(r => r.status === 'rejected').length;
      
      expect(successful).toBe(9); // Available ports
      expect(failed).toBe(2);     // Excess requests
      
      // Check that failures are due to port exhaustion
      const rejectedResults = results.filter(r => r.status === 'rejected') as PromiseRejectedResult[];
      rejectedResults.forEach(result => {
        expect(result.reason).toBeInstanceOf(ConnectionBrokerError);
        expect(result.reason.code).toBe('NO_AVAILABLE_PORTS');
      });
    });
  });

  describe('Error Handling', () => {
    it('should handle transport factory errors gracefully', async () => {
      mockFactory.createTransport = jest.fn().mockRejectedValue(new Error('Transport creation failed'));

      const params = createTestConnectionParams();
      
      await expect(broker.connect(params)).rejects.toThrow('Transport creation failed');
    });

    it('should retry on transient failures', async () => {
      let attemptCount = 0;
      mockFactory.createTransport = jest.fn().mockImplementation(() => {
        attemptCount++;
        if (attemptCount < 3) {
          throw new Error('Transient failure');
        }
        return Promise.resolve({
          send: jest.fn().mockResolvedValue({ success: true, timestamp: new Date().toISOString() }),
          disconnect: jest.fn().mockResolvedValue(undefined),
          isConnected: jest.fn().mockReturnValue(true),
          getConnectionInfo: jest.fn().mockReturnValue({ protocol: 'websocket', isConnected: true, connectionTime: Date.now() })
        });
      });

      const params = createTestConnectionParams({ maxRetries: 5 });
      
      const result = await broker.connect(params);
      
      expect(result).toBeDefined();
      expect(result.metadata.retryAttempts).toBe(2); // Failed twice, succeeded on third attempt
      expect(attemptCount).toBe(3);
    });

    it('should fail after max retries exceeded', async () => {
      mockFactory.createTransport = jest.fn().mockRejectedValue(new Error('Persistent failure'));

      const params = createTestConnectionParams({ maxRetries: 2 });
      
      await expect(broker.connect(params)).rejects.toThrow();
    });
  });

  describe('Statistics Tracking', () => {
    it('should track connection statistics correctly', async () => {
      // Create some connections
      await broker.connect(createTestConnectionParams({ mode: 'isolated' }));
      await broker.connect(createTestConnectionParams({ mode: 'isolated' }));
      
      // Create reusable server and connect to it twice
      const serverData = createTestServer({ port: 9050, name: 'stats-server' });
      const serverGuid = await broker.registerServer(serverData);
      const state = await broker.getRegistryState();
      const server = state.servers.find(s => s.guid === serverGuid);
      if (server) {
        (server as any).status = 'ready';
      }

      await broker.connect(createTestConnectionParams({ mode: 'shared' }));
      await broker.connect(createTestConnectionParams({ mode: 'shared' }));

      const finalState = await broker.getRegistryState();
      const stats = finalState.statistics;
      
      expect(stats.totalConnections).toBe(4);
      expect(stats.successfulConnections).toBe(4);
      expect(stats.reuseRate).toBeCloseTo(0.5, 1); // 2 reused out of 4 total
      expect(stats.avgConnectionTime).toBeGreaterThan(0);
      expect(stats.portUtilization.allocated).toBeGreaterThan(0);
    });
  });

  describe('Registry Cleanup', () => {
    it('should clean up stopped servers', async () => {
      const serverData = createTestServer({ port: 9008 });
      const serverGuid = await broker.registerServer(serverData);
      
      let state = await broker.getRegistryState();
      expect(state.servers).toHaveLength(1);
      
      // Mark server as stopped
      const server = state.servers.find(s => s.guid === serverGuid);
      if (server) {
        (server as any).status = 'stopped';
      }
      
      await broker.cleanup();
      
      state = await broker.getRegistryState();
      expect(state.servers).toHaveLength(0);
    });

    it('should clean up stale servers based on age', async () => {
      // Create broker with very short max age for testing
      const shortAgeConfig = {
        ...testConfig,
        registry: {
          ...testConfig.registry!,
          maxEntryAge: 100 // 100ms
        }
      };
      
      const testBroker = new ConnectionBroker(shortAgeConfig, mockFactory);
      
      const serverData = createTestServer({ port: 9009 });
      await testBroker.registerServer(serverData);
      
      let state = await testBroker.getRegistryState();
      expect(state.servers).toHaveLength(1);
      
      // Wait for server to become stale
      await new Promise(resolve => setTimeout(resolve, 150));
      
      await testBroker.cleanup();
      
      state = await testBroker.getRegistryState();
      expect(state.servers).toHaveLength(0);
      
      await testBroker.shutdown();
    });
  });

  describe('Configuration Management', () => {
    it('should use default configuration when not provided', () => {
      const defaultBroker = new ConnectionBroker({}, mockFactory);
      expect(defaultBroker).toBeDefined();
      // Test that it doesn't throw during construction
    });

    it('should merge partial configuration with defaults', async () => {
      const partialConfig = {
        portPool: {
          startPort: 8000,
          endPort: 8010,
          reservedPorts: [],
          allocationStrategy: 'random' as const
        }
      };
      
      const customBroker = new ConnectionBroker(partialConfig, mockFactory);
      
      // Test that custom config is used
      const result = await customBroker.connect(createTestConnectionParams({ mode: 'isolated' }));
      expect(result.server.port).toBeGreaterThanOrEqual(8000);
      expect(result.server.port).toBeLessThanOrEqual(8010);
      
      await customBroker.shutdown();
    });
  });
});