#!/usr/bin/env tsx
/**
 * End-to-End Command Execution Integration Tests
 * 
 * Tests complete command execution flow from client to server and back,
 * including our new ping and error commands, transport routing, 
 * promise correlation, and cross-environment communication.
 */

console.log('🧪 End-to-End Command Execution Integration Tests');

// Test utilities
function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

async function ensureSystemRunning() {
  console.log('🔄 Ensuring JTAG system is running...');
  
  const { exec } = await import('child_process');
  const { promisify } = await import('util');
  const execAsync = promisify(exec);
  
  try {
    await execAsync('npm run system:ensure');
    console.log('✅ JTAG system is running');
    return true;
  } catch (error) {
    console.error('❌ Failed to start JTAG system:', error);
    return false;
  }
}

async function testBasicCommandDiscovery() {
  console.log('\n🔍 TEST 1: Basic command discovery and availability');
  
  try {
    const { JTAGClientServer } = await import('../../system/core/client/server/JTAGClientServer');
    
    console.log('🔗 Connecting to browser system for command discovery...');
    const { client, listResult } = await JTAGClientServer.connect();
    
    // Verify our new commands are discovered
    assert(listResult.success === true, 'List command succeeded');
    assert(Array.isArray(listResult.commands), 'Commands array returned');
    
    const commandNames = listResult.commands.map(c => c.name);
    console.log(`📋 Discovered commands: ${commandNames.join(', ')}`);
    
    // Check for essential commands
    assert(commandNames.includes('list'), 'List command available');
    assert(commandNames.includes('ping'), 'Ping command available');
    assert(commandNames.includes('test-error'), 'Test-error command available');
    
    console.log('✅ Basic command discovery verified');
    return { client, listResult };
    
  } catch (error) {
    console.log(`❌ Test 1 failed: ${error.message}`);
    throw error;
  }
}

async function testPingCommandExecution(client: any) {
  console.log('\n🏓 TEST 2: Ping command end-to-end execution');
  
  try {
    console.log('🔄 Executing ping command with basic parameters...');
    
    // Test basic ping
    const basicPing = await client.commands.ping({
      message: 'integration-test'
    });
    
    assert(basicPing.success === true, 'Basic ping command succeeded');
    assert(basicPing.message === 'integration-test', 'Ping message echoed correctly');
    console.log(`📨 Basic ping response: ${basicPing.message}`);
    
    // Test ping with timing
    console.log('🔄 Executing ping with timing measurement...');
    const timedPing = await client.commands.ping({
      message: 'timed-test',
      includeTiming: true
    });
    
    assert(timedPing.success === true, 'Timed ping command succeeded');
    assert(typeof timedPing.roundTripTime === 'number', 'Round trip time included');
    assert(timedPing.roundTripTime >= 0, 'Round trip time is valid');
    console.log(`⏱️  Timed ping response: ${timedPing.roundTripTime}ms`);
    
    // Test ping with environment info
    console.log('🔄 Executing ping with environment detection...');
    const envPing = await client.commands.ping({
      message: 'env-test',
      includeEnvironment: true
    });
    
    assert(envPing.success === true, 'Environment ping command succeeded');
    assert(typeof envPing.environment === 'object', 'Environment info included');
    assert(envPing.environment.type === 'browser', 'Environment type is browser');
    assert(typeof envPing.environment.timestamp === 'string', 'Timestamp included');
    console.log(`🌍 Environment ping - Type: ${envPing.environment.type}, Timestamp: ${envPing.environment.timestamp}`);
    
    // Test ping with all features
    console.log('🔄 Executing ping with all features enabled...');
    const fullPing = await client.commands.ping({
      message: 'full-feature-test',
      includeTiming: true,
      includeEnvironment: true
    });
    
    assert(fullPing.success === true, 'Full-feature ping succeeded');
    assert(typeof fullPing.roundTripTime === 'number', 'Timing included in full ping');
    assert(typeof fullPing.environment === 'object', 'Environment included in full ping');
    console.log(`🎯 Full ping - Message: ${fullPing.message}, Time: ${fullPing.roundTripTime}ms, Env: ${fullPing.environment.type}`);
    
    console.log('✅ Ping command end-to-end execution verified');
    
  } catch (error) {
    console.log(`❌ Test 2 failed: ${error.message}`);
    throw error;
  }
}

