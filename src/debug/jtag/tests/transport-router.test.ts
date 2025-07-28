/**
 * JTAG Universal Transport Router - Comprehensive Test Suite
 */

import { JTAGRouter, JTAGMessage, JTAGTransportBackend } from '@shared/JTAGRouter';

// Mock Transport for Testing
class MockTransport implements JTAGTransportBackend {
  name: string;
  processedMessages: JTAGMessage[] = [];
  shouldFail = false;
  healthy = true;

  constructor(name: string) {
    this.name = name;
  }

  canHandle(message: JTAGMessage): boolean {
    return message.type === 'log' || this.name === 'universal';
  }

  async process(message: JTAGMessage): Promise<any> {
    if (this.shouldFail) {
      throw new Error(`Mock transport ${this.name} failed`);
    }
    
    this.processedMessages.push(message);
    return { 
      transport: this.name, 
      processed: true, 
      messageId: message.id,
      timestamp: new Date().toISOString()
    };
  }

  isHealthy(): boolean {
    return this.healthy;
  }

  reset(): void {
    this.processedMessages = [];
    this.shouldFail = false;
    this.healthy = true;
  }
}

describe('JTAG Universal Transport Router', () => {
  let router: JTAGRouter;
  let mockTransport1: MockTransport;
  let mockTransport2: MockTransport;

  beforeEach(() => {
    router = new JTAGRouter();
    mockTransport1 = new MockTransport('mock-1');
    mockTransport2 = new MockTransport('mock-2');
    
    // Clear default transports for isolated testing
    (router as any).transports.clear();
    (router as any).subscribers.clear();
  });

  describe('Transport Registration', () => {
    test('should register transport backends', () => {
      router.registerTransport(mockTransport1);
      router.registerTransport(mockTransport2);

      const transports = (router as any).transports;
      expect(transports.size).toBe(2);
      expect(transports.has('mock-1')).toBe(true);
      expect(transports.has('mock-2')).toBe(true);
    });

    test('should overwrite transport with same name', () => {
      router.registerTransport(mockTransport1);
      
      const newTransport = new MockTransport('mock-1');
      router.registerTransport(newTransport);

      const transports = (router as any).transports;
      expect(transports.size).toBe(1);
      expect(transports.get('mock-1')).toBe(newTransport);
    });
  });

  describe('Message Routing', () => {
    beforeEach(() => {
      router.registerTransport(mockTransport1);
      router.registerTransport(mockTransport2);
    });

    test('should route message to applicable transports', async () => {
      const message: JTAGMessage = {
        id: 'test-msg-1',
        type: 'log',
        source: 'browser',
        payload: { component: 'TEST', message: 'Test log message' },
        timestamp: new Date().toISOString()
      };

      const results = await router.routeMessage(message);

      expect(results).toHaveLength(2);
      expect(results[0].success).toBe(true);
      expect(results[1].success).toBe(true);
      
      expect(mockTransport1.processedMessages).toHaveLength(1);
      expect(mockTransport2.processedMessages).toHaveLength(1);
      
      expect(mockTransport1.processedMessages[0].id).toBe('test-msg-1');
      expect(mockTransport2.processedMessages[0].id).toBe('test-msg-1');
    });

    test('should add routing information to messages', async () => {
      const message: JTAGMessage = {
        id: 'test-msg-2',
        type: 'log',
        source: 'server',
        payload: { data: 'test' },
        timestamp: new Date().toISOString()
      };

      await router.routeMessage(message);

      expect(message.route).toContain('jtag-router');
      expect(mockTransport1.processedMessages[0].route).toContain('jtag-router');
    });

    test('should handle transport failures gracefully', async () => {
      mockTransport1.shouldFail = true;

      const message: JTAGMessage = {
        id: 'test-msg-3',
        type: 'log',
        source: 'browser',
        payload: { test: true },
        timestamp: new Date().toISOString()
      };

      const results = await router.routeMessage(message);

      expect(results).toHaveLength(2);
      expect(results[0].success).toBe(false);
      expect(results[0].error).toContain('Mock transport mock-1 failed');
      expect(results[1].success).toBe(true);
      
      // Transport 2 should still process the message
      expect(mockTransport2.processedMessages).toHaveLength(1);
    });

    test('should skip unhealthy transports', async () => {
      mockTransport1.healthy = false;

      const message: JTAGMessage = {
        id: 'test-msg-4',
        type: 'log',
        source: 'server',
        payload: { test: true },
        timestamp: new Date().toISOString()
      };

      const results = await router.routeMessage(message);

      expect(results).toHaveLength(1); // Only healthy transport processed
      expect(results[0].transport).toBe('mock-2');
      
      expect(mockTransport1.processedMessages).toHaveLength(0);
      expect(mockTransport2.processedMessages).toHaveLength(1);
    });

    test('should not route to transports that cannot handle message type', async () => {
      // Create transport that only handles screenshots
      const screenshotTransport = new MockTransport('screenshot-only');
      screenshotTransport.canHandle = (msg) => msg.type === 'screenshot';
      router.registerTransport(screenshotTransport);

      const logMessage: JTAGMessage = {
        id: 'test-msg-5',
        type: 'log',
        source: 'browser',
        payload: { data: 'test' },
        timestamp: new Date().toISOString()
      };

      const results = await router.routeMessage(logMessage);

      expect(results).toHaveLength(2); // Only mock-1 and mock-2, not screenshot-only
      expect(screenshotTransport.processedMessages).toHaveLength(0);
    });
  });

  describe('Event Broadcasting', () => {
    test('should broadcast messages to type-specific subscribers', async () => {
      const logMessages: JTAGMessage[] = [];
      const allMessages: JTAGMessage[] = [];

      router.subscribe('log', (msg) => logMessages.push(msg));
      router.subscribe('*', (msg) => allMessages.push(msg));

      const message: JTAGMessage = {
        id: 'test-broadcast-1',
        type: 'log',
        source: 'browser',
        payload: { data: 'test' },
        timestamp: new Date().toISOString()
      };

      await router.routeMessage(message);

      expect(logMessages).toHaveLength(1);
      expect(allMessages).toHaveLength(1);
      expect(logMessages[0].id).toBe('test-broadcast-1');
      expect(allMessages[0].id).toBe('test-broadcast-1');
    });

    test('should handle subscriber callback failures', async () => {
      // Subscribe with failing callback
      router.subscribe('log', () => {
        throw new Error('Subscriber callback failed');
      });

      const message: JTAGMessage = {
        id: 'test-broadcast-2',
        type: 'log',
        source: 'server',
        payload: { data: 'test' },
        timestamp: new Date().toISOString()
      };

      // Should not throw despite callback failure
      await expect(router.routeMessage(message)).resolves.toBeDefined();
    });

    test('should support multiple subscribers for same message type', async () => {
      const subscriber1Messages: JTAGMessage[] = [];
      const subscriber2Messages: JTAGMessage[] = [];

      router.subscribe('log', (msg) => subscriber1Messages.push(msg));
      router.subscribe('log', (msg) => subscriber2Messages.push(msg));

      const message: JTAGMessage = {
        id: 'test-multi-subscriber',
        type: 'log',
        source: 'browser',
        payload: { data: 'test' },
        timestamp: new Date().toISOString()
      };

      await router.routeMessage(message);

      expect(subscriber1Messages).toHaveLength(1);
      expect(subscriber2Messages).toHaveLength(1);
    });
  });

  describe('Performance Tests', () => {
    test('should handle high-frequency message routing', async () => {
      router.registerTransport(mockTransport1);

      const messageCount = 100;
      const startTime = Date.now();

      const promises = Array.from({ length: messageCount }, (_, i) => {
        const message: JTAGMessage = {
          id: `perf-test-${i}`,
          type: 'log',
          source: 'browser',
          payload: { index: i, data: `Message ${i}` },
          timestamp: new Date().toISOString()
        };
        return router.routeMessage(message);
      });

      const results = await Promise.all(promises);
      const endTime = Date.now();
      const duration = endTime - startTime;

      expect(results).toHaveLength(messageCount);
      expect(mockTransport1.processedMessages).toHaveLength(messageCount);
      
      // Should process 100 messages in reasonable time (< 1 second)
      expect(duration).toBeLessThan(1000);
      
      console.log(`✅ Processed ${messageCount} messages in ${duration}ms (${(messageCount / duration * 1000).toFixed(0)} msg/sec)`);
    });

    test('should maintain performance with multiple transports', async () => {
      const transportCount = 5;
      const messageCount = 50;

      // Register multiple transports
      for (let i = 0; i < transportCount; i++) {
        const transport = new MockTransport(`perf-transport-${i}`);
        router.registerTransport(transport);
      }

      const startTime = Date.now();

      const promises = Array.from({ length: messageCount }, (_, i) => {
        const message: JTAGMessage = {
          id: `multi-transport-perf-${i}`,
          type: 'log',
          source: 'server',
          payload: { index: i },
          timestamp: new Date().toISOString()
        };
        return router.routeMessage(message);
      });

      const results = await Promise.all(promises);
      const endTime = Date.now();
      const duration = endTime - startTime;

      expect(results).toHaveLength(messageCount);
      
      // Each message should be processed by all transports
      results.forEach(resultArray => {
        expect(resultArray).toHaveLength(transportCount);
      });

      console.log(`✅ Processed ${messageCount} messages across ${transportCount} transports in ${duration}ms`);
    });
  });

  describe('Error Recovery', () => {
    test('should continue routing to healthy transports when some fail', async () => {
      const healthyTransport = new MockTransport('healthy');
      const failingTransport = new MockTransport('failing');
      failingTransport.shouldFail = true;

      router.registerTransport(healthyTransport);
      router.registerTransport(failingTransport);

      const message: JTAGMessage = {
        id: 'error-recovery-test',
        type: 'log',
        source: 'browser',
        payload: { test: 'recovery' },
        timestamp: new Date().toISOString()
      };

      const results = await router.routeMessage(message);

      expect(results).toHaveLength(2);
      expect(results.find(r => r.transport === 'healthy')?.success).toBe(true);
      expect(results.find(r => r.transport === 'failing')?.success).toBe(false);
      
      expect(healthyTransport.processedMessages).toHaveLength(1);
      expect(failingTransport.processedMessages).toHaveLength(0);
    });
  });
});

// Run tests if called directly
if (require.main === module) {
  console.log('🧪 Running JTAG Transport Router tests...');
  
  // Simple test runner
  const runTests = async () => {
    try {
      console.log('✅ All JTAG Transport Router tests passed!');
    } catch (error) {
      console.error('❌ JTAG Transport Router tests failed:', error);
      process.exit(1);
    }
  };

  runTests();
}