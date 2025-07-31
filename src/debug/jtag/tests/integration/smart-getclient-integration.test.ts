#!/usr/bin/env tsx
/**
 * PRIMARY INTEGRATION TEST: Smart getClient() → Cross-Context Screenshot
 * 
 * This is our main integration test that validates the complete real-world flow:
 * 1. Smart getClient() chooses best connection (local system vs remote client)
 * 2. Single dependency pattern discovers commands via list()
 * 3. Cross-context routing: server → browser execution for screenshot
 * 4. WebSocket transport, command pipeline, file system integration
 * 
 * This test exercises the entire architecture in the most realistic scenario.
 */

console.log('🧪 PRIMARY INTEGRATION TEST: Smart getClient() → Cross-Context Screenshot');

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

async function testSmartGetClientConnection() {
  console.log('\n📡 PHASE 1: Testing Smart getClient() Connection');
  
  try {
    // Import the main JTAG entry point
    const { jtag } = await import('../../index');
    
    console.log('🔌 Environment:', typeof window === 'undefined' ? 'Server (Node.js)' : 'Browser');
    
    // Test smart getClient()
    console.log('🔄 Calling jtag.getClient()...');
    const client = await jtag.getClient();
    
    // Verify we got a client
    assert(typeof client === 'object', 'getClient() returned client object');
    assert(typeof client.sessionId === 'string', 'Client has session ID');
    assert(typeof client.context === 'object', 'Client has context');
    assert(typeof client.commands === 'object', 'Client has commands interface');
    
    console.log(`✅ Smart client connected: ${client.constructor.name}`);
    console.log(`✅ Session ID: ${client.sessionId}`);
    console.log(`✅ Environment: ${client.context.environment}`);
    
    return client;
    
  } catch (error) {
    console.log(`❌ Phase 1 failed: ${error.message}`);
    throw error;
  }
}

async function testSingleDependencyPattern(client: any) {
  console.log('\n🔍 PHASE 2: Testing Single Dependency Pattern');
  
  try {
    // List command should always be available (our single dependency)
    assert(typeof client.commands.list === 'function', 'List command always available');
    
    console.log('🔄 Executing list command (single dependency)...');
    const listResult = await client.commands.list();
    
    // Verify list command results
    assert(listResult.success === true, 'List command succeeded');
    assert(Array.isArray(listResult.commands), 'Commands array returned');
    assert(listResult.commands.length > 0, 'Commands discovered');
    
    console.log(`✅ Commands discovered: ${listResult.totalCount}`);
    console.log(`✅ Screenshot available: ${listResult.commands.some(c => c.name === 'screenshot')}`);
    
    // Verify command discovery worked
    if (client.discoveredCommands) {
      assert(client.discoveredCommands.size > 0, 'Client has discovered commands after list');
    }
    
    return listResult;
    
  } catch (error) {
    console.log(`❌ Phase 2 failed: ${error.message}`);
    throw error;
  }
}

async function testCrossContextScreenshot(client: any) {
  console.log('\n📸 PHASE 3: Testing Cross-Context Screenshot Execution');
  
  try {
    const screenshotFilename = `primary-integration-test-${Date.now()}.png`;
    
    console.log('🔄 Executing screenshot command...');
    console.log('📋 Server → WebSocket → Browser → Capture → WebSocket → Server → File');
    
    // Execute screenshot command (this tests the entire cross-context flow)
    const screenshotResult = await client.commands.screenshot({
      filename: screenshotFilename,
      querySelector: 'body'
    });
    
    // Verify screenshot command results
    assert(screenshotResult.success === true, 'Screenshot command succeeded');
    assert(typeof screenshotResult.filename === 'string', 'Screenshot filename returned');
    assert(typeof screenshotResult.width === 'number', 'Screenshot width returned');
    assert(typeof screenshotResult.height === 'number', 'Screenshot height returned');
    
    console.log(`✅ Screenshot command completed`);
    console.log(`✅ Success: ${screenshotResult.success}`);
    console.log(`✅ Filename: ${screenshotResult.filename}`);
    console.log(`✅ Dimensions: ${screenshotResult.width}x${screenshotResult.height}`);
    
    return { screenshotResult, filename: screenshotFilename };
    
  } catch (error) {
    console.log(`❌ Phase 3 failed: ${error.message}`);
    throw error;
  }
}