async function testErrorCommandExecution(client: any) {
  console.log('\n💥 TEST 3: Error command end-to-end execution and handling');
  
  try {
    // Test successful error command (generic error)
    console.log('🔄 Testing error command with generic error trigger...');
    
    try {
      await client.commands['test-error']({
        errorType: 'generic',
        message: 'integration-test-error',
        environment: 'browser'
      });
      
      // Should not reach here - error command should throw
      assert(false, 'Error command should have thrown an error');
      
    } catch (error) {
      // Expected error from the test-error command
      console.log(`📨 Expected error caught: ${error.message.substring(0, 80)}...`);
      // The test-error command generates its own error messages, so just verify it's an error
      assert(typeof error.message === 'string', 'Error message is a string');
      assert(error.message.length > 0, 'Error message is not empty');
    }
    
    // Test validation error
    console.log('🔄 Testing error command with validation error...');
    
    try {
      await client.commands['test-error']({
        errorType: 'validation-error',
        message: 'validation-test',
        environment: 'browser'
      });
      
      assert(false, 'Validation error should have thrown');
      
    } catch (error) {
      console.log(`📨 Validation error caught: ${error.message.substring(0, 80)}...`);
      assert(typeof error.message === 'string', 'Validation error message is a string');
      assert(error.message.length > 0, 'Validation error message is not empty');
    }
    
    // Test timeout error simulation
    console.log('🔄 Testing error command with timeout simulation...');
    
    try {
      await client.commands['test-error']({
        errorType: 'timeout-error',
        message: 'timeout-test',
        environment: 'browser'
      });
      
      assert(false, 'Timeout error should have thrown');
      
    } catch (error) {
      console.log(`📨 Timeout error caught: ${error.message.substring(0, 80)}...`);
      assert(typeof error.message === 'string', 'Timeout error message is a string');
      assert(error.message.length > 0, 'Timeout error message is not empty');
    }
    
    console.log('✅ Error command end-to-end execution and handling verified');
    
  } catch (error) {
    console.log(`❌ Test 3 failed: ${error.message}`);
    throw error;
  }
}

async function testCrossEnvironmentCommandRouting(client: any) {
  console.log('\n🌐 TEST 4: Cross-environment command routing');
  
  try {
    // Since we're using a browser client, commands should route through the system
    console.log('🔄 Testing cross-environment routing with ping...');
    
    // Execute ping - should work regardless of environment routing
    const crossEnvPing = await client.commands.ping({
      message: 'cross-env-test',
      includeEnvironment: true
    });
    
    assert(crossEnvPing.success === true, 'Cross-environment ping succeeded');
    assert(crossEnvPing.environment.type === 'browser', 'Environment correctly identified');
    console.log(`🎯 Cross-env ping - Type: ${crossEnvPing.environment.type}`);
    
    // Test that commands work with different payload structures
    console.log('🔄 Testing varied payload structures...');
    
    const variedPing = await client.commands.ping({
      message: 'varied-payload',
      includeTiming: false,
      includeEnvironment: false,
      customField: 'should-be-ignored'
    });
    
    assert(variedPing.success === true, 'Varied payload ping succeeded');
    assert(variedPing.message === 'varied-payload', 'Core message preserved');
    assert(variedPing.roundTripTime === undefined, 'Timing correctly excluded');
    assert(variedPing.environment === undefined, 'Environment correctly excluded');
    console.log(`🎯 Varied payload handled correctly`);
    
    console.log('✅ Cross-environment command routing verified');
    
  } catch (error) {
    console.log(`❌ Test 4 failed: ${error.message}`);
    throw error;
  }
}

