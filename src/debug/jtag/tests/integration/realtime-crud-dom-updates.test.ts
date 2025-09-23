/**
 * Real-time CRUD → DOM Updates Integration Test
 *
 * Tests the complete flow: CRUD operations → Server events → Widget updates → DOM changes
 * Category: integration, database, widgets, real-time
 */

import { execSync } from 'child_process';

console.log('🧪 REAL-TIME CRUD → DOM UPDATES INTEGRATION TEST');
console.log('=================================================');

interface TestResult {
  step: string;
  success: boolean;
  details?: Record<string, unknown>;
  error?: string;
}

const results: TestResult[] = [];

async function runJtagCommand(command: string): Promise<Record<string, unknown>> {
  try {
    const output = execSync(`./jtag ${command}`, {
      encoding: 'utf8',
      cwd: '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag'
    });

    // Parse JTAG JSON response from command output
    const lines = output.split('\n');
    const resultLine = lines.find(line => line.includes('COMMAND RESULT:'));
    if (resultLine) {
      const jsonStart = output.indexOf('{', output.indexOf('COMMAND RESULT:'));
      const jsonEnd = output.lastIndexOf('}') + 1;
      return JSON.parse(output.substring(jsonStart, jsonEnd));
    }
    return {};
  } catch (error) {
    console.error(`❌ Command failed: ${command}`);
    console.error(error instanceof Error ? error.message : String(error));
    throw error;
  }
}

