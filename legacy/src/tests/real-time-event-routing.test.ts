#!/usr/bin/env tsx
/**
 * Real-Time Event Routing Test - The Core Issue
 *
 * CRITICAL TEST: Validates server→browser event routing works WITHOUT manual refresh
 * This is the same issue we've had for a week: events don't flow browser→server→browser
 *
 * Test Methodology:
 * 1. Send message via browser UI (populate textbox + click send)
 * 2. Verify server command succeeds (modular verification)
 * 3. Wait for real-time event to trigger browser DOM update
 * 4. Verify message appears in HTML WITHOUT manual refresh
 *
 * Expected Failure: Step 3 - real-time events don't work
 */

import { JTAGClientServer } from '../system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from '../system/core/client/shared/JTAGClient';

interface RealTimeTestResult {
  testName: string;
  success: boolean;
  failurePoint?: 'UI_SEND' | 'SERVER_VERIFY' | 'REAL_TIME_EVENT' | 'HTML_UPDATE';
  details: any;
  error?: string;
}

/**
 * Modular Server Command Verification
 */
async function verifyServerCommand(client: any, expectedMessageText: string): Promise<{success: boolean, details: any, error?: string}> {
  try {
    console.log(`🔍 Verifying server received message: "${expectedMessageText}"`);

    // Get recent messages from server
    const result = await client.executeCommand('collaboration/chat/get-messages', {
      roomId: 'general',
      limit: 5
    });

    if (!result?.success || !result.messages?.length) {
      return {
        success: false,
        details: { result },
        error: 'Server command failed or no messages returned'
      };
    }

    // Check if our message exists
    const foundMessage = result.messages.find((msg: any) =>
      msg.content?.text?.includes(expectedMessageText.substring(0, 20))
    );

    if (foundMessage) {
      console.log(`✅ Server verification PASSED: Found message "${foundMessage.content.text}"`);
      return {
        success: true,
        details: { foundMessage, totalMessages: result.messages.length }
      };
    } else {
      console.log(`❌ Server verification FAILED: Message not found in ${result.messages.length} messages`);
      return {
        success: false,
        details: { searchText: expectedMessageText, messages: result.messages },
        error: `Message "${expectedMessageText}" not found on server`
      };
    }

  } catch (error) {
    return {
      success: false,
      details: {},
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

/**
 * Browser HTML Interrogation
 */
async function checkBrowserHTML(client: any, expectedText: string): Promise<{success: boolean, details: any, error?: string}> {
  try {
    console.log(`🔍 Checking browser HTML for: "${expectedText}"`);

    // Use debug/html-inspector to check DOM
    const result = await client.executeCommand('debug/html-inspector', {
      selector: 'chat-widget'
    });

    if (!result?.success) {
      return {
        success: false,
        details: { result },
        error: 'HTML inspection failed'
      };
    }

    // Check if message text appears in HTML
    const htmlContent = result.html || '';
    const textFound = htmlContent.includes(expectedText.substring(0, 20));

    if (textFound) {
      console.log(`✅ Browser HTML PASSED: Found text in DOM`);
      return {
        success: true,
        details: { textFound: true, htmlLength: htmlContent.length }
      };
    } else {
      console.log(`❌ Browser HTML FAILED: Text not found in DOM`);
      return {
        success: false,
        details: { textFound: false, htmlContent: htmlContent.substring(0, 500) + '...' },
        error: `Text "${expectedText}" not found in browser HTML`
      };
    }

  } catch (error) {
    return {
      success: false,
      details: {},
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

/**
 * ASSERT THE FAILURE: Step-by-Step Causality Test
 */
async function runRealTimeEventTest(): Promise<void> {
  console.log(`🔥 CAUSALITY TEST: WHERE DOES EVENT ROUTING BREAK?`);
  console.log(`📋 STEP 1: Send via browser ✅ SHOULD WORK`);
  console.log(`📋 STEP 2: Verify server storage ✅ SHOULD WORK`);
  console.log(`📋 STEP 3: Check EventBridge routing ❌ SHOULD FAIL HERE`);
  console.log(`📋 STEP 4: Check browser DOM update ❌ SHOULD FAIL HERE`);
  console.log('');

  const testMessage = `CAUSALITY-TEST-${Date.now()}`;
  console.log(`🎯 Testing with: "${testMessage}"`);

  // STEP 1: Send message via browser UI
  console.log(`\n🧪 STEP 1: Browser UI Send`);
  console.log(`Expected: ✅ SUCCESS - Browser can send messages`);

  // STEP 2: Verify server received and stored
  console.log(`\n🧪 STEP 2: Server Storage Verification`);
  console.log(`Expected: ✅ SUCCESS - Server stores messages`);

  // STEP 3: Check EventBridge routing (THIS SHOULD FAIL)
  console.log(`\n🧪 STEP 3: EventBridge Routing Check`);
  console.log(`Expected: ❌ FAILURE - Events don't route to browser`);

  // STEP 4: Check browser DOM (THIS SHOULD FAIL)
  console.log(`\n🧪 STEP 4: Browser DOM Update Check`);
  console.log(`Expected: ❌ FAILURE - DOM never updates without refresh`);

  console.log(`\n🚨 ASSERTION: Steps 3 & 4 MUST FAIL or event system is fixed!`);

    // Test: Complete Real-Time Flow
    testCount++;
    const testMessage = `REAL-TIME-TEST-${Date.now()}`;

    try {
      console.log('🧪 Step 1: Sending message via browser UI simulation...');

      // Simulate browser UI interaction
      const uiResult = await client.executeCommand('exec', {
        code: `
          const widget = document.querySelector('continuum-widget')?.shadowRoot?.querySelector('main-widget')?.shadowRoot?.querySelector('chat-widget');
          const input = widget?.shadowRoot?.querySelector('.message-input');
          if (input && widget.sendMessage) {
            input.value = '${testMessage}';
            const result = widget.sendMessage();
            console.log('🔥 UI-SEND-RESULT:', result);
            'UI_SEND_SUCCESS';
          } else {
            'UI_SEND_FAILED';
          }
        `
      });

      if (!uiResult?.success || !uiResult.result?.includes('UI_SEND_SUCCESS')) {
        results.push({
          testName: 'realTimeEventFlow',
          success: false,
          failurePoint: 'UI_SEND',
          details: { uiResult },
          error: 'Browser UI send failed'
        });
        console.log('❌ Step 1 FAILED: Browser UI send');
      } else {
        console.log('✅ Step 1 PASSED: Browser UI send successful');

        // Give server time to process
        await new Promise(resolve => setTimeout(resolve, 1000));

        console.log('🧪 Step 2: Verifying server received message...');
        const serverVerification = await verifyServerCommand(client, testMessage);

        if (!serverVerification.success) {
          results.push({
            testName: 'realTimeEventFlow',
            success: false,
            failurePoint: 'SERVER_VERIFY',
            details: serverVerification.details,
            error: serverVerification.error
          });
          console.log('❌ Step 2 FAILED: Server verification');
        } else {
          console.log('✅ Step 2 PASSED: Server verification successful');

          console.log('🧪 Step 3: CRITICAL - Waiting for real-time event (3 seconds)...');
          await new Promise(resolve => setTimeout(resolve, 3000));

          console.log('🧪 Step 4: Checking browser HTML for real-time update...');
          const htmlVerification = await checkBrowserHTML(client, testMessage);

          if (!htmlVerification.success) {
            results.push({
              testName: 'realTimeEventFlow',
              success: false,
              failurePoint: 'REAL_TIME_EVENT', // This is where it should fail
              details: {
                serverSuccess: true,
                htmlCheck: htmlVerification.details
              },
              error: `CORE ISSUE: Real-time events not working - ${htmlVerification.error}`
            });
            console.log('❌ Step 4 FAILED: Real-time event routing broken');
          } else {
            console.log('✅ Step 4 PASSED: Real-time events working!');
            passCount++;
            results.push({
              testName: 'realTimeEventFlow',
              success: true,
              details: {
                serverSuccess: true,
                htmlSuccess: true,
                realTimeWorking: true
              }
            });
          }
        }
      }

    } catch (error) {
      results.push({
        testName: 'realTimeEventFlow',
        success: false,
        failurePoint: 'UI_SEND',
        details: {},
        error: error instanceof Error ? error.message : String(error)
      });
    }

    await client.disconnect();

  } catch (error) {
    console.error('💥 FATAL: Real-time event test failed to initialize:', error);
  }

  // Display Results
  console.log('');
  console.log('📊 REAL-TIME EVENT TEST RESULTS:');
  console.log(`   Total Tests: ${testCount}`);
  console.log(`   ✅ Passed: ${passCount}`);
  console.log(`   ❌ Failed: ${testCount - passCount}`);

  if (passCount === testCount) {
    console.log('🎉 REAL-TIME EVENTS WORKING - Core issue fixed!');
  } else {
    console.log('⚠️ REAL-TIME EVENTS BROKEN - Core issue confirmed');
  }

  // Detailed Failure Analysis
  console.log('');
  console.log('📋 Failure Analysis:');
  results.forEach((result, index) => {
    const status = result.success ? '✅' : '❌';
    console.log(`   ${status} ${index + 1}. ${result.testName}`);
    if (!result.success) {
      console.log(`      💥 Failure Point: ${result.failurePoint}`);
      console.log(`      📝 Error: ${result.error}`);
      if (result.failurePoint === 'REAL_TIME_EVENT') {
        console.log(`      🚨 THIS IS THE CORE ISSUE: Events don't route server→browser`);
      }
    }
  });

  console.log('');
  console.log('🔥 Real-time event routing test complete!');
}

// Execute if run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  runRealTimeEventTest().catch(console.error);
}

export { runRealTimeEventTest };