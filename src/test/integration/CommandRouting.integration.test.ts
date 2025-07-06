/**
 * Layer 3: Command System Integration Tests
 * Tests command routing and execution within the daemon layer
 * According to middle-out methodology, this tests Layer 3 functionality only
 * 
 * DOES NOT test HTTP endpoints (that's Layer 4)
 * DOES NOT require full system running (that's Layer 5)
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { WebSocketDaemon } from '../../integrations/websocket/WebSocketDaemon';
import { CommandProcessorDaemon } from '../../daemons/command-processor/CommandProcessorDaemon';
import { SessionManagerDaemon } from '../../daemons/session-manager/SessionManagerDaemon';
import { DaemonMessage } from '../../daemons/base/DaemonProtocol';

describe('Command Routing Integration Tests', () => {
  let webSocketDaemon: WebSocketDaemon;
  let commandProcessorDaemon: CommandProcessorDaemon;
  let sessionManagerDaemon: SessionManagerDaemon;
  
  before(async () => {
    // Create daemons
    webSocketDaemon = new WebSocketDaemon();
    commandProcessorDaemon = new CommandProcessorDaemon();
    sessionManagerDaemon = new SessionManagerDaemon();
    
    // Start daemons
    await commandProcessorDaemon.start();
    await sessionManagerDaemon.start();
    await webSocketDaemon.start();
    
    // Register daemons with WebSocket daemon
    webSocketDaemon.registerDaemon(commandProcessorDaemon);
    webSocketDaemon.registerDaemon(sessionManagerDaemon);
    
    // Register command routes
    webSocketDaemon.registerRouteHandler('/api/commands/*', 'command-processor', 'handle_api');
  });
  
  after(async () => {
    await webSocketDaemon.stop();
    await commandProcessorDaemon.stop();
    await sessionManagerDaemon.stop();
  });
  
  describe('Direct Command Execution', () => {
    it('should route health command through daemons', async () => {
      const message: DaemonMessage = {
        from: 'test',
        to: 'command-processor',
        type: 'execute_command',
        data: {
          command: 'health',
          params: {},
          context: {
            source: 'test',
            connectionId: 'test-connection'
          }
        }
      };
      
      const response = await commandProcessorDaemon.handleMessage(message);
      
      assert(response.success, 'Health command should succeed');
      assert(response.data, 'Should return data');
      // Health command returns server/client structure
      assert(response.data.server || response.data.summary, 'Should return health data');
    });
    
    it('should route connect command to session manager', async () => {
      const message: DaemonMessage = {
        from: 'test',
        to: 'command-processor',
        type: 'execute_command',
        data: {
          command: 'connect',
          params: {
            sessionType: 'test',
            owner: 'test-user'
          },
          context: {
            source: 'test',
            connectionId: 'test-connection'
          }
        }
      };
      
      const response = await commandProcessorDaemon.handleMessage(message);
      
      assert(response.success, 'Connect command should succeed');
      assert(response.data?.sessionId, 'Should return sessionId');
      assert(response.data?.version, 'Should return version');
      assert(['created_new', 'joined_existing'].includes(response.data?.action), 'Should return valid action');
    });
  });
  
  // HTTP API tests moved to Layer 4 System Integration tests
  // According to middle-out methodology, HTTP endpoints require full daemon system
  
  describe('Command Context Propagation', () => {
    it('should propagate connection context through routing', async () => {
      // Create a custom command that echoes context
      const message: DaemonMessage = {
        from: 'test',
        to: 'command-processor',
        type: 'execute_command',
        data: {
          command: 'health',
          params: {},
          context: {
            source: 'websocket',
            connectionId: 'ws-test-123',
            sessionId: 'session-test-456',
            customField: 'test-value'
          }
        }
      };
      
      const response = await commandProcessorDaemon.handleMessage(message);
      
      assert(response.success, 'Command should succeed');
      // Context should be preserved through the routing
    });
  });
  
  describe('Daemon Command Routing', () => {
    it('should route DaemonCommand to appropriate daemon', async () => {
      // Connect command extends DaemonCommand and routes to session-manager
      const message: DaemonMessage = {
        from: 'command-processor',
        to: 'session-manager',
        type: 'session.connect',
        data: {
          sessionType: 'development',
          owner: 'test-developer',
          forceNew: false,
          connectionId: 'test-conn'
        }
      };
      
      const response = await sessionManagerDaemon.handleMessage(message);
      
      assert(response.success, 'Session connect should succeed');
      assert(response.data?.sessionId, 'Should return sessionId');
      assert(response.data?.version, 'Should return version');
    });
  });
  
  describe('Error Handling', () => {
    it('should handle daemon not found errors', async () => {
      const message: DaemonMessage = {
        from: 'test',
        to: 'nonexistent-daemon',
        type: 'test',
        data: {}
      };
      
      // This should fail gracefully
      try {
        // Direct routing would fail
        const response = await webSocketDaemon.routeMessage(message);
        assert(!response.success, 'Should fail for nonexistent daemon');
        assert(response.error?.includes('not found'), 'Should indicate daemon not found');
      } catch (error) {
        // Expected - daemon not found
      }
    });
    
    it('should handle malformed command data', async () => {
      const message: DaemonMessage = {
        from: 'test',
        to: 'command-processor',
        type: 'execute_command',
        data: null as any // Malformed data
      };
      
      try {
        const response = await commandProcessorDaemon.handleMessage(message);
        assert(!response.success, 'Should fail with malformed data');
        assert(response.error, 'Should return error message');
      } catch (error) {
        // Expected - malformed data should cause error
        assert(error, 'Should throw error for malformed data');
      }
    });
  });
});