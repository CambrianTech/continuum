#!/usr/bin/env tsx
/**
 * Error Serialization Validation Test
 * 
 * Tests that Error objects are properly serialized in console logging after our fix.
 * This test creates a controlled error in the browser to verify our serialization fix works.
 */

import { JTAGClientServer } from '../system/core/client/server/JTAGClientServer';
import { promises as fs } from 'fs';

class ErrorSerializationTest {
  
  async runErrorSerializationTest(): Promise<void> {
    console.log('🧪 ERROR SERIALIZATION VALIDATION TEST');
    console.log('=' .repeat(60));
    console.log('🎯 Mission: Verify Error objects are properly serialized after our fix');
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

      // Test 1: Generate an Error in browser that should be properly serialized
      console.log('');
      console.log('🔬 Test 1: Generating controlled Error in browser...');
      
      const result = await client.commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            // Create a test error that should be properly serialized
            try {
              // Call a method that doesn't exist - this will create a ReferenceError
              window.nonExistentFunction();
            } catch (error) {
              console.error('🧪 TEST ERROR (should be properly serialized):', error);
              throw error; // Re-throw so CommandDaemon handles it
            }
          `
        }
      });

      console.log('📊 Exec result:', result.success);

      // Wait a moment for logs to be written
      await new Promise(resolve => setTimeout(resolve, 2000));

      // Test 2: Check if error was properly serialized in browser logs
      console.log('');
      console.log('🔬 Test 2: Checking browser console logs for proper error serialization...');
      
      const browserLogPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      const browserErrorLogPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log';
      
      try {
        const browserLog = await fs.readFile(browserLogPath, 'utf-8');
        
        // Look for our test error
        const testErrorLines = browserLog.split('\n').filter(line => 
          line.includes('🧪 TEST ERROR') && line.includes('should be properly serialized')
        );
        
        if (testErrorLines.length > 0) {
          console.log('   ✅ Found test error in browser logs');
          console.log('   📋 Error log line:', testErrorLines[0]);
          
          // Check if the error contains actual error details (not just {})
          const hasErrorDetails = testErrorLines.some(line => 
            line.includes('ReferenceError') && 
            line.includes('nonExistentFunction') &&
            !line.includes('Failed: {}')
          );
          
          if (hasErrorDetails) {
            console.log('   ✅ SUCCESS: Error details properly serialized!');
            console.log('   🎉 Fix confirmed: Error objects no longer serialize to "{}"');
          } else {
            console.log('   ❌ STILL BROKEN: Error details missing - may still be serializing to "{}"');
          }
          
        } else {
          console.log('   ❌ Test error not found in browser logs');
        }
        
      } catch (logError) {
        console.error('   ❌ Failed to read browser logs:', logError);
      }

      // Test 3: Check server console logs as well
      console.log('');
      console.log('🔬 Test 3: Checking server console logs...');
      
      try {
        const serverLogPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/server-console-log.log';
        const serverLog = await fs.readFile(serverLogPath, 'utf-8');
        
        // Look for command daemon error handling
        const commandErrors = serverLog.split('\n').filter(line => 
          line.includes('Command execution failed') || 
          line.includes('runtime') || 
          (line.includes('error') && line.includes('exec'))
        );
        
        console.log(`   📊 Found ${commandErrors.length} potential command errors in server logs`);
        if (commandErrors.length > 0) {
          console.log('   📋 Sample:', commandErrors[0]);
        }
        
      } catch (serverLogError) {
        console.error('   ❌ Failed to read server logs:', serverLogError);
      }

      console.log('');
      console.log('✅ Error serialization test completed');
      
    } catch (error) {
      console.error('❌ Test failed:', error);
      throw error;
    }
  }
}

// Run the test if called directly
if (require.main === module) {
  const tester = new ErrorSerializationTest();
  tester.runErrorSerializationTest().catch(console.error);
}

export { ErrorSerializationTest };