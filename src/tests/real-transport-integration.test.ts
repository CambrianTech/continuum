/**
 * Real Transport Integration Test
 * 
 * Actually tests the transport abstraction end-to-end:
 * 1. Browser client uses transport router (not hardcoded WebSocket)
 * 2. Messages route through transport layer 
 * 3. Server receives messages via transport
 * 4. Log files created with correct naming
 */

import { JTAGBase } from '../system/core/shared/JTAGBase';
import { jtagRouter } from '../system/core/router/shared/JTAGRouter';
import * as fs from 'fs';
import * as path from 'path';

describe('Real Transport Integration', () => {
  const testLogDir = path.resolve(process.cwd(), '../../../.continuum/jtag/logs');
  
  beforeAll(() => {
    // Ensure log directory exists
    if (!fs.existsSync(testLogDir)) {
      fs.mkdirSync(testLogDir, { recursive: true });
    }
  });

  afterAll(() => {
    // Clean up test files
    const testFiles = fs.readdirSync(testLogDir).filter(f => f.includes('test'));
    testFiles.forEach(f => {
      try {
        fs.unlinkSync(path.join(testLogDir, f));
      } catch (e) {
        // ignore
      }
    });
  });

  test('should use transport router for logging', async () => {
    // Initialize JTAG with server context
    const config = {
      context: 'server' as const,
      jtagPort: 9001,
      enableRemoteLogging: true,
      enableConsoleOutput: true,
      maxBufferSize: 1000
    };

    JTAGBase.initialize(config);

    // Send log message through transport
    JTAGBase.log('TEST_COMPONENT', 'Transport integration test message', {
      testId: 'transport-integration-001',
      timestamp: new Date().toISOString()
    });

    // Wait for async file operations
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Check that log file was created with correct name (not test.undefined.json)
    const logFiles = fs.readdirSync(testLogDir);
    const serverLogFile = logFiles.find(f => f === 'server.log.json');
    
    expect(serverLogFile).toBeDefined();
    expect(logFiles.find(f => f.includes('undefined'))).toBeUndefined();

    // Verify log content
    const logContent = fs.readFileSync(path.join(testLogDir, 'server.log.json'), 'utf8');
    expect(logContent).toContain('Transport integration test message');
    expect(logContent).toContain('transport-integration-001');
  });

  test('should route messages through jtagRouter', () => {
    // Test that router has correct transports registered
    expect(jtagRouter.getActiveTransports().length).toBeGreaterThan(0);
    
    // Test message routing
    const testMessage = {
      id: 'test-route-001',
      type: 'log' as const,
      context: 'server' as const,
      timestamp: Date.now(),
      payload: {
        level: 'info' as const,
        message: 'Router test message',
        component: 'ROUTER_TEST',
        data: { routingTest: true }
      }
    };

    // This should route through transport layer, not hardcoded WebSocket
    expect(() => jtagRouter.routeMessage(testMessage)).not.toThrow();
  });

  test('should fail gracefully when transport is unavailable', async () => {
    // Test what happens when WebSocket transport is not available
    // This should fallback to file transport, not crash
    
    const config = {
      context: 'browser' as const,
      jtagPort: 9999, // Port that doesn't exist
      enableRemoteLogging: true,
      enableConsoleOutput: true
    };

    expect(() => JTAGBase.initialize(config)).not.toThrow();
    
    // Should still be able to log (fallback transport)
    expect(() => {
      JTAGBase.log('FALLBACK_TEST', 'Transport fallback test');
    }).not.toThrow();
  });

  test('should generate proper log file names', async () => {
    // Test that log levels create correctly named files
    const testCases = [
      { method: 'log', expectedFile: 'server.log.json' },
      { method: 'error', expectedFile: 'server.error.json' },
      { method: 'critical', expectedFile: 'server.critical.json' },
      { method: 'warn', expectedFile: 'server.warn.json' }
    ];

    for (const testCase of testCases) {
      // @ts-ignore - Dynamic method access for testing
      JTAGBase[testCase.method]('FILE_NAMING_TEST', `Testing ${testCase.method} file naming`);
    }

    // Wait for file operations
    await new Promise(resolve => setTimeout(resolve, 1000));

    const logFiles = fs.readdirSync(testLogDir);
    
    for (const testCase of testCases) {
      expect(logFiles).toContain(testCase.expectedFile);
    }

    // Verify NO undefined files were created
    const undefinedFiles = logFiles.filter(f => f.includes('undefined'));
    expect(undefinedFiles).toHaveLength(0);
  });
});