#!/usr/bin/env tsx
/**
 * Process Registry Command Unit Tests
 * 
 * Tests process registry command logic in isolation using mock dependencies.
 * Follows middle-out Layer 3 (Command System) testing methodology.
 */

import { 
  createTestContext, 
  createTestPayload, 
  validateCommandResult,
  createMockCommandExecution,
  testCommandErrorHandling,
  assertCommandResult,
  testCommandPerformance
} from '../../../test/utils/CommandTestUtils';

import {
  createMockContext,
  MOCK_SERVER_ENV,
  createMockPayload
} from '../../../test/utils/MockUtils';

import type { 
  RegisterProcessParams, 
  RegisterProcessResult,
  ListProcessesParams,
  ListProcessesResult,
  CleanupProcessesParams,
  CleanupProcessesResult,
  ProcessRegistryEntry
} from '../../shared/ProcessRegistryTypes';
import { 
  validateRegisterProcessParams,
  getProcessCapabilities,
  generateProcessId
} from '../../shared/ProcessRegistryCommand';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';

console.log('🧪 Process Registry Command Unit Tests');

function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

/**
 * Test 1: Process ID generation validation
 */
function testProcessIdGeneration() {
  console.log('\n📋 Test 1: Process ID generation validation');
  
  const processId1 = generateProcessId();
  const processId2 = generateProcessId();
  
  assert(typeof processId1 === 'string', 'Process ID is string');
  assert(processId1.startsWith('jtag-'), 'Process ID has correct prefix');
  assert(processId1 !== processId2, 'Process IDs are unique');
  assert(processId1.length > 10, 'Process ID has reasonable length');
}

/**
 * Test 2: Process capabilities mapping
 */
function testProcessCapabilities() {
  console.log('\n⚙️ Test 2: Process capabilities mapping');
  
  const serverCaps = getProcessCapabilities('server');
  const browserCaps = getProcessCapabilities('browser');
  const testCaps = getProcessCapabilities('test');
  const clientCaps = getProcessCapabilities('client');
  
  assert(Array.isArray(serverCaps), 'Server capabilities is array');
  assert(serverCaps.includes('websocket-server'), 'Server has websocket capability');
  assert(serverCaps.includes('file-operations'), 'Server has file operations capability');
  
  assert(Array.isArray(browserCaps), 'Browser capabilities is array');
  assert(browserCaps.includes('screenshot'), 'Browser has screenshot capability');
  assert(browserCaps.includes('dom-interaction'), 'Browser has DOM interaction capability');
  
  assert(Array.isArray(testCaps), 'Test capabilities is array');
  assert(testCaps.includes('test-execution'), 'Test has test execution capability');
  
  assert(Array.isArray(clientCaps), 'Client capabilities is array');
  assert(clientCaps.includes('command-sending'), 'Client has command sending capability');
}

/**
 * Test 3: Parameter validation
 */
function testParameterValidation() {
  console.log('\n✅ Test 3: Parameter validation');
  
  const context = createTestContext('server');
  const sessionId = generateUUID();
  
  // Valid parameters
  const validParams: RegisterProcessParams = createTestPayload(context, sessionId, {
    processType: 'server',
    description: 'Test server process',
    ports: [9001, 9002],
    capabilities: ['websocket-server', 'command-execution']
  });
  
  const validationResult = validateRegisterProcessParams(validParams);
  assert(validationResult === null, 'Valid parameters pass validation');
  
  // Invalid parameters - missing processType
  const invalidParams1 = createTestPayload(context, sessionId, {
    description: 'Test process'
  }) as any;
  delete invalidParams1.processType;
  
  const error1 = validateRegisterProcessParams(invalidParams1);
  assert(error1 === 'processType is required', 'Missing processType validation works');
  
  // Invalid parameters - empty description
  const invalidParams2: RegisterProcessParams = createTestPayload(context, sessionId, {
    processType: 'server',
    description: '',
    ports: [9001]
  });
  
  const error2 = validateRegisterProcessParams(invalidParams2);
  assert(error2 === 'description is required and cannot be empty', 'Empty description validation works');
  
  // Invalid parameters - invalid processType
  const invalidParams3: RegisterProcessParams = createTestPayload(context, sessionId, {
    processType: 'invalid' as any,
    description: 'Test process'
  });
  
  const error3 = validateRegisterProcessParams(invalidParams3);
  assert(error3?.includes('processType must be one of'), 'Invalid processType validation works');
  
  // Invalid parameters - invalid port range
  const invalidParams4: RegisterProcessParams = createTestPayload(context, sessionId, {
    processType: 'server',
    description: 'Test process',
    ports: [70000] // Too high
  });
  
  const error4 = validateRegisterProcessParams(invalidParams4);
  assert(error4 === 'all ports must be between 1 and 65535', 'Invalid port range validation works');
}

/**
 * Test 4: Mock process registration
 */
async function testMockProcessRegistration() {
  console.log('\n🏷️ Test 4: Mock process registration');
  
  const context = createTestContext('server');
  const sessionId = generateUUID();
  
  const mockResult: RegisterProcessResult = {
    success: true,
    processId: generateProcessId(),
    context,
    sessionId
  };
  
  const mockRegisterCommand = createMockCommandExecution<RegisterProcessParams, RegisterProcessResult>(mockResult, 50);
  
  const params: RegisterProcessParams = createTestPayload(context, sessionId, {
    processType: 'server',
    description: 'Mock test server process',
    ports: [9001, 9002],
    capabilities: ['websocket-server', 'command-execution']
  });
  
  const result = await mockRegisterCommand(params);
  
  assert(validateCommandResult(result, ['processId']), 'Mock registration result has correct structure');
  assert(result.success === true, 'Mock registration result shows success');
  assert(typeof result.processId === 'string', 'Mock registration returns processId');
  assert(result.processId!.startsWith('jtag-'), 'Mock registration returns valid processId format');
}

