/**
 * CRUD Update Persistence Integration Test
 *
 * Tests the fix for SQLite UPDATE using same table selection as READ
 * Category: database, integration
 */

import { execSync } from 'child_process';
import { readFileSync } from 'fs';
import Database from 'better-sqlite3';

console.log('🧪 CRUD UPDATE PERSISTENCE TEST');
console.log('================================');

interface TestResult {
  step: string;
  success: boolean;
  details?: any;
  error?: string;
}

const results: TestResult[] = [];

async function runJtagCommand(command: string): Promise<any> {
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
    return null;
  } catch (error: any) {
    console.error(`❌ Command failed: ${command}`);
    console.error(error.message);
    throw error;
  }
}

async function queryDatabase(sql: string): Promise<any[]> {
  const dbPath = '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/.continuum/jtag/data/database.sqlite';
  const db = new Database(dbPath, { readonly: true });
  try {
    return db.prepare(sql).all();
  } finally {
    db.close();
  }
}

async function testUserUpdatePersistence(): Promise<void> {
  console.log('\n📋 TEST 1: User Update Persistence (Entity-Specific Table)');
  console.log('----------------------------------------------------------');

  try {
    // Step 1: Read initial user data
    console.log('🔍 Step 1: Reading initial User data...');
    const initialRead = await runJtagCommand('data/read --collection=User --id=test-user');
    const initialDisplayName = initialRead.data.displayName;
    console.log(`   Initial displayName: "${initialDisplayName}"`);

    results.push({
      step: 'Initial Read',
      success: true,
      details: { displayName: initialDisplayName }
    });

    // Step 2: Update user data
    console.log('🔄 Step 2: Updating User displayName...');
    const updateResult = await runJtagCommand('data/update --collection=User --id=002350cc-0031-408d-8040-004f000f --data=\'{"displayName": "INTEGRATION TEST SUCCESS"}\'');
    const updatedDisplayName = updateResult.data.data.displayName;
    console.log(`   Update response displayName: "${updatedDisplayName}"`);

    results.push({
      step: 'Update Command',
      success: updateResult.found === true,
      details: {
        found: updateResult.found,
        displayName: updatedDisplayName,
        previousVersion: updateResult.previousVersion,
        newVersion: updateResult.newVersion
      }
    });

    // Step 3: Verify update persisted via JTAG read
    console.log('✅ Step 3: Verifying update persisted (JTAG read)...');
    const verifyRead = await runJtagCommand('data/read --collection=User --id=002350cc-0031-408d-8040-004f000f');
    const persistedDisplayName = verifyRead.data.displayName;
    console.log(`   Persisted displayName: "${persistedDisplayName}"`);

    const jtagPersistSuccess = persistedDisplayName === 'INTEGRATION TEST SUCCESS';
    results.push({
      step: 'JTAG Read Verification',
      success: jtagPersistSuccess,
      details: { displayName: persistedDisplayName }
    });

    // Step 4: Verify update persisted via direct database query
    console.log('🔍 Step 4: Verifying update persisted (Direct DB query)...');
    const dbResults = await queryDatabase(
      "SELECT id, display_name as displayName FROM user WHERE id = '002350cc-0031-408d-8040-004f000f'"
    );

    const dbDisplayName = dbResults[0]?.displayName;
    console.log(`   Database displayName: "${dbDisplayName}"`);

    const dbPersistSuccess = dbDisplayName === 'INTEGRATION TEST SUCCESS';
    results.push({
      step: 'Database Verification',
      success: dbPersistSuccess,
      details: { displayName: dbDisplayName, rowCount: dbResults.length }
    });

    // Final assessment
    const overallSuccess = jtagPersistSuccess && dbPersistSuccess;
    console.log(`\n🎯 TEST 1 RESULT: ${overallSuccess ? '✅ PASS' : '❌ FAIL'}`);

    if (!overallSuccess) {
      console.log('   ❌ Update command succeeded but data did not persist');
      console.log(`   📝 JTAG read: "${persistedDisplayName}"`);
      console.log(`   📝 DB query: "${dbDisplayName}"`);
      console.log(`   📝 Expected: "INTEGRATION TEST SUCCESS"`);
    }

  } catch (error: any) {
    console.error('❌ Test 1 failed with error:', error.message);
    results.push({
      step: 'User Update Test',
      success: false,
      error: error.message
    });
  }
}

async function testVersionTracking(): Promise<void> {
  console.log('\n📋 TEST 2: Version Tracking');
  console.log('---------------------------');

  try {
    // Multiple updates to test version incrementing
    console.log('🔄 Update 1: Setting displayName to "Version Test 1"...');
    const update1 = await runJtagCommand('data/update --collection=User --id=002350cc-0031-408d-8040-004f000f --data=\'{"displayName": "Version Test 1"}\'');

    console.log('🔄 Update 2: Setting displayName to "Version Test 2"...');
    const update2 = await runJtagCommand('data/update --collection=User --id=002350cc-0031-408d-8040-004f000f --data=\'{"displayName": "Version Test 2"}\'');

    // Check final state
    const finalRead = await runJtagCommand('data/read --collection=User --id=002350cc-0031-408d-8040-004f000f');

    const versionCorrect = finalRead.data.version > update1.newVersion;
    const dataCorrect = finalRead.data.displayName === 'Version Test 2';

    console.log(`   Final version: ${finalRead.data.version}`);
    console.log(`   Final displayName: "${finalRead.data.displayName}"`);

    const versionSuccess = versionCorrect && dataCorrect;
    console.log(`\n🎯 TEST 2 RESULT: ${versionSuccess ? '✅ PASS' : '❌ FAIL'}`);

    results.push({
      step: 'Version Tracking',
      success: versionSuccess,
      details: {
        finalVersion: finalRead.data.version,
        finalDisplayName: finalRead.data.displayName,
        expectedDisplayName: 'Version Test 2'
      }
    });

  } catch (error: any) {
    console.error('❌ Test 2 failed with error:', error.message);
    results.push({
      step: 'Version Tracking Test',
      success: false,
      error: error.message
    });
  }
}

async function runTests(): Promise<void> {
  try {
    await testUserUpdatePersistence();
    await testVersionTracking();

    // Summary
    console.log('\n🎉 CRUD UPDATE PERSISTENCE TEST SUMMARY');
    console.log('=====================================');

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
      console.log('🎯 SQLite UPDATE now uses same table selection as READ');
      console.log('🎯 Real-time events should now work with actual persisted data');
      console.log('🎯 CRUD system is ready for multi-paradigm real-time architecture');
    }

    // Exit with proper code
    process.exit(overallSuccess ? 0 : 1);

  } catch (error: any) {
    console.error('❌ Test suite failed:', error.message);
    process.exit(1);
  }
}

// Run the tests
runTests();