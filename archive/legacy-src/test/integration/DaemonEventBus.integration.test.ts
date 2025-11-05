/**
 * Integration tests for DaemonEventBus
 * Tests inter-daemon communication, event propagation, and type safety
 */

import { describe, it, before, after, beforeEach } from 'node:test';
import assert from 'node:assert';
import { DAEMON_EVENT_BUS, DaemonEventBus } from '../../daemons/base/DaemonEventBus';
import { BaseDaemon } from '../../daemons/base/BaseDaemon';

// Test daemon that listens and emits events
class TestDaemon extends BaseDaemon {
  public receivedEvents: Array<{ event: string; data: any }> = [];
  readonly name: string;
  readonly version: string = '1.0.0';
  
  constructor(name: string) {
    super();
    this.name = name;
  }
  
  async onStart(): Promise<void> {
    // Subscribe to test events
    DAEMON_EVENT_BUS.onEvent('session_created', (data) => {
      this.receivedEvents.push({ event: 'session_created', data });
    });
    
    DAEMON_EVENT_BUS.onEvent('session_joined', (data) => {
      this.receivedEvents.push({ event: 'session_joined', data });
    });
    
    DAEMON_EVENT_BUS.onEvent('browser_launched', (data) => {
      this.receivedEvents.push({ event: 'browser_launched', data });
    });
    
    DAEMON_EVENT_BUS.onEvent('browser_connected', (data) => {
      this.receivedEvents.push({ event: 'browser_connected', data });
    });
  }
  
  async onStop(): Promise<void> {
    // Cleanup
  }
  
  async handleMessage(message: any): Promise<any> {
    return { success: true };
  }
}

