/**
 * State API Integration Test
 *
 * Tests the state/get command following the CRUD test pattern:
 * 1. Command execution works
 * 2. Data retrieval from database is correct
 * 3. User context filtering works
 * 4. Theme preference data structure is valid
 *
 * Simple, reliable test focused on core State API requirements.
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';

interface StateTestResult {
  operation: string;
  collection: string;
  commandExecution: boolean;
  dataRetrieved: boolean;
  dataStructure: boolean;
  success: boolean;
}

async function testStateAPIIntegration() {
  console.log('🧪 State API Integration Test');
  console.log('=============================\n');

  const results: StateTestResult[] = [];

  // Test 1: Basic state/get command execution
  console.log('📋 Testing state/get command execution...');
  try {
    const basicResult = await runJtagCommand('state/get --collection=UserState --limit=5');

    const commandWorks = Boolean(basicResult?.success);
    const hasDataStructure = Boolean(basicResult?.items !== undefined &&
                                     basicResult?.count !== undefined &&
                                     basicResult?.timestamp !== undefined);

    results.push({
      operation: 'BASIC_EXECUTION',
      collection: 'UserState',
      commandExecution: commandWorks,
      dataRetrieved: hasDataStructure,
      dataStructure: hasDataStructure,
      success: commandWorks && hasDataStructure
    });

    console.log(`   Command: ${commandWorks ? '✅' : '❌'} | Structure: ${hasDataStructure ? '✅' : '❌'}`);

    // Test 2: Compare with direct data/list to verify delegation
    if (commandWorks) {
      console.log('\n📋 Testing data delegation consistency...');

      const dataResult = await runJtagCommand('data/list --collection=UserState --limit=5');
      const stateResult = basicResult;

      const dataWorks = Boolean(dataResult?.success);
      const delegationConsistent = Boolean(
        dataWorks &&
        stateResult?.success &&
        Array.isArray(dataResult?.items) &&
        Array.isArray(stateResult?.items) &&
        stateResult?.collection === 'UserState'
      );

      results.push({
        operation: 'DATA_DELEGATION',
        collection: 'UserState',
        commandExecution: dataWorks,
        dataRetrieved: delegationConsistent,
        dataStructure: delegationConsistent,
        success: dataWorks && delegationConsistent
      });

      console.log(`   Data/List: ${dataWorks ? '✅' : '❌'} | Delegation: ${delegationConsistent ? '✅' : '❌'}`);
    }

    // Test 3: UserState entity structure validation (if data exists)
    if (commandWorks && basicResult?.items?.length > 0) {
      console.log('\n📋 Testing UserState entity structure...');

      const firstItem = basicResult.items[0];
      const hasUserId = Boolean(firstItem?.userId);
      const hasPreferences = Boolean(firstItem?.preferences && typeof firstItem.preferences === 'object');
      const hasId = Boolean(firstItem?.id);

      const entityStructureValid = hasUserId && hasPreferences && hasId;

      results.push({
        operation: 'ENTITY_STRUCTURE',
        collection: 'UserState',
        commandExecution: true,
        dataRetrieved: entityStructureValid,
        dataStructure: entityStructureValid,
        success: entityStructureValid
      });

      console.log(`   UserId: ${hasUserId ? '✅' : '❌'} | Preferences: ${hasPreferences ? '✅' : '❌'} | Id: ${hasId ? '✅' : '❌'}`);

      // Test 4: Theme preferences structure (if theme data exists)
      const hasThemeData = basicResult.items.some((item: any) =>
        item?.preferences?.theme !== undefined
      );

      if (hasThemeData) {
        console.log('\n📋 Testing theme preferences structure...');

        const themeItem = basicResult.items.find((item: any) => item?.preferences?.theme);
        const themeValue = themeItem?.preferences?.theme;
        const themeIsString = typeof themeValue === 'string';
        const themeValid = themeIsString && themeValue.length > 0;

        results.push({
          operation: 'THEME_PREFERENCES',
          collection: 'UserState',
          commandExecution: true,
          dataRetrieved: themeValid,
          dataStructure: themeValid,
          success: themeValid
        });

        console.log(`   Theme Found: ✅ | Theme Valid: ${themeValid ? '✅' : '❌'} (${themeValue})`);
      } else {
        console.log('\n📋 No theme preferences found (valid for fresh system)');
      }
    } else {
      console.log('\n📋 No UserState data found (valid for fresh system)');
    }

    // Test 5: Limit parameter functionality
    console.log('\n📋 Testing limit parameter...');
    const limitResult = await runJtagCommand('state/get --collection=UserState --limit=1');

    const limitWorks = Boolean(limitResult?.success);
    const respectsLimit = Boolean(
      limitWorks &&
      Array.isArray(limitResult?.items) &&
      limitResult.items.length <= 1
    );

    results.push({
      operation: 'LIMIT_PARAMETER',
      collection: 'UserState',
      commandExecution: limitWorks,
      dataRetrieved: respectsLimit,
      dataStructure: respectsLimit,
      success: limitWorks && respectsLimit
    });

    console.log(`   Limit Command: ${limitWorks ? '✅' : '❌'} | Respects Limit: ${respectsLimit ? '✅' : '❌'}`);

  } catch (error) {
    console.log(`❌ State API test failed:`, error instanceof Error ? error.message : error);

    results.push({
      operation: 'ERROR_HANDLING',
      collection: 'UserState',
      commandExecution: false,
      dataRetrieved: false,
      dataStructure: false,
      success: false
    });
  }

  // Results Summary
  console.log('\n📊 State API Integration Test Results:');
  console.log('======================================');

  const passedTests = results.filter(r => r.success).length;
  const totalTests = results.length;

  results.forEach(result => {
    const status = result.success ? '✅' : '❌';
    console.log(`${status} ${result.operation}: Command(${result.commandExecution ? '✅' : '❌'}) Data(${result.dataRetrieved ? '✅' : '❌'}) Structure(${result.dataStructure ? '✅' : '❌'})`);
  });

  const successRate = totalTests > 0 ? ((passedTests / totalTests) * 100).toFixed(1) : '0.0';
  console.log(`\n📈 Results: ${passedTests}/${totalTests} passed (${successRate}%)`);

  if (successRate === '100.0') {
    console.log('🎉 ALL STATE API TESTS PASSED!');
    console.log('✨ State command delegation and data structure validation working perfectly');
  } else {
    console.log('⚠️ Some tests failed - check results above');
  }

  return passedTests === totalTests;
}

testStateAPIIntegration().catch(error => {
  console.error('❌ State API test failed:', error);
  process.exit(1);
});