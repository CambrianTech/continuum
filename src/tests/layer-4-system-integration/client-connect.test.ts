#!/usr/bin/env tsx
/**
 * Client Connect Integration Test
 * 
 * Tests that jtag.connect() works properly and returns a functional client.
 * This is the foundation test - everything else depends on connect() working.
 */

console.log('🧪 Client Connect Integration Test');

function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

async function testConnect() {
  console.log('\n🔌 Testing jtag.connect()');
  
  try {
    const { jtag } = await import('../../server-index');
    
    console.log('🔄 Calling jtag.connect()...');
    const connectionResult = await jtag.connect();
    
    // Validate connection result structure
    assert(connectionResult !== null && connectionResult !== undefined, 'Connection result returned');
    assert(typeof connectionResult === 'object', 'Connection result is an object');
    assert(connectionResult.client !== null, 'Client returned from connect()');
    assert(typeof connectionResult.client.commands === 'object', 'Client has commands interface');
    
    // Validate connection metadata
    assert(typeof connectionResult.connectionType === 'string', 'Connection type provided');
    assert(typeof connectionResult.reason === 'string', 'Connection reason provided');
    assert(typeof connectionResult.localSystemAvailable === 'boolean', 'Local system status provided');
    assert(typeof connectionResult.sessionId === 'string', 'Session ID provided');
    
    console.log('📋 Connection Details:');
    console.log(`   Connection Type: ${connectionResult.connectionType}`);
    console.log(`   Reason: ${connectionResult.reason}`);
    console.log(`   Local System Available: ${connectionResult.localSystemAvailable}`);
    console.log(`   Session ID: ${connectionResult.sessionId}`);
    console.log(`   Commands Discovered: ${connectionResult.listResult.totalCount}`);
    
    return connectionResult;
    
  } catch (error) {
    console.error('❌ Connect test failed:', error.message);
    throw error;
  }
}

async function testClientProperties(connectionResult: any) {
  console.log('\n🔍 Testing client properties and connection info');
  
  try {
    const client = connectionResult.client;
    const connectionInfo = client.getConnectionInfo();
    
    // Test basic client properties
    assert(typeof client.sessionId === 'string', 'Client has sessionId');
    assert(typeof client.context === 'object', 'Client has context');
    assert(client.context.environment === 'server', 'Client has server environment');
    
    // Test connection info method
    assert(typeof connectionInfo === 'object', 'getConnectionInfo() returns object');
    assert(['local', 'remote'].includes(connectionInfo.connectionType), 'Valid connection type');
    assert(typeof connectionInfo.reason === 'string', 'Connection reason provided');
    assert(typeof connectionInfo.localSystemAvailable === 'boolean', 'Local system status provided');
    assert(connectionInfo.environment === 'server', 'Environment matches');
    
    // CRITICAL: Check for deadbeef session - indicates session system failure
    if (connectionInfo.isBootstrapSession) {
      console.error('🚨 CRITICAL: Client stuck with UNKNOWN_SESSION (deadbeef)');
      console.error('🚨 This means SessionDaemon failed to assign a real session');
      console.error('🚨 Connection info shows the problem:');
      console.error(`🚨   Connection Type: ${connectionInfo.connectionType}`);
      console.error(`🚨   Reason: ${connectionInfo.reason}`);
      console.error(`🚨   Local System Available: ${connectionInfo.localSystemAvailable}`);
      throw new Error('Session system failure: Client stuck with bootstrap session');
    }
    
    console.log('📊 Client Connection Info:');
    console.log(`   Connection Type: ${connectionInfo.connectionType}`);
    console.log(`   Reason: ${connectionInfo.reason}`);
    console.log(`   Local System Available: ${connectionInfo.localSystemAvailable}`);
    console.log(`   Session ID: ${connectionInfo.sessionId}`);
    console.log(`   Environment: ${connectionInfo.environment}`);
    console.log(`   Is Bootstrap Session: ${connectionInfo.isBootstrapSession}`);
    console.log(`   ✅ Session properly assigned (not deadbeef)`);
    
  } catch (error) {
    console.error('❌ Client properties test failed:', error.message);
    throw error;
  }
}

async function testCommandsInterface(client: any) {
  console.log('\n🎯 Testing commands interface');
  
  try {
    // Test that commands interface exists and is functional
    assert(typeof client.commands === 'object', 'Commands interface exists');
    
    // Test that we can access command methods (even if they fail)
    const hasListCommand = typeof client.commands.list === 'function';
    const hasPingCommand = typeof client.commands.ping === 'function';
    
    console.log('📋 Available commands interface:');
    console.log(`   list command: ${hasListCommand ? 'available' : 'missing'}`);
    console.log(`   ping command: ${hasPingCommand ? 'available' : 'missing'}`);
    
    // Don't assert these since commands might not be discovered yet
    // The important thing is that the commands interface exists
    
  } catch (error) {
    console.error('❌ Commands interface test failed:', error.message);
    throw error;
  }
}

async function runClientConnectTest() {
  console.log('🚀 Starting Client Connect Integration Test\n');
  
  try {
    // Test 1: Basic connection
    const connectionResult = await testConnect();
    
    // Test 2: Client properties and connection info
    await testClientProperties(connectionResult);
    
    // Test 3: Commands interface
    await testCommandsInterface(connectionResult.client);
    
    console.log('\n🎉 CLIENT CONNECT TEST COMPLETED!');
    console.log('✅ jtag.connect() works correctly');
    console.log('✅ Connection metadata is accessible');
    console.log('✅ Client object is properly formed');
    console.log('✅ Commands interface is available');
    
  } catch (error) {
    console.error('\n❌ Client connect test failed:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runClientConnectTest();
} else {
  module.exports = { runClientConnectTest };
}