describe('DaemonEventBus Integration Tests', () => {
  let testDaemon1: TestDaemon;
  let testDaemon2: TestDaemon;
  let testDaemon3: TestDaemon;
  
  before(async () => {
    // Create test daemons
    testDaemon1 = new TestDaemon('test-daemon-1');
    testDaemon2 = new TestDaemon('test-daemon-2');
    testDaemon3 = new TestDaemon('test-daemon-3');
    
    // Start daemons to register event listeners
    await testDaemon1.start();
    await testDaemon2.start();
    await testDaemon3.start();
  });
  
  after(async () => {
    // Stop daemons
    await testDaemon1.stop();
    await testDaemon2.stop();
    await testDaemon3.stop();
  });
  
  beforeEach(() => {
    // Clear received events
    testDaemon1.receivedEvents = [];
    testDaemon2.receivedEvents = [];
    testDaemon3.receivedEvents = [];
  });
  
  describe('Event Propagation', () => {
    it('should propagate events to all listening daemons', async () => {
      // Emit session_created event
      DAEMON_EVENT_BUS.emitEvent('session_created', {
        sessionId: 'test-123',
        sessionType: 'development',
        owner: 'test-user',
        serverLogPath: '/tmp/test.log'
      });
      
      // Give time for event to propagate
      await new Promise(resolve => setTimeout(resolve, 10));
      
      // All daemons should receive the event
      assert.strictEqual(testDaemon1.receivedEvents.length, 1);
      assert.strictEqual(testDaemon1.receivedEvents[0].event, 'session_created');
      assert.strictEqual(testDaemon1.receivedEvents[0].data.sessionId, 'test-123');
      
      assert.strictEqual(testDaemon2.receivedEvents.length, 1);
      assert.strictEqual(testDaemon2.receivedEvents[0].event, 'session_created');
      assert.strictEqual(testDaemon2.receivedEvents[0].data.sessionId, 'test-123');
      
      assert.strictEqual(testDaemon3.receivedEvents.length, 1);
      assert.strictEqual(testDaemon3.receivedEvents[0].event, 'session_created');
      assert.strictEqual(testDaemon3.receivedEvents[0].data.sessionId, 'test-123');
    });
    
    it('should handle multiple events in sequence', async () => {
      // Emit multiple events
      DAEMON_EVENT_BUS.emitEvent('session_created', {
        sessionId: 'session-1',
        sessionType: 'development',
        owner: 'user-1'
      });
      
      DAEMON_EVENT_BUS.emitEvent('session_joined', {
        sessionId: 'session-1',
        sessionType: 'development',
        owner: 'user-1',
        source: 'browser'
      });
      
      DAEMON_EVENT_BUS.emitEvent('browser_launched', {
        sessionId: 'session-1',
        browserPid: 12345,
        url: 'http://localhost:9000'
      });
      
      // Give time for events to propagate
      await new Promise(resolve => setTimeout(resolve, 50));
      
      // All daemons should receive all events
      assert.strictEqual(testDaemon1.receivedEvents.length, 3);
      assert.strictEqual(testDaemon1.receivedEvents[0].event, 'session_created');
      assert.strictEqual(testDaemon1.receivedEvents[1].event, 'session_joined');
      assert.strictEqual(testDaemon1.receivedEvents[2].event, 'browser_launched');
    });
  });
  
  describe('Event Type Safety', () => {
    it('should maintain type safety for known events', () => {
      // This would fail TypeScript compilation if types were wrong
      DAEMON_EVENT_BUS.emitEvent('session_created', {
        sessionId: 'type-test',
        sessionType: 'development',
        owner: 'test-user',
        serverLogPath: '/logs/test.log'
      });
      
      // @ts-expect-error - Missing required field
      DAEMON_EVENT_BUS.emitEvent('session_created', {
        sessionId: 'incomplete'
        // Missing sessionType and owner
      });
    });
    
    it('should allow custom events', () => {
      // Custom events are allowed through the event bus
      DAEMON_EVENT_BUS.emitEvent('custom_event' as any, {
        customData: 'test'
      });
      
      // No assertion needed - just verifying it doesn't throw
    });
  });
  
  describe('Event Bus Isolation', () => {
    it('should use global singleton instance', () => {
      // Get instance through getInstance method
      const newBus = DaemonEventBus.getInstance();
      
      // It should be the same instance as DAEMON_EVENT_BUS
      assert.strictEqual(newBus, DAEMON_EVENT_BUS);
    });
    
    it('should handle listener removal', async () => {
      let eventCount = 0;
      const listener = () => { eventCount++; };
      
      // Add listener
      DAEMON_EVENT_BUS.onEvent('session_created', listener);
      
      // Emit event
      DAEMON_EVENT_BUS.emitEvent('session_created', {
        sessionId: 'remove-test',
        sessionType: 'test',
        owner: 'test'
      });
      
      await new Promise(resolve => setTimeout(resolve, 10));
      assert.strictEqual(eventCount, 1);
      
      // Remove listener
      DAEMON_EVENT_BUS.offEvent('session_created', listener);
      
      // Emit again
      DAEMON_EVENT_BUS.emitEvent('session_created', {
        sessionId: 'remove-test-2',
        sessionType: 'test',
        owner: 'test'
      });
      
      await new Promise(resolve => setTimeout(resolve, 10));
      assert.strictEqual(eventCount, 1); // Should not increase
    });
  });
  
  describe('Real-World Scenarios', () => {
    it('should handle session creation flow', async () => {
      // Simulate SessionManagerDaemon creating a session
      DAEMON_EVENT_BUS.emitEvent('session_created', {
        sessionId: 'real-session-123',
        sessionType: 'development',
        owner: 'developer',
        serverLogPath: '/sessions/real-session-123/server.log'
      });
      
      // BrowserManagerDaemon should receive the event
      await new Promise(resolve => setTimeout(resolve, 10));
      
      const sessionEvent = testDaemon1.receivedEvents.find(
        e => e.event === 'session_created' && e.data.sessionId === 'real-session-123'
      );
      
      assert(sessionEvent, 'Session created event should be received');
      assert.strictEqual(sessionEvent.data.owner, 'developer');
      assert(sessionEvent.data.serverLogPath.includes('real-session-123'));
    });
    
    it('should handle browser launch coordination', async () => {
      // Simulate complete browser launch flow
      
      // 1. Session created
      DAEMON_EVENT_BUS.emitEvent('session_created', {
        sessionId: 'browser-test',
        sessionType: 'development',
        owner: 'test-user'
      });
      
      // 2. Browser launched
      DAEMON_EVENT_BUS.emitEvent('browser_launched', {
        sessionId: 'browser-test',
        browserPid: 54321,
        url: 'http://localhost:9000'
      });
      
      // 3. Browser connected
      DAEMON_EVENT_BUS.emitEvent('browser_connected', {
        sessionId: 'browser-test',
        connectionId: 'ws-123',
        userAgent: 'Test Browser'
      });
      
      await new Promise(resolve => setTimeout(resolve, 50));
      
      // Verify event sequence
      const events = testDaemon1.receivedEvents;
      assert(events.some(e => e.event === 'session_created'));
      assert(events.some(e => e.event === 'browser_launched'));
      assert(events.some(e => e.event === 'browser_connected'));
    });
  });
});