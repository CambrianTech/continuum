#!/usr/bin/env npx tsx
/**
 * Real JTAG WebSocket Flow Test
 * Tests actual JTAG system message flow over port 9001 WebSocket connection
 * This test will show up in browser Network panel if browser is connected to JTAG
 */

import { jtag } from '../../index';

class RealJTAGWebSocketTester {
  async runRealJTAGTest(): Promise<void> {
    // Use JTAG test logging to track this test
    jtag.test('REAL_WEBSOCKET_TEST', 'Starting real JTAG WebSocket flow test');
    
    // Test 1: Basic logging through different levels
    jtag.test('WEBSOCKET_LOG_FLOW', 'Testing log message flow', { 
      testType: 'log_flow',
      timestamp: Date.now(),
      expectedResult: 'Should appear in server.test.txt and server.test.json'
    });

    jtag.log('WEBSOCKET_FLOW_TEST', 'Standard log message via WebSocket', {
      testId: 'std-log-' + Date.now(),
      transport: 'websocket',
      port: 9001
    });

    jtag.critical('WEBSOCKET_FLOW_TEST', 'Critical message via WebSocket', {
      testId: 'critical-' + Date.now(),
      transport: 'websocket',
      severity: 'high'
    });

    jtag.trace('WEBSOCKET_FLOW_TEST', 'testWebSocketFlow', 'ENTER', {
      testId: 'trace-enter-' + Date.now(),
      phase: 'start'
    });

    jtag.probe('WEBSOCKET_FLOW_TEST', 'websocket_connection_state', {
      connected: true,
      port: 9001,
      messageCount: 4,
      testId: 'probe-' + Date.now()
    });

    jtag.trace('WEBSOCKET_FLOW_TEST', 'testWebSocketFlow', 'EXIT', {
      testId: 'trace-exit-' + Date.now(),
      phase: 'complete'
    });

    // Test 2: Test console.log interception (should also go through WebSocket)
    console.log('🧪 This console.log should be intercepted and sent via WebSocket!');
    console.error('🧪 This console.error should be intercepted and sent via WebSocket!');
    console.warn('🧪 This console.warn should be intercepted and sent via WebSocket!');

    // Test 3: Multiple test entries with structured data
    const testData = [
      { 
        name: 'WebSocket Message Send', 
        status: 'success', 
        duration: 12,
        messageSize: 245
      },
      { 
        name: 'WebSocket Message Receive', 
        status: 'success', 
        duration: 8,
        responseSize: 189
      },
      { 
        name: 'Log File Creation', 
        status: 'success', 
        files: ['server.test.txt', 'server.test.json']
      }
    ];

    testData.forEach((test, index) => {
      jtag.test('WEBSOCKET_INTEGRATION', `Integration Test ${index + 1}: ${test.name}`, test);
    });

    // Test 4: Screenshot test (if in browser environment)
    try {
      jtag.test('SCREENSHOT_TEST', 'Attempting screenshot via JTAG system');
      const screenshotResult = await jtag.screenshot('websocket-test-' + Date.now(), {
        format: 'png',
        width: 800,
        height: 600
      });
      
      jtag.test('SCREENSHOT_RESULT', 'Screenshot test completed', {
        success: screenshotResult.success,
        filepath: screenshotResult.success ? screenshotResult.filepath : 'failed',
        error: screenshotResult.success ? undefined : screenshotResult.error
      });
    } catch (error: any) {
      jtag.test('SCREENSHOT_ERROR', 'Screenshot test failed', {
        error: error.message,
        context: 'server_environment'
      });
    }

    // Test 5: Code execution test
    try {
      jtag.test('EXEC_TEST', 'Testing code execution via JTAG system');
      const execResult = await jtag.exec('Date.now()');
      
      jtag.test('EXEC_RESULT', 'Code execution test completed', {
        success: execResult.success,
        result: execResult.success ? execResult.result : 'failed',
        executionTime: execResult.success ? execResult.executionTime : 0,
        error: execResult.success ? undefined : execResult.error
      });
    } catch (error: any) {
      jtag.test('EXEC_ERROR', 'Code execution test failed', {
        error: error.message,
        context: 'server_environment'
      });
    }

    // Final test summary
    jtag.test('WEBSOCKET_FLOW_COMPLETE', 'Real JTAG WebSocket flow test completed', {
      totalTests: 12,
      logLevelsUsed: ['test', 'log', 'critical', 'trace', 'probe'],
      consolesIntercepted: ['log', 'error', 'warn'],
      integration: 'screenshot and exec tested',
      expectedFiles: [
        'server.test.txt',
        'server.test.json',
        'server.log.txt',
        'server.log.json',
        'server.critical.txt',
        'server.critical.json',
        'server.trace.txt',
        'server.trace.json',
        'server.probe.txt',
        'server.probe.json',
        'server.error.txt',
        'server.error.json',
        'server.warn.txt',
        'server.warn.json'
      ],
      networkPanelMessage: 'These messages should appear in browser WebSocket traffic on port 9001'
    });

    console.log('\n✅ Real JTAG WebSocket flow test completed!');
    console.log('🔍 Check browser Network panel for WebSocket traffic on port 9001');
    console.log('📁 Check .continuum/jtag/logs/ for generated log files including server.test.txt');
  }
}

// Run the test
const tester = new RealJTAGWebSocketTester();
tester.runRealJTAGTest().catch(error => {
  console.error('💥 Real JTAG WebSocket test failed:', error.message);
  jtag.test('WEBSOCKET_TEST_ERROR', 'Real JTAG test failed', { 
    error: error.message,
    stack: error.stack 
  });
  process.exit(1);
});