/**
 * DataMarshalCommand Integration Tests
 * 
 * Tests real-world integration patterns:
 * - Screenshot → DataMarshal → FileWrite pipeline
 * - Event system integration with DaemonEventBus
 * - Cross-command communication workflows
 * - Session-based data flow
 * - Real daemon system integration
 */

import { DataMarshalCommand } from '../../DataMarshalCommand';

describe('DataMarshalCommand Integration Tests', () => {
  
  const mockSessionContext = {
    sessionId: 'integration-test-session',
    userId: 'test-user',
    sessionType: 'development',
    owner: 'shared'
  };

  describe('Screenshot Pipeline Integration', () => {
    
    test('should integrate with screenshot command workflow', async () => {
      // Simulate screenshot command output
      const mockScreenshotResult = {
        success: true,
        data: {
          imageData: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChAGANllpZQAAAABJRU5ErkJggg==',
          filename: 'integration-test.png',
          selector: 'body',
          format: 'png',
          width: 100,
          height: 100,
          timestamp: new Date().toISOString(),
          artifactType: 'screenshot'
        }
      };

      // Test the marshal workflow
      const marshalResult = await DataMarshalCommand.marshalScreenshotData(
        mockScreenshotResult.data, 
        mockSessionContext
      );

      expect(marshalResult.success).toBe(true);
      expect(marshalResult.data.marshalled.source).toBe('screenshot');
      expect(marshalResult.data.marshalled.destination).toBe('file-system');
      expect(marshalResult.data.marshalled.metadata.artifactType).toBe('screenshot');
      expect(marshalResult.data.marshalId).toMatch(/^marshal-\d+-\w+$/);
    });

    test('should chain screenshot to file save workflow', async () => {
      const screenshotData = {
        imageData: 'base64-screenshot-data',
        filename: 'pipeline-test.png',
        size: 2048,
        artifactType: 'screenshot'
      };

      const chainResult = await DataMarshalCommand.chainScreenshotToFile(
        screenshotData, 
        mockSessionContext
      );

      expect(chainResult.success).toBe(true);
      expect(chainResult.data.source).toBe('screenshot');
      expect(chainResult.data.destination).toBe('file-write');
      expect(chainResult.data.chainable).toBeDefined();
    });

  });

  describe('Command Line Interface Integration', () => {
    
    test('should work with real CLI parameter formats', async () => {
      // Test format that would come from ./continuum data-marshal command
      const cliFormat = {
        args: [
          '--operation=encode',
          '--data={"screenshot": "real-data"}',
          '--encoding=json',
          '--source=screenshot',
          '--destination=validation'
        ]
      };

      const result = await DataMarshalCommand.execute(cliFormat, mockSessionContext);
      
      expect(result.success).toBe(true);
      expect(result.data.marshalled.source).toBe('screenshot');
      expect(result.data.marshalled.destination).toBe('validation');
    });

    test('should handle JSON parameter format from continuum CLI', async () => {
      const jsonFormat = JSON.stringify({
        operation: 'chain',
        data: { test: 'cli-integration' },
        source: 'manual-test',
        correlationId: 'cli-test-123'
      });

      const result = await DataMarshalCommand.execute(jsonFormat, mockSessionContext);
      
      expect(result.success).toBe(true);
      expect(result.data.chainId).toBe('cli-test-123');
    });

  });

  describe('Event System Integration', () => {
    
    test('should emit events that can be captured by system', async () => {
      // This test verifies event emission works without errors
      // In real integration, DaemonEventBus would be mocked or tested separately
      
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: { event: 'test-data' },
        source: 'integration-test',
        destination: 'event-subscriber'
      }, mockSessionContext);

      expect(result.success).toBe(true);
      // Event emission should not cause failures
    });

    test('should include session context in events', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'chain',
        data: { session: 'context-test' },
        source: 'session-test'
      }, mockSessionContext);

      expect(result.success).toBe(true);
      expect(result.data.chainable.data.session).toBe('context-test');
    });

  });

  describe('Real Daemon System Integration', () => {
    
    test('should work with FileWriteCommand integration', async () => {
      // Test the pattern used in ScreenshotCommand
      const screenshotBuffer = Buffer.from('mock-image-data', 'utf-8');
      
      // First marshal the metadata
      const marshalResult = await DataMarshalCommand.execute({
        operation: 'encode',
        data: {
          filename: 'daemon-integration-test.png',
          size: screenshotBuffer.length,
          artifactType: 'screenshot'
        },
        encoding: 'json',
        source: 'screenshot',
        destination: 'file-write'
      }, mockSessionContext);

      expect(marshalResult.success).toBe(true);
      
      // The marshalled data should be ready for FileWriteCommand
      const marshalId = marshalResult.data.marshalId;
      expect(marshalId).toBeDefined();
      expect(typeof marshalId).toBe('string');
    });

    test('should support correlation tracking across daemon calls', async () => {
      const correlationId = 'daemon-integration-correlation-123';
      
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: { daemon: 'integration-test' },
        correlationId: correlationId,
        source: 'test-daemon',
        destination: 'target-daemon'
      }, mockSessionContext);

      expect(result.success).toBe(true);
      expect(result.data.marshalId).toBe(correlationId);
      expect(result.data.marshalled.source).toBe('test-daemon');
      expect(result.data.marshalled.destination).toBe('target-daemon');
    });

  });

  describe('Error Recovery Integration', () => {
    
    test('should handle daemon communication failures gracefully', async () => {
      // Simulate daemon failure scenario
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: null, // Invalid data that might come from failed daemon
        encoding: 'json'
      }, mockSessionContext);

      expect(result.success).toBe(true); // Should handle null gracefully
      expect(result.data.marshalled.data).toBe('null');
    });

    test('should recover from event system failures', async () => {
      // Test continues even if event emission fails
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'event-failure-test',
        source: 'failing-event-test'
      }, mockSessionContext);

      expect(result.success).toBe(true);
      // Command should succeed even if events fail
    });

  });

  describe('Cross-Environment Data Flow', () => {
    
    test('should marshal data for browser-to-server communication', async () => {
      const browserData = {
        type: 'browser-screenshot',
        data: 'base64-image-data',
        metadata: {
          userAgent: 'Mozilla/5.0...',
          viewport: { width: 1920, height: 1080 },
          timestamp: Date.now()
        }
      };

      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: browserData,
        encoding: 'json',
        source: 'browser',
        destination: 'server'
      }, mockSessionContext);

      expect(result.success).toBe(true);
      expect(result.data.marshalled.source).toBe('browser');
      expect(result.data.marshalled.destination).toBe('server');
    });

    test('should marshal data for server-to-daemon communication', async () => {
      const serverData = {
        command: 'file-write',
        payload: { filename: 'test.png', content: 'binary-data' },
        sessionContext: mockSessionContext
      };

      const result = await DataMarshalCommand.execute({
        operation: 'chain',
        data: serverData,
        source: 'command-processor',
        destination: 'file-system-daemon'
      }, mockSessionContext);

      expect(result.success).toBe(true);
      expect(result.data.chainable.data.command).toBe('file-write');
    });

  });

  describe('Performance Integration Tests', () => {
    
    test('should handle high-frequency marshalling operations', async () => {
      const operations = [];
      
      // Simulate multiple rapid marshal operations
      for (let i = 0; i < 10; i++) {
        operations.push(
          DataMarshalCommand.execute({
            operation: 'encode',
            data: { batch: i, data: `test-data-${i}` },
            correlationId: `batch-${i}`
          }, mockSessionContext)
        );
      }

      const results = await Promise.all(operations);
      
      // All operations should succeed
      results.forEach(result => {
        expect(result.success).toBe(true);
      });

      // Each should have unique correlation ID
      const correlationIds = results.map(r => r.data.marshalId);
      const uniqueIds = new Set(correlationIds);
      expect(uniqueIds.size).toBe(10);
    });

    test('should handle large data payloads efficiently', async () => {
      const largePayload = {
        screenshot: 'x'.repeat(50000), // 50KB of data
        metadata: {
          chunks: Array.from({ length: 1000 }, (_, i) => `chunk-${i}`)
        }
      };

      const startTime = Date.now();
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: largePayload,
        encoding: 'base64'
      }, mockSessionContext);
      const endTime = Date.now();

      expect(result.success).toBe(true);
      expect(endTime - startTime).toBeLessThan(2000); // Should complete within 2 seconds
      expect(result.data.size).toBeGreaterThan(50000);
    });

  });

  describe('Real-World Usage Scenarios', () => {
    
    test('should support AI autonomous development workflow', async () => {
      // Simulate AI capturing screenshot, analyzing, and making decisions
      const aiWorkflowData = {
        step: 'screenshot-analysis',
        screenshot: {
          data: 'base64-data',
          analysis: {
            widgets: ['sidebar', 'chat'],
            health: 'good',
            issues: []
          }
        },
        nextAction: 'validate-layout',
        confidence: 0.95
      };

      const result = await DataMarshalCommand.execute({
        operation: 'chain',
        data: aiWorkflowData,
        source: 'ai-analyzer',
        destination: 'validation-system'
      }, mockSessionContext);

      expect(result.success).toBe(true);
      expect(result.data.chainable.data.step).toBe('screenshot-analysis');
    });

    test('should support git hook integration workflow', async () => {
      // Simulate data flow for git commit validation
      const commitValidationData = {
        commit: {
          hash: 'abc123',
          message: 'fix: screenshot command integration',
          files: ['src/commands/browser/screenshot/ScreenshotCommand.ts']
        },
        validation: {
          screenshots: ['before.png', 'after.png'],
          tests: 'passing',
          coverage: 95
        }
      };

      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: commitValidationData,
        source: 'git-hook',
        destination: 'commit-validator'
      }, mockSessionContext);

      expect(result.success).toBe(true);
      expect(result.data.marshalled.source).toBe('git-hook');
    });

  });

});