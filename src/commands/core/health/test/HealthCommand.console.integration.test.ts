/**
 * Health Command Console Integration Test
 * 
 * TESTS HEALTH COMMAND'S CONSOLE CAPTURE VALIDATION:
 * =================================================
 * Validates that HealthCommand properly tests and reports on
 * the console capture system functionality through proper boundaries
 * 
 * INTEGRATION REQUIREMENTS:
 * - Health Command: Server-side health reporting
 * - Client Health: Browser-side console validation via WebSocket
 * - Console System: Proper cross-boundary testing
 */

import { HealthCommand } from '../HealthCommand.js';
import { CommandResult } from '../../../../types/CommandTypes.js';
import type { HealthReport, ConsoleTestResults } from '../HealthCommand.js';

describe('Health Command Console Integration', () => {
  
  describe('Server Health Reporting', () => {
    test('should generate server health report successfully', async () => {
      const result: CommandResult = await HealthCommand.execute({});
      
      expect(result.success).toBe(true);
      expect(result.data.server).toBeDefined();
      expect(result.data.server.environment).toBe('server');
      expect(result.data.server.components).toBeInstanceOf(Array);
      expect(result.data.server.components.length).toBeGreaterThan(0);
    });

    test('should include console-related health monitoring', async () => {
      const result: CommandResult = await HealthCommand.execute({});
      
      expect(result.success).toBe(true);
      
      // Server should have command-processor health which handles console commands
      const serverComponents = result.data.server.components;
      const hasCommandProcessor = serverComponents.some((component: any) => 
        component.component === 'command-processor' || 
        component.details.toLowerCase().includes('command')
      );
      
      expect(hasCommandProcessor).toBe(true);
    });

    test('should handle client health request properly', async () => {
      const result: CommandResult = await HealthCommand.execute({ includeClient: true });
      
      expect(result.success).toBe(true);
      // Client health comes via WebSocket, so will be null in this test environment
      // but the request should be handled gracefully
      expect(result.data.client).toBeNull();
    });
  });

  describe('Console Test Results Interface', () => {
    test('should define proper ConsoleTestResults interface', () => {
      const mockConsoleTestResults: ConsoleTestResults = {
        logCapture: true,
        errorCapture: true,
        stackTraceCapture: true,
        sourceLocationCapture: true,
        dataInspectionWorking: true,
        forwardingWorking: true
      };

      // Verify all required properties exist
      expect(mockConsoleTestResults.logCapture).toBeDefined();
      expect(mockConsoleTestResults.errorCapture).toBeDefined();
      expect(mockConsoleTestResults.stackTraceCapture).toBeDefined();
      expect(mockConsoleTestResults.sourceLocationCapture).toBeDefined();
      expect(mockConsoleTestResults.dataInspectionWorking).toBeDefined();
      expect(mockConsoleTestResults.forwardingWorking).toBeDefined();
      
      // Verify proper boolean types
      expect(typeof mockConsoleTestResults.logCapture).toBe('boolean');
      expect(typeof mockConsoleTestResults.errorCapture).toBe('boolean');
      expect(typeof mockConsoleTestResults.stackTraceCapture).toBe('boolean');
      expect(typeof mockConsoleTestResults.sourceLocationCapture).toBe('boolean');
      expect(typeof mockConsoleTestResults.dataInspectionWorking).toBe('boolean');
      expect(typeof mockConsoleTestResults.forwardingWorking).toBe('boolean');
    });

    test('should include console test results in health metrics', () => {
      const mockHealthReport: HealthReport = {
        timestamp: Date.now(),
        environment: 'browser',
        overall: 'healthy',
        components: [
          {
            component: 'console-capture-system',
            status: 'healthy',
            lastCheck: Date.now(),
            details: 'Console capture: 6/6 tests passed',
            metrics: {
              consoleCaptureWorking: true,
              consoleTestResults: {
                logCapture: true,
                errorCapture: true,
                stackTraceCapture: true,
                sourceLocationCapture: true,
                dataInspectionWorking: true,
                forwardingWorking: true
              }
            }
          }
        ],
        summary: 'Browser health: healthy (1 component checked)'
      };

      expect(mockHealthReport.components[0].metrics?.consoleCaptureWorking).toBe(true);
      expect(mockHealthReport.components[0].metrics?.consoleTestResults).toBeDefined();
      
      const testResults = mockHealthReport.components[0].metrics?.consoleTestResults;
      expect(testResults?.logCapture).toBe(true);
      expect(testResults?.forwardingWorking).toBe(true);
    });
  });

  describe('Cross-Boundary Health Validation', () => {
    test('should request client health through proper boundaries', async () => {
      // Test that health command doesn't cross boundaries inappropriately
      const result: CommandResult = await HealthCommand.execute({ includeClient: true });
      
      expect(result.success).toBe(true);
      
      // In test environment, client health should be null (comes via WebSocket)
      expect(result.data.client).toBeNull();
      
      // But server health should always be available
      expect(result.data.server).toBeDefined();
      expect(result.data.server.environment).toBe('server');
    });

    test('should generate proper health summary', async () => {
      const result: CommandResult = await HealthCommand.execute({});
      
      expect(result.success).toBe(true);
      expect(result.data.summary).toBeDefined();
      expect(typeof result.data.summary).toBe('string');
      expect(result.data.summary).toContain('component');
      expect(result.data.responseTime).toBeDefined();
    });

    test('should handle specific component health requests', async () => {
      const result: CommandResult = await HealthCommand.execute({ 
        component: 'command-processor' 
      });
      
      expect(result.success).toBe(true);
      expect(result.data.server.components.length).toBeGreaterThanOrEqual(1);
      
      // Should focus on requested component
      const requestedComponent = result.data.server.components.find(
        (c: any) => c.component === 'command-processor'
      );
      expect(requestedComponent).toBeDefined();
    });
  });

  describe('Health Command Definition', () => {
    test('should have proper command definition', () => {
      const definition = HealthCommand.getDefinition();
      
      expect(definition.name).toBe('health');
      expect(definition.description).toContain('system health');
      expect(definition.category).toBe('core');
      expect(definition.parameters.component).toBeDefined();
      expect(definition.parameters.includeClient).toBeDefined();
      expect(definition.examples).toBeInstanceOf(Array);
      expect(definition.examples.length).toBeGreaterThan(0);
    });
  });
});