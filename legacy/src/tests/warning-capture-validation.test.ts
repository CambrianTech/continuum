#!/usr/bin/env tsx
/**
 * Warning Capture Validation Test
 * 
 * Tests that console.warn() calls are properly captured and logged.
 * This creates a controlled warning in the browser to verify warning capture.
 */

import { JTAGClientServer } from '../system/core/client/server/JTAGClientServer';
import { promises as fs } from 'fs';

class WarningCaptureTest {
  
  async runWarningCaptureTest(): Promise<void> {
    console.log('🧪 WARNING CAPTURE VALIDATION TEST');
    console.log('=' .repeat(60));
    console.log('🎯 Mission: Verify console.warn() calls are captured in warning logs');
    console.log('');

    try {
      // Connect to the running JTAG system
      console.log('🔗 Connecting to JTAG system...');
      const { client } = await JTAGClientServer.connect({
        targetEnvironment: 'server',
        transportType: 'websocket',
        serverUrl: 'ws://localhost:9001'
      });
      
      console.log('✅ Connected to JTAG system');

      // Test 1: Generate warnings in browser
      console.log('');
      console.log('🔬 Test 1: Generating controlled warnings in browser...');
      
      const result = await client.commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            // Generate test warnings that should be captured
            console.warn('🧪 TEST WARNING 1: removeEventListener not fully implemented for browser WebSocket adapter');
            console.warn('🧪 TEST WARNING 2: Console logging test warning');
            console.warn('🧪 TEST WARNING 3: Browser automation test warning');
            
            return { 
              proof: 'WARNINGS_GENERATED',
              warningCount: 3,
              timestamp: new Date().toISOString()
            };
          `
        }
      });

      console.log('📊 Exec result:', result.success);
      console.log('📋 Result data:', result);

      // Wait a moment for logs to be written
      await new Promise(resolve => setTimeout(resolve, 3000));

      // Test 2: Check if warnings were captured in warning log file
      console.log('');
      console.log('🔬 Test 2: Checking browser console warning logs...');
      
      const browserWarningLogPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-warn.log';
      
      try {
        const warningLog = await fs.readFile(browserWarningLogPath, 'utf-8');
        
        console.log(`   📊 Warning log size: ${warningLog.length} bytes`);
        console.log(`   📊 Warning log lines: ${warningLog.split('\n').filter(l => l.trim()).length}`);
        
        // Look for our test warnings
        const testWarningLines = warningLog.split('\n').filter(line => 
          line.includes('🧪 TEST WARNING') && line.includes('browser')
        );
        
        console.log(`   📊 Test warnings found: ${testWarningLines.length}/3`);
        
        if (testWarningLines.length >= 3) {
          console.log('   ✅ SUCCESS: All test warnings captured in warning logs!');
          testWarningLines.forEach((line, index) => {
            console.log(`   📋 Warning ${index + 1}: ${line}`);
          });
        } else if (testWarningLines.length > 0) {
          console.log(`   ⚠️ PARTIAL: Only ${testWarningLines.length}/3 test warnings captured`);
        } else {
          console.log('   ❌ FAILED: Test warnings not found in warning logs');
        }
        
        // Check for the specific warning from our original browser devtools
        const expectedWarning = warningLog.includes('removeEventListener not fully implemented');
        console.log(`   🔍 Original WebSocket warning found: ${expectedWarning ? '✅' : '❌'}`);
        
      } catch (warningLogError) {
        console.error('   ❌ Failed to read browser warning log:', warningLogError);
        console.log('   🔍 Warning log file may not exist - warnings might not be generated yet');
      }

      // Test 3: Check server console warning logs as well
      console.log('');
      console.log('🔬 Test 3: Checking server console warning logs...');
      
      try {
        const serverWarningLogPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/server-console-warn.log';
        
        try {
          const serverWarningLog = await fs.readFile(serverWarningLogPath, 'utf-8');
          console.log(`   📊 Server warning log size: ${serverWarningLog.length} bytes`);
        } catch (serverWarningLogError) {
          console.log('   🔍 Server warning log does not exist (this is normal if no server warnings)');
        }
        
      } catch (serverLogError) {
        console.error('   ❌ Failed to check server warning logs:', serverLogError);
      }

      console.log('');
      console.log('✅ Warning capture test completed');
      
    } catch (error) {
      console.error('❌ Test failed:', error);
      throw error;
    }
  }
}

// Run the test if called directly
if (require.main === module) {
  const tester = new WarningCaptureTest();
  tester.runWarningCaptureTest().catch(console.error);
}

export { WarningCaptureTest };