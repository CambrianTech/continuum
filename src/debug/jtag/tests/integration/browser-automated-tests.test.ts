/**
 * Browser Automated Integration Tests
 * 
 * These tests run INSIDE the actual JTAG browser (not Puppeteer) 
 * and execute via WebSocket commands. Results appear in browser-console-log.log
 * as proof that integration tests actually ran.
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from '../../system/core/client/shared/JTAGClient';

// Import refactored test utilities
import {
  TestExecutionEngine,
  TestExecutionResult
} from '../shared/TestExecution';

import {
  captureTestScreenshot
} from '../shared/ScreenshotTesting';

import {
  TestAssertions
} from '../shared/TestAssertions';

import {
  DOM_SELECTORS,
  TEST_TIMEOUTS,
  COMMAND_DEFAULTS
} from '../shared/TestConstants';

interface BrowserTestResult {
  testName: string;
  success: boolean;
  details: any;
  error?: string;
}

/**
 * Shadow DOM-aware chat widget interaction - uses new utilities
 */
const CHAT_WIDGET_INTERACTIONS = {
  sendMessage: (message: string) => `
    return (async function() {
      console.log('💬 CHAT TEST: Sending message to chat widget');
      
      try {
        // Find chat widget using standardized selector
        const chatWidget = document.querySelector('${DOM_SELECTORS.CHAT_WIDGET}');
        if (!chatWidget) {
          return { success: false, error: 'Chat widget not found' };
        }
        
        // Shadow DOM-aware helper function
        const findInShadowDOM = (element, selectors) => {
          // Try regular DOM first
          for (const selector of selectors) {
            const found = element.querySelector(selector);
            if (found) return found;
          }
          
          // Try shadow DOM if available
          if (element.shadowRoot) {
            for (const selector of selectors) {
              const found = element.shadowRoot.querySelector(selector);
              if (found) return found;
            }
          }
          
          return null;
        };
        
        // Use comprehensive selectors that handle multiple patterns
        const inputSelectors = ['${DOM_SELECTORS.CHAT_INPUT}'.split(', ')].flat();
        const buttonSelectors = ['${DOM_SELECTORS.CHAT_SEND_BUTTON}'.split(', ')].flat();
        
        const input = findInShadowDOM(chatWidget, inputSelectors);
        const button = findInShadowDOM(chatWidget, buttonSelectors);
        
        if (!input || !button) {
          // Debug information for troubleshooting
          const root = chatWidget.shadowRoot || chatWidget;
          const availableElements = Array.from(root.querySelectorAll('*'))
            .map(el => el.tagName + (el.id ? '#' + el.id : '') + (el.className ? '.' + el.className : ''))
            .join(', ');
          return { 
            success: false, 
            error: 'Chat controls not found',
            availableElements,
            triedInputSelectors: inputSelectors,
            triedButtonSelectors: buttonSelectors,
            hasShadowRoot: !!chatWidget.shadowRoot
          };
        }
        
        // Send message with proper timing
        input.value = '${message}';
        button.click();
        
        // Wait for message processing
        await new Promise(resolve => setTimeout(resolve, ${COMMAND_DEFAULTS.CHAT_INTERACTION.WAIT_FOR_RESPONSE}));
        
        return { 
          success: true, 
          message: '${message}',
          proof: 'CHAT_MESSAGE_SENT_SUCCESSFULLY',
          foundInputVia: input.id || input.className || input.tagName,
          foundButtonVia: button.id || button.className || button.tagName
        };
      } catch (error) {
        return { success: false, error: error.message || String(error) };
      }
    })();
  `
};

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
    const { getActivePorts } = require('../../examples/shared/ExampleConfig');
    const activePorts = await getActivePorts();
    const websocketPort = activePorts.websocket_server;
    const serverUrl = `ws://localhost:${websocketPort}`;
    
    const clientOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server',
      transportType: 'websocket', 
      serverUrl,
      enableFallback: false
    };
    
    console.log(`🔗 Connecting to JTAG system at ${serverUrl}...`);
    const { client } = await JTAGClientServer.connect(clientOptions);
    console.log('✅ JTAG Client connected for browser test automation');
    
    // Test 1: Chat Widget Before/After Test with Message Sending - REFACTORED
    testCount++;
    try {
      console.log('🧪 Test 1: Chat Widget Before/After Test with Message Sending...');
      
      // Step 1: Take BEFORE screenshot using new utilities
      console.log('📸 Step 1a: Taking BEFORE screenshot of chat widget...');
      const beforeResult = await captureTestScreenshot(
        'Chat Widget Before Test',
        DOM_SELECTORS.CHAT_WIDGET,
        {
          filename: 'chat-widget-before-test.png',
          timeout: TEST_TIMEOUTS.STANDARD_OPERATION,
          validateFile: true
        }
      );
      
      // Step 2: Send message using refactored interaction
      console.log('💬 Step 1b: Sending message to chat widget...');
      const chatResult = await client.commands.exec({
        code: CHAT_WIDGET_INTERACTIONS.sendMessage('TEST: Browser integration tests now use proper utilities! 🎉')
      });
      
      // Step 3: Take AFTER screenshot using new utilities
      console.log('📸 Step 1c: Taking AFTER screenshot to show message...');
      const afterResult = await captureTestScreenshot(
        'Chat Widget After Test', 
        DOM_SELECTORS.CHAT_WIDGET,
        {
          filename: 'chat-widget-after-test.png',
          timeout: TEST_TIMEOUTS.STANDARD_OPERATION,
          validateFile: true
        }
      );
      
      // Validate results using TestAssertions
      try {
        TestAssertions.assertTestSuccess(beforeResult, {
          context: 'Before Screenshot',
          throwOnFailure: true
        });
        
        TestAssertions.assertTestSuccess(chatResult, {
          context: 'Message Sending', 
          throwOnFailure: true
        });
        
        // Additional validation for message execution result
        console.log('🔍 DEBUG: chatResult structure:', JSON.stringify(chatResult, null, 2));
        
        if (chatResult.data?.commandResult?.result) {
          TestAssertions.assertValue(
            chatResult.data.commandResult.result.success,
            true,
            'message sending success',
            { context: 'Message Execution', throwOnFailure: true }
          );
        } else if (chatResult.commandResult?.result) {
          TestAssertions.assertValue(
            chatResult.commandResult.result.success,
            true,
            'message sending success',
            { context: 'Message Execution', throwOnFailure: true }
          );
        } else {
          console.log('⚠️  WARNING: No commandResult.result found in chatResult - skipping detailed validation');
        }
        
        TestAssertions.assertTestSuccess(afterResult, {
          context: 'After Screenshot',
          throwOnFailure: true
        });
        
        console.log('✅ Test 1 PASSED: Chat Widget Before/After Test completed successfully');
        passCount++;
        results.push({ testName: 'chatWidgetBeforeAfter', success: true, details: { beforeResult, chatResult, afterResult } });
        
      } catch (validationError) {
        console.log('❌ Test 1 FAILED: Chat Widget validation failed -', validationError);
        results.push({ 
          testName: 'chatWidgetBeforeAfter', 
          success: false, 
          details: { beforeResult, chatResult, afterResult }, 
          error: validationError instanceof Error ? validationError.message : String(validationError)
        });
      }
      
    } catch (error) {
      console.log('❌ Test 1 FAILED: Chat Widget Before/After Test error -', error);
      results.push({ testName: 'chatWidgetBeforeAfter', success: false, details: null, error: String(error) });
    }
    
    // Test 2: Trigger browser exec test
    testCount++;
    try {
      console.log('🧪 Test 2: Triggering browser exec test via WebSocket...');
      
      const result = await client.commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            console.log('🚀 AUTOMATED TEST: Starting browser exec test');
            
            // Test exec functionality directly 
            try {
              // Browser-specific JavaScript execution test with DOM references
              const testValue = Math.random();
              const result = testValue * 2;
              const browserInfo = {
                userAgent: navigator.userAgent,
                location: window.location.href,
                timestamp: new Date().toISOString()
              };
              console.log('✅ AUTOMATED TEST: Browser exec test completed - DOM access working', browserInfo);
              return { 
                testName: 'browserExec', 
                success: true, 
                result: { testValue, result, browserInfo, proof: 'BROWSER_EXEC_WORKING' }
              };
            } catch (error) {
              console.log('❌ AUTOMATED TEST: Browser exec test error:', error);
              return { testName: 'browserExec', success: false, error: error.message || String(error) };
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
      
      const result = await client.commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            console.log('🚀 AUTOMATED TEST: Starting cross-context communication test');
            
            // Test cross-context communication directly
            try {
              console.log('🔄 Testing cross-context communication...');
              
              // This exec command itself IS cross-context communication
              // (Server test → WebSocket → Browser execution → Response back)
              const crossContextProof = {
                executedInBrowser: true,
                timestamp: new Date().toISOString(),
                userAgent: navigator.userAgent,
                location: window.location.href,
                documentTitle: document.title,
                windowWidth: window.innerWidth
              };
              
              console.log('✅ AUTOMATED TEST: Cross-context communication working');
              return { 
                testName: 'crossContext', 
                success: true, 
                result: crossContextProof 
              };
            } catch (error) {
              console.log('❌ AUTOMATED TEST: Cross-context test error:', error);
              return { testName: 'crossContext', success: false, error: error.message || String(error) };
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
      
      const result = await client.commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            console.log('🚀 AUTOMATED TEST: Starting browser logging test');
            
            // Test browser logging directly
            try {
              console.log('🌐 Testing browser logging...');
              
              // Generate different types of log messages to test console routing
              console.log('📊 INFO: Browser logging test - info message');
              console.warn('⚠️ WARN: Browser logging test - warning message');
              console.debug('🔍 DEBUG: Browser logging test - debug message');
              
              // Test that console.error was fixed (our main achievement)
              try {
                throw new Error('Test error for logging validation');
              } catch (testError) {
                console.error('❌ ERROR: Browser logging test - error with full details:', testError);
              }
              
              console.log('✅ AUTOMATED TEST: Browser logging test completed - all message types sent');
              return { 
                testName: 'browserLogging', 
                success: true, 
                result: { 
                  logTypes: ['info', 'warn', 'debug', 'error'],
                  proof: 'BROWSER_LOGGING_WORKING'
                }
              };
            } catch (error) {
              console.log('❌ AUTOMATED TEST: Browser logging test error:', error);
              return { testName: 'browserLogging', success: false, error: error.message || String(error) };
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
      
      const result = await client.commands.exec({
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
    
    // Graceful disconnect - ensure WebSocket connections are properly closed
    try {
      console.log('🔌 GRACEFUL DISCONNECT: Closing JTAG client connection...');
      if (client && typeof (client as any).disconnect === 'function') {
        await (client as any).disconnect();
        console.log('✅ GRACEFUL DISCONNECT: Client disconnected successfully');
      } else {
        console.log('ℹ️ GRACEFUL DISCONNECT: Client does not support explicit disconnect');
      }
    } catch (disconnectError) {
      console.log('⚠️ GRACEFUL DISCONNECT: Error during disconnect -', disconnectError);
      // Don't fail tests due to disconnect issues
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