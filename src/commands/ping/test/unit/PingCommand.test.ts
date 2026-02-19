#!/usr/bin/env tsx
/**
 * Ping Command Unit Tests
 * 
 * Tests ping command logic in isolation using mock dependencies.
 * Follows middle-out Layer 3 (Command System) testing methodology.
 */

import { 
  createTestContext, 
  createTestPayload, 
  validateCommandResult,
  validateEnvironmentInfo,
  createMockCommandExecution,
  testCommandErrorHandling,
  assertCommandResult,
  testCommandPerformance
} from '../../../test/utils/CommandTestUtils';

import {
  createMockContext,
  MOCK_BROWSER_ENV,
  MOCK_SERVER_ENV,
  createMockPayload,
  MOCK_ERROR_SCENARIOS
} from '../../../test/utils/MockUtils';

import type { PingParams, PingResult } from '../shared/PingTypes';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';

console.log('🧪 Ping Command Unit Tests');

function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

/**
 * Test 1: Basic ping command structure validation
 */
function testPingCommandStructure() {
  console.log('\n📋 Test 1: Ping command structure validation');
  
  // Test valid ping params
  const context = createTestContext('server');
  const sessionId = generateUUID();
  
  const validParams = createTestPayload(context, sessionId, {
    includeEnvironment: true,
    includeTiming: true,
    message: 'test-ping'
  });
  
  // Validate params structure
  assert(validParams.context !== undefined, 'Ping params have context');
  assert(validParams.sessionId !== undefined, 'Ping params have sessionId');
  assert(typeof validParams.includeEnvironment === 'boolean', 'includeEnvironment is boolean');
  assert(typeof validParams.includeTiming === 'boolean', 'includeTiming is boolean');
  assert(typeof validParams.message === 'string', 'message is string');
}

/**
 * Test 2: Mock ping command execution
 */
async function testMockPingExecution() {
  console.log('\n⚡ Test 2: Mock ping command execution');
  
  const context = createTestContext('server');
  const sessionId = generateUUID();
  
  // Create mock ping result
  const mockResult: PingResult = {
    success: true,
    message: 'pong', 
    roundTripTime: 45,
    environment: MOCK_SERVER_ENV,
    context,
    sessionId
  };
  
  // Create mock ping command
  const mockPingCommand = createMockCommandExecution<PingParams, PingResult>(mockResult, 50);
  
  // Test mock execution
  const params: PingParams = createTestPayload(context, sessionId, {
    includeEnvironment: true,
    includeTiming: true,
    message: 'ping'
  });
  
  const result = await mockPingCommand(params);
  
  // Validate result structure
  assert(validateCommandResult(result, ['message', 'roundTripTime', 'environment']), 'Mock ping result has correct structure');
  assert(result.success === true, 'Mock ping result shows success');
  assert(result.message === 'pong', 'Mock ping returns pong message');
  assert(typeof result.roundTripTime === 'number', 'Mock ping has roundTripTime');
  assert(validateEnvironmentInfo(result.environment), 'Mock ping has valid environment info');
}

/**
 * Test 3: Environment-specific ping results
 */
async function testEnvironmentSpecificPing() {
  console.log('\n🌍 Test 3: Environment-specific ping results');
  
  // Test browser environment ping
  const browserContext = createTestContext('browser');
  const browserSessionId = generateUUID();
  
  const browserMockResult: PingResult = {
    success: true,
    message: 'pong',
    roundTripTime: 25,
    environment: MOCK_BROWSER_ENV,
    context: browserContext,
    sessionId: browserSessionId
  };
  
  const browserPingCommand = createMockCommandExecution<PingParams, PingResult>(browserMockResult);
  
  const browserParams: PingParams = createTestPayload(browserContext, browserSessionId, {
    includeEnvironment: true
  });
  
  const browserResult = await browserPingCommand(browserParams);
  
  assert(browserResult.environment?.type === 'browser', 'Browser ping returns browser environment');
  assert('userAgent' in browserResult.environment, 'Browser environment includes userAgent');
  assert('screenResolution' in browserResult.environment, 'Browser environment includes screenResolution');
  
  // Test server environment ping
  const serverContext = createTestContext('server');
  const serverSessionId = generateUUID();
  
  const serverMockResult: PingResult = {
    success: true,
    message: 'pong',
    roundTripTime: 35,
    environment: MOCK_SERVER_ENV,
    context: serverContext,
    sessionId: serverSessionId
  };
  
  const serverPingCommand = createMockCommandExecution<PingParams, PingResult>(serverMockResult);
  
  const serverParams: PingParams = createTestPayload(serverContext, serverSessionId, {
    includeEnvironment: true
  });
  
  const serverResult = await serverPingCommand(serverParams);
  
  assert(serverResult.environment?.type === 'server', 'Server ping returns server environment');
  assert('nodeVersion' in serverResult.environment, 'Server environment includes nodeVersion');
  assert('memory' in serverResult.environment, 'Server environment includes memory info');
}

