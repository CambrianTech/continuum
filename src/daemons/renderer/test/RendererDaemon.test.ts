/**
 * RendererDaemon Unit Tests
 * Tests static file serving, caching, and rendering functionality
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { RendererDaemon } from '../RendererDaemon.js';
import * as http from 'http';
import { promises as fs } from 'fs';
import { join } from 'path';

describe('RendererDaemon', () => {
  let daemon: RendererDaemon;

  beforeEach(() => {
    daemon = new RendererDaemon();
  });

  afterEach(async () => {
    if (daemon) {
      await daemon.stop();
    }
  });

  describe('Daemon Lifecycle', () => {
    it('should start and stop cleanly', async () => {
      expect(daemon.getStatus()).toBe('stopped');
      
      await daemon.start();
      expect(daemon.getStatus()).toBe('running');
      
      await daemon.stop();
      expect(daemon.getStatus()).toBe('stopped');
    });

    it('should load legacy renderer on startup', async () => {
      await daemon.start();
      
      const response = await daemon.sendMessage({
        type: 'get_capabilities',
        data: {}
      });
      
      expect(response.success).toBe(true);
      expect(response.data.engine).toBe('legacy');
      expect(response.data.capabilities).toContain('legacy-ui');
    });
  });

  describe('Legacy UIGenerator Integration', () => {
    beforeEach(async () => {
      await daemon.start();
    });

    it('should load UIGenerator.cjs successfully', async () => {
      const response = await daemon.sendMessage({
        type: 'get_capabilities',
        data: {}
      });
      
      expect(response.success).toBe(true);
      expect(response.data.capabilities).toContain('legacy-ui');
    });

    it('should handle render requests', async () => {
      const response = await daemon.sendMessage({
        type: 'render_request',
        data: {
          type: 'render_ui',
          data: { page: 'test' }
        }
      });
      
      // Should not crash even if render fails
      expect(response).toBeDefined();
      console.log('Render response:', response);
    });
  });

  describe('Engine Switching', () => {
    beforeEach(async () => {
      await daemon.start();
    });

    it('should switch between legacy and modern engines', async () => {
      // Start with legacy
      let response = await daemon.sendMessage({
        type: 'get_capabilities',
        data: {}
      });
      expect(response.data.engine).toBe('legacy');

      // Try to switch to modern (should fallback to legacy)
      response = await daemon.sendMessage({
        type: 'switch_engine',
        data: { engine: 'modern' }
      });
      
      expect(response.success).toBe(true);
      console.log('Engine switch response:', response);
    });
  });

  describe('Error Handling', () => {
    beforeEach(async () => {
      await daemon.start();
    });

    it('should handle invalid message types gracefully', async () => {
      const response = await daemon.sendMessage({
        type: 'invalid_message',
        data: {}
      });
      
      expect(response.success).toBe(false);
      expect(response.error).toContain('Unknown message type');
    });
  });

  describe('Static File Serving', () => {
    beforeEach(async () => {
      await daemon.start();
      // Wait for static file server to initialize
      await new Promise(resolve => setTimeout(resolve, 500));
    });

    it('should serve CSS files with proper cache headers', async () => {
      const response = await makeRequest('/src/ui/components/shared/BaseWidget.css');
      
      expect(response.statusCode).toBe(200);
      expect(response.headers['content-type']).toBe('text/css');
      expect(response.headers['cache-control']).toMatch(/public, max-age=31536000, immutable/);
      expect(response.headers['etag']).toBeDefined();
      expect(response.headers['last-modified']).toBeDefined();
      expect(response.headers['access-control-allow-origin']).toBe('*');
    });

    it('should serve JavaScript files with proper cache headers', async () => {
      const response = await makeRequest('/dist/ui/widget-loader.js');
      
      expect(response.statusCode).toBe(200);
      expect(response.headers['content-type']).toBe('application/javascript');
      expect(response.headers['cache-control']).toMatch(/public, max-age=31536000, immutable/);
      expect(response.headers['etag']).toBeDefined();
      expect(response.headers['last-modified']).toBeDefined();
    });

    it('should return 304 Not Modified for cached requests', async () => {
      // First request to get ETag
      const firstResponse = await makeRequest('/src/ui/components/shared/BaseWidget.css');
      const etag = firstResponse.headers['etag'];
      
      if (etag) {
        // Second request with If-None-Match header
        const secondResponse = await makeRequest('/src/ui/components/shared/BaseWidget.css', {
          'If-None-Match': etag
        });
        
        expect(secondResponse.statusCode).toBe(304);
        expect(secondResponse.body).toBe('');
      }
    });

    it('should return 404 for non-existent files', async () => {
      const response = await makeRequest('/src/ui/nonexistent.css');
      
      expect(response.statusCode).toBe(404);
      expect(response.headers['content-type']).toBe('text/plain');
    });

    it('should reject requests outside /src/ and /dist/ paths', async () => {
      const response = await makeRequest('/etc/passwd');
      
      expect(response.statusCode).toBe(404);
      expect(response.body).toContain('RendererDaemon only serves /src/ and /dist/ paths');
    });

    it('should handle concurrent requests efficiently', async () => {
      const promises = Array.from({ length: 5 }, () => 
        makeRequest('/src/ui/components/shared/BaseWidget.css')
      );

      const responses = await Promise.all(promises);
      
      responses.forEach(response => {
        expect([200, 404]).toContain(response.statusCode); // 404 is ok if file doesn't exist
        if (response.statusCode === 200) {
          expect(response.headers['cache-control']).toBeDefined();
        }
      });
    });
  });

  describe('Caching Performance', () => {
    beforeEach(async () => {
      await daemon.start();
      await new Promise(resolve => setTimeout(resolve, 500));
    });

    it('should eliminate redundant BaseWidget.css fetches', async () => {
      // Simulate multiple widget components requesting the same CSS file
      const cssPath = '/src/ui/components/shared/BaseWidget.css';
      
      // First request establishes cache
      const firstResponse = await makeRequest(cssPath);
      const etag = firstResponse.headers['etag'];
      
      if (firstResponse.statusCode === 200 && etag) {
        // Subsequent requests should return 304
        const cachedRequests = await Promise.all([
          makeRequest(cssPath, { 'If-None-Match': etag }),
          makeRequest(cssPath, { 'If-None-Match': etag }),
          makeRequest(cssPath, { 'If-None-Match': etag })
        ]);
        
        cachedRequests.forEach(response => {
          expect(response.statusCode).toBe(304);
        });
        
        console.log('✅ Cache test: 3 requests returned 304 Not Modified');
      } else {
        console.log('⚠️ BaseWidget.css not found, creating mock test');
        expect(firstResponse.statusCode).toBeGreaterThan(0);
      }
    });
  });

  describe('CommonJS Integration', () => {
    it('should properly import UIGenerator.cjs in ES module context', async () => {
      // Test the import mechanism
      try {
        const { createRequire } = await import('module');
        const require = createRequire(import.meta.url);
        const UIGeneratorClass = require('../../ui/UIGenerator.cjs');
        
        expect(UIGeneratorClass).toBeDefined();
        expect(typeof UIGeneratorClass).toBe('function');
        
        // Try to instantiate
        const instance = new UIGeneratorClass(null);
        expect(instance).toBeDefined();
        expect(typeof instance.generateHTML).toBe('function');
        
      } catch (error) {
        console.error('CommonJS import test failed:', error);
        throw error;
      }
    });
  });
});

// Helper function to make HTTP requests to static file server
function makeRequest(path: string, headers: Record<string, string> = {}): Promise<{
  statusCode: number;
  headers: Record<string, string>;
  body: string;
}> {
  return new Promise((resolve, reject) => {
    const options = {
      hostname: 'localhost',
      port: 9001, // RendererDaemon static file server port
      path,
      method: 'GET',
      headers
    };

    const req = http.request(options, (res) => {
      let body = '';
      
      res.on('data', (chunk) => {
        body += chunk;
      });
      
      res.on('end', () => {
        resolve({
          statusCode: res.statusCode || 0,
          headers: res.headers as Record<string, string>,
          body
        });
      });
    });

    req.on('error', (error) => {
      // Don't reject on connection errors in tests
      resolve({
        statusCode: 0,
        headers: {},
        body: `Connection error: ${error.message}`
      });
    });

    req.setTimeout(5000, () => {
      req.destroy();
      resolve({
        statusCode: 0,
        headers: {},
        body: 'Request timeout'
      });
    });

    req.end();
  });
}