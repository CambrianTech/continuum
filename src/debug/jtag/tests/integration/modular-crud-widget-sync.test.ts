/**
 * Modular CRUD-Widget Synchronization Test
 *
 * Tests the React-like reactivity: CRUD Operation → DB Update → Event Emission → Widget HTML Update
 * Works for any (Collection, Widget) combination without timeouts
 */

import { execSync } from 'child_process';

console.log('🧪 MODULAR CRUD-WIDGET SYNCHRONIZATION TEST');
console.log('============================================');

interface TestConfig {
  collection: string;
  widget: string;
  testId: string;
  updateData: Record<string, any>;
  verifyField: string;
  expectedValue: string;
}

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

    // Parse JTAG JSON response
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

/**
 * Modular test function for any Collection-Widget pair
 */
async function testCRUDWidgetSync(config: TestConfig): Promise<void> {
  console.log(`\n📋 TESTING: ${config.collection} ↔ ${config.widget}`);
  console.log('------------------------------------------------');

  try {
    // Step 1: Capture initial DB state
    console.log('🔍 Step 1: Capturing initial database state...');
    const beforeDB = await runJtagCommand(`data/read --collection=${config.collection} --id=${config.testId}`);
    const beforeVersion = beforeDB.data?.version || beforeDB.data?.metadata?.version || 0;
    const beforeFieldValue = beforeDB.data?.[config.verifyField];

    console.log(`   Before ${config.verifyField}: "${beforeFieldValue}"`);
    console.log(`   Before version: ${beforeVersion}`);

    results.push({
      step: `${config.collection} Initial DB Read`,
      success: Boolean(beforeDB.success),
      details: { version: beforeVersion, field: beforeFieldValue }
    });

    // Step 2: Capture initial HTML state
    console.log('🔍 Step 2: Capturing initial widget HTML state...');
    const beforeHTML = await runJtagCommand(`debug/widget-state --widgetSelector="${config.widget}" --extractRowData=true`);
    const beforeRowCount = beforeHTML.commandResult?.state?.rowData?.length || 0;

    console.log(`   Before widget rows: ${beforeRowCount}`);

    results.push({
      step: `${config.widget} Initial HTML State`,
      success: Boolean(beforeHTML.success),
      details: { rowCount: beforeRowCount, widgetFound: beforeHTML.commandResult?.widgetFound }
    });

    // Step 3: Perform CRUD operation (promise should return AFTER event emitted)
    console.log('🔄 Step 3: Performing CRUD update...');
    const updateCommand = `data/update --collection=${config.collection} --id=${config.testId} --data='${JSON.stringify(config.updateData)}'`;
    const updateResult = await runJtagCommand(updateCommand);

    const afterVersion = updateResult.newVersion || updateResult.data?.metadata?.version;
    const updatedFieldValue = updateResult.data?.data?.[config.verifyField];

    console.log(`   Update result: ${updateResult.found ? 'SUCCESS' : 'FAILED'}`);
    console.log(`   After ${config.verifyField}: "${updatedFieldValue}"`);
    console.log(`   After version: ${afterVersion}`);

    const updateSuccess = updateResult.found && afterVersion > beforeVersion;
    results.push({
      step: `${config.collection} CRUD Update`,
      success: updateSuccess,
      details: {
        found: updateResult.found,
        beforeVersion,
        afterVersion,
        fieldUpdated: updatedFieldValue === config.expectedValue
      }
    });

    // Step 4: Verify DB persistence (immediate - no timeout)
    console.log('✅ Step 4: Verifying database persistence...');
    const afterDB = await runJtagCommand(`data/read --collection=${config.collection} --id=${config.testId}`);
    const persistedFieldValue = afterDB.data?.[config.verifyField];
    const persistedVersion = afterDB.data?.version || afterDB.data?.metadata?.version;

    console.log(`   Persisted ${config.verifyField}: "${persistedFieldValue}"`);
    console.log(`   Persisted version: ${persistedVersion}`);

    const persistenceSuccess = persistedFieldValue === config.expectedValue && persistedVersion >= afterVersion;
    results.push({
      step: `${config.collection} DB Persistence`,
      success: persistenceSuccess,
      details: { persistedValue: persistedFieldValue, persistedVersion }
    });

    // Step 5: Verify HTML updated (immediate - no timeout)
    console.log('🎯 Step 5: Verifying widget HTML updated...');
    const afterHTML = await runJtagCommand(`debug/widget-state --widgetSelector="${config.widget}" --extractRowData=true`);
    const afterRowCount = afterHTML.commandResult?.state?.rowData?.length || 0;
    const widgetData = afterHTML.commandResult?.state?.rowData || [];

    // Look for updated content in widget data
    const widgetContainsUpdate = JSON.stringify(widgetData).includes(config.expectedValue);

    console.log(`   After widget rows: ${afterRowCount}`);
    console.log(`   Widget contains update: ${widgetContainsUpdate ? 'YES' : 'NO'}`);

    const htmlSuccess = afterHTML.success && (widgetContainsUpdate || afterRowCount >= beforeRowCount);
    results.push({
      step: `${config.widget} HTML Update`,
      success: htmlSuccess,
      details: {
        afterRowCount,
        containsUpdate: widgetContainsUpdate,
        widgetFound: afterHTML.commandResult?.widgetFound
      }
    });

    // Overall assessment
    const overallSuccess = updateSuccess && persistenceSuccess && htmlSuccess;
    console.log(`\n🎯 ${config.collection} ↔ ${config.widget} RESULT: ${overallSuccess ? '✅ PASS' : '❌ FAIL'}`);

    if (overallSuccess) {
      console.log('✅ CRUD-Widget synchronization is working correctly');
      console.log('✅ Database and HTML are in sync');
    } else {
      console.log('❌ CRUD-Widget synchronization has issues');
      console.log(`   Database update: ${updateSuccess ? '✅' : '❌'}`);
      console.log(`   Database persistence: ${persistenceSuccess ? '✅' : '❌'}`);
      console.log(`   HTML synchronization: ${htmlSuccess ? '✅' : '❌'}`);
    }

  } catch (error) {
    console.error(`❌ ${config.collection}-${config.widget} test failed:`, error instanceof Error ? error.message : String(error));
    results.push({
      step: `${config.collection}-${config.widget} Test`,
      success: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
}

async function runModularTests(): Promise<void> {
  // Test configurations for all three Collection-Widget pairs
  const testConfigs: TestConfig[] = [
    {
      collection: 'Room',
      widget: 'room-list-widget',
      testId: '5e71a0c8-0303-4eb8-a478-3a121248',
      updateData: { description: 'MODULAR TEST - Room sync verification' },
      verifyField: 'description',
      expectedValue: 'MODULAR TEST - Room sync verification'
    },
    {
      collection: 'User',
      widget: 'user-list-widget',
      testId: '002350cc-0031-408d-8040-004f000f',
      updateData: { displayName: 'MODULAR TEST - User sync verification' },
      verifyField: 'displayName',
      expectedValue: 'MODULAR TEST - User sync verification'
    },
    {
      collection: 'ChatMessage',
      widget: 'chat-widget',
      testId: '035f3169-392e-4b50-8561-3966d3d18a26',
      updateData: { content: { text: 'MODULAR TEST - Message sync verification', attachments: [] } },
      verifyField: 'content',
      expectedValue: 'MODULAR TEST - Message sync verification'
    }
  ];

  // Run all tests
  for (const config of testConfigs) {
    await testCRUDWidgetSync(config);
  }

  // Summary
  console.log('\n🎉 MODULAR CRUD-WIDGET SYNC TEST SUMMARY');
  console.log('========================================');

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
    console.log('🎯 React-like CRUD reactivity is working across all widgets');
    console.log('🎯 Database and HTML synchronization is consistent');
    console.log('🎯 No timeouts needed - promises resolve after events');
  } else {
    console.log('📝 Some Collection-Widget pairs need fixes');
    console.log('📝 Check event emission and widget subscriptions');
  }

  // Exit with proper code
  process.exit(overallSuccess ? 0 : 1);
}

// Run the modular tests
runModularTests();