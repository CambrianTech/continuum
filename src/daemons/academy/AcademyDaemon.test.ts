/**
 * Academy Daemon Unit Tests
 * Verifies message type registration, handling, and integration
 */

import { AcademyDaemon } from './AcademyDaemon.js';
import { DaemonMessage } from '../base/DaemonProtocol.js';

describe('AcademyDaemon', () => {
  let daemon: AcademyDaemon;

  beforeEach(async () => {
    daemon = new AcademyDaemon();
    await daemon.start();
  });

  afterEach(async () => {
    await daemon.stop();
  });

  describe('Message Type Registration', () => {
    test('should declare all required message types', () => {
      const expectedMessageTypes = [
        'get_initial_academy_status',
        'academy_message', 
        'get_training_progress',
        'start_training',
        'stop_training'
      ];

      const actualMessageTypes = daemon.getMessageTypes();
      
      expectedMessageTypes.forEach(messageType => {
        expect(actualMessageTypes).toContain(messageType);
      });
    });

    test('should have getMessageTypes method', () => {
      expect(typeof daemon.getMessageTypes).toBe('function');
    });

    test('should return array from getMessageTypes', () => {
      const messageTypes = daemon.getMessageTypes();
      expect(Array.isArray(messageTypes)).toBe(true);
      expect(messageTypes.length).toBeGreaterThan(0);
    });
  });

  describe('Capabilities', () => {
    test('should declare Academy capabilities', () => {
      const expectedCapabilities = [
        'academy-management',
        'persona-training', 
        'progress-tracking',
        'academy-ui-integration'
      ];

      const actualCapabilities = daemon.getCapabilities();
      
      expectedCapabilities.forEach(capability => {
        expect(actualCapabilities).toContain(capability);
      });
    });

    test('should handle get_capabilities message', async () => {
      const message: DaemonMessage = {
        type: 'get_capabilities',
        data: {}
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data).toBeDefined();
    });
  });

  describe('Academy Status Messages', () => {
    test('should handle get_initial_academy_status', async () => {
      const message: DaemonMessage = {
        type: 'get_initial_academy_status',
        data: {}
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.status).toBeDefined();
      expect(response.data.timestamp).toBeDefined();
      expect(response.data.version).toBe('1.0.0');
    });

    test('should handle academy_message with get_status action', async () => {
      const message: DaemonMessage = {
        type: 'academy_message',
        data: { action: 'get_status' }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.status).toBeDefined();
    });

    test('should handle academy_message with update_progress action', async () => {
      const message: DaemonMessage = {
        type: 'academy_message',
        data: { 
          action: 'update_progress',
          personaId: 'test-persona',
          progress: 75
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.updated).toBe(true);
    });

    test('should handle academy_message with set_mode action', async () => {
      const message: DaemonMessage = {
        type: 'academy_message',
        data: { 
          action: 'set_mode',
          mode: 'training'
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.mode).toBe('training');
    });
  });

  describe('Training Management', () => {
    test('should handle start_training message', async () => {
      const message: DaemonMessage = {
        type: 'start_training',
        data: { 
          personaId: 'test-persona',
          config: { threshold: 80 }
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.personaId).toBe('test-persona');
      expect(response.data.status).toBe('training_started');
    });

    test('should handle stop_training message', async () => {
      // First start training
      await daemon.handleMessage({
        type: 'start_training',
        data: { personaId: 'test-persona' }
      });

      // Then stop it
      const message: DaemonMessage = {
        type: 'stop_training',
        data: { personaId: 'test-persona' }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.personaId).toBe('test-persona');
      expect(response.data.status).toBe('training_stopped');
    });

    test('should handle get_training_progress message', async () => {
      // Set up some progress first
      await daemon.handleMessage({
        type: 'academy_message',
        data: { 
          action: 'update_progress',
          personaId: 'test-persona',
          progress: 65
        }
      });

      const message: DaemonMessage = {
        type: 'get_training_progress',
        data: { personaId: 'test-persona' }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true);
      expect(response.data.personaId).toBe('test-persona');
      expect(response.data.progress).toBe(65);
    });
  });

  describe('Error Handling', () => {
    test('should handle unknown message types', async () => {
      const message: DaemonMessage = {
        type: 'unknown_message_type',
        data: {}
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(false);
      expect(response.error).toContain('Unknown Academy message type');
    });

    test('should handle unknown academy actions', async () => {
      const message: DaemonMessage = {
        type: 'academy_message',
        data: { action: 'unknown_action' }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(false);
      expect(response.error).toContain('Unknown Academy action');
    });

    test('should validate set_mode values', async () => {
      const message: DaemonMessage = {
        type: 'academy_message',
        data: { 
          action: 'set_mode',
          mode: 'invalid_mode'
        }
      };

      const response = await daemon.handleMessage(message);
      
      expect(response.success).toBe(true); // Should not error, just ignore invalid mode
    });
  });

  describe('Integration Tests', () => {
    test('should be discoverable by message router', () => {
      // Test that the daemon has the required methods for discovery
      expect(typeof daemon.handleMessage).toBe('function');
      expect(typeof daemon.getMessageTypes).toBe('function');
      expect(typeof daemon.getCapabilities).toBe('function');
    });

    test('should have consistent message type declarations', () => {
      const declaredTypes = daemon.getMessageTypes();
      
      // All declared types should be testable
      const testableTypes = [
        'get_initial_academy_status',
        'academy_message',
        'get_training_progress', 
        'start_training',
        'stop_training'
      ];

      testableTypes.forEach(type => {
        expect(declaredTypes).toContain(type);
      });
    });

    test('should handle all declared message types without errors', async () => {
      const messageTypes = daemon.getMessageTypes();
      
      for (const messageType of messageTypes) {
        let testData = {};
        
        // Provide appropriate test data for each message type
        if (messageType === 'academy_message') {
          testData = { action: 'get_status' };
        } else if (messageType === 'start_training' || messageType === 'stop_training') {
          testData = { personaId: 'test-persona' };
        } else if (messageType === 'get_training_progress') {
          testData = { personaId: 'test-persona' };
        }

        const message: DaemonMessage = {
          type: messageType,
          data: testData
        };

        const response = await daemon.handleMessage(message);
        
        // Should not throw errors and should return a response
        expect(response).toBeDefined();
        expect(typeof response.success).toBe('boolean');
      }
    });
  });

  describe('State Management', () => {
    test('should maintain training progress state', async () => {
      const personaId = 'test-persona';
      const progress = 85;

      // Update progress
      await daemon.handleMessage({
        type: 'academy_message',
        data: { 
          action: 'update_progress',
          personaId,
          progress
        }
      });

      // Retrieve progress
      const response = await daemon.handleMessage({
        type: 'get_training_progress',
        data: { personaId }
      });

      expect(response.success).toBe(true);
      expect(response.data.progress).toBe(progress);
    });

    test('should track academy mode changes', async () => {
      // Set to training mode
      await daemon.handleMessage({
        type: 'academy_message',
        data: { 
          action: 'set_mode',
          mode: 'training'
        }
      });

      // Check status
      const response = await daemon.handleMessage({
        type: 'get_initial_academy_status',
        data: {}
      });

      expect(response.success).toBe(true);
      expect(response.data.status.academyMode).toBe('training');
    });
  });
});