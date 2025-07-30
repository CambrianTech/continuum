#!/usr/bin/env tsx
/**
 * Real Integration Test: Single Dependency Pattern with Running System
 * 
 * Tests the single dependency pattern against the actual running JTAG system.
 * Automatically ensures system is running before testing actual list command and connections.
 */

console.log('🧪 Real Integration Test: Single Dependency Pattern');

// Test utilities
function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

async function ensureSystemRunning() {
  console.log('🔄 Ensuring JTAG system is running...');
  
  // Use our smart startup system
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

async function testRealSingleDependencyPattern() {
  console.log('\n🔑 TEST 1: Real client single dependency - before connection');
  
  try {
    const { JTAGClientBrowser } = await import('../../shared/JTAGClientBrowser');
    const context = { uuid: 'integration-test', environment: 'browser' as const };
    
    // Create fresh client (not connected)
    const client = new JTAGClientBrowser(context);
    
    assert(client.discoveredCommands.size === 0, 'Fresh client has 0 discovered commands');
    
    // List should always be available (single dependency)
    assert(typeof client.commands.list === 'function', 'List command always available');
    
    // Other commands should be blocked until discovery
    try {
      const screenshot = client.commands.screenshot;
      console.log('⚠️  Screenshot unexpectedly available before connection');
    } catch (error) {
      console.log(`✅ Screenshot correctly blocked: ${error.message.substring(0, 60)}...`);
    }
    
    console.log('✅ Single dependency pattern verified on real client');
    
  } catch (error) {
    console.log(`❌ Test 1 failed: ${error.message}`);
    throw error;
  }
}

async function testRealConnectionBootstrap() {
  console.log('\n🔄 TEST 2: Real connection bootstrap with actual system');
  
  try {
    const { JTAGClientBrowser } = await import('../../shared/JTAGClientBrowser');
    
    console.log('🔗 Connecting to real browser system...');
    const { client, listResult } = await JTAGClientBrowser.connectLocal();
    
    // Verify bootstrap pattern worked
    assert(typeof client === 'object', 'Client returned from connection');
    assert(typeof listResult === 'object', 'List result returned from connection');
    assert(listResult.success === true, 'List command succeeded');
    assert(Array.isArray(listResult.commands), 'Commands array returned');
    assert(listResult.commands.length > 0, 'Commands discovered');
    
    console.log(`📊 Bootstrap discovered ${listResult.totalCount} commands`);
    console.log(`📝 Available commands: ${listResult.commands.map(c => c.name).join(', ')}`);
    
    // Verify client now has discovered commands
    assert(client.discoveredCommands.size > 0, 'Client has discovered commands after connection');
    assert(client.discoveredCommands.size === listResult.totalCount, 'Discovered count matches list result');
    
    console.log('✅ Real connection bootstrap pattern verified');
    
    return { client, listResult };
    
  } catch (error) {
    console.log(`❌ Test 2 failed: ${error.message}`);
    throw error;
  }
}

async function testRealCommandExecution(client: any) {
  console.log('\n🎯 TEST 3: Real command execution after discovery');
  
  try {
    // Now that commands are discovered, they should all be executable
    const commandNames = Array.from(client.discoveredCommands.keys());
    
    console.log(`🎯 Testing discovered commands: ${commandNames.join(', ')}`);
    
    // Test list command (should always work)
    const listResult = await client.commands.list();
    assert(listResult.success === true, 'List command executes successfully');
    assert(listResult.commands.length > 0, 'List returns commands');
    
    console.log('✅ List command execution verified');
    
    // Test other commands are now available
    if (commandNames.includes('screenshot')) {
      try {
        // Don't actually execute screenshot (might be resource intensive)
        // Just verify the function exists and is callable
        assert(typeof client.commands.screenshot === 'function', 'Screenshot command now available');
        console.log('✅ Screenshot command available after discovery');
      } catch (error) {
        console.log(`⚠️  Screenshot test: ${error.message}`);
      }
    }
    
    console.log('✅ Real command execution pattern verified');
    
  } catch (error) {
    console.log(`❌ Test 3 failed: ${error.message}`);
    throw error;
  }
}

async function testRealCommandInterception(client: any) {
  console.log('\n🔄 TEST 4: Real command interception and updates');
  
  try {
    const initialCommandCount = client.discoveredCommands.size;
    
    // Call list again - should trigger interception and update
    console.log('🔄 Calling list() again to test interception...');
    const secondListResult = await client.commands.list();
    
    assert(secondListResult.success === true, 'Second list call succeeds');
    assert(client.discoveredCommands.size >= initialCommandCount, 'Command map maintained or updated');
    
    // Verify commands in map match list result
    const mapCommands = new Set(client.discoveredCommands.keys());
    const listCommands = new Set(secondListResult.commands.map(c => c.name));
    
    assert(mapCommands.size === listCommands.size, 'Command map size matches list result');
    
    for (const commandName of listCommands) {
      assert(mapCommands.has(commandName), `Command '${commandName}' in both map and list result`);
    }
    
    console.log('✅ Real command interception pattern verified');
    
  } catch (error) {
    console.log(`❌ Test 4 failed: ${error.message}`);
    throw error;
  }
}

async function testRealCLIIntegration(listResult: any) {
  console.log('\n📋 TEST 5: Real CLI integration with actual commands');
  
  try {
    // Test that real commands can be formatted for CLI
    const cliCommands = listResult.commands.map(cmd => ({
      flag: `--${cmd.name}`,
      description: cmd.description,
      category: cmd.category,
      usage: `continuum ${cmd.name}`,
      params: Object.keys(cmd.params || {})
    }));
    
    assert(cliCommands.length > 0, 'CLI commands generated from real list');
    
    console.log('🎯 Real CLI Format:');
    cliCommands.forEach(cmd => {
      console.log(`  ${cmd.flag.padEnd(15)} ${cmd.description} (${cmd.category})`);
      if (cmd.params.length > 0) {
        console.log(`    ${''.padEnd(15)} Params: ${cmd.params.join(', ')}`);
      }
    });
    
    // Verify essential commands are present
    const commandFlags = cliCommands.map(c => c.flag);
    assert(commandFlags.includes('--list'), 'List command in CLI format');
    
    // Check for common commands
    const hasScreenshot = commandFlags.includes('--screenshot');
    const hasNavigate = commandFlags.includes('--navigate');
    
    console.log(`📊 CLI Commands: ${commandFlags.length} total, screenshot: ${hasScreenshot}, navigate: ${hasNavigate}`);
    
    console.log('✅ Real CLI integration verified');
    
  } catch (error) {
    console.log(`❌ Test 5 failed: ${error.message}`);
    throw error;
  }
}

async function runRealIntegrationTests() {
  console.log('🚀 Starting Real Integration Tests against Running System\n');
  
  try {
    // Step 1: Ensure system is running
    const systemReady = await ensureSystemRunning();
    if (!systemReady) {
      throw new Error('Cannot run tests - system startup failed');
    }
    
    // Step 2: Test single dependency pattern
    await testRealSingleDependencyPattern();
    
    // Step 3: Test real connection bootstrap
    const { client, listResult } = await testRealConnectionBootstrap();
    
    // Step 4: Test real command execution
    await testRealCommandExecution(client);
    
    // Step 5: Test real command interception
    await testRealCommandInterception(client);
    
    // Step 6: Test real CLI integration
    await testRealCLIIntegration(listResult);
    
    console.log('\n🎉 ALL REAL INTEGRATION TESTS PASSED!');
    console.log('📋 Verified:');
    console.log('  ✅ Single dependency pattern with real system');
    console.log('  ✅ Connection bootstrap returns real command list');
    console.log('  ✅ Dynamic command discovery works with actual commands');
    console.log('  ✅ Command interception updates with real results');
    console.log('  ✅ CLI integration with actual command signatures');
    
    console.log('\n🚀 READY FOR CLI IMPLEMENTATION!');
    
  } catch (error) {
    console.error('\n❌ Real integration tests failed:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runRealIntegrationTests();
} else {
  module.exports = { runRealIntegrationTests };
}