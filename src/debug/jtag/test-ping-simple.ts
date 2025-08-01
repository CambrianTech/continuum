#!/usr/bin/env tsx
/**
 * Simple Ping Command Test
 * 
 * Tests ping command with direct client connection to running server
 */

import { JTAGClientServer } from './server/JTAGClientServer';

async function testPingCommand() {
  console.log('🧪 Simple Ping Command Test');
  console.log('🔗 Connecting to running JTAG server...');
  
  try {
    // Connect to the existing server via WebSocket
    const { client } = await JTAGClientServer.connectRemote();
    console.log('✅ Connected to server');
    
    // Execute ping command
    console.log('🏓 Executing ping command...');
    const pingResult = await client.commands.ping({
      message: 'test-ping',
      includeEnvironment: true,
      includeTiming: true
    });
    
    console.log('📊 Ping Result:');
    console.log('   Success:', pingResult.success);
    console.log('   Message:', pingResult.message);
    console.log('   Environment:', pingResult.environment);
    console.log('   Round-trip time:', pingResult.roundTripTime + 'ms');
    console.log('   Timestamp:', pingResult.timestamp);
    
    // Validate result
    if (pingResult.success && pingResult.message === 'pong') {
      console.log('✅ Ping command working end-to-end!');
      return true;
    } else {
      console.log('❌ Ping command validation failed');
      return false;
    }
    
  } catch (error) {
    console.error('❌ Ping test failed:', error instanceof Error ? error.message : String(error));
    return false;
  }
}

// Run the test
testPingCommand().then(success => {
  process.exit(success ? 0 : 1);
});