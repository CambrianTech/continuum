/**
 * ConnectionManager Unit Tests - TypeScript Implementation
 */

import { ConnectionManager } from '../core/ConnectionManager';
import WebSocket from 'ws';
import { EventEmitter } from 'events';

// Mock WebSocket for testing
class MockWebSocket extends EventEmitter {
  public readyState = WebSocket.OPEN;
  public sent: string[] = [];

  send(data: string): void {
    this.sent.push(data);
  }

  ping(): void {
    this.emit('pong');
  }

  close(code?: number, reason?: string): void {
    this.readyState = WebSocket.CLOSED;
    this.emit('close', code, reason);
  }
}

describe('ConnectionManager Unit Tests', () => {
  let connectionManager: ConnectionManager;
  let mockSocket: MockWebSocket;

  beforeEach(() => {
    connectionManager = new ConnectionManager({
      maxClients: 5,
      heartbeatInterval: 1000,
      clientTimeout: 2000,
      enableHeartbeat: false // Disable for testing
    });
    mockSocket = new MockWebSocket();
  });

  afterEach(() => {
    connectionManager.shutdown();
  });

  describe('Client Management', () => {
    test('should add client and generate unique ID', () => {
      const clientId = connectionManager.addClient(mockSocket as any, { test: true });
      
      expect(clientId).toBeDefined();
      expect(typeof clientId).toBe('string');
      expect(clientId.startsWith('client_')).toBe(true);
      
      const client = connectionManager.getClient(clientId);
      expect(client).toBeDefined();
      expect(client!.id).toBe(clientId);
      expect(client!.connected).toBe(true);
      expect(client!.metadata.test).toBe(true);
    });

    test('should track connection count correctly', () => {
      expect(connectionManager.getConnectedCount()).toBe(0);
      
      const clientId1 = connectionManager.addClient(new MockWebSocket() as any);
      expect(connectionManager.getConnectedCount()).toBe(1);
      
      const clientId2 = connectionManager.addClient(new MockWebSocket() as any);
      expect(connectionManager.getConnectedCount()).toBe(2);
      
      connectionManager.removeClient(clientId1);
      expect(connectionManager.getConnectedCount()).toBe(1);
    });

    test('should enforce maximum client limit', () => {
      // Add maximum number of clients
      for (let i = 0; i < 5; i++) {
        connectionManager.addClient(new MockWebSocket() as any);
      }
      
      // Attempt to add one more should throw
      expect(() => {
        connectionManager.addClient(new MockWebSocket() as any);
      }).toThrow('Maximum client limit reached: 5');
    });

    test('should remove client and clean up', () => {
      const clientId = connectionManager.addClient(mockSocket as any);
      
      expect(connectionManager.getClient(clientId)).toBeDefined();
      
      const removed = connectionManager.removeClient(clientId);
      expect(removed).toBe(true);
      expect(connectionManager.getClient(clientId)).toBeUndefined();
      expect(connectionManager.getConnectedCount()).toBe(0);
    });

    test('should handle removing non-existent client', () => {
      const removed = connectionManager.removeClient('nonexistent');
      expect(removed).toBe(false);
    });
  });

  describe('Message Sending', () => {
    test('should send message to specific client', () => {
      const clientId = connectionManager.addClient(mockSocket as any);
      const message = { type: 'test', data: 'hello' };
      
      const sent = connectionManager.sendToClient(clientId, message);
      
      expect(sent).toBe(true);
      expect(mockSocket.sent).toHaveLength(1);
      expect(JSON.parse(mockSocket.sent[0])).toEqual(message);
    });

    test('should fail to send to disconnected client', () => {
      const clientId = connectionManager.addClient(mockSocket as any);
      mockSocket.readyState = WebSocket.CLOSED;
      
      const sent = connectionManager.sendToClient(clientId, { type: 'test' });
      
      expect(sent).toBe(false);
    });

    test('should broadcast message to all clients', () => {
      const socket1 = new MockWebSocket();
      const socket2 = new MockWebSocket();
      const socket3 = new MockWebSocket();
      
      const clientId1 = connectionManager.addClient(socket1 as any);
      const clientId2 = connectionManager.addClient(socket2 as any);
      const clientId3 = connectionManager.addClient(socket3 as any);
      
      const message = { type: 'broadcast', data: 'hello all' };
      const sentCount = connectionManager.broadcast(message);
      
      expect(sentCount).toBe(3);
      expect(socket1.sent).toHaveLength(1);
      expect(socket2.sent).toHaveLength(1);
      expect(socket3.sent).toHaveLength(1);
    });

    test('should exclude specific client from broadcast', () => {
      const socket1 = new MockWebSocket();
      const socket2 = new MockWebSocket();
      
      const clientId1 = connectionManager.addClient(socket1 as any);
      const clientId2 = connectionManager.addClient(socket2 as any);
      
      const message = { type: 'broadcast', data: 'hello' };
      const sentCount = connectionManager.broadcast(message, clientId1);
      
      expect(sentCount).toBe(1);
      expect(socket1.sent).toHaveLength(0); // Excluded
      expect(socket2.sent).toHaveLength(1); // Received
    });
  });

  describe('Client Metadata', () => {
    test('should update client metadata', () => {
      const clientId = connectionManager.addClient(mockSocket as any, { initial: true });
      
      const updated = connectionManager.updateClientMetadata(clientId, { updated: true, count: 1 });
      
      expect(updated).toBe(true);
      
      const client = connectionManager.getClient(clientId);
      expect(client!.metadata).toEqual({
        initial: true,
        updated: true,
        count: 1
      });
    });

    test('should fail to update metadata for non-existent client', () => {
      const updated = connectionManager.updateClientMetadata('nonexistent', { test: true });
      expect(updated).toBe(false);
    });
  });

  describe('Statistics', () => {
    test('should provide accurate statistics', () => {
      const start = Date.now();
      
      connectionManager.addClient(new MockWebSocket() as any);
      connectionManager.addClient(new MockWebSocket() as any);
      
      const stats = connectionManager.getStats();
      
      expect(stats.totalClients).toBe(2);
      expect(stats.maxClients).toBe(5);
      expect(stats.heartbeatEnabled).toBe(false);
      expect(stats.heartbeatInterval).toBe(1000);
      expect(stats.averageConnectionTime).toBeGreaterThanOrEqual(0);
      expect(stats.oldestConnection).toBeGreaterThanOrEqual(start);
    });

    test('should handle empty statistics', () => {
      const stats = connectionManager.getStats();
      
      expect(stats.totalClients).toBe(0);
      expect(stats.averageConnectionTime).toBe(0);
      expect(stats.oldestConnection).toBeNull();
    });
  });

  describe('Event Handling', () => {
    test('should emit client connected event', (done) => {
      connectionManager.on('client:connected', (client) => {
        expect(client).toBeDefined();
        expect(client.id).toBeDefined();
        expect(client.connected).toBe(true);
        done();
      });
      
      connectionManager.addClient(mockSocket as any);
    });

    test('should emit client disconnected event', (done) => {
      const clientId = connectionManager.addClient(mockSocket as any);
      
      connectionManager.on('client:disconnected', (client) => {
        expect(client.id).toBe(clientId);
        expect(client.connected).toBe(false);
        done();
      });
      
      connectionManager.removeClient(clientId);
    });

    test('should handle socket close event', (done) => {
      const clientId = connectionManager.addClient(mockSocket as any);
      
      connectionManager.on('client:disconnected', (client) => {
        expect(client.id).toBe(clientId);
        done();
      });
      
      mockSocket.close();
    });

    test('should handle socket error event', (done) => {
      const clientId = connectionManager.addClient(mockSocket as any);
      
      connectionManager.on('client:disconnected', (client) => {
        expect(client.id).toBe(clientId);
        done();
      });
      
      mockSocket.emit('error', new Error('Test error'));
    });
  });

  describe('Shutdown', () => {
    test('should shutdown gracefully', () => {
      const socket1 = new MockWebSocket();
      const socket2 = new MockWebSocket();
      
      connectionManager.addClient(socket1 as any);
      connectionManager.addClient(socket2 as any);
      
      expect(connectionManager.getConnectedCount()).toBe(2);
      
      connectionManager.shutdown();
      
      expect(connectionManager.getConnectedCount()).toBe(0);
      expect(socket1.readyState).toBe(WebSocket.CLOSED);
      expect(socket2.readyState).toBe(WebSocket.CLOSED);
    });
  });
});