/**
 * DynamicTransportStrategy Unit Tests
 * 
 * Tests the dynamic transport management capabilities
 */

import { DynamicTransportStrategy } from '../../system/core/router/shared/DynamicTransportStrategy';
import { TRANSPORT_TYPES } from '../../system/transports';
import type { JTAGTransport, JTAGContext } from '../../system/core/types/JTAGTypes';

// Mock transport for testing
class MockTransport implements Partial<JTAGTransport> {
  public name = 'mock-transport';
  private connected = true;
  
  isConnected(): boolean {
    return this.connected;
  }
  
  setConnected(status: boolean): void {
    this.connected = status;
  }
  
  setMessageHandler(handler: (message: any) => void): void {
    // Mock implementation
  }
  
  async disconnect(): Promise<void> {
    this.connected = false;
  }
}

// Mock transport factory
class MockTransportFactory {
  async createTransport(environment: string, config: any): Promise<JTAGTransport> {
    return new MockTransport() as JTAGTransport;
  }
}

describe('DynamicTransportStrategy', () => {
  let strategy: DynamicTransportStrategy;
  let transports: Map<TRANSPORT_TYPES, JTAGTransport>;
  let mockContext: JTAGContext;
  
  beforeEach(() => {
    transports = new Map();
    strategy = new DynamicTransportStrategy(transports, true);
    mockContext = {
      environment: 'browser' as any,
      uuid: 'test-uuid'
    };
  });

  describe('initialization', () => {
    it('should initialize with cross-context transport', async () => {
      const factory = new MockTransportFactory();
      const config = {
        protocol: 'websocket' as any,
        role: 'client' as any,
        sessionId: 'test-session'
      };
      
      await strategy.initializeTransports(factory as any, mockContext, config as any);
      
      expect(transports.has(TRANSPORT_TYPES.CROSS_CONTEXT)).toBe(true);
      expect(transports.size).toBeGreaterThanOrEqual(1);
    });

    it('should handle P2P transport gracefully when unavailable', async () => {
      const factory = new MockTransportFactory();
      const config = {
        protocol: 'websocket' as any,
        role: 'client' as any,
        sessionId: 'test-session'
      };
      
      // Should not throw even if P2P is not available
      await expect(strategy.initializeTransports(factory as any, mockContext, config as any))
        .resolves.not.toThrow();
    });
  });

  describe('transport access', () => {
    beforeEach(async () => {
      const factory = new MockTransportFactory();
      const config = {
        protocol: 'websocket' as any,
        role: 'client' as any,
        sessionId: 'test-session'
      };
      
      await strategy.initializeTransports(factory as any, mockContext, config as any);
    });

    it('should return cross-context transport when available', () => {
      const transport = strategy.getCrossContextTransport();
      expect(transport).toBeDefined();
      expect(transport?.name).toBe('mock-transport');
    });

    it('should return undefined for P2P transport when not initialized', () => {
      const transport = strategy.getP2PTransport();
      expect(transport).toBeUndefined();
    });
  });

  describe('status reporting', () => {
    it('should provide comprehensive transport status', async () => {
      const factory = new MockTransportFactory();
      const config = {
        protocol: 'websocket' as any,
        role: 'client' as any,
        sessionId: 'test-session'
      };
      
      await strategy.initializeTransports(factory as any, mockContext, config as any);
      
      const status = strategy.getTransportStatusInfo();
      
      expect(status.initialized).toBe(true);
      expect(status.transportCount).toBeGreaterThan(0);
      expect(status.transports).toBeInstanceOf(Array);
      expect(status).toHaveProperty('p2pEnabled');
      expect(status).toHaveProperty('discovery');
    });
  });

  describe('message handling', () => {
    it('should setup message handlers without errors', async () => {
      const factory = new MockTransportFactory();
      const config = {
        protocol: 'websocket' as any,
        role: 'client' as any,
        sessionId: 'test-session'
      };
      
      await strategy.initializeTransports(factory as any, mockContext, config as any);
      
      const mockHandler = jest.fn();
      
      await expect(strategy.setupMessageHandlers(mockHandler))
        .resolves.not.toThrow();
    });
  });

  describe('shutdown', () => {
    it('should shutdown all transports cleanly', async () => {
      const factory = new MockTransportFactory();
      const config = {
        protocol: 'websocket' as any,
        role: 'client' as any,
        sessionId: 'test-session'
      };
      
      await strategy.initializeTransports(factory as any, mockContext, config as any);
      
      await strategy.shutdownAllTransports();
      
      expect(transports.size).toBe(0);
    });
  });
});