/**
 * Test 5: Mock process listing
 */
async function testMockProcessListing() {
  console.log('\n📋 Test 5: Mock process listing');
  
  const context = createTestContext('server');
  const sessionId = generateUUID();
  
  const mockProcesses: ProcessRegistryEntry[] = [
    {
      processId: generateProcessId(),
      nodeId: 'test-node-1',
      pid: 12345,
      ports: [9001, 9002],
      startTime: Date.now() - 60000, // 1 minute ago
      processType: 'server',
      description: 'Test server process',
      capabilities: ['websocket-server', 'command-execution']
    },
    {
      processId: generateProcessId(),
      nodeId: 'test-node-1',
      pid: 12346,
      ports: [8080],
      startTime: Date.now() - 30000, // 30 seconds ago
      processType: 'browser',
      description: 'Test browser process',
      capabilities: ['screenshot', 'dom-interaction']
    }
  ];
  
  const mockResult: ListProcessesResult = {
    success: true,
    processes: mockProcesses,
    context,
    sessionId
  };
  
  const mockListCommand = createMockCommandExecution<ListProcessesParams, ListProcessesResult>(mockResult, 25);
  
  const params: ListProcessesParams = createTestPayload(context, sessionId, {
    includeStale: false
  });
  
  const result = await mockListCommand(params);
  
  assert(validateCommandResult(result, ['processes']), 'Mock list result has correct structure');
  assert(result.success === true, 'Mock list result shows success');
  assert(Array.isArray(result.processes), 'Mock list returns processes array');
  assert(result.processes.length === 2, 'Mock list returns expected number of processes');
  
  const serverProcess = result.processes.find(p => p.processType === 'server');
  const browserProcess = result.processes.find(p => p.processType === 'browser');
  
  assert(serverProcess !== undefined, 'Mock list includes server process');
  assert(browserProcess !== undefined, 'Mock list includes browser process');
  assert(serverProcess!.ports.includes(9001), 'Server process has expected ports');
  assert(browserProcess!.capabilities.includes('screenshot'), 'Browser process has expected capabilities');
}

/**
 * Test 6: Mock process cleanup
 */
async function testMockProcessCleanup() {
  console.log('\n🧹 Test 6: Mock process cleanup');
  
  const context = createTestContext('server');
  const sessionId = generateUUID();
  
  const killedProcesses: ProcessRegistryEntry[] = [
    {
      processId: generateProcessId(),
      nodeId: 'test-node-1',
      pid: 99999,
      ports: [9999],
      startTime: Date.now() - 120000, // 2 minutes ago (stale)
      processType: 'test',
      description: 'Stale test process',
      capabilities: ['test-execution']
    }
  ];
  
  const preservedProcesses: ProcessRegistryEntry[] = [
    {
      processId: generateProcessId(),
      nodeId: 'test-node-1',
      pid: 12345,
      ports: [9001, 9002],
      startTime: Date.now() - 10000, // 10 seconds ago (active)
      processType: 'server',
      description: 'Active server process',
      capabilities: ['websocket-server', 'command-execution']
    }
  ];
  
  const mockResult: CleanupProcessesResult = {
    success: true,
    killedProcesses,
    preservedProcesses,
    cleanedPorts: [9999],
    errors: [],
    context,
    sessionId
  };
  
  const mockCleanupCommand = createMockCommandExecution<CleanupProcessesParams, CleanupProcessesResult>(mockResult, 100);
  
  const params: CleanupProcessesParams = createTestPayload(context, sessionId, {
    preserveActive: true,
    forceAll: false
  });
  
  const result = await mockCleanupCommand(params);
  
  assert(validateCommandResult(result, ['killedProcesses', 'preservedProcesses', 'cleanedPorts', 'errors']), 'Mock cleanup result has correct structure');
  assert(result.success === true, 'Mock cleanup result shows success');
  assert(Array.isArray(result.killedProcesses), 'Mock cleanup returns killed processes array');
  assert(Array.isArray(result.preservedProcesses), 'Mock cleanup returns preserved processes array');
  assert(Array.isArray(result.cleanedPorts), 'Mock cleanup returns cleaned ports array');
  assert(Array.isArray(result.errors), 'Mock cleanup returns errors array');
  
  assert(result.killedProcesses.length === 1, 'Mock cleanup killed expected number of processes');
  assert(result.preservedProcesses.length === 1, 'Mock cleanup preserved expected number of processes');
  assert(result.cleanedPorts.includes(9999), 'Mock cleanup cleaned expected ports');
  assert(result.errors.length === 0, 'Mock cleanup completed without errors');
}

/**
 * Run all unit tests
 */
async function runAllProcessRegistryUnitTests() {
  console.log('🚀 Starting Process Registry Command Unit Tests\n');
  
  try {
    testProcessIdGeneration();
    testProcessCapabilities();
    testParameterValidation();
    await testMockProcessRegistration();
    await testMockProcessListing();
    await testMockProcessCleanup();
    
    console.log('\n🎉 ALL PROCESS REGISTRY UNIT TESTS PASSED!');
    console.log('📋 Validated:');
    console.log('  ✅ Process ID generation and uniqueness');
    console.log('  ✅ Process capabilities mapping by type');
    console.log('  ✅ Parameter validation logic');
    console.log('  ✅ Mock process registration');
    console.log('  ✅ Mock process listing with filtering');
    console.log('  ✅ Mock process cleanup with preservation logic');
    
  } catch (error) {
    console.error('\n❌ Process registry unit tests failed:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runAllProcessRegistryUnitTests();
} else {
  module.exports = { runAllProcessRegistryUnitTests };
}