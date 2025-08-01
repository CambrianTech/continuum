#!/usr/bin/env tsx
/**
 * Ping Command Integration Tests
 * 
 * Tests ping command with real client connections and system integration.
 * Follows middle-out Layer 4 (System Integration) testing methodology.
 */

import {
  testClientConnection,
  testBootstrapSessionHandling,
  testClientCommandExecution,
  testConnectionScenarios,
  validateConnectionResult,
  assertConnectionResult,
  STANDARD_CONNECTION_SCENARIOS,
  type ConnectionTestScenario
} from '../../../test/utils/ClientTestUtils';

import {
  validateCommandResult,
  validateEnvironmentInfo,
  testCommandWithTimeout,
  assertCommandResult
} from '../../../test/utils/CommandTestUtils';

import type { PingParams, PingResult } from '../shared/PingTypes';
import { createListParams } from '../../list/shared/ListTypes';

console.log('🧪 Ping Command Integration Tests');

function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

/**
 * Test 1: Client connection for ping testing
 */
async function testPingClientConnection() {
  console.log('\n🔌 Test 1: Client connection for ping testing');
  
  try {
    // Import the server client
    const { jtag } = await import('../../server-index');
    
    console.log('🔄 Connecting to JTAG system...');
    const connectionResult = await jtag.connect({ targetEnvironment: 'server' });
    
    // Validate connection
    validateConnectionResult(connectionResult, 'ping client connection');
    const connectionInfo = connectionResult.client.getConnectionInfo();
    
    assert(connectionInfo.environment === 'server', 'Connected to server environment');
    assert(!connectionInfo.isBootstrapSession, 'Session properly assigned (not bootstrap)');
    assert(connectionResult.listResult.totalCount > 0, 'Commands discovered');
    
    console.log(`📊 Connection Details:`);
    console.log(`   Environment: ${connectionInfo.environment}`);
    console.log(`   Connection Type: ${connectionInfo.connectionType}`);
    console.log(`   Session ID: ${connectionInfo.sessionId}`);
    console.log(`   Commands Available: ${connectionResult.listResult.totalCount}`);
    
    return connectionResult;
    
  } catch (error) {
    console.error('❌ Client connection failed:', error.message);
    throw error;
  }
}

/**
 * Test 2: Real ping command execution
 */
async function testRealPingExecution() {
  console.log('\n⚡ Test 2: Real ping command execution');
  
  try {
    const { jtag } = await import('../../server-index');
    const connectionResult = await jtag.connect();
    const client = connectionResult.client;
    
    console.log('🏓 Executing ping command...');
    
    // Test basic ping
    const { result: basicResult, executionTime: basicTime } = await testClientCommandExecution<PingResult>(
      client,
      'ping',
      { includeEnvironment: false },
      5000
    );
    
    assert(validateCommandResult(basicResult, ['message']), 'Basic ping has correct structure');
    assert(basicResult.success === true, 'Basic ping succeeded');
    assert(basicResult.message === 'pong', 'Basic ping returns pong');
    console.log(`   Basic ping: ${basicTime}ms`);
    
    // Test ping with environment data
    const { result: envResult, executionTime: envTime } = await testClientCommandExecution<PingResult>(
      client,
      'ping', 
      { includeEnvironment: true, includeTimestamp: true },
      5000
    );
    
    assert(validateCommandResult(envResult, ['message', 'environment', 'timestamp']), 'Environment ping has correct structure');
    assert(envResult.success === true, 'Environment ping succeeded');
    assert(validateEnvironmentInfo(envResult.environment), 'Environment ping has valid environment data');
    console.log(`   Environment ping: ${envTime}ms`);
    
    // Validate environment data content
    if (envResult.environment) {
      assert(envResult.environment.type === 'server', 'Environment type is server');
      assert(typeof envResult.environment.timestamp === 'string', 'Environment has timestamp');
      assert(typeof envResult.environment.platform === 'string', 'Environment has platform');
      
      // Server-specific checks
      if (envResult.environment.type === 'server') {
        assert('nodeVersion' in envResult.environment, 'Server environment has nodeVersion');
        assert('memory' in envResult.environment, 'Server environment has memory info');
        console.log(`   Node Version: ${(envResult.environment as any).nodeVersion}`);
        console.log(`   Memory Usage: ${(envResult.environment as any).memory?.usage || 'unknown'}`);
      }
    }
    
    return { basicResult, envResult };
    
  } catch (error) {
    console.error('❌ Real ping execution failed:', error.message);
    throw error;
  }
}

/**
 * Test 3: Bootstrap session handling in ping context
 */
async function testPingBootstrapHandling() {
  console.log('\n🏷️ Test 3: Bootstrap session handling in ping context');
  
  try {
    const { JTAGClientServer } = await import('../../server/JTAGClientServer');
    
    const bootstrapResult = await testBootstrapSessionHandling(JTAGClientServer);
    
    assert(bootstrapResult.sessionAssigned, 'Real session assigned during bootstrap');
    assert(bootstrapResult.bootstrapCompleted, 'Bootstrap process completed');
    assert(bootstrapResult.finalSessionId !== 'deadbeef-cafe-4bad-8ace-5e551000c0de', 'Final session is not bootstrap session');
    
    console.log(`📋 Bootstrap Results:`);
    console.log(`   Initial Session: ${bootstrapResult.initialSessionId.slice(0, 8)}...`);
    console.log(`   Final Session: ${bootstrapResult.finalSessionId.slice(0, 8)}...`);
    console.log(`   Bootstrap Completed: ${bootstrapResult.bootstrapCompleted}`);
    console.log(`   Session Assigned: ${bootstrapResult.sessionAssigned}`);
    
    return bootstrapResult;
    
  } catch (error) {
    console.error('❌ Bootstrap handling test failed:', error.message);
    throw error;
  }
}

