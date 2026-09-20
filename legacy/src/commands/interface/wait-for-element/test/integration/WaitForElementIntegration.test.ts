/**
 * TEMPLATE: WaitForElement Command Tests
 * 
 * Replace WaitForElement with actual command name and customize test parameters.
 * This template provides the standard structure for all JTAG command tests.
 */

import { 
  createTestContext, 
  createTestPayload, 
  validateCommandResult,
  createMockCommandExecution,
  testCommandErrorHandling,
  assertCommandResult,
  testCommandPerformance
} from '../utils/CommandTestUtils';

import {
  testClientCommandExecution,
  testConnectionScenarios,
  STANDARD_CONNECTION_SCENARIOS
} from '../utils/ClientTestUtils';

import {
  testCommandCrossEnvironment,
  type EnvironmentTestConfig
} from '../utils/CrossEnvironmentTestUtils';

import {
  createMockContext,
  createMockPayload,
  MOCK_BROWSER_ENV,
  MOCK_SERVER_ENV
} from '../utils/MockUtils';

import type { WaitForElementParams, WaitForElementResult } from '../shared/WaitForElementTypes';
import { generateUUID } from '@system/core/types/CrossPlatformUUID';

console.log('🧪 WaitForElement Command Integration Tests');

function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

/**
 * INTEGRATION TESTS - Test command logic in isolation
 */

async function testUnitBasicExecution() {
  console.log('\n⚡ Integration Test: Basic WaitForElement execution');
  
  const context = createTestContext('server');
  const sessionId = generateUUID();
  
  // Create mock result - customize based on your command
  const mockResult: WaitForElementResult = {
    success: true,
    // Add command-specific result fields here
    context,
    sessionId
  };
  
  const mockCommand = createMockCommandExecution<WaitForElementParams, WaitForElementResult>(mockResult, 50);
  
  // Create test parameters - customize based on your command
  const params: WaitForElementParams = createTestPayload({
    // Add command-specific parameters here
  }, context, sessionId);
  
  const result = await mockCommand(params);
  
  assert(validateCommandResult(result), 'WaitForElement mock result has correct structure');
  assert(result.success === true, 'WaitForElement mock execution succeeded');
  
  // Add command-specific assertions here
}

async function testUnitErrorHandling() {
  console.log('\n🚨 Integration Test: WaitForElement error handling');
  
  const context = createTestContext('server');
  
  const strictCommand = async (params: WaitForElementParams): Promise<WaitForElementResult> => {
    if (!params.context || !params.sessionId) {
      throw new Error('Invalid parameters: missing context or sessionId');
    }
    
    // Add command-specific validation here
    
    return {
      success: true,
      context: params.context,
      sessionId: params.sessionId
    };
  };
  
  // Create invalid parameter scenarios - customize based on your command
  const invalidParams = [
    createMockPayload({}, undefined as any, generateUUID()), // Missing context
    createMockPayload({}, context, undefined as any), // Missing sessionId
    // Add command-specific invalid scenarios here
  ];
  
  const expectedErrors = [
    'missing context',
    'missing context or sessionId'
    // Add command-specific expected errors here
  ];
  
  await testCommandErrorHandling(strictCommand, invalidParams, expectedErrors);
}

async function testUnitPerformance() {
  console.log('\n⚡ Integration Test: WaitForElement performance');
  
  const context = createTestContext('server');
  const sessionId = generateUUID();
  
  const mockResult: WaitForElementResult = {
    success: true,
    context,
    sessionId
  };
  
  const fastCommand = createMockCommandExecution<WaitForElementParams, WaitForElementResult>(mockResult, 10);
  
  const { result, executionTime } = await testCommandPerformance(
    () => fastCommand(createTestPayload({}, context, sessionId)),
    200, // Adjust max time based on command complexity
    'WaitForElement'
  );
  
  assert(executionTime < 200, `WaitForElement completed in ${executionTime}ms (under 200ms limit)`);
  assert(result.success === true, 'Fast WaitForElement returned success');
}

/**
 * INTEGRATION TESTS - Test command with real client connections
 */

