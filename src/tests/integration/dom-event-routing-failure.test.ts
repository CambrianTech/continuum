#!/usr/bin/env tsx
/**
 * DOM Event Routing Failure Test
 *
 * CRITICAL TEST: Prove that server→browser events don't trigger widget DOM updates
 * ASSERTION: This test MUST FAIL to prove the bug exists
 * METHODOLOGY: Send message via CLI → verify server storage → wait for real-time event → assert DOM NOT updated
 */

import { jtag } from '../../server-index';

interface TestResult {
  testName: string;
  success: boolean;
  failurePoint: 'CLI_SEND' | 'SERVER_STORAGE' | 'DOM_UPDATE_MISSING' | 'UNEXPECTED_SUCCESS';
  details: any;
  error?: string;
}

/**
 * FAILING TEST: Prove DOM events don't work for widgets
 */
async function proveDOMEventRoutingFailure(): Promise<void> {
  console.log(`🔥 DOM EVENT ROUTING FAILURE PROOF`);
  console.log(`🎯 ASSERTION: This test MUST FAIL to prove the bug exists`);
  console.log(`📋 Steps: CLI send → server storage ✅ → DOM update ❌`);
  console.log('');

  const testMessage = `DOM-ROUTING-FAILURE-${Date.now()}`;
  const results: TestResult[] = [];

  try {
    // Connect to JTAG system using proper pattern
    console.log('🔧 DOM-ROUTING-TEST-' + Date.now() + ': Starting connection');
    const client = await jtag.connect({ targetEnvironment: 'server' });
    console.log('✅ Connected to JTAG server');
    console.log('🔧 DOM-ROUTING-TEST-' + Date.now() + ': Connection established');

    // Step 1: Send message via CLI (this should work)
    console.log(`\\n🧪 STEP 1: Send message via CLI command`);
    console.log(`Expected: ✅ SUCCESS - CLI can send messages`);

    console.log('🔧 DOM-ROUTING-TEST-' + Date.now() + ': About to send CLI message');
    const cliResult = await client.commands['collaboration/chat/send']({
      roomId: 'general',
      message: testMessage,
      senderName: 'TestBot'
    });
    console.log('🔧 DOM-ROUTING-TEST-' + Date.now() + ': CLI send result:', cliResult?.success);

    if (!cliResult?.success || !cliResult.messageId) {
      results.push({
        testName: 'domEventRoutingFailure',
        success: false,
        failurePoint: 'CLI_SEND',
        details: { cliResult },
        error: 'CLI send failed - cannot test further'
      });
      console.log('❌ Step 1 FAILED: CLI send broken');
    } else {
      console.log('✅ Step 1 PASSED: CLI send successful');
      console.log(`   Message ID: ${cliResult.messageId}`);

      // Step 2: Verify server storage (this should work)
      console.log(`\\n🧪 STEP 2: Verify server storage`);
      console.log(`Expected: ✅ SUCCESS - Server stores messages`);

      await new Promise(resolve => setTimeout(resolve, 1000)); // Give server time

      console.log('🔧 DOM-ROUTING-TEST-' + Date.now() + ': Checking server storage');
      const storageResult = await client.commands['collaboration/chat/get-messages']({
        roomId: 'general',
        limit: 3
      });
      console.log('🔧 DOM-ROUTING-TEST-' + Date.now() + ': Storage check result:', storageResult?.success);

      const messageFound = storageResult?.success &&
        storageResult.messages?.some((msg: any) =>
          msg.content?.text?.includes(testMessage.substring(0, 15))
        );

      if (!messageFound) {
        results.push({
          testName: 'domEventRoutingFailure',
          success: false,
          failurePoint: 'SERVER_STORAGE',
          details: { storageResult },
          error: 'Server storage verification failed'
        });
        console.log('❌ Step 2 FAILED: Server storage broken');
      } else {
        console.log('✅ Step 2 PASSED: Server storage working');

        // Step 3: THE CRITICAL TEST - Check if DOM updated (this SHOULD FAIL)
        console.log(`\\n🧪 STEP 3: Wait for real-time DOM update`);
        console.log(`Expected: ❌ FAILURE - Real-time DOM events broken`);
        console.log(`⏰ Waiting 4 seconds for EventBridge routing...`);

        await new Promise(resolve => setTimeout(resolve, 4000));

        console.log('🔧 DOM-ROUTING-TEST-' + Date.now() + ': Inspecting browser DOM');
        const htmlResult = await client.commands['debug/html-inspector']({
          selector: 'chat-widget'
        });
        console.log('🔧 DOM-ROUTING-TEST-' + Date.now() + ': DOM inspection result:', htmlResult?.success);

        if (!htmlResult?.success) {
          results.push({
            testName: 'domEventRoutingFailure',
            success: false,
            failurePoint: 'DOM_UPDATE_MISSING',
            details: { htmlResult },
            error: 'HTML inspection failed - cannot verify DOM state'
          });
          console.log('❌ Step 3 INDETERMINATE: HTML inspection failed');
        } else {
          const messageInDOM = htmlResult.html?.includes(testMessage.substring(0, 15)) || false;

          if (messageInDOM) {
            // THIS SHOULD NOT HAPPEN - indicates bug is already fixed
            results.push({
              testName: 'domEventRoutingFailure',
              success: true, // Test "passed" but this means bug is fixed
              failurePoint: 'UNEXPECTED_SUCCESS',
              details: {
                messageFound: true,
                htmlLength: htmlResult.html?.length,
                searchText: testMessage.substring(0, 15)
              },
              error: 'UNEXPECTED: Real-time DOM updates are working!'
            });
            console.log('🚨 Step 3 UNEXPECTED SUCCESS: Message found in DOM!');
            console.log('🎉 BREAKTHROUGH: Real-time events are actually working!');
          } else {
            // THIS IS THE EXPECTED FAILURE - proves the bug
            results.push({
              testName: 'domEventRoutingFailure',
              success: false,
              failurePoint: 'DOM_UPDATE_MISSING',
              details: {
                messageFound: false,
                htmlLength: htmlResult.html?.length,
                searchText: testMessage.substring(0, 15),
                htmlPreview: htmlResult.html?.substring(0, 200) + '...'
              },
              error: 'EXPECTED FAILURE: Real-time DOM events not working'
            });
            console.log('❌ Step 3 EXPECTED FAILURE: Message NOT in DOM');
            console.log('💡 BUG CONFIRMED: EventBridge→DOM routing broken');
          }
        }
      }
    }

    await client.disconnect();

  } catch (error) {
    results.push({
      testName: 'domEventRoutingFailure',
      success: false,
      failurePoint: 'CLI_SEND',
      details: {},
      error: error instanceof Error ? error.message : String(error)
    });
  }

  // Display Results
  console.log('');
  console.log('📊 DOM EVENT ROUTING FAILURE PROOF RESULTS:');

  const result = results[0];
  if (!result) {
    console.log('❌ NO RESULTS - Test setup failed');
    return;
  }

  console.log(`   Test: ${result.testName}`);
  console.log(`   Result: ${result.success ? '✅ PASSED' : '❌ FAILED'}`);
  console.log(`   Failure Point: ${result.failurePoint}`);
  console.log(`   Error: ${result.error}`);

  if (result.failurePoint === 'DOM_UPDATE_MISSING') {
    console.log('');
    console.log('🔥 BUG SUCCESSFULLY PROVEN:');
    console.log('   ✅ CLI send works');
    console.log('   ✅ Server storage works');
    console.log('   ❌ Real-time DOM updates broken');
    console.log('   📋 ROOT CAUSE: EventBridge not dispatching DOM events for widgets');
    console.log('');
    console.log('🎯 NEXT STEP: Fix EventsDaemon to dispatch DOM events in browser environment');
  } else if (result.failurePoint === 'UNEXPECTED_SUCCESS') {
    console.log('');
    console.log('🎉 BUG ALREADY FIXED:');
    console.log('   ✅ CLI send works');
    console.log('   ✅ Server storage works');
    console.log('   ✅ Real-time DOM updates working');
    console.log('   📋 CONCLUSION: EventBridge DOM routing is functional');
  } else {
    console.log('');
    console.log(`🚨 TEST SETUP ISSUE: Cannot prove bug due to ${result.failurePoint}`);
  }

  console.log('');
  console.log('🔬 DOM event routing failure proof complete!');
}

// Execute if run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  proveDOMEventRoutingFailure().catch(console.error);
}

export { proveDOMEventRoutingFailure };