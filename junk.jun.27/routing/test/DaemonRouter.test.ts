/**
 * DaemonRouter Unit Tests
 * Tests request routing to appropriate daemons
 */

import { DaemonRouter } from '../DaemonRouter';
import { IncomingMessage, ServerResponse } from 'http';

// Mock HTTP objects
const createMockRequest = (url: string, method: string = 'GET'): IncomingMessage => ({
  url,
  method,
} as IncomingMessage);

const createMockResponse = (): { res: ServerResponse; data: any } => {
  const data = { headers: {}, statusCode: 200, body: '' };
  const res = {
    writeHead: jest.fn((code, headers) => {
      data.statusCode = code;
      data.headers = headers;
    }),
    end: jest.fn((body) => {
      data.body = body;
    })
  } as unknown as ServerResponse;
  
  return { res, data };
};

describe('DaemonRouter', () => {
  let router: DaemonRouter;
  let mockRendererDaemon: any;
  let mockCommandDaemon: any;

  beforeEach(() => {
    router = new DaemonRouter();
    
    mockRendererDaemon = {
      handleHttpRequest: jest.fn().mockResolvedValue(undefined)
    };
    
    mockCommandDaemon = {
      sendMessage: jest.fn().mockResolvedValue({ 
        success: true, 
        data: { result: 'command executed' } 
      })
    };

    router.registerDaemon('renderer', mockRendererDaemon);
    router.registerDaemon('command-processor', mockCommandDaemon);
  });

  describe('Route Registration', () => {
    test('should register daemons correctly', () => {
      const daemons = router.getRegisteredDaemons();
      expect(daemons).toContain('renderer');
      expect(daemons).toContain('command-processor');
    });

    test('should provide routing table', () => {
      const table = router.getRoutingTable();
      expect(table.length).toBeGreaterThan(0);
      expect(table.some(rule => rule.pattern === '/')).toBe(true);
      expect(table.some(rule => rule.daemon === 'renderer')).toBe(true);
    });
  });

  describe('Request Routing', () => {
    test('should route root request to renderer daemon', async () => {
      const req = createMockRequest('/');
      const { res } = createMockResponse();

      const handled = await router.routeRequest(req, res);

      expect(handled).toBe(true);
      expect(mockRendererDaemon.handleHttpRequest).toHaveBeenCalledWith(req, res);
    });

    test('should route static files to renderer daemon', async () => {
      const req = createMockRequest('/src/ui/components/test.css');
      const { res } = createMockResponse();

      const handled = await router.routeRequest(req, res);

      expect(handled).toBe(true);
      expect(mockRendererDaemon.handleHttpRequest).toHaveBeenCalledWith(req, res);
    });

    test('should route API requests to appropriate daemon', async () => {
      const req = createMockRequest('/api/commands/test');
      const { res } = createMockResponse();

      const handled = await router.routeRequest(req, res);

      expect(handled).toBe(true);
      expect(mockCommandDaemon.sendMessage).toHaveBeenCalledWith({
        type: 'http_request',
        data: { pathname: '/api/commands/test', method: 'GET' }
      });
    });

    test('should return false for unmatched routes', async () => {
      const req = createMockRequest('/unknown/path');
      const { res } = createMockResponse();

      const handled = await router.routeRequest(req, res);

      expect(handled).toBe(false);
    });
  });

  describe('Error Handling', () => {
    test('should handle missing daemon gracefully', async () => {
      const req = createMockRequest('/');
      const { res, data } = createMockResponse();
      
      // Remove renderer daemon
      router = new DaemonRouter();

      const handled = await router.routeRequest(req, res);

      expect(handled).toBe(true);
      expect(data.statusCode).toBe(503);
      expect(data.body).toContain('Daemon renderer not available');
    });

    test('should handle daemon errors gracefully', async () => {
      const req = createMockRequest('/');
      const { res, data } = createMockResponse();
      
      mockRendererDaemon.handleHttpRequest.mockRejectedValue(new Error('Daemon failed'));

      const handled = await router.routeRequest(req, res);

      expect(handled).toBe(true);
      expect(data.statusCode).toBe(500);
      expect(data.body).toContain('Error routing to renderer');
    });

    test('should handle daemons without HTTP capabilities', async () => {
      const incompleteDaemon = {}; // No handleHttpRequest or sendMessage
      router.registerDaemon('incomplete', incompleteDaemon);
      
      // Add a route for this daemon
      const routingTable = router.getRoutingTable();
      routingTable.push({ pattern: '/incomplete', daemon: 'incomplete', description: 'Test' });

      const req = createMockRequest('/incomplete');
      const { res, data } = createMockResponse();

      const handled = await router.routeRequest(req, res);

      expect(handled).toBe(true);
      expect(data.statusCode).toBe(501);
      expect(data.body).toContain('cannot handle HTTP requests');
    });
  });
});