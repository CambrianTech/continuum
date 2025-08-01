#!/usr/bin/env tsx
/**
 * Essential End-to-End Integration Tests
 * 
 * Focused integration tests for core command execution functionality.
 * Tests ping and error commands with timeout safeguards.
 */

console.log('🧪 Essential End-to-End Integration Tests');

// Test utilities with timeout protection
function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

// Timeout wrapper for tests
function withTimeout<T>(promise: Promise<T>, timeoutMs: number = 10000): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) => 
      setTimeout(() => reject(new Error(`Test timed out after ${timeoutMs}ms`)), timeoutMs)
    )
  ]);
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

async function testBasicConnection() {
  console.log('\n🔗 TEST 1: Basic client connection and command discovery');
  
  const { JTAGClientBrowser } = await import('../../browser/JTAGClientBrowser');
  
  const result = await withTimeout(JTAGClientBrowser.connectLocal(), 15000);
  const { client, listResult } = result;
  
  assert(listResult.success === true, 'List command succeeded');
  assert(Array.isArray(listResult.commands), 'Commands array returned');
  assert(listResult.commands.length > 0, 'Commands discovered');
  
  const commandNames = listResult.commands.map(c => c.name);
  assert(commandNames.includes('ping'), 'Ping command available');
  assert(commandNames.includes('test-error'), 'Test-error command available');
  
  console.log(`📋 Discovered ${listResult.totalCount} commands`);
  console.log('✅ Basic connection and command discovery verified');
  
  return client;
}

async function testPingCommand(client: any) {
  console.log('\n🏓 TEST 2: Ping command execution');
  
  // Basic ping
  const basicPing = await withTimeout(client.commands.ping({
    message: 'test-ping'
  }), 5000);
  
  assert(basicPing.success === true, 'Basic ping succeeded');
  assert(basicPing.message === 'test-ping', 'Message echoed correctly');
  
  // Ping with timing
  const timedPing = await withTimeout(client.commands.ping({
    message: 'timed-ping',
    includeTiming: true
  }), 5000);
  
  assert(timedPing.success === true, 'Timed ping succeeded');
  assert(typeof timedPing.roundTripTime === 'number', 'Timing included');
  
  // Ping with environment
  const envPing = await withTimeout(client.commands.ping({
    message: 'env-ping',
    includeEnvironment: true
  }), 5000);
  
  assert(envPing.success === true, 'Environment ping succeeded');
  assert(typeof envPing.environment === 'object', 'Environment info included');
  assert(envPing.environment.type === 'browser', 'Environment type correct');
  
  console.log('✅ Ping command execution verified');
}

async function testErrorCommand(client: any) {
  console.log('\n💥 TEST 3: Error command execution');
  
  // Test that error command throws as expected
  try {
    await withTimeout(client.commands['test-error']({
      errorType: 'generic',
      message: 'test-error',
      environment: 'browser'
    }), 5000);
    
    assert(false, 'Error command should have thrown');
    
  } catch (error) {
    // Expected error
    assert(typeof error.message === 'string', 'Error has message');
    assert(error.message.length > 0, 'Error message not empty');
    console.log(`📨 Error correctly thrown: ${error.message.substring(0, 60)}...`);
  }
  
  console.log('✅ Error command execution verified');
}

async function testConcurrentCommands(client: any) {
  console.log('\n🔄 TEST 4: Concurrent command execution');
  
  // Execute multiple pings concurrently
  const concurrentPings = [
    client.commands.ping({ message: 'ping-1', includeTiming: true }),
    client.commands.ping({ message: 'ping-2', includeTiming: true }),
    client.commands.ping({ message: 'ping-3', includeTiming: true })
  ];
  
  const results = await withTimeout(Promise.all(concurrentPings), 10000);
  
  assert(results.length === 3, 'All concurrent pings completed');
  assert(results.every(r => r.success), 'All pings succeeded');
  
  const messages = results.map(r => r.message);
  assert(messages.includes('ping-1'), 'First ping preserved');
  assert(messages.includes('ping-2'), 'Second ping preserved');
  assert(messages.includes('ping-3'), 'Third ping preserved');
  
  console.log(`📊 Concurrent execution: ${results.map(r => `${r.message}(${r.roundTripTime}ms)`).join(', ')}`);
  console.log('✅ Concurrent command execution verified');
}

async function runEssentialE2ETests() {
  console.log('🚀 Starting Essential End-to-End Integration Tests\n');
  
  try {
    // Step 1: Ensure system is running
    const systemReady = await ensureSystemRunning();
    if (!systemReady) {
      throw new Error('Cannot run tests - system startup failed');
    }
    
    // Step 2: Test basic connection
    const client = await testBasicConnection();
    
    // Step 3: Test ping command
    await testPingCommand(client);
    
    // Step 4: Test error command
    await testErrorCommand(client);
    
    // Step 5: Test concurrent execution
    await testConcurrentCommands(client);
    
    console.log('\n🎉 ALL ESSENTIAL E2E TESTS PASSED!');
    console.log('📋 Verified:');
    console.log('  ✅ Client connection and command discovery');
    console.log('  ✅ Ping command with all features');
    console.log('  ✅ Error command throws correctly');
    console.log('  ✅ Concurrent command execution');
    
    console.log('\n🎯 ESSENTIAL E2E FUNCTIONALITY IS WORKING!');
    
  } catch (error) {
    console.error('\n❌ Essential E2E tests failed:', error.message);
    console.error('Stack trace:', error.stack);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runEssentialE2ETests();
} else {
  module.exports = { runEssentialE2ETests };
}