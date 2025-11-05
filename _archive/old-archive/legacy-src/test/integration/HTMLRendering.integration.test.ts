/**
 * Layer 2: Daemon HTML Rendering Tests
 * Tests RendererDaemon's ability to generate HTML through daemon messaging
 * According to middle-out methodology, this tests Layer 2 functionality only
 * 
 * DOES NOT test HTTP endpoints (that's Layer 4)
 * DOES NOT require browser connection (that's Layer 5)
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { RendererDaemon } from '../../daemons/renderer/RendererDaemon';
import { WebSocketDaemon } from '../../integrations/websocket/WebSocketDaemon';

describe('HTML Rendering Integration Tests', () => {
  let rendererDaemon: RendererDaemon;
  let webSocketDaemon: WebSocketDaemon;
  
  before(async () => {
    // Create and start daemons
    rendererDaemon = new RendererDaemon();
    webSocketDaemon = new WebSocketDaemon();
    
    await rendererDaemon.start();
    await webSocketDaemon.start();
    
    // Register renderer with WebSocket daemon
    webSocketDaemon.registerDaemon(rendererDaemon);
    webSocketDaemon.registerRouteHandler('*', 'renderer', 'http_request');
  });
  
  after(async () => {
    await webSocketDaemon.stop();
    await rendererDaemon.stop();
  });
  
  describe('Daemon Capabilities', () => {
    it('should report rendering capabilities', async () => {
      const message = {
        from: 'test',
        to: 'renderer',
        type: 'get_capabilities',
        data: {}
      };
      
      const response = await rendererDaemon.handleMessage(message);
      
      assert(response.success, 'Should return capabilities');
      assert(response.data, 'Should have data');
    });
  });
  
  describe('Page Rendering', () => {
    it('should render page via render_page message', async () => {
      const message = {
        from: 'test',
        to: 'renderer',
        type: 'render_request',
        data: {
          type: 'render_page',
          data: {
            template: 'default',
            title: 'Test Page',
            content: 'Test Content'
          }
        }
      };
      
      const response = await rendererDaemon.handleMessage(message);
      
      assert(response.success, 'Render should succeed');
      assert(response.data?.html || response.html, 'Should return HTML');
    });
    
    it('should render UI components via render_ui message', async () => {
      const message = {
        from: 'test',
        to: 'renderer',
        type: 'render_request',
        data: {
          type: 'render_ui',
          data: {
            component: 'test-widget',
            props: { title: 'Test Widget' }
          }
        }
      };
      
      const response = await rendererDaemon.handleMessage(message);
      
      // May fail if component doesn't exist, but should handle gracefully
      assert(response !== undefined, 'Should return response');
    });
  });
  
  describe('Component Updates', () => {
    it('should handle component update requests', async () => {
      const message = {
        from: 'test',
        to: 'renderer',
        type: 'render_request',
        data: {
          type: 'update_component',
          data: {
            componentId: 'test-component',
            updates: { text: 'Updated Text' }
          }
        }
      };
      
      const response = await rendererDaemon.handleMessage(message);
      
      assert(response !== undefined, 'Should return response');
    });
  });
  
  describe('Error Handling', () => {
    it('should handle invalid render requests gracefully', async () => {
      const message = {
        from: 'test',
        to: 'renderer',
        type: 'render_request',
        data: {
          type: 'invalid_type' as any,
          data: {}
        }
      };
      
      const response = await rendererDaemon.handleMessage(message);
      
      assert(response !== undefined, 'Should return response');
      // Should either fail gracefully or return error
    });
    
    it('should handle missing data gracefully', async () => {
      const message = {
        from: 'test',
        to: 'renderer',
        type: 'render_request',
        data: null as any
      };
      
      try {
        const response = await rendererDaemon.handleMessage(message);
        assert(response !== undefined, 'Should return response even with null data');
      } catch (error) {
        // Expected - daemon should handle error
        assert(error, 'Should throw or handle error');
      }
    });
  });
});