async function testIntegrationRealExecution() {
  console.log('\n🔌 Integration Test: Real WaitForElement execution');
  
  try {
    const { jtag } = await import('../../server-index');
    const connectionResult = await jtag.connect();
    const client = connectionResult.client;
    
    console.log('⚡ Executing real WaitForElement command...');
    
    // Customize test parameters for real execution
    const testParams = {
      // Add realistic test parameters here
    };
    
    const { result, executionTime } = await testClientCommandExecution<WaitForElementResult>(
      client,
      'wait-for-element', // Use actual command name
      testParams,
      10000 // Adjust timeout based on command complexity
    );
    
    assert(validateCommandResult(result), 'Real WaitForElement has correct structure');
    assert(result.success === true, 'Real WaitForElement succeeded');
    
    // Add command-specific result validation here
    
    console.log(`   Execution time: ${executionTime}ms`);
    
    return result;
    
  } catch (error) {
    console.error('❌ Real WaitForElement execution failed:', error.message);
    throw error;
  }
}

async function testIntegrationCrossEnvironment() {
  console.log('\n🌍 Integration Test: WaitForElement cross-environment');
  
  const config: EnvironmentTestConfig = {
    environments: ['browser', 'server'],
    commandName: 'wait-for-element',
    testParams: {
      // Add cross-environment test parameters here
    },
    expectedFields: ['success'], // Add command-specific expected fields
    performanceThresholdMs: 10000, // Adjust based on command complexity
    validateEnvironmentData: false // Set to true if command returns environment data
  };
  
  const result = await testCommandCrossEnvironment(config);
  
  assert(result.summary.passed > 0, 'WaitForElement succeeded in at least one environment');
  
  // Log results
  console.log(`📊 Cross-Environment Results:`);
  console.log(`   Environments tested: ${result.environments.length}`);
  console.log(`   Successful: ${result.summary.passed}`);
  console.log(`   Failed: ${result.summary.failed}`);
  console.log(`   Average time: ${result.summary.avgExecutionTime.toFixed(1)}ms`);
  
  result.environments.forEach(env => {
    const status = env.success ? '✅' : '❌';
    const time = env.success ? ` (${env.executionTime}ms)` : '';
    const error = env.error ? ` - ${env.error}` : '';
    console.log(`   ${env.environment}: ${status}${time}${error}`);
  });
  
  return result;
}

/**
 * Run all tests for this command
 */
async function runAllWaitForElementTests(): Promise<void> {
  console.log('🚀 Starting WaitForElement Command Integration Tests\n');
  
  try {
    // Unit tests
    console.log('📋 INTEGRATION TESTS');
    await testUnitBasicExecution();
    await testUnitErrorHandling();
    await testUnitPerformance();
    
    // Integration tests
    console.log('\n📋 INTEGRATION TESTS');
    await testIntegrationRealExecution();
    await testIntegrationCrossEnvironment();
    
    console.log('\n🎉 ALL WaitForElement TESTS PASSED!');
    console.log('📋 Validated:');
    console.log('  ✅ Unit test execution patterns');
    console.log('  ✅ Error handling scenarios');
    console.log('  ✅ Performance requirements');
    console.log('  ✅ Real command execution');
    console.log('  ✅ Cross-environment compatibility');
    
  } catch (error) {
    console.error('\n❌ WaitForElement tests failed:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runAllWaitForElementTests();
} else {
  module.exports = { runAllWaitForElementTests };
}

/**
 * CUSTOMIZATION CHECKLIST:
 * 
 * 1. Replace WaitForElement with actual command name (e.g., Screenshot)
 * 2. Replace wait-for-element with lowercase command name (e.g., screenshot)
 * 3. Import correct types from ../shared/[CommandName]Types
 * 4. Customize test parameters in testParams objects
 * 5. Add command-specific result validation
 * 6. Add command-specific error scenarios
 * 7. Adjust performance thresholds based on command complexity
 * 8. Set validateEnvironmentData to true if command returns environment info
 * 9. Update expectedFields array with command-specific fields
 * 10. Add any command-specific assertions or validation logic
 */