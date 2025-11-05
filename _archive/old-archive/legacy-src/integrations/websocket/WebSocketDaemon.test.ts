/**
 * WebSocket Daemon Unit Tests
 * Tests message type discovery and daemon registration to verify Academy daemon integration
 */

import { WebSocketDaemon } from './WebSocketDaemon.js';
import { AcademyDaemon } from '../../daemons/academy/AcademyDaemon.js';
import { DynamicMessageRouter } from './core/DynamicMessageRouter.js';
import { WebSocket } from 'ws';

describe('WebSocketDaemon Message Type Discovery', () => {
  let webSocketDaemon: WebSocketDaemon;
  let academyDaemon: AcademyDaemon;
  let messageRouter: DynamicMessageRouter;
  const testPort = 9005;

  beforeEach(async () => {
    webSocketDaemon = new WebSocketDaemon({ 
      port: testPort,
      daemonConfig: { autoConnect: false } 
    });
    academyDaemon = new AcademyDaemon();
    messageRouter = new DynamicMessageRouter();
    
    await academyDaemon.start();
  });

  afterEach(async () => {
    await academyDaemon.stop();
    if (webSocketDaemon.getSimpleStatus() === 'running') {
      await webSocketDaemon.stop();
    }
  });

  describe('Academy Daemon Registration', () => {
    test('should register Academy daemon with WebSocket daemon', async () => {
      // Register Academy daemon with WebSocket daemon
      await webSocketDaemon.registerExternalDaemon('academy', academyDaemon);
      
      const systemStatus = webSocketDaemon.getSystemStatus();
      
      expect(systemStatus.registeredDaemons).toContain('academy');
      expect(systemStatus.dynamicRouter.registeredDaemons).toBeGreaterThan(0);
    });

    test('should discover Academy daemon capabilities', async () => {
      await messageRouter.registerDaemon('academy', academyDaemon);
      
      const systemStatus = messageRouter.getSystemStatus();
      const academyRegistration = systemStatus.daemons.find(d => d.name === 'academy');
      
      expect(academyRegistration).toBeDefined();
      expect(academyRegistration.capabilities).toEqual([
        'academy-management',
        'persona-training',
        'progress-tracking',
        'academy-ui-integration'
      ]);
    });

    test('should discover ALL Academy daemon message types', async () => {
      await messageRouter.registerDaemon('academy', academyDaemon);
      
      const systemStatus = messageRouter.getSystemStatus();
      const academyRegistration = systemStatus.daemons.find(d => d.name === 'academy');
      
      expect(academyRegistration).toBeDefined();
      
      // CRITICAL: These Academy message types should be discovered
      const requiredAcademyTypes = [
        'get_initial_academy_status',
        'academy_message',
        'get_training_progress',
        'start_training',
        'stop_training'
      ];
      
      requiredAcademyTypes.forEach(messageType => {
        expect(academyRegistration.messageTypes).toContain(messageType);
      });
    });

    test('should route Academy message types to Academy daemon', async () => {
      await messageRouter.registerDaemon('academy', academyDaemon);
      
      // Test routing of get_initial_academy_status
      const academyStatusMessage = {
        type: 'get_initial_academy_status',
        data: {},
        timestamp: new Date().toISOString(),
        requestId: 'test-1'
      };
      
      const response = await messageRouter.routeMessage(academyStatusMessage, 'test-client');
      
      expect(response).toBeDefined();
      expect(response.type).toBe('get_initial_academy_status_response');
      expect(response.processedBy).toBe('academy');
      expect(response.data).toBeDefined();
    });

    test('should route academy_message to Academy daemon', async () => {
      await messageRouter.registerDaemon('academy', academyDaemon);
      
      const academyMessage = {
        type: 'academy_message',
        data: { action: 'get_status' },
        timestamp: new Date().toISOString(),
        requestId: 'test-2'
      };
      
      const response = await messageRouter.routeMessage(academyMessage, 'test-client');
      
      expect(response).toBeDefined();
      expect(response.type).toBe('academy_message_response');
      expect(response.processedBy).toBe('academy');
      expect(response.data).toBeDefined();
    });

    test('should handle all Academy message types without errors', async () => {
      await messageRouter.registerDaemon('academy', academyDaemon);
      
      const academyMessageTypes = [
        'get_initial_academy_status',
        'academy_message',
        'get_training_progress',
        'start_training',
        'stop_training'
      ];
      
      for (const messageType of academyMessageTypes) {
        let testData = {};
        
        // Provide appropriate test data for each message type
        if (messageType === 'academy_message') {
          testData = { action: 'get_status' };
        } else if (messageType === 'start_training' || messageType === 'stop_training') {
          testData = { personaId: 'test-persona' };
        } else if (messageType === 'get_training_progress') {
          testData = { personaId: 'test-persona' };
        }

        const message = {
          type: messageType,
          data: testData,
          timestamp: new Date().toISOString(),
          requestId: `test-${messageType}`
        };

        const response = await messageRouter.routeMessage(message, 'test-client');
        
        // Should get successful response, not error
        expect(response).toBeDefined();
        expect(response.type).toBe(`${messageType}_response`);
        expect(response.processedBy).toBe('academy');
        expect(response.type).not.toBe('error');
      }
    });
  });

  describe('Message Router Discovery Issues', () => {
    test('should not return error for registered Academy message types', async () => {
      await messageRouter.registerDaemon('academy', academyDaemon);
      
      const academyStatusMessage = {
        type: 'get_initial_academy_status',
        data: {},
        timestamp: new Date().toISOString(),
        requestId: 'test-discovery'
      };
      
      const response = await messageRouter.routeMessage(academyStatusMessage, 'test-client');
      
      // This should NOT be an error response
      expect(response.type).not.toBe('error');
      expect(response.data.error).toBeUndefined();
    });

    test('should show all registered message types in system status', async () => {
      await messageRouter.registerDaemon('academy', academyDaemon);
      
      const allMessageTypes = messageRouter.getAllMessageTypes();
      
      // Academy message types should be included
      expect(allMessageTypes).toContain('get_initial_academy_status');
      expect(allMessageTypes).toContain('academy_message');
      expect(allMessageTypes).toContain('get_training_progress');
      expect(allMessageTypes).toContain('start_training');
      expect(allMessageTypes).toContain('stop_training');
    });

    test('should have proper daemon discovery method', async () => {
      // Test that Academy daemon has required methods for discovery
      expect(typeof academyDaemon.getMessageTypes).toBe('function');
      expect(typeof academyDaemon.getCapabilities).toBe('function');
      expect(typeof academyDaemon.handleMessage).toBe('function');
      
      // Test that getMessageTypes returns the expected types
      const messageTypes = academyDaemon.getMessageTypes();
      expect(Array.isArray(messageTypes)).toBe(true);
      expect(messageTypes.length).toBeGreaterThan(0);
      
      const expectedTypes = [
        'get_initial_academy_status',
        'academy_message',
        'get_training_progress',
        'start_training',
        'stop_training'
      ];
      
      expectedTypes.forEach(type => {
        expect(messageTypes).toContain(type);
      });
    });
  });

  describe('Integration with WebSocket System', () => {
    test('should register Academy daemon with full WebSocket system', async () => {
      // Start WebSocket daemon
      await webSocketDaemon.start();
      
      // Register Academy daemon
      await webSocketDaemon.registerExternalDaemon('academy', academyDaemon);
      
      const systemStatus = webSocketDaemon.getSystemStatus();
      
      // Verify Academy daemon is registered
      expect(systemStatus.registeredDaemons).toContain('academy');
      
      // Verify Academy message types are in dynamic router
      const academyDaemon = systemStatus.dynamicRouter.daemons.find(d => d.name === 'academy');
      expect(academyDaemon).toBeDefined();
      expect(academyDaemon.messageTypes).toContain('get_initial_academy_status');
      expect(academyDaemon.messageTypes).toContain('academy_message');
    });
  });

  describe('Error Scenarios', () => {
    test('should handle missing daemon gracefully', async () => {
      const unknownMessage = {
        type: 'unknown_academy_message',
        data: {},
        timestamp: new Date().toISOString(),
        requestId: 'test-unknown'
      };
      
      const response = await messageRouter.routeMessage(unknownMessage, 'test-client');
      
      expect(response.type).toBe('error');
      expect(response.data.error).toContain('No daemon registered for message type');
      expect(response.data.availableTypes).toBeDefined();
    });

    test('should provide helpful error message with available types', async () => {
      await messageRouter.registerDaemon('academy', academyDaemon);
      
      const unknownMessage = {
        type: 'totally_unknown_message',
        data: {},
        timestamp: new Date().toISOString(),
        requestId: 'test-helpful-error'
      };
      
      const response = await messageRouter.routeMessage(unknownMessage, 'test-client');
      
      expect(response.type).toBe('error');
      expect(response.data.availableTypes).toContain('get_initial_academy_status');
      expect(response.data.availableTypes).toContain('academy_message');
    });
  });
});