/**
 * Test 4: Ping across different connection scenarios
 */
async function testPingConnectionScenarios() {
  console.log('\n🌐 Test 4: Ping across different connection scenarios');
  
  try {
    const { JTAGClientServer } = await import('../../server/JTAGClientServer');
    
    // Define ping-specific scenarios
    const pingScenarios: ConnectionTestScenario[] = [
      {
        name: 'Server ping connection', 
        options: { targetEnvironment: 'server' },
        expectedEnvironment: 'server',
        minimumCommands: 15,
        shouldSucceed: true
      },
      {
        name: 'Browser ping connection',
        options: { targetEnvironment: 'browser' },
        expectedEnvironment: 'browser', 
        minimumCommands: 15,
        shouldSucceed: true
      }
    ];
    
    const results = await testConnectionScenarios(JTAGClientServer, pingScenarios);
    
    // Validate all scenarios succeeded
    for (const result of results) {
      assertConnectionResult(result, true, result.scenario);
    }
    
    console.log(`📊 Scenario Results:`);
    results.forEach(result => {
      console.log(`   ${result.scenario}: ${result.success ? '✅' : '❌'} (${result.connectionType}, ${result.commandCount} commands)`);
    });
    
    return results;
    
  } catch (error) {
    console.error('❌ Connection scenarios test failed:', error.message);
    throw error;
  }
}

/**
 * Test 5: Ping performance across environments
 */
async function testPingPerformanceIntegration() {
  console.log('\n⚡ Test 5: Ping performance across environments');
  
  try {
    const { jtag } = await import('../../server-index');
    const connectionResult = await jtag.connect();
    const client = connectionResult.client;
    
    // Test multiple ping executions for performance consistency
    const pingTimes: number[] = [];
    const iterations = 5;
    
    for (let i = 0; i < iterations; i++) {
      const { executionTime } = await testClientCommandExecution<PingResult>(
        client,
        'ping',
        { includeEnvironment: i % 2 === 0 }, // Alternate environment inclusion
        3000
      );
      pingTimes.push(executionTime);
    }
    
    const avgTime = pingTimes.reduce((sum, time) => sum + time, 0) / pingTimes.length;
    const maxTime = Math.max(...pingTimes);
    const minTime = Math.min(...pingTimes);
    
    assert(avgTime < 1000, `Average ping time (${avgTime.toFixed(1)}ms) under 1 second`);
    assert(maxTime < 2000, `Maximum ping time (${maxTime}ms) under 2 seconds`);
    
    console.log(`📊 Performance Results:`);
    console.log(`   Iterations: ${iterations}`);
    console.log(`   Average: ${avgTime.toFixed(1)}ms`);
    console.log(`   Min: ${minTime}ms`);
    console.log(`   Max: ${maxTime}ms`);
    console.log(`   Range: ${(maxTime - minTime)}ms`);
    
    return { avgTime, maxTime, minTime, pingTimes };
    
  } catch (error) {
    console.error('❌ Performance integration test failed:', error.message);
    throw error;
  }
}

/**
 * Test 6: Cross-environment ping validation
 */
async function testCrossEnvironmentPing() {
  console.log('\n🔄 Test 6: Cross-environment ping validation');
  
  try {
    const { jtag } = await import('../../server-index');
    
    // Test server-target ping
    const serverConnection = await jtag.connect({ targetEnvironment: 'server' });
    const serverPingResult = await testClientCommandExecution<PingResult>(
      serverConnection.client,
      'ping',
      { includeEnvironment: true },
      3000
    );
    
    assert(serverPingResult.result.success, 'Server ping succeeded');
    assert(serverPingResult.result.environment?.type === 'server', 'Server ping returns server environment');
    
    // Test browser-target ping (uses same local system)
    const browserConnection = await jtag.connect({ targetEnvironment: 'browser' });
    const browserPingResult = await testClientCommandExecution<PingResult>(
      browserConnection.client,
      'ping', 
      { includeEnvironment: true },
      3000
    );
    
    assert(browserPingResult.result.success, 'Browser ping succeeded');
    // Note: Browser target may return server environment if running through local system
    
    console.log(`📊 Cross-Environment Results:`);
    console.log(`   Server Target: ${serverPingResult.result.environment?.type} (${serverPingResult.executionTime}ms)`);
    console.log(`   Browser Target: ${browserPingResult.result.environment?.type} (${browserPingResult.executionTime}ms)`);
    
    return { serverPingResult, browserPingResult };
    
  } catch (error) {
    console.error('❌ Cross-environment ping test failed:', error.message);
    throw error;
  }
}

/**
 * Run all integration tests
 */
async function runAllPingIntegrationTests() {
  console.log('🚀 Starting Ping Command Integration Tests\n');
  
  try {
    await testPingClientConnection();
    await testRealPingExecution();
    await testPingBootstrapHandling();
    await testPingConnectionScenarios();
    await testPingPerformanceIntegration();
    await testCrossEnvironmentPing();
    
    console.log('\n🎉 ALL PING INTEGRATION TESTS PASSED!');
    console.log('📋 Validated:');
    console.log('  ✅ Client connection establishment');  
    console.log('  ✅ Real ping command execution');
    console.log('  ✅ Bootstrap session handling');
    console.log('  ✅ Multiple connection scenarios');
    console.log('  ✅ Performance across environments');
    console.log('  ✅ Cross-environment compatibility');
    
    console.log('\n🏁 Integration testing confirms ping command works end-to-end!');
    
  } catch (error) {
    console.error('\n❌ Ping integration tests failed:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runAllPingIntegrationTests();
} else {
  module.exports = { runAllPingIntegrationTests };
}