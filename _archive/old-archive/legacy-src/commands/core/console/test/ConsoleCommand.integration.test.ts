/**
 * Console Command Integration Test
 * 
 * TESTS CONSOLE CAPTURE AND FORWARDING SYSTEM:
 * ============================================
 * Validates that browser console logs are properly captured and forwarded
 * through the ConsoleCommand to the development portal
 * 
 * INTEGRATION REQUIREMENTS:
 * - Browser: Console capture system override
 * - Transport: WebSocket message forwarding  
 * - Server: ConsoleCommand processing
 * - Portal: Development feedback visibility
 */

import { ConsoleCommand } from '../ConsoleCommand.js';
import { CommandResult } from '../../../../types/CommandTypes.js';

describe('Console Command Integration', () => {
  
  describe('Console Log Processing', () => {
    test('should process browser console log forwarding', async () => {
      const mockConsoleData = {
        action: 'browser_console',
        message: '[LOG] Test log message',
        source: 'console-complete-capture',
        data: {
          type: 'log',
          timestamp: new Date().toISOString(),
          level: 'log',
          args: [
            { type: 'string', value: 'Test log message' },
            { type: 'Object', constructor: 'Object', keys: ['data'], value: { data: true } }
          ],
          message: 'Test log message { "data": true }',
          stackTrace: 'Error: \\n    at testFunction (http://localhost:9000/test.js:123:45)',
          sourceLocation: 'http://localhost:9000/test.js:123:45',
          url: 'http://localhost:9000',
          userAgent: 'Mozilla/5.0 (test)',
          viewport: { width: 1920, height: 1080 }
        }
      };

      const result: CommandResult = await ConsoleCommand.execute('console', mockConsoleData);
      
      expect(result.success).toBe(true);
      expect(result.data.message).toBe('[LOG] Test log message');
      expect(result.data.source).toBe('console-complete-capture');
      expect(result.data.logType).toBe('browser_console');
    });

    test('should process browser console error forwarding', async () => {
      const mockErrorData = {
        action: 'browser_error',
        message: '[ERROR] Test error message',
        source: 'console-error-capture', 
        data: {
          type: 'error',
          timestamp: new Date().toISOString(),
          args: [
            {
              type: 'Error',
              name: 'Error',
              message: 'Test error message',
              stack: 'Error: Test error message\\n    at testFunction (http://localhost:9000/test.js:123:45)'
            }
          ],
          message: 'Test error message',
          stackTrace: 'Error: Test error message\\n    at testFunction (http://localhost:9000/test.js:123:45)',
          sourceLocation: 'http://localhost:9000/test.js:123:45',
          url: 'http://localhost:9000',
          userAgent: 'Mozilla/5.0 (test)'
        }
      };

      const result: CommandResult = await ConsoleCommand.execute('console', mockErrorData);
      
      expect(result.success).toBe(true);
      expect(result.data.message).toBe('[ERROR] Test error message');
      expect(result.data.source).toBe('console-error-capture');
      expect(result.data.logType).toBe('browser_error');
    });

    test('should process health report forwarding', async () => {
      const mockHealthData = {
        action: 'health_report',
        message: 'Client health: healthy (6 components)',
        source: 'health-validator',
        data: {
          timestamp: Date.now(),
          environment: 'browser',
          overall: 'healthy',
          components: [
            {
              component: 'console-capture-system',
              status: 'healthy',
              details: 'Console capture: 6/6 tests passed',
              metrics: {
                consoleCaptureWorking: true,
                consoleTestResults: {
                  logOverride: true,
                  errorOverride: true,
                  stackTraceGeneration: true,
                  sourceLocationDetection: true,
                  dataInspection: true,
                  forwardingCapability: true
                }
              }
            }
          ]
        }
      };

      const result: CommandResult = await ConsoleCommand.execute('console', mockHealthData);
      
      expect(result.success).toBe(true);
      expect(result.data.message).toBe('Client health: healthy (6 components)');
      expect(result.data.source).toBe('health-validator');
      expect(result.data.logType).toBe('health_report');
    });
  });

  describe('Console Integration Validation', () => {
    test('should validate console command definition', () => {
      const definition = ConsoleCommand.getDefinition();
      
      expect(definition.name).toBe('console');
      expect(definition.description).toContain('bridge browser console logs');
      expect(definition.category).toBe('core');
      expect(definition.parameters.action).toBeDefined();
      expect(definition.parameters.message).toBeDefined();
      expect(definition.parameters.source).toBeDefined();
    });

    test('should handle invalid console data gracefully', async () => {
      const invalidData = {
        action: 'invalid_action',
        message: '',
        source: ''
      };

      const result: CommandResult = await ConsoleCommand.execute('console', invalidData);
      
      // Should still succeed but handle invalid data appropriately
      expect(result.success).toBe(true);
      expect(result.data.logType).toBe('invalid_action');
    });
  });

  describe('Development Portal Integration', () => {
    test('should format console data for development portal', async () => {
      const complexConsoleData = {
        action: 'browser_console',
        message: '[LOG] Complex object test',
        source: 'console-complete-capture',
        data: {
          type: 'log',
          timestamp: new Date().toISOString(),
          args: [
            {
              type: 'Object',
              constructor: 'Object',
              keys: ['nested', 'array', 'function'],
              value: {
                nested: { deep: { data: true } },
                array: [1, 2, { inner: 'value' }],
                function: '[Function testFunc]'
              }
            }
          ],
          message: 'Complex object test',
          stackTrace: 'Error: \\n    at Object.<anonymous> (http://localhost:9000/test.js:456:78)',
          sourceLocation: 'http://localhost:9000/test.js:456:78'
        }
      };

      const result: CommandResult = await ConsoleCommand.execute('console', complexConsoleData);
      
      expect(result.success).toBe(true);
      expect(result.data.consoleArgs).toBeDefined();
      expect(result.data.stackTrace).toBeDefined();
      expect(result.data.sourceLocation).toBeDefined();
      
      // Verify complex data is preserved
      const preservedData = result.data.consoleArgs[0];
      expect(preservedData.type).toBe('Object');
      expect(preservedData.keys).toContain('nested');
      expect(preservedData.keys).toContain('array');
    });
  });
});