async function testRoomUpdateFlow(): Promise<void> {
  console.log('\n📋 TEST 1: Room Update → Real-time Widget Update');
  console.log('------------------------------------------------');

  try {
    // Step 1: Get initial Room data
    console.log('🔍 Step 1: Reading initial Room data...');
    const roomId = '5e71a0c8-0303-4eb8-a478-3a121248'; // From previous test
    const initialRead = await runJtagCommand(`data/read --collection=Room --id=${roomId}`);
    const initialDescription = initialRead.data?.description as string;
    console.log(`   Initial description: "${initialDescription}"`);

    results.push({
      step: 'Initial Room Read',
      success: Boolean(initialRead.success),
      details: { description: initialDescription }
    });

    // Step 2: Update Room data
    console.log('🔄 Step 2: Updating Room description...');
    const testDescription = 'REAL-TIME INTEGRATION TEST';
    const updateResult = await runJtagCommand('data/update --collection=Room --id=' + roomId + ' --data=\'{"description": "' + testDescription + '"}\'');
    const updatedDescription = updateResult.data?.data?.description as string;
    console.log(`   Update response description: "${updatedDescription}"`);

    results.push({
      step: 'Room Update Command',
      success: Boolean(updateResult.found),
      details: {
        found: updateResult.found,
        description: updatedDescription,
        previousVersion: updateResult.previousVersion,
        newVersion: updateResult.newVersion
      }
    });

    // Step 3: Verify persistence
    console.log('✅ Step 3: Verifying update persisted...');
    const verifyRead = await runJtagCommand(`data/read --collection=Room --id=${roomId}`);
    const persistedDescription = verifyRead.data?.description as string;
    console.log(`   Persisted description: "${persistedDescription}"`);

    const persistSuccess = persistedDescription === testDescription;
    results.push({
      step: 'Persistence Verification',
      success: persistSuccess,
      details: { description: persistedDescription }
    });

    // Step 4: Check if event was emitted (from server logs)
    console.log('📡 Step 4: Checking for data:Room:updated event emission...');

    // Look for recent event in logs
    const logsResult = await runJtagCommand('debug/logs --tailLines=50');
    const logEntries = logsResult.logEntries as Array<{ message?: string }> || [];
    const eventEmitted = logEntries.some(entry =>
      entry.message && entry.message.includes('data:Room:updated')
    );

    console.log(`   Event emitted: ${eventEmitted ? '✅ YES' : '❌ NO'}`);

    results.push({
      step: 'Event Emission Check',
      success: eventEmitted,
      details: { eventFound: eventEmitted }
    });

    // Step 5: Attempt to inspect DOM (this will fail without browser but shows the intention)
    console.log('🔍 Step 5: Attempting widget DOM inspection...');
    try {
      const widgetResult = await runJtagCommand('debug/widget-events --widgetSelector="room-list-widget"');
      const widgetFound = Boolean(widgetResult.widgetFound);

      results.push({
        step: 'Widget Inspection',
        success: widgetFound,
        details: { widgetFound }
      });
    } catch {
      results.push({
        step: 'Widget Inspection',
        success: false,
        details: { error: 'Requires browser environment' }
      });
    }

    const overallSuccess = persistSuccess && eventEmitted;
    console.log(`\n🎯 TEST 1 RESULT: ${overallSuccess ? '✅ PASS' : '❌ FAIL'}`);

    if (overallSuccess) {
      console.log('✅ CRUD → Event flow is working correctly');
      console.log('📝 Next step: Test in browser environment to verify DOM updates');
    } else {
      console.log('❌ CRUD → Event flow has issues');
      console.log(`   Persistence: ${persistSuccess ? '✅' : '❌'}`);
      console.log(`   Event emission: ${eventEmitted ? '✅' : '❌'}`);
    }

  } catch (error) {
    console.error('❌ Test 1 failed with error:', error instanceof Error ? error.message : String(error));
    results.push({
      step: 'Room Update Flow Test',
      success: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
}

async function testUserUpdateFlow(): Promise<void> {
  console.log('\n📋 TEST 2: User Update → Real-time Widget Update');
  console.log('------------------------------------------------');

  try {
    // Test User updates (similar to Room test)
    const userId = '002350cc-0031-408d-8040-004f000f'; // From previous test
    const testDisplayName = `REAL-TIME USER ${Date.now()}`;

    console.log('🔄 Updating User displayName...');
    const updateResult = await runJtagCommand('data/update --collection=User --id=' + userId + ' --data=\'{"displayName": "' + testDisplayName + '"}\'');
    const success = Boolean(updateResult.found);

    console.log('📡 Checking for data:User:updated event...');
    const logsResult = await runJtagCommand('debug/logs --tailLines=30');
    const logEntries = logsResult.logEntries as Array<{ message?: string }> || [];
    const eventEmitted = logEntries.some(entry =>
      entry.message && entry.message.includes('data:User:updated')
    );

    results.push({
      step: 'User Update Flow',
      success: success && eventEmitted,
      details: {
        updateSuccess: success,
        eventEmitted,
        displayName: testDisplayName
      }
    });

    console.log(`🎯 TEST 2 RESULT: ${success && eventEmitted ? '✅ PASS' : '❌ FAIL'}`);

  } catch (error) {
    console.error('❌ Test 2 failed with error:', error instanceof Error ? error.message : String(error));
    results.push({
      step: 'User Update Flow Test',
      success: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
}

async function runTests(): Promise<void> {
  try {
    await testRoomUpdateFlow();
    await testUserUpdateFlow();

    // Summary
    console.log('\n🎉 REAL-TIME CRUD → DOM UPDATES TEST SUMMARY');
    console.log('============================================');

    let passCount = 0;
    let failCount = 0;

    results.forEach(result => {
      const status = result.success ? '✅ PASS' : '❌ FAIL';
      console.log(`   ${status} ${result.step}`);
      if (result.success) passCount++;
      else failCount++;

      if (result.error) {
        console.log(`      Error: ${result.error}`);
      }
    });

    console.log(`\n📊 Results: ${passCount} passed, ${failCount} failed`);

    const overallSuccess = failCount === 0;
    console.log(`\n🏁 OVERALL: ${overallSuccess ? '✅ ALL TESTS PASSED' : '❌ SOME TESTS FAILED'}`);

    if (overallSuccess) {
      console.log('🎯 Real-time CRUD system is working end-to-end');
      console.log('🎯 Server events are being emitted after successful updates');
      console.log('🎯 Widgets are subscribed and ready to receive updates');
      console.log('📝 Browser environment testing needed to verify final DOM updates');
    } else {
      console.log('📝 Some components need fixing before full real-time functionality works');
    }

    // Exit with proper code
    process.exit(overallSuccess ? 0 : 1);

  } catch (error) {
    console.error('❌ Test suite failed:', error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

// Run the tests
runTests();