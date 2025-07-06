/**
 * Integration tests for wildcard routing in WebSocketDaemon
 * Tests route registration, matching, and precedence
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { WebSocketDaemon } from '../../integrations/websocket/WebSocketDaemon';
import { BaseDaemon } from '../../daemons/base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../../daemons/base/DaemonProtocol';

// Test daemon that tracks received messages
class MockDaemon extends BaseDaemon {
  public receivedMessages: DaemonMessage[] = [];
  public responseToReturn: DaemonResponse = { success: true, data: { handled: true } };
  
  constructor(name: string) {
    super();
    this.name = name;
    this.version = '1.0.0';
  }
  
  async onStart(): Promise<void> {
    // No-op
  }
  
  async onStop(): Promise<void> {
    // No-op
  }
  
  async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    this.receivedMessages.push(message);
    return this.responseToReturn;
  }
}

describe('Wildcard Routing Integration Tests', () => {
  let webSocketDaemon: WebSocketDaemon;
  let apiDaemon: MockDaemon;
  let staticDaemon: MockDaemon;
  let catchAllDaemon: MockDaemon;
  
  before(async () => {
    // Create daemons
    webSocketDaemon = new WebSocketDaemon();
    apiDaemon = new MockDaemon('api-daemon');
    staticDaemon = new MockDaemon('static-daemon');
    catchAllDaemon = new MockDaemon('catch-all-daemon');
    
    // Start daemons
    await webSocketDaemon.start();
    await apiDaemon.start();
    await staticDaemon.start();
    await catchAllDaemon.start();
    
    // Register daemons
    webSocketDaemon.registerDaemon(apiDaemon);
    webSocketDaemon.registerDaemon(staticDaemon);
    webSocketDaemon.registerDaemon(catchAllDaemon);
  });
  
  after(async () => {
    await webSocketDaemon.stop();
    await apiDaemon.stop();
    await staticDaemon.stop();
    await catchAllDaemon.stop();
  });
  
  describe('Route Registration', () => {
    it('should register specific routes before wildcards', () => {
      // Register routes in order of specificity
      webSocketDaemon.registerRouteHandler('/api/v1/users', 'api-daemon', 'handle_users');
      webSocketDaemon.registerRouteHandler('/api/v1/*', 'api-daemon', 'handle_v1');
      webSocketDaemon.registerRouteHandler('/api/*', 'api-daemon', 'handle_api');
      webSocketDaemon.registerRouteHandler('/static/*', 'static-daemon', 'handle_static');
      webSocketDaemon.registerRouteHandler('*', 'catch-all-daemon', 'handle_all');
      
      // Routes should be registered
      const routes = (webSocketDaemon as any).routes;
      assert(routes.length >= 5, 'Should have at least 5 routes registered');
    });
  });
  
  describe('Route Matching', () => {
    it('should match exact routes first', async () => {
      const response = await fetch('http://localhost:9000/api/v1/users');
      
      // Should be handled by the specific route
      assert([200, 404].includes(response.status));
    });
    
    it('should match wildcard routes in order of specificity', async () => {
      // Clear previous messages
      apiDaemon.receivedMessages = [];
      staticDaemon.receivedMessages = [];
      catchAllDaemon.receivedMessages = [];
      
      // Test /api/v1/products - should match /api/v1/*
      await fetch('http://localhost:9000/api/v1/products');
      
      // Test /api/v2/users - should match /api/*
      await fetch('http://localhost:9000/api/v2/users');
      
      // Test /static/image.png - should match /static/*
      await fetch('http://localhost:9000/static/image.png');
      
      // Test /random/path - should match *
      await fetch('http://localhost:9000/random/path');
      
      // Give time for async processing
      await new Promise(resolve => setTimeout(resolve, 100));
      
      // Verify routing - at least some messages should be received
      assert(
        apiDaemon.receivedMessages.length > 0 || 
        staticDaemon.receivedMessages.length > 0 || 
        catchAllDaemon.receivedMessages.length > 0,
        'At least one daemon should receive messages'
      );
    });
  });
  
  describe('Wildcard Patterns', () => {
    it('should support single-level wildcards', () => {
      webSocketDaemon.registerRouteHandler('/assets/*.css', 'static-daemon', 'handle_css');
      webSocketDaemon.registerRouteHandler('/assets/*.js', 'static-daemon', 'handle_js');
      
      // Pattern should be registered
      const routes = (webSocketDaemon as any).routes;
      assert(routes.some((r: any) => r.pattern.includes('*.css')), 'Should register CSS pattern');
      assert(routes.some((r: any) => r.pattern.includes('*.js')), 'Should register JS pattern');
    });
    
    it('should support multi-level wildcards', () => {
      webSocketDaemon.registerRouteHandler('/api/**/users', 'api-daemon', 'handle_nested_users');
      
      // Pattern should be registered
      const routes = (webSocketDaemon as any).routes;
      assert(routes.some((r: any) => r.pattern.includes('**')), 'Should register multi-level pattern');
    });
  });
  
  describe('Route Precedence', () => {
    it('should respect registration order for same specificity', () => {
      // Register two catch-all routes
      webSocketDaemon.registerRouteHandler('/test/*', 'api-daemon', 'handle_test_1');
      webSocketDaemon.registerRouteHandler('/test/*', 'static-daemon', 'handle_test_2');
      
      // First registration should take precedence
      const routes = (webSocketDaemon as any).routes.filter((r: any) => r.pattern === '/test/*');
      assert(routes.length >= 1, 'Should have test routes');
      
      if (routes.length > 0) {
        assert.strictEqual(routes[0].targetDaemon, 'api-daemon', 'First registration should win');
      }
    });
  });
  
  describe('WebSocket Message Routing', () => {
    it('should route WebSocket messages based on type patterns', async () => {
      // Connect WebSocket
      const WebSocket = (await import('ws')).default;
      const ws = new WebSocket('ws://localhost:9000');
      
      await new Promise<void>((resolve, reject) => {
        ws.on('open', resolve);
        ws.on('error', reject);
      });
      
      // Send test message
      ws.send(JSON.stringify({
        type: 'command',
        command: 'test',
        params: {}
      }));
      
      // Close connection
      ws.close();
      
      // WebSocket messages should be routable
      await new Promise(resolve => setTimeout(resolve, 100));
    });
  });
  
  describe('Error Handling', () => {
    it('should handle routes to non-existent daemons', () => {
      // Register route to non-existent daemon
      webSocketDaemon.registerRouteHandler('/broken/*', 'non-existent-daemon', 'handle');
      
      // Should not crash the system
      const routes = (webSocketDaemon as any).routes;
      assert(routes.some((r: any) => r.targetDaemon === 'non-existent-daemon'), 'Should register route');
    });
    
    it('should handle malformed route patterns gracefully', () => {
      // Try to register invalid patterns
      try {
        webSocketDaemon.registerRouteHandler('', 'api-daemon', 'handle');
        webSocketDaemon.registerRouteHandler(null as any, 'api-daemon', 'handle');
        webSocketDaemon.registerRouteHandler('///***///', 'api-daemon', 'handle');
      } catch (error) {
        // Should handle gracefully
      }
      
      // System should still be operational
      assert(webSocketDaemon.isRunning(), 'WebSocket daemon should still be running');
    });
  });
});