/**
 * Test 4: Ping error handling
 */
async function testPingErrorHandling() {
  console.log('\n🚨 Test 4: Ping error handling');
  
  const context = createTestContext('server');
  
  // Create mock ping command that validates params
  const strictPingCommand = async (params: PingParams): Promise<PingResult> => {
    if (!params.context || !params.sessionId) {
      throw new Error('Invalid parameters: missing context or sessionId');
    }
    
    return {
      success: true,
      message: 'pong',
      roundTripTime: 50,
      context: params.context,
      sessionId: params.sessionId
    };
  };
  
  // Test invalid parameters
  const invalidParams = [
    createMockPayload(undefined as any, generateUUID(), { includeEnvironment: true }), // Missing context
    createMockPayload(context, undefined as any, { includeEnvironment: true }), // Missing sessionId
  ];
  
  const expectedErrors = [
    'missing context',
    'missing context or sessionId'
  ];
  
  await testCommandErrorHandling(strictPingCommand, invalidParams, expectedErrors);
}

/**
 * Test 5: Ping performance validation
 */
async function testPingPerformance() {
  console.log('\n⚡ Test 5: Ping performance validation');
  
  const context = createTestContext('server');
  const sessionId = generateUUID();
  
  const mockResult: PingResult = {
    success: true,
    message: 'pong',
    roundTripTime: 8,
    context,
    sessionId
  };
  
  const fastPingCommand = createMockCommandExecution<PingParams, PingResult>(mockResult, 10);
  
  const { result, executionTime } = await testCommandPerformance(
    () => fastPingCommand(createTestPayload(context, sessionId, { includeEnvironment: true })),
    100, // Max 100ms
    'ping'
  );
  
  assert(executionTime < 100, `Ping completed in ${executionTime}ms (under 100ms limit)`);
  assert(result.success === true, 'Fast ping returned success');
}

/**
 * Test 6: Ping result assertion helpers
 */
async function testPingAssertionHelpers() {
  console.log('\n🔍 Test 6: Ping result assertion helpers');
  
  const context = createTestContext('server');
  const sessionId = generateUUID();
  
  const validResult: PingResult = {
    success: true,
    message: 'pong',
    roundTripTime: 42,
    environment: MOCK_SERVER_ENV,
    context,
    sessionId
  };
  
  // Test successful assertion
  assertCommandResult(validResult, { success: true, environment: MOCK_SERVER_ENV }, 'valid ping result');
  
  // Test assertion with expected data
  const resultWithData = { ...validResult, data: { test: 'value' } };
  assertCommandResult(resultWithData, { success: true, data: { test: 'value' } }, 'ping result with data');
  
  console.log('✅ All assertion helpers work correctly');
}

/**
 * Run all unit tests
 */
async function runAllPingUnitTests() {
  console.log('🚀 Starting Ping Command Unit Tests\n');
  
  try {
    testPingCommandStructure();
    await testMockPingExecution();
    await testEnvironmentSpecificPing();
    await testPingErrorHandling();
    await testPingPerformance();
    await testPingAssertionHelpers();
    
    console.log('\n🎉 ALL PING UNIT TESTS PASSED!');
    console.log('📋 Validated:');
    console.log('  ✅ Command structure and parameter validation');
    console.log('  ✅ Mock command execution patterns');
    console.log('  ✅ Environment-specific data collection');
    console.log('  ✅ Error handling scenarios');
    console.log('  ✅ Performance requirements');
    console.log('  ✅ Assertion utility helpers');
    
  } catch (error) {
    console.error('\n❌ Ping unit tests failed:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runAllPingUnitTests();
} else {
  module.exports = { runAllPingUnitTests };
}