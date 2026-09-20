#!/usr/bin/env node
/**
 * Simple ExecCommand Integration Test
 * 
 * Tests actual JavaScript execution with proper JTAG integration.
 * This connects to the live JTAG system and executes real code.
 */

import WebSocket from 'ws';

/**
 * Test ExecCommand with real JTAG system
 */
async function testExecCommandExecution(): Promise<void> {
  console.log('🚀 Testing ExecCommand with Live JTAG System');
  console.log('=' .repeat(50));
  
  return new Promise((resolve, reject) => {
    const ws = new WebSocket('ws://localhost:9001');
    const testId = `exec-test-${Date.now()}`;
    let responseReceived = false;
    
    // Test timeout
    const timeout = setTimeout(() => {
      if (!responseReceived) {
        console.log('❌ Test timed out - no response from exec command');
        ws.close();
        reject(new Error('Test timeout'));
      }
    }, 10000);
    
    ws.on('open', () => {
      console.log('✅ WebSocket connected to JTAG system');
      
      // Create a properly structured ExecCommand request
      const execMessage = {
        type: 'request',
        endpoint: 'commands/exec',
        payload: {
          sessionId: 'test-session-id',
          context: {
            uuid: `test-context-${testId}`,
            environment: 'browser'
          },
          code: {
            type: 'inline',
            language: 'javascript',
            source: `
              // Simple test script
              console.log('🎯 ExecCommand test script is running!');
              
              const result = {
                success: true,
                message: 'ExecCommand integration test successful!',
                timestamp: new Date().toISOString(),
                testId: '${testId}',
                environment: 'browser',
                testData: {
                  numbers: [1, 2, 3],
                  calculation: 1 + 2 + 3,
                  browserInfo: typeof window !== 'undefined' ? 'Browser environment detected' : 'Non-browser environment'
                }
              };
              
              console.log('✅ Test script completed successfully');
              return result;
            `
          }
        },
        correlationId: `exec-integration-${testId}`,
        timestamp: new Date().toISOString()
      };
      
      console.log(`📤 Sending exec command (correlation: ${execMessage.correlationId})`);
      ws.send(JSON.stringify(execMessage));
    });
    
    ws.on('message', (data: Buffer) => {
      try {
        const message = JSON.parse(data.toString());
        
        if (message.correlationId === `exec-integration-${testId}`) {
          responseReceived = true;
          clearTimeout(timeout);
          
          console.log('📥 Received response from ExecCommand:');
          console.log(JSON.stringify(message, null, 2));
          
          if (message.type === 'response' && message.payload) {
            if (message.payload.success) {
              console.log('🏆 EXEC COMMAND INTEGRATION TEST PASSED!');
              console.log(`✅ Result: ${message.payload.result?.message}`);
              console.log(`✅ Test ID: ${message.payload.result?.testId}`);
              console.log(`✅ Environment: ${message.payload.result?.environment}`);
              resolve(message.payload);
            } else {
              console.log('❌ ExecCommand returned failure');
              console.log(`   Error: ${message.payload.error?.message}`);
              reject(new Error(`ExecCommand failed: ${message.payload.error?.message}`));
            }
          } else {
            console.log('❌ Unexpected response format');
            reject(new Error('Invalid response format'));
          }
          
          ws.close();
        } else {
          // Ignore unrelated messages
          console.log(`ℹ️ Ignoring unrelated message (correlation: ${message.correlationId || 'none'})`);
        }
        
      } catch (error) {
        console.error('❌ Failed to parse response:', error);
        clearTimeout(timeout);
        reject(error);
        ws.close();
      }
    });
    
    ws.on('error', (error) => {
      console.error('❌ WebSocket error:', error);
      clearTimeout(timeout);
      reject(error);
    });
    
    ws.on('close', () => {
      console.log('🔌 WebSocket connection closed');
    });
  });
}

/**
 * Main test runner
 */
async function runSimpleExecTest(): Promise<void> {
  try {
    await testExecCommandExecution();
    console.log('\n🎉 All tests completed successfully!');
    
  } catch (error) {
    console.error('\n💥 Test failed:', error instanceof Error ? error.message : String(error));
    console.log('\n🔍 Debug steps:');
    console.log('1. Make sure JTAG system is running: npm run system:start');
    console.log('2. Check browser logs: .continuum/jtag/currentUser/logs/');
    console.log('3. Check server logs for exec command registration');
    console.log('4. Verify WebSocket endpoint is accessible: ws://localhost:9001');
    
    process.exit(1);
  }
}

// Run test if this file is executed directly
if (require.main === module) {
  runSimpleExecTest().catch((error) => {
    console.error('❌ Test suite crashed:', error);
    process.exit(1);
  });
}

export { runSimpleExecTest };