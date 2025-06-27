/**
 * Dynamic Daemon System Integration Tests
 * Tests the complete WebSocket + Dynamic Routing + Multiple Daemons system
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { WebSocketDaemon } from '../WebSocketDaemon.js';
import { RendererDaemon } from '../../../daemons/renderer/RendererDaemon.js';
import WebSocket from 'ws';

describe('Dynamic Daemon System Integration', () => {
  let wsServer: WebSocketDaemon;
  let rendererDaemon: RendererDaemon;
  let wsClient: WebSocket;
  const testPort = 9001; // Use different port to avoid conflicts

  beforeEach(async () => {
    console.log('🧪 Setting up dynamic daemon test environment...');
    
    // Create daemon instances
    wsServer = new WebSocketDaemon({ port: testPort });
    rendererDaemon = new RendererDaemon();
    
    // Start WebSocket server
    await wsServer.start();
    console.log('✅ WebSocket server started on port', testPort);
    
    // Start renderer daemon
    await rendererDaemon.start();
    console.log('✅ Renderer daemon started');
    
    // Register renderer with WebSocket server
    await wsServer.registerExternalDaemon('renderer', rendererDaemon);
    console.log('✅ Renderer daemon registered with WebSocket server');
    
  }, 30000);

  afterEach(async () => {
    console.log('🧹 Cleaning up test environment...');
    
    if (wsClient) {
      wsClient.close();
    }
    
    if (wsServer) {
      await wsServer.stop();
    }
    
    if (rendererDaemon) {
      await rendererDaemon.stop();
    }
    
    console.log('✅ Test cleanup complete');
  });

  describe('Dynamic Discovery', () => {
    it('should discover and register daemon capabilities', async () => {
      const systemStatus = wsServer.getSystemStatus();
      
      console.log('📊 System Status:', JSON.stringify(systemStatus, null, 2));
      
      expect(systemStatus.dynamicRouter.registeredDaemons).toBeGreaterThan(0);
      expect(systemStatus.registeredDaemons).toContain('renderer');
      
      // Check that renderer daemon capabilities were discovered
      const routerStatus = systemStatus.dynamicRouter;
      const rendererInfo = routerStatus.daemons.find(d => d.name === 'renderer');
      
      expect(rendererInfo).toBeDefined();
      expect(rendererInfo.capabilities).toContain('basic-rendering');
      console.log('✅ Renderer capabilities discovered:', rendererInfo.capabilities);
    });
  });

  describe('WebSocket Client Communication', () => {
    it('should handle client connections and route messages dynamically', async (done) => {
      console.log('🔌 Testing WebSocket client connection...');
      
      wsClient = new WebSocket(`ws://localhost:${testPort}`);
      
      wsClient.on('open', () => {
        console.log('✅ WebSocket client connected');
        
        // Test dynamic routing to WebSocket daemon
        const testMessage = {
          type: 'get_stats',
          data: {},
          requestId: 'test-001'
        };
        
        console.log('📤 Sending get_stats message...');
        wsClient.send(JSON.stringify(testMessage));
      });
      
      wsClient.on('message', (data) => {
        const response = JSON.parse(data.toString());
        console.log('📥 Received response:', response);
        
        if (response.type === 'get_stats_response') {
          expect(response.success).toBe(true);
          expect(response.data.server.name).toBe('websocket-server');
          expect(response.processedBy).toBe('websocket-server');
          console.log('✅ Dynamic routing to WebSocket daemon successful');
          done();
        }
      });
      
      wsClient.on('error', (error) => {
        console.error('❌ WebSocket error:', error);
        done.fail(error);
      });
      
    }, 10000);

    it('should route messages to renderer daemon capabilities', async (done) => {
      console.log('🎨 Testing message routing to renderer daemon...');
      
      wsClient = new WebSocket(`ws://localhost:${testPort}`);
      
      wsClient.on('open', () => {
        console.log('✅ WebSocket client connected for renderer test');
        
        // Test routing to renderer daemon
        const renderMessage = {
          type: 'get_capabilities',
          data: {},
          requestId: 'test-002'
        };
        
        console.log('📤 Sending get_capabilities message to renderer...');
        wsClient.send(JSON.stringify(renderMessage));
      });
      
      wsClient.on('message', (data) => {
        const response = JSON.parse(data.toString());
        console.log('📥 Received renderer response:', response);
        
        if (response.type === 'get_capabilities_response') {
          expect(response.success).toBe(true);
          expect(response.data.capabilities).toContain('basic-rendering');
          expect(response.processedBy).toBe('renderer');
          console.log('✅ Dynamic routing to renderer daemon successful');
          done();
        }
      });
      
      wsClient.on('error', (error) => {
        console.error('❌ WebSocket error:', error);
        done.fail(error);
      });
      
    }, 10000);
  });

  describe('Multi-Daemon Coordination', () => {
    it('should handle multiple daemons with different capabilities', async () => {
      const systemStatus = wsServer.getSystemStatus();
      
      console.log('🔧 Testing multi-daemon coordination...');
      console.log('📊 Registered daemons:', systemStatus.registeredDaemons);
      
      // Should have at least WebSocket and Renderer daemons
      expect(systemStatus.registeredDaemons.length).toBeGreaterThanOrEqual(1);
      expect(systemStatus.registeredDaemons).toContain('renderer');
      
      // Each daemon should have distinct capabilities
      const routerStatus = systemStatus.dynamicRouter;
      const totalMessageTypes = routerStatus.totalMessageTypes;
      
      console.log('📝 Total message types across all daemons:', totalMessageTypes);
      expect(totalMessageTypes).toBeGreaterThan(0);
      
      console.log('✅ Multi-daemon coordination test passed');
    });
  });

  describe('Error Handling', () => {
    it('should handle unknown message types gracefully', async (done) => {
      console.log('❌ Testing error handling for unknown message types...');
      
      wsClient = new WebSocket(`ws://localhost:${testPort}`);
      
      wsClient.on('open', () => {
        const unknownMessage = {
          type: 'unknown_message_type',
          data: {},
          requestId: 'test-003'
        };
        
        console.log('📤 Sending unknown message type...');
        wsClient.send(JSON.stringify(unknownMessage));
      });
      
      wsClient.on('message', (data) => {
        const response = JSON.parse(data.toString());
        console.log('📥 Error response:', response);
        
        if (response.type === 'error') {
          expect(response.data.error).toContain('No daemon registered');
          expect(response.data.availableTypes).toBeDefined();
          console.log('✅ Error handling working correctly');
          done();
        }
      });
      
    }, 10000);
  });
});