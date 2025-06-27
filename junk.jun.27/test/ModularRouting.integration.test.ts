/**
 * Modular Routing Integration Tests
 * Tests the new modular architecture where daemons register their own routes and APIs
 */

import { WebSocketDaemon } from '../WebSocketDaemon';
import { RendererDaemon } from '../../../daemons/renderer/RendererDaemon';
import fetch from 'node-fetch';
import { WebSocket } from 'ws';

describe('Modular Routing Integration Tests', () => {
  let webSocketDaemon: WebSocketDaemon;
  let rendererDaemon: RendererDaemon;
  let wsClient: WebSocket;

  beforeAll(async () => {
    // Start WebSocket daemon
    webSocketDaemon = new WebSocketDaemon({ port: 9998 });
    await webSocketDaemon.start();

    // Start Renderer daemon
    rendererDaemon = new RendererDaemon();
    await rendererDaemon.start();

    // Register renderer with websocket daemon (modular registration)
    await webSocketDaemon.registerExternalDaemon('renderer', rendererDaemon);

    // Connect WebSocket client
    wsClient = new WebSocket('ws://localhost:9998');
    await new Promise((resolve) => {
      wsClient.on('open', resolve);
    });
  });

  afterAll(async () => {
    if (wsClient) wsClient.close();
    if (rendererDaemon) await rendererDaemon.stop();
    if (webSocketDaemon) await webSocketDaemon.stop();
  });

  describe('Route Registration and Handling', () => {
    test('should register routes from RendererDaemon during external daemon registration', async () => {
      // Verify routes were registered by testing actual HTTP requests
      const staticFileResponse = await fetch('http://localhost:9998/src/ui/components/shared/BaseWidget.css');
      expect(staticFileResponse.status).toBe(200);
      expect(staticFileResponse.headers.get('content-type')).toContain('text/css');
    });

    test('should serve main UI through RendererDaemon route handler', async () => {
      const uiResponse = await fetch('http://localhost:9998/');
      expect(uiResponse.status).toBe(200);
      expect(uiResponse.headers.get('content-type')).toContain('text/html');
      
      const htmlContent = await uiResponse.text();
      expect(htmlContent).toContain('Continuum');
    });

    test('should serve static files with proper caching headers', async () => {
      const response = await fetch('http://localhost:9998/src/ui/components/shared/BaseWidget.css');
      
      expect(response.status).toBe(200);
      expect(response.headers.get('etag')).toBeDefined();
      expect(response.headers.get('last-modified')).toBeDefined();
      expect(response.headers.get('cache-control')).toBeDefined();
    });

    test('should return 304 Not Modified for cached resources', async () => {
      // First request to get ETag
      const firstResponse = await fetch('http://localhost:9998/src/ui/components/shared/BaseWidget.css');
      const etag = firstResponse.headers.get('etag');
      
      // Second request with If-None-Match header
      const secondResponse = await fetch('http://localhost:9998/src/ui/components/shared/BaseWidget.css', {
        headers: { 'If-None-Match': etag! }
      });
      
      expect(secondResponse.status).toBe(304);
    });
  });

  describe('API Endpoint Registration and Handling', () => {
    test('should register and handle /api/agents through RendererDaemon', async () => {
      const response = await fetch('http://localhost:9998/api/agents');
      expect(response.status).toBe(200);
      
      const agents = await response.json();
      expect(Array.isArray(agents)).toBe(true);
      expect(agents).toHaveLength(2);
      expect(agents[0]).toHaveProperty('id', 'claude');
      expect(agents[1]).toHaveProperty('id', 'developer');
    });

    test('should register and handle /api/personas through RendererDaemon', async () => {
      const response = await fetch('http://localhost:9998/api/personas');
      expect(response.status).toBe(200);
      
      const personas = await response.json();
      expect(Array.isArray(personas)).toBe(true);
      expect(personas).toHaveLength(2);
      expect(personas[0]).toHaveProperty('id', 'coding-expert');
      expect(personas[1]).toHaveProperty('id', 'creative-writer');
    });

    test('should fall back to built-in API handlers for unregistered endpoints', async () => {
      const response = await fetch('http://localhost:9998/api/system');
      expect(response.status).toBe(200);
      
      const systemData = await response.json();
      expect(systemData).toHaveProperty('server');
    });
  });

  describe('WebSocket Message Routing', () => {
    test('should route messages to registered daemons', (done) => {
      const testMessage = {
        type: 'render_request',
        data: { type: 'render_ui', data: {} },
        timestamp: new Date().toISOString()
      };

      wsClient.on('message', (data) => {
        const response = JSON.parse(data.toString());
        if (response.type === 'render_response') {
          expect(response.success).toBe(true);
          done();
        }
      });

      wsClient.send(JSON.stringify(testMessage));
    });

    test('should handle daemon-specific capabilities', (done) => {
      const capabilitiesMessage = {
        type: 'get_capabilities',
        data: {},
        timestamp: new Date().toISOString()
      };

      wsClient.on('message', (data) => {
        const response = JSON.parse(data.toString());
        if (response.type === 'capabilities_response') {
          expect(response.data.capabilities).toContain('basic-rendering');
          expect(response.data.capabilities).toContain('static-file-serving');
          done();
        }
      });

      wsClient.send(JSON.stringify(capabilitiesMessage));
    });
  });

  describe('System Health and Status', () => {
    test('should provide comprehensive system status including registered daemons', async () => {
      const response = await fetch('http://localhost:9998/api/system');
      expect(response.status).toBe(200);
      
      const systemStatus = await response.json();
      expect(systemStatus).toHaveProperty('server');
      expect(systemStatus.server.name).toBe('websocket-server');
    });

    test('should list registered daemons', async () => {
      const response = await fetch('http://localhost:9998/api/daemons');
      expect(response.status).toBe(200);
      
      const daemonData = await response.json();
      expect(daemonData.daemons).toContain('renderer');
      expect(daemonData.daemons).toContain('websocket-server');
    });
  });

  describe('Error Handling and Fallbacks', () => {
    test('should return 404 for unregistered routes', async () => {
      const response = await fetch('http://localhost:9998/nonexistent/route');
      expect(response.status).toBe(404);
    });

    test('should return 404 for unregistered API endpoints', async () => {
      const response = await fetch('http://localhost:9998/api/nonexistent');
      expect(response.status).toBe(404);
    });

    test('should handle daemon route handler errors gracefully', async () => {
      // Test accessing a file that doesn't exist
      const response = await fetch('http://localhost:9998/src/nonexistent/file.css');
      expect(response.status).toBe(404);
    });
  });

  describe('Performance and Caching', () => {
    test('should serve multiple widgets efficiently with proper caching', async () => {
      const promises = [];
      
      // Simulate multiple widget requests for the same CSS file
      for (let i = 0; i < 5; i++) {
        promises.push(fetch('http://localhost:9998/src/ui/components/shared/BaseWidget.css'));
      }
      
      const responses = await Promise.all(promises);
      
      // All should succeed
      responses.forEach(response => {
        expect(response.status).toBe(200);
        expect(response.headers.get('etag')).toBeDefined();
      });
      
      // ETags should be consistent
      const etags = responses.map(r => r.headers.get('etag'));
      expect(new Set(etags).size).toBe(1); // All ETags should be the same
    });

    test('should handle concurrent API requests efficiently', async () => {
      const startTime = Date.now();
      
      const promises = [
        fetch('http://localhost:9998/api/agents'),
        fetch('http://localhost:9998/api/personas'),
        fetch('http://localhost:9998/api/system'),
        fetch('http://localhost:9998/api/agents'),
        fetch('http://localhost:9998/api/personas')
      ];
      
      const responses = await Promise.all(promises);
      const endTime = Date.now();
      
      // All should succeed
      responses.forEach(response => {
        expect(response.status).toBe(200);
      });
      
      // Should be reasonably fast (under 1 second for 5 concurrent requests)
      expect(endTime - startTime).toBeLessThan(1000);
    });
  });

  describe('Modular Architecture Verification', () => {
    test('should allow daemons to register multiple route patterns', () => {
      // Verify that a daemon can register multiple patterns
      const routeHandlers = (webSocketDaemon as any).routeHandlers;
      
      expect(routeHandlers.has('/src/*')).toBe(true);
      expect(routeHandlers.has('/dist/*')).toBe(true);
      expect(routeHandlers.has('/')).toBe(true);
      
      // All should point to the renderer daemon
      expect(routeHandlers.get('/src/*').daemon.name).toBe('renderer');
      expect(routeHandlers.get('/dist/*').daemon.name).toBe('renderer');
      expect(routeHandlers.get('/').daemon.name).toBe('renderer');
    });

    test('should allow daemons to register multiple API endpoints', () => {
      // Verify that a daemon can register multiple API endpoints
      const apiHandlers = (webSocketDaemon as any).apiHandlers;
      
      expect(apiHandlers.has('/api/agents')).toBe(true);
      expect(apiHandlers.has('/api/personas')).toBe(true);
      
      // Both should point to the renderer daemon
      expect(apiHandlers.get('/api/agents').daemon.name).toBe('renderer');
      expect(apiHandlers.get('/api/personas').daemon.name).toBe('renderer');
    });

    test('should maintain separation of concerns between daemons', () => {
      // WebSocketDaemon should handle WebSocket connections and routing
      expect(webSocketDaemon.name).toBe('websocket-server');
      
      // RendererDaemon should handle UI rendering and static files
      expect(rendererDaemon.name).toBe('renderer');
      
      // Neither should know about the other's internal implementation
      expect((webSocketDaemon as any).legacyRenderer).toBeUndefined();
      expect((rendererDaemon as any).connectionManager).toBeUndefined();
    });
  });
});