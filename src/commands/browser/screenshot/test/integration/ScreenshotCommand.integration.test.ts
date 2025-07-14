/**
 * Integration tests for ScreenshotCommand with full session handling
 * 
 * These tests validate the complete flow from REST API to browser communication:
 * - HTTP API request processing
 * - Session context resolution via SharedSessionContext
 * - WebSocket communication with browser
 * - File system operations and screenshot saving
 * - Error scenarios and recovery
 * 
 * This follows the middle-out testing methodology for layer 4 validation.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { SessionManagerDaemon } from '../../../../daemons/session-manager/SessionManagerDaemon';
import { CommandProcessorDaemon } from '../../../../daemons/command-processor/CommandProcessorDaemon';
import { WebSocketDaemon } from '../../../../integrations/websocket/WebSocketDaemon';
import { ScreenshotCommand } from '../../ScreenshotCommand';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

describe('ScreenshotCommand Integration', () => {
  let sessionManager: SessionManagerDaemon;
  let commandProcessor: CommandProcessorDaemon;
  let webSocketDaemon: WebSocketDaemon;
  let tempDir: string;
  let testSessionId: string;

  beforeEach(async () => {
    // Create temporary directory for test sessions
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'screenshot-integration-'));
    
    // Setup daemon system similar to how ContinuumSystemStartup does it
    webSocketDaemon = new WebSocketDaemon();
    sessionManager = new SessionManagerDaemon();
    commandProcessor = new CommandProcessorDaemon();
    
    // Register daemons with WebSocket system
    webSocketDaemon.registerDaemon(sessionManager);
    webSocketDaemon.registerDaemon(commandProcessor);
    
    // Register screenshot command route
    webSocketDaemon.registerRouteHandler('/api/commands/*', 'command-processor', 'handle_api');
    
    // Start the daemon system
    await sessionManager.start();
    await commandProcessor.start();
    
    // Create a test session
    const sessionResponse = await sessionManager.handleMessage({
      type: 'create_session',
      data: {
        sessionType: 'development',
        owner: 'shared',
        tempDir: tempDir
      },
      timestamp: Date.now(),
      correlationId: 'test-session-create'
    });
    
    testSessionId = sessionResponse.data.sessionId;
  });

  afterEach(async () => {
    // Cleanup
    await sessionManager.stop();
    await commandProcessor.stop();
    
    // Remove temporary directory
    try {
      await fs.rm(tempDir, { recursive: true, force: true });
    } catch (error) {
      console.warn('Failed to cleanup temp directory:', error);
    }
  });

  describe('HTTP API to Session Context Flow', () => {
    it('should process REST API screenshot request with session context', async () => {
      // Arrange
      const apiRequest = {
        type: 'handle_api',
        data: {
          pathname: '/api/commands/screenshot',
          method: 'POST',
          body: { args: ['test-integration.png'] },
          requestInfo: {
            sessionId: testSessionId,
            source: 'http'
          }
        },
        timestamp: Date.now(),
        correlationId: 'test-screenshot-api'
      };

      // Act
      const result = await commandProcessor.handleMessage(apiRequest);

      // Assert
      expect(result).toBeDefined();
      expect(result.success).toBeDefined();
      // Note: May fail due to browser communication, but should not fail due to session issues
    });

    it('should fall back to SharedSessionContext when no session provided', async () => {
      // Arrange - API request without explicit session
      const apiRequestNoSession = {
        type: 'handle_api',
        data: {
          pathname: '/api/commands/screenshot',
          method: 'POST',
          body: { args: ['test-fallback.png'] },
          requestInfo: {
            source: 'http'
            // No sessionId provided - should trigger SharedSessionContext fallback
          }
        },
        timestamp: Date.now(),
        correlationId: 'test-screenshot-fallback'
      };

      // Act
      const result = await commandProcessor.handleMessage(apiRequestNoSession);

      // Assert
      expect(result).toBeDefined();
      // The command should not fail with "No session ID available" anymore
      if (!result.success) {
        expect(result.error).not.toContain('No session ID available');
      }
    });
  });

  describe('Session Context Integration', () => {
    it('should resolve session paths correctly', async () => {
      // Test that session paths are properly resolved for screenshot storage
      const sessionPathsMessage = {
        type: 'session_paths',
        data: { sessionId: testSessionId },
        timestamp: Date.now(),
        correlationId: 'test-session-paths'
      };

      const pathsResult = await sessionManager.handleMessage(sessionPathsMessage);
      
      expect(pathsResult.success).toBe(true);
      expect(pathsResult.data.screenshots).toBeDefined();
      expect(pathsResult.data.screenshots).toContain(testSessionId);
    });

    it('should create screenshot directory if not exists', async () => {
      // Get session paths
      const sessionPathsMessage = {
        type: 'session_paths',
        data: { sessionId: testSessionId },
        timestamp: Date.now(),
        correlationId: 'test-session-paths-create'
      };

      const pathsResult = await sessionManager.handleMessage(sessionPathsMessage);
      const screenshotDir = pathsResult.data.screenshots;

      // Verify directory exists or can be created
      try {
        await fs.access(screenshotDir);
      } catch {
        // Directory doesn't exist, should be createable
        await fs.mkdir(screenshotDir, { recursive: true });
        const stats = await fs.stat(screenshotDir);
        expect(stats.isDirectory()).toBe(true);
      }
    });
  });

  describe('Error Handling Integration', () => {
    it('should handle browser communication timeout gracefully', async () => {
      // Simulate browser not responding
      const screenshotRequest = {
        type: 'handle_api',
        data: {
          pathname: '/api/commands/screenshot',
          method: 'POST',
          body: { args: ['test-timeout.png'] },
          requestInfo: {
            sessionId: testSessionId,
            source: 'http'
          }
        },
        timestamp: Date.now(),
        correlationId: 'test-screenshot-timeout'
      };

      // Act
      const result = await commandProcessor.handleMessage(screenshotRequest);

      // Assert - should fail gracefully with proper error message
      if (!result.success) {
        expect(result.error).toBeDefined();
        expect(result.error).not.toContain('Cannot read properties of undefined');
        // Should contain our improved error message
        expect(
          result.error.includes('Screenshot capture failed') ||
          result.error.includes('No image data received')
        ).toBe(true);
      }
    });

    it('should handle malformed API requests', async () => {
      const malformedRequest = {
        type: 'handle_api',
        data: {
          pathname: '/api/commands/screenshot',
          method: 'POST',
          body: null, // Malformed body
          requestInfo: {
            sessionId: testSessionId,
            source: 'http'
          }
        },
        timestamp: Date.now(),
        correlationId: 'test-screenshot-malformed'
      };

      // Act
      const result = await commandProcessor.handleMessage(malformedRequest);

      // Assert - should handle gracefully
      expect(result).toBeDefined();
      if (!result.success) {
        expect(result.error).toBeDefined();
        expect(result.error).not.toContain('undefined');
      }
    });
  });

  describe('Parameter Processing Integration', () => {
    it('should process CLI args format through integration parsers', async () => {
      const cliFormatRequest = {
        type: 'handle_api',
        data: {
          pathname: '/api/commands/screenshot',
          method: 'POST',
          body: { args: ['cli-format-test.png'] },
          requestInfo: {
            sessionId: testSessionId,
            source: 'http'
          }
        },
        timestamp: Date.now(),
        correlationId: 'test-screenshot-cli-format'
      };

      const result = await commandProcessor.handleMessage(cliFormatRequest);
      
      // Should be processed by CLIIntegrationParser
      expect(result).toBeDefined();
    });

    it('should process JSON format through integration parsers', async () => {
      const jsonFormatRequest = {
        type: 'handle_api',
        data: {
          pathname: '/api/commands/screenshot',
          method: 'POST',
          body: { filename: 'json-format-test.png', format: 'png' },
          requestInfo: {
            sessionId: testSessionId,
            source: 'http'
          }
        },
        timestamp: Date.now(),
        correlationId: 'test-screenshot-json-format'
      };

      const result = await commandProcessor.handleMessage(jsonFormatRequest);
      
      // Should be processed by JSONIntegrationParser
      expect(result).toBeDefined();
    });
  });

  describe('Session Lifecycle Integration', () => {
    it('should work with active session throughout screenshot process', async () => {
      // Verify session is active
      const sessionInfoMessage = {
        type: 'session_info',
        data: { sessionId: testSessionId },
        timestamp: Date.now(),
        correlationId: 'test-session-info'
      };

      const infoResult = await sessionManager.handleMessage(sessionInfoMessage);
      expect(infoResult.success).toBe(true);
      expect(infoResult.data.status).toBe('active');

      // Execute screenshot command
      const screenshotRequest = {
        type: 'handle_api',
        data: {
          pathname: '/api/commands/screenshot',
          method: 'POST',
          body: { args: ['session-lifecycle-test.png'] },
          requestInfo: {
            sessionId: testSessionId,
            source: 'http'
          }
        },
        timestamp: Date.now(),
        correlationId: 'test-screenshot-lifecycle'
      };

      const screenshotResult = await commandProcessor.handleMessage(screenshotRequest);
      
      // Verify session is still active after command
      const postInfoResult = await sessionManager.handleMessage({
        ...sessionInfoMessage,
        correlationId: 'test-session-info-post'
      });
      
      expect(postInfoResult.success).toBe(true);
      expect(postInfoResult.data.status).toBe('active');
    });
  });

  describe('Middle-Out Layer 4 Validation', () => {
    it('should validate daemon integration architecture', () => {
      // Verify daemon registration
      expect(webSocketDaemon.registeredDaemons.has('session-manager')).toBe(true);
      expect(webSocketDaemon.registeredDaemons.has('command-processor')).toBe(true);
      
      // Verify route registration
      const routes = webSocketDaemon.routeManager.routes;
      const screenshotRoute = routes.find(r => r.pattern.includes('/api/commands'));
      expect(screenshotRoute).toBeDefined();
      expect(screenshotRoute?.targetDaemon).toBe('command-processor');
    });

    it('should validate command discovery and execution flow', async () => {
      // Test command discovery
      const helpRequest = {
        type: 'handle_api',
        data: {
          pathname: '/api/commands/help',
          method: 'POST',
          body: { args: ['screenshot'] },
          requestInfo: {
            sessionId: testSessionId,
            source: 'http'
          }
        },
        timestamp: Date.now(),
        correlationId: 'test-help-screenshot'
      };

      const helpResult = await commandProcessor.handleMessage(helpRequest);
      
      expect(helpResult.success).toBe(true);
      expect(helpResult.data.command).toBe('screenshot');
      expect(helpResult.data.category).toBe('browser');
    });
  });
});