async function testFileSystemIntegration(client: any, screenshotFilename: string) {
  console.log('\n💾 PHASE 4: Testing File System Integration');
  
  try {
    const { access, stat } = await import('fs/promises');
    const { join } = await import('path');
    
    // Find session directory
    const sessionDir = `.continuum/sessions/user/shared/${client.sessionId}`;
    const screenshotPath = join(sessionDir, 'screenshots', screenshotFilename);
    
    console.log(`🔍 Checking for screenshot file: ${screenshotPath}`);
    
    // Check if file exists
    await access(screenshotPath);
    console.log('✅ Screenshot file exists');
    
    // Check file size
    const stats = await stat(screenshotPath);
    const sizeKB = Math.round(stats.size / 1024);
    
    assert(stats.size > 1000, 'Screenshot file is valid size (>1KB)');
    console.log(`✅ File size: ${sizeKB}KB`);
    console.log(`✅ File is valid: ${stats.size > 1000 ? 'Yes' : 'No (too small)'}`);
    
    return { path: screenshotPath, size: stats.size };
    
  } catch (error) {
    console.log(`❌ Phase 4 failed: ${error.message}`);
    throw error;
  }
}

async function runPrimaryIntegrationTest() {
  console.log('🚀 Starting Primary Integration Test: Complete Architecture Validation\n');
  
  const startTime = Date.now();
  let client = null;
  let testResults = {};
  
  try {
    // Ensure system is running
    const systemReady = await ensureSystemRunning();
    if (!systemReady) {
      throw new Error('Cannot run test - system startup failed');
    }
    
    // Phase 1: Smart Connection
    client = await testSmartGetClientConnection();
    
    // Phase 2: Command Discovery
    const listResult = await testSingleDependencyPattern(client);
    
    // Phase 3: Cross-Context Screenshot
    const { screenshotResult, filename } = await testCrossContextScreenshot(client);
    
    // Phase 4: File System Verification
    const fileInfo = await testFileSystemIntegration(client, filename);
    
    // SUCCESS SUMMARY
    const duration = Date.now() - startTime;
    
    console.log('\n🎉 PRIMARY INTEGRATION TEST: ✅ SUCCESS');
    console.log('='.repeat(80));
    console.log(`✅ Smart Connection Strategy: Working (${client.constructor.name})`);
    console.log(`✅ Single Dependency Pattern: Working (${listResult.totalCount} commands)`);
    console.log(`✅ Cross-Context Routing: Working (Screenshot: ${screenshotResult.width}x${screenshotResult.height})`);
    console.log(`✅ WebSocket Transport: Working (Real browser capture)`);
    console.log(`✅ Command Pipeline: Working (End-to-end execution)`);
    console.log(`✅ File System Integration: Working (${Math.round(fileInfo.size / 1024)}KB saved)`);
    console.log(`✅ Total test time: ${duration}ms`);
    console.log('');
    console.log('🌟 ENTIRE ARCHITECTURE VALIDATED IN REAL-WORLD SCENARIO');
    console.log('🎯 Smart getClient() pattern proven with complete end-to-end flow');
    
    return {
      success: true,
      duration,
      client: client.constructor.name,
      commands: listResult.totalCount,
      screenshot: screenshotResult,
      filepath: fileInfo.path,
      fileSize: fileInfo.size
    };
    
  } catch (error) {
    const duration = Date.now() - startTime;
    console.error('\n❌ PRIMARY INTEGRATION TEST: FAILED');
    console.error('='.repeat(80));
    console.error(`❌ Error: ${error.message}`);
    console.error(`❌ Type: ${error.constructor.name}`);
    console.error(`❌ Duration: ${duration}ms`);
    
    if (error.stack) {
      console.error('\n📋 Stack Trace:');
      console.error(error.stack);
    }
    
    console.error('\n🔧 Debug Information:');
    console.error(`   Client: ${client?.constructor?.name || 'Not connected'}`);
    console.error(`   Session: ${client?.sessionId || 'Not available'}`);
    console.error(`   Environment: ${client?.context?.environment || 'Unknown'}`);
    
    throw error;
  }
}

// Run if called directly
if (require.main === module) {
  runPrimaryIntegrationTest()
    .then(() => {
      console.log('\n🎯 Primary integration test completed successfully');
      process.exit(0);
    })
    .catch((error) => {
      console.error('\n💥 Primary integration test failed');
      process.exit(1);
    });
} else {
  module.exports = { runPrimaryIntegrationTest };
}