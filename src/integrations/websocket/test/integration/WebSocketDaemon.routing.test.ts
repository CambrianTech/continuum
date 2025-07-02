/**
 * WebSocketDaemon Routing Integration Tests
 * 
 * These tests define the CORRECT behavior for WebSocketDaemon:
 * - It should ONLY route requests to appropriate daemons
 * - It should NOT serve files directly
 * - It should NOT compile TypeScript
 * - It should delegate all content handling to specialized daemons
 */

import { describe, test, expect, beforeEach, afterEach } from '@jest/globals';
import { WebSocketDaemon } from '../../WebSocketDaemon.js';
import { RendererDaemon } from '../../../../daemons/renderer/RendererDaemon.js';
import { StaticFileDaemon } from '../../../../daemons/static-file/StaticFileDaemon.js';
import http from 'http';
import WebSocket from 'ws';

describe('WebSocketDaemon Routing Behavior', () => {
  let wsDaemon: WebSocketDaemon;
  let rendererDaemon: RendererDaemon;
  let staticFileDaemon: StaticFileDaemon;
  const TEST_PORT = 9876;

  beforeEach(async () => {
    // Create daemons
    wsDaemon = new WebSocketDaemon({ port: TEST_PORT });
    rendererDaemon = new RendererDaemon();
    staticFileDaemon = new StaticFileDaemon();
    
    // Start daemons
    await rendererDaemon.start();
    await staticFileDaemon.start();
    await wsDaemon.start();
    
    // Register daemons with router
    wsDaemon.registerDaemon(rendererDaemon);
    wsDaemon.registerDaemon(staticFileDaemon);
    
    // Register routes
    wsDaemon.registerRouteHandler('/', 'renderer', 'render_ui');
    wsDaemon.registerRouteHandler('/api/*', 'command-processor', 'execute');
    wsDaemon.registerRouteHandler('/static/*', 'static-file', 'serve');
    wsDaemon.registerRouteHandler('*.css', 'static-file', 'serve');
    wsDaemon.registerRouteHandler('*.js', 'static-file', 'serve');
  });

  afterEach(async () => {
    await wsDaemon.stop();
    await rendererDaemon.stop();
    await staticFileDaemon.stop();
  });

  describe('Route Delegation', () => {
    test('should route / to RendererDaemon', async () => {
      const response = await fetch(`http://localhost:${TEST_PORT}/`);
      const html = await response.text();
      
      expect(response.status).toBe(200);
      expect(response.headers.get('content-type')).toBe('text/html');
      expect(html).toContain('<!DOCTYPE html>');
      expect(html).toContain('<chat-widget>');
    });

    test('should route CSS files to StaticFileDaemon', async () => {
      const response = await fetch(`http://localhost:${TEST_PORT}/src/ui/components/shared/BaseWidget.css`);
      const css = await response.text();
      
      expect(response.status).toBe(200);
      expect(response.headers.get('content-type')).toBe('text/css');
      expect(css).toContain('.widget-container');
    });

    test('should route JS files to StaticFileDaemon', async () => {
      const response = await fetch(`http://localhost:${TEST_PORT}/dist/ui/continuum-browser.js`);
      
      expect(response.status).toBe(200);
      expect(response.headers.get('content-type')).toBe('application/javascript');
    });

    test('should return 404 for unregistered routes', async () => {
      const response = await fetch(`http://localhost:${TEST_PORT}/unknown/path`);
      
      expect(response.status).toBe(404);
    });
  });

  describe('WebSocket Routing', () => {
    test('should route WebSocket commands to CommandProcessor', (done) => {
      const ws = new WebSocket(`ws://localhost:${TEST_PORT}`);
      
      ws.on('open', () => {
        ws.send(JSON.stringify({
          type: 'execute_command',
          data: {
            command: 'health',
            requestId: 'test-123'
          }
        }));
      });
      
      ws.on('message', (data) => {
        const message = JSON.parse(data.toString());
        if (message.type === 'execute_command_response') {
          expect(message.data.requestId).toBe('test-123');
          expect(message.data.result).toBeDefined();
          ws.close();
          done();
        }
      });
    });

    test('should broadcast to all connections', (done) => {
      const ws1 = new WebSocket(`ws://localhost:${TEST_PORT}`);
      const ws2 = new WebSocket(`ws://localhost:${TEST_PORT}`);
      let receivedCount = 0;
      
      const checkDone = () => {
        receivedCount++;
        if (receivedCount === 2) {
          ws1.close();
          ws2.close();
          done();
        }
      };
      
      ws1.on('open', () => {
        ws2.on('open', () => {
          // Trigger a broadcast
          ws1.send(JSON.stringify({
            type: 'broadcast',
            data: { message: 'test' }
          }));
        });
      });
      
      ws1.on('message', (data) => {
        const message = JSON.parse(data.toString());
        if (message.type === 'broadcast') {
          checkDone();
        }
      });
      
      ws2.on('message', (data) => {
        const message = JSON.parse(data.toString());
        if (message.type === 'broadcast') {
          checkDone();
        }
      });
    });
  });

  describe('No Direct File Handling', () => {
    test('should NOT compile TypeScript files', async () => {
      // WebSocketDaemon should delegate, not compile
      const response = await fetch(`http://localhost:${TEST_PORT}/src/ui/components/Chat/ChatWidget.ts`);
      
      // Should be handled by StaticFileDaemon or return 404
      // NOT compiled by WebSocketDaemon
      expect(response.headers.get('x-compiled-by')).not.toBe('websocket-daemon');
    });

    test('should NOT read files from filesystem directly', async () => {
      // Mock file system access to ensure WebSocketDaemon doesn't use it
      const originalReadFile = require('fs').readFileSync;
      let fsAccessedByWsDaemon = false;
      
      require('fs').readFileSync = function(...args: any[]) {
        const stack = new Error().stack || '';
        if (stack.includes('WebSocketDaemon')) {
          fsAccessedByWsDaemon = true;
        }
        return originalReadFile.apply(this, args);
      };
      
      await fetch(`http://localhost:${TEST_PORT}/test.css`);
      
      expect(fsAccessedByWsDaemon).toBe(false);
      
      // Restore
      require('fs').readFileSync = originalReadFile;
    });
  });

  describe('Pure Routing Behavior', () => {
    test('should route based on patterns', async () => {
      // Test various route patterns
      const routes = [
        { path: '/api/health', expectedDaemon: 'command-processor' },
        { path: '/static/image.png', expectedDaemon: 'static-file' },
        { path: '/styles.css', expectedDaemon: 'static-file' },
        { path: '/app.js', expectedDaemon: 'static-file' },
        { path: '/', expectedDaemon: 'renderer' }
      ];
      
      for (const route of routes) {
        const response = await fetch(`http://localhost:${TEST_PORT}${route.path}`);
        const handledBy = response.headers.get('x-handled-by');
        expect(handledBy).toBe(route.expectedDaemon);
      }
    });

    test('should support daemon registration and deregistration', () => {
      const testDaemon = {
        name: 'test-daemon',
        handleMessage: jest.fn()
      };
      
      wsDaemon.registerDaemon(testDaemon);
      expect(wsDaemon.getDaemonNames()).toContain('test-daemon');
      
      wsDaemon.deregisterDaemon('test-daemon');
      expect(wsDaemon.getDaemonNames()).not.toContain('test-daemon');
    });
  });
});