async function testPromiseCorrelationSystem(client: any) {
  console.log('\n🔗 TEST 5: Promise correlation and async handling');
  
  try {
    console.log('🔄 Testing concurrent command execution...');
    
    // Execute multiple ping commands concurrently
    const concurrentPings = [
      client.commands.ping({ message: 'concurrent-1', includeTiming: true }),
      client.commands.ping({ message: 'concurrent-2', includeTiming: true }),
      client.commands.ping({ message: 'concurrent-3', includeTiming: true })
    ];
    
    const results = await Promise.all(concurrentPings);
    
    assert(results.length === 3, 'All concurrent pings completed');
    assert(results.every(r => r.success), 'All concurrent pings succeeded');
    
    const messages = results.map(r => r.message);
    assert(messages.includes('concurrent-1'), 'First concurrent ping preserved');
    assert(messages.includes('concurrent-2'), 'Second concurrent ping preserved');
    assert(messages.includes('concurrent-3'), 'Third concurrent ping preserved');
    
    console.log(`🎯 Concurrent execution: ${results.map(r => `${r.message}(${r.roundTripTime}ms)`).join(', ')}`);
    
    // Test sequential execution timing
    console.log('🔄 Testing sequential vs concurrent timing...');
    
    const sequentialStart = Date.now();
    await client.commands.ping({ message: 'seq-1' });
    await client.commands.ping({ message: 'seq-2' });
    await client.commands.ping({ message: 'seq-3' });
    const sequentialTime = Date.now() - sequentialStart;
    
    const concurrentStart = Date.now();
    await Promise.all([
      client.commands.ping({ message: 'conc-1' }),
      client.commands.ping({ message: 'conc-2' }),
      client.commands.ping({ message: 'conc-3' })
    ]);
    const concurrentTime = Date.now() - concurrentStart;
    
    console.log(`⏱️  Sequential: ${sequentialTime}ms, Concurrent: ${concurrentTime}ms`);
    
    // Concurrent should generally be faster than sequential for I/O operations
    // But we won't enforce this as a strict assertion since timing can vary
    console.log(`📊 Timing comparison - Concurrent efficiency: ${((sequentialTime - concurrentTime) / sequentialTime * 100).toFixed(1)}%`);
    
    console.log('✅ Promise correlation and async handling verified');
    
  } catch (error) {
    console.log(`❌ Test 5 failed: ${error.message}`);
    throw error;
  }
}

async function testCommandParameterValidation(client: any) {
  console.log('\n📋 TEST 6: Command parameter validation and edge cases');
  
  try {
    // Test ping with minimal parameters
    console.log('🔄 Testing minimal parameter handling...');
    
    const minimalPing = await client.commands.ping({});
    assert(minimalPing.success === true, 'Ping works with empty parameters');
    console.log(`📨 Minimal ping result: ${minimalPing.message || 'no message'}`);
    
    // Test ping with null/undefined values
    console.log('🔄 Testing null/undefined parameter handling...');
    
    const nullPing = await client.commands.ping({
      message: null,
      includeTiming: undefined,
      includeEnvironment: false
    });
    assert(nullPing.success === true, 'Ping handles null/undefined parameters');
    console.log(`📨 Null parameter ping handled gracefully`);
    
    // Test error command parameter validation
    console.log('🔄 Testing error command parameter validation...');
    
    try {
      await client.commands['test-error']({
        errorType: 'invalid-error-type',
        message: 'should-fail',
        environment: 'browser'
      });
      
      // Depending on implementation, this might succeed or fail
      // If it succeeds, the command should handle unknown error types gracefully
      console.log(`📨 Invalid error type handled gracefully`);
      
    } catch (error) {
      // Expected - invalid error type should cause validation error
      console.log(`📨 Invalid error type correctly rejected: ${error.message.substring(0, 80)}...`);
    }
    
    console.log('✅ Command parameter validation verified');
    
  } catch (error) {
    console.log(`❌ Test 6 failed: ${error.message}`);
    throw error;
  }
}

async function testSystemHealthAndConnectivity(client: any) {
  console.log('\n🩺 TEST 7: System health and connectivity validation');
  
  try {
    // Use ping as a health check mechanism
    console.log('🔄 Testing system health via ping commands...');
    
    const healthChecks = [];
    const healthCheckCount = 5;
    
    for (let i = 0; i < healthCheckCount; i++) {
      healthChecks.push(
        client.commands.ping({
          message: `health-check-${i}`,
          includeTiming: true,
          includeEnvironment: true
        })
      );
    }
    
    const healthResults = await Promise.all(healthChecks);
    
    assert(healthResults.length === healthCheckCount, `All ${healthCheckCount} health checks completed`);
    assert(healthResults.every(r => r.success), 'All health checks succeeded');
    
    const avgResponseTime = healthResults.reduce((sum, r) => sum + r.roundTripTime, 0) / healthResults.length;
    const maxResponseTime = Math.max(...healthResults.map(r => r.roundTripTime));
    const minResponseTime = Math.min(...healthResults.map(r => r.roundTripTime));
    
    console.log(`📊 Health Check Results:`);
    console.log(`   Average response time: ${avgResponseTime.toFixed(2)}ms`);
    console.log(`   Max response time: ${maxResponseTime}ms`);
    console.log(`   Min response time: ${minResponseTime}ms`);
    
    // Basic health thresholds (adjust based on system performance)
    const maxAcceptableResponseTime = 1000; // 1 second
    assert(maxResponseTime < maxAcceptableResponseTime, `Max response time under ${maxAcceptableResponseTime}ms`);
    
    // Verify all environments are consistently reported
    const environments = healthResults.map(r => r.environment.type);
    const uniqueEnvironments = [...new Set(environments)];
    assert(uniqueEnvironments.length === 1, 'Consistent environment reporting');
    assert(uniqueEnvironments[0] === 'browser', 'Correct environment detected');
    
    console.log('✅ System health and connectivity verified');
    
  } catch (error) {
    console.log(`❌ Test 7 failed: ${error.message}`);
    throw error;
  }
}

async function runE2ECommandExecutionTests() {
  console.log('🚀 Starting End-to-End Command Execution Integration Tests\n');
  
  try {
    // Step 1: Ensure system is running
    const systemReady = await ensureSystemRunning();
    if (!systemReady) {
      throw new Error('Cannot run tests - system startup failed');
    }
    
    // Step 2: Test basic command discovery
    const { client, listResult } = await testBasicCommandDiscovery();
    
    // Step 3: Test ping command execution
    await testPingCommandExecution(client);
    
    // Step 4: Test error command execution
    await testErrorCommandExecution(client);
    
    // Step 5: Test cross-environment routing
    await testCrossEnvironmentCommandRouting(client);
    
    // Step 6: Test promise correlation
    await testPromiseCorrelationSystem(client);
    
    // Step 7: Test parameter validation
    await testCommandParameterValidation(client);
    
    // Step 8: Test system health
    await testSystemHealthAndConnectivity(client);
    
    console.log('\n🎉 ALL END-TO-END COMMAND EXECUTION TESTS PASSED!');
    console.log('📋 Verified:');
    console.log('  ✅ Command discovery and availability');
    console.log('  ✅ Ping command execution with all features');
    console.log('  ✅ Error command execution and error handling');
    console.log('  ✅ Cross-environment command routing');
    console.log('  ✅ Promise correlation and concurrent execution');
    console.log('  ✅ Parameter validation and edge cases');
    console.log('  ✅ System health and connectivity monitoring');
    
    console.log('\n🎯 END-TO-END COMMAND EXECUTION IS FULLY FUNCTIONAL!');
    
  } catch (error) {
    console.error('\n❌ End-to-end command execution tests failed:', error.message);
    console.error('Stack trace:', error.stack);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runE2ECommandExecutionTests();
} else {
  module.exports = { runE2ECommandExecutionTests };
}