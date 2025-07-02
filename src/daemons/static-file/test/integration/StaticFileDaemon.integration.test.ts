/**
 * StaticFileDaemon Integration Tests
 * 
 * Tests for the daemon responsible for serving all static files:
 * - CSS, JS, HTML, images, etc.
 * - TypeScript compilation on-demand
 * - Security (path traversal prevention)
 * - Caching headers
 * - Content-type detection
 */

import { describe, test, expect, beforeEach, afterEach } from '@jest/globals';
import { StaticFileDaemon } from '../../StaticFileDaemon.js';
import { DaemonMessage } from '../../../core/base-daemon/types.js';
import path from 'path';
import fs from 'fs/promises';

describe('StaticFileDaemon Integration Tests', () => {
  let daemon: StaticFileDaemon;
  const testFilesDir = path.join(__dirname, 'test-files');

  beforeEach(async () => {
    daemon = new StaticFileDaemon();
    await daemon.start();
    
    // Create test files
    await fs.mkdir(testFilesDir, { recursive: true });
    await fs.writeFile(
      path.join(testFilesDir, 'test.css'),
      '.test { color: red; }'
    );
    await fs.writeFile(
      path.join(testFilesDir, 'test.js'),
      'console.log("test");'
    );
    await fs.writeFile(
      path.join(testFilesDir, 'test.ts'),
      'const message: string = "typescript";'
    );
  });

  afterEach(async () => {
    await daemon.stop();
    await fs.rm(testFilesDir, { recursive: true, force: true });
  });

  describe('File Serving', () => {
    test('should serve CSS files with correct content-type', async () => {
      const message: DaemonMessage = {
        id: 'test-1',
        from: 'websocket',
        to: 'static-file',
        type: 'serve_file',
        timestamp: new Date(),
        data: {
          pathname: '/src/ui/components/shared/BaseWidget.css',
          method: 'GET'
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data?.contentType).toBe('text/css');
      expect(response.data?.content).toContain('.widget-container');
      expect(response.data?.headers['Cache-Control']).toBeDefined();
    });

    test('should serve JS files', async () => {
      const message: DaemonMessage = {
        id: 'test-2',
        from: 'websocket',
        to: 'static-file',
        type: 'serve_file',
        timestamp: new Date(),
        data: {
          pathname: '/dist/ui/continuum-browser.js',
          method: 'GET'
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data?.contentType).toBe('application/javascript');
    });

    test('should compile TypeScript files on-demand', async () => {
      const message: DaemonMessage = {
        id: 'test-3',
        from: 'websocket',
        to: 'static-file',
        type: 'serve_file',
        timestamp: new Date(),
        data: {
          pathname: '/src/ui/components/Chat/ChatWidget.js',
          method: 'GET'
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data?.contentType).toBe('application/javascript');
      expect(response.data?.content).not.toContain(': string'); // TypeScript syntax removed
      expect(response.data?.headers['X-Compiled-From']).toBe('typescript');
    });

    test('should return 404 for non-existent files', async () => {
      const message: DaemonMessage = {
        id: 'test-4',
        from: 'websocket',
        to: 'static-file',
        type: 'serve_file',
        timestamp: new Date(),
        data: {
          pathname: '/non/existent/file.css',
          method: 'GET'
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(false);
      expect(response.error).toContain('not found');
      expect(response.data?.status).toBe(404);
    });
  });

  describe('Security', () => {
    test('should prevent path traversal attacks', async () => {
      const maliciousPath = '/../../../../../../etc/passwd';
      const message: DaemonMessage = {
        id: 'test-5',
        from: 'websocket',
        to: 'static-file',
        type: 'serve_file',
        timestamp: new Date(),
        data: {
          pathname: maliciousPath,
          method: 'GET'
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(false);
      expect(response.error).toContain('forbidden');
      expect(response.data?.status).toBe(403);
    });

    test('should only serve files within project root', async () => {
      const message: DaemonMessage = {
        id: 'test-6',
        from: 'websocket',
        to: 'static-file',
        type: 'serve_file',
        timestamp: new Date(),
        data: {
          pathname: '/absolute/path/outside/project',
          method: 'GET'
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(false);
      expect(response.data?.status).toBe(403);
    });
  });

  describe('Content Types', () => {
    const contentTypeTests = [
      { ext: '.css', expectedType: 'text/css' },
      { ext: '.js', expectedType: 'application/javascript' },
      { ext: '.json', expectedType: 'application/json' },
      { ext: '.html', expectedType: 'text/html' },
      { ext: '.png', expectedType: 'image/png' },
      { ext: '.jpg', expectedType: 'image/jpeg' },
      { ext: '.svg', expectedType: 'image/svg+xml' },
      { ext: '.woff2', expectedType: 'font/woff2' },
    ];

    contentTypeTests.forEach(({ ext, expectedType }) => {
      test(`should serve ${ext} files with correct content-type`, async () => {
        // Create test file
        const testFile = path.join(testFilesDir, `test${ext}`);
        await fs.writeFile(testFile, 'test content');

        const message: DaemonMessage = {
          id: `test-${ext}`,
          from: 'websocket',
          to: 'static-file',
          type: 'serve_file',
          timestamp: new Date(),
          data: {
            pathname: `/test-files/test${ext}`,
            method: 'GET'
          }
        };

        const response = await daemon.handleMessage(message);
        
        expect(response.data?.contentType).toBe(expectedType);
      });
    });
  });

  describe('Caching', () => {
    test('should set appropriate cache headers for assets', async () => {
      const message: DaemonMessage = {
        id: 'test-cache',
        from: 'websocket',
        to: 'static-file',
        type: 'serve_file',
        timestamp: new Date(),
        data: {
          pathname: '/src/ui/components/shared/BaseWidget.css',
          method: 'GET'
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.data?.headers['Cache-Control']).toBe('public, max-age=3600');
      expect(response.data?.headers['ETag']).toBeDefined();
    });

    test('should respect If-None-Match header', async () => {
      // First request to get ETag
      const firstMessage: DaemonMessage = {
        id: 'test-etag-1',
        from: 'websocket',
        to: 'static-file',
        type: 'serve_file',
        timestamp: new Date(),
        data: {
          pathname: '/test-files/test.css',
          method: 'GET'
        }
      };

      const firstResponse = await daemon.handleMessage(firstMessage);
      const etag = firstResponse.data?.headers['ETag'];

      // Second request with If-None-Match
      const secondMessage: DaemonMessage = {
        id: 'test-etag-2',
        from: 'websocket',
        to: 'static-file',
        type: 'serve_file',
        timestamp: new Date(),
        data: {
          pathname: '/test-files/test.css',
          method: 'GET',
          headers: {
            'If-None-Match': etag
          }
        }
      };

      const secondResponse = await daemon.handleMessage(secondMessage);
      
      expect(secondResponse.data?.status).toBe(304);
      expect(secondResponse.data?.content).toBeUndefined();
    });
  });

  describe('TypeScript Compilation', () => {
    test('should compile TypeScript with source maps', async () => {
      const message: DaemonMessage = {
        id: 'test-ts',
        from: 'websocket',
        to: 'static-file',
        type: 'serve_file',
        timestamp: new Date(),
        data: {
          pathname: '/test-files/test.js',
          method: 'GET'
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data?.content).toContain('//# sourceMappingURL=');
    });

    test('should handle TypeScript compilation errors gracefully', async () => {
      // Create invalid TypeScript
      await fs.writeFile(
        path.join(testFilesDir, 'invalid.ts'),
        'const x: = "invalid syntax";'
      );

      const message: DaemonMessage = {
        id: 'test-ts-error',
        from: 'websocket',
        to: 'static-file',
        type: 'serve_file',
        timestamp: new Date(),
        data: {
          pathname: '/test-files/invalid.js',
          method: 'GET'
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(false);
      expect(response.error).toContain('compilation');
      expect(response.data?.status).toBe(500);
    });
  });
});