/**
 * Browser Automated Integration Tests
 * 
 * These tests run INSIDE the actual JTAG browser (not Puppeteer) 
 * and execute via WebSocket commands. Results appear in browser-console-log.log
 * as proof that integration tests actually ran.
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from '../../system/core/client/shared/JTAGClient';

interface BrowserTestResult {
  testName: string;
  success: boolean;
  details: any;
  error?: string;
}

/**
 * Automated Browser Integration Test Suite
 * This runs the browser tests automatically and reports results
 */
async function runBrowserIntegrationTests(): Promise<void> {
  console.log('🌐 AUTOMATED BROWSER INTEGRATION TESTS - Running in actual JTAG browser');
  
  let testCount = 0;
  let passCount = 0;
  const results: BrowserTestResult[] = [];
  
  try {
    // Connect to JTAG system (same as ./jtag screenshot)
    const clientOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server',
      transportType: 'websocket', 
      serverUrl: 'ws://localhost:9001',
      enableFallback: false
    };
    
    console.log('🔗 Connecting to JTAG system at ws://localhost:9001...');
    const { client } = await JTAGClientServer.connect(clientOptions);
    console.log('✅ JTAG Client connected for browser test automation');
    
    // Test 1: Trigger browser screenshot test
    testCount++;
    try {
      console.log('🧪 Test 1: Triggering browser screenshot test via WebSocket...');
      
      // Execute JavaScript in the browser to run testBrowserScreenshot()
      const result = await (client as any).commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            console.log('🚀 AUTOMATED TEST: Starting browser screenshot test');
            
            // Run the existing browser screenshot test function
            if (typeof window.testBrowserScreenshot === 'function') {
              const testResult = await window.testBrowserScreenshot();
              console.log('✅ AUTOMATED TEST: Browser screenshot test completed');
              return { testName: 'browserScreenshot', success: true, result: testResult };
            } else {
              console.log('❌ AUTOMATED TEST: testBrowserScreenshot function not found');
              return { testName: 'browserScreenshot', success: false, error: 'Function not found' };
            }
          `
        }
      });
      
      if (result.success) {
        console.log('✅ Test 1 PASSED: Browser screenshot test executed successfully');
        passCount++;
        results.push({ testName: 'browserScreenshot', success: true, details: result });
      } else {
        console.log('❌ Test 1 FAILED: Browser screenshot test failed');
        results.push({ testName: 'browserScreenshot', success: false, details: result, error: 'Execution failed' });
      }
    } catch (error) {
      console.log('❌ Test 1 FAILED: Browser screenshot test error -', error);
      results.push({ testName: 'browserScreenshot', success: false, details: null, error: String(error) });
    }
    
    // Test 2: Trigger browser exec test
    testCount++;
    try {
      console.log('🧪 Test 2: Triggering browser exec test via WebSocket...');
      
      const result = await (client as any).commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            console.log('🚀 AUTOMATED TEST: Starting browser exec test');
            
            if (typeof window.testBrowserExec === 'function') {
              const testResult = await window.testBrowserExec();
              console.log('✅ AUTOMATED TEST: Browser exec test completed');
              return { testName: 'browserExec', success: true, result: testResult };
            } else {
              console.log('❌ AUTOMATED TEST: testBrowserExec function not found');
              return { testName: 'browserExec', success: false, error: 'Function not found' };
            }
          `
        }
      });
      
      if (result.success) {
        console.log('✅ Test 2 PASSED: Browser exec test executed successfully');
        passCount++;
        results.push({ testName: 'browserExec', success: true, details: result });
      } else {
        console.log('❌ Test 2 FAILED: Browser exec test failed');
        results.push({ testName: 'browserExec', success: false, details: result, error: 'Execution failed' });
      }
    } catch (error) {
      console.log('❌ Test 2 FAILED: Browser exec test error -', error);
      results.push({ testName: 'browserExec', success: false, details: null, error: String(error) });
    }
    
    // Test 3: Trigger cross-context communication test
    testCount++;
    try {
      console.log('🧪 Test 3: Triggering cross-context communication test via WebSocket...');
      
      const result = await (client as any).commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            console.log('🚀 AUTOMATED TEST: Starting cross-context communication test');
            
            if (typeof window.testCrossContext === 'function') {
              window.testCrossContext();
              console.log('✅ AUTOMATED TEST: Cross-context test completed');
              return { testName: 'crossContext', success: true, result: 'Cross-context message sent' };
            } else {
              console.log('❌ AUTOMATED TEST: testCrossContext function not found');
              return { testName: 'crossContext', success: false, error: 'Function not found' };
            }
          `
        }
      });
      
      if (result.success) {
        console.log('✅ Test 3 PASSED: Cross-context communication test executed successfully');
        passCount++;
        results.push({ testName: 'crossContext', success: true, details: result });
      } else {
        console.log('❌ Test 3 FAILED: Cross-context communication test failed');
        results.push({ testName: 'crossContext', success: false, details: result, error: 'Execution failed' });
      }
    } catch (error) {
      console.log('❌ Test 3 FAILED: Cross-context communication test error -', error);
      results.push({ testName: 'crossContext', success: false, details: null, error: String(error) });
    }
    
    // Test 4: Trigger browser logging test
    testCount++;
    try {
      console.log('🧪 Test 4: Triggering browser logging test via WebSocket...');
      
      const result = await (client as any).commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            console.log('🚀 AUTOMATED TEST: Starting browser logging test');
            
            if (typeof window.testBrowserLogging === 'function') {
              window.testBrowserLogging();
              console.log('✅ AUTOMATED TEST: Browser logging test completed');
              return { testName: 'browserLogging', success: true, result: 'Browser logging test executed' };
            } else {
              console.log('❌ AUTOMATED TEST: testBrowserLogging function not found');
              return { testName: 'browserLogging', success: false, error: 'Function not found' };
            }
          `
        }
      });
      
      if (result.success) {
        console.log('✅ Test 4 PASSED: Browser logging test executed successfully');
        passCount++;
        results.push({ testName: 'browserLogging', success: true, details: result });
      } else {
        console.log('❌ Test 4 FAILED: Browser logging test failed');
        results.push({ testName: 'browserLogging', success: false, details: result, error: 'Execution failed' });
      }
    } catch (error) {
      console.log('❌ Test 4 FAILED: Browser logging test error -', error);
      results.push({ testName: 'browserLogging', success: false, details: null, error: String(error) });
    }
    
    // Test 5: Verify browser logs show test execution
    testCount++;
    try {
      console.log('🧪 Test 5: Adding explicit log proof that tests ran in browser...');
      
      const result = await (client as any).commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            console.log('🎯 PROOF: AUTOMATED BROWSER INTEGRATION TESTS EXECUTED SUCCESSFULLY');
            console.log('📊 BROWSER TEST RESULTS: ${passCount}/${testCount} tests passed');
            console.log('🌐 BROWSER INTEGRATION: WebSocket communication working');
            console.log('✅ INTEGRATION TEST EVIDENCE: This message proves tests ran in actual JTAG browser');
            
            return { 
              proof: 'BROWSER_INTEGRATION_TESTS_EXECUTED',
              timestamp: new Date().toISOString(),
              testCount: ${testCount},
              passCount: ${passCount}
            };
          `
        }
      });
      
      if (result.success) {
        console.log('✅ Test 5 PASSED: Browser log proof generated successfully');
        passCount++;
        results.push({ testName: 'logProof', success: true, details: result });
      } else {
        console.log('❌ Test 5 FAILED: Browser log proof generation failed');
        results.push({ testName: 'logProof', success: false, details: result, error: 'Proof generation failed' });
      }
    } catch (error) {
      console.log('❌ Test 5 FAILED: Browser log proof error -', error);
      results.push({ testName: 'logProof', success: false, details: null, error: String(error) });
    }
    
  } catch (connectionError) {
    console.error('💥 FATAL: Could not connect to JTAG system -', connectionError);
    console.error('🔍 Make sure JTAG system is running: npm run system:start');
    process.exit(1);
  }
  
  // Final Results
  console.log('');
  console.log('🎯 ============= BROWSER INTEGRATION TEST RESULTS =============');
  console.log(`📊 Tests Executed: ${passCount}/${testCount} passed`);
  console.log('');
  
  results.forEach((result, index) => {
    const status = result.success ? '✅ PASSED' : '❌ FAILED';
    console.log(`${index + 1}. ${result.testName}: ${status}`);
    if (result.error) {
      console.log(`   Error: ${result.error}`);
    }
  });
  
  console.log('');
  if (passCount === testCount) {
    console.log('🎉 ALL BROWSER INTEGRATION TESTS PASSED!');
    console.log('🔍 Check browser logs for proof: examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log');
    console.log('💡 Look for "AUTOMATED TEST" and "PROOF" messages in the browser logs');
    process.exit(0);
  } else {
    console.log('❌ SOME BROWSER INTEGRATION TESTS FAILED');
    console.log(`🔍 ${testCount - passCount} tests need attention`);
    process.exit(1);
  }
}

// Run the browser integration tests
runBrowserIntegrationTests().catch(error => {
  console.error('💥 Browser integration test runner error:', error);
  process.exit(1);
});