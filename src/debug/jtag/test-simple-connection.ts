#!/usr/bin/env tsx

/**
 * Simple connection test - just try to connect and ping
 */

import { jtag } from './server-index';

async function testSimpleConnection() {
  console.log('🧪 Testing simple connection to JTAG system...');
  
  try {
    console.log('🔌 Connecting to JTAG system...');
    
    // Try to connect to the server system directly
    const connectionResult = await jtag.connect({ 
      targetEnvironment: 'server'
    });
    
    console.log('✅ Connected successfully!');
    console.log('📋 Available commands:', connectionResult.listResult.totalCount);
    console.log('🎯 Commands:', Array.from(connectionResult.client.discoveredCommands.keys()).join(', '));
    
    // Now try a simple command
    if (connectionResult.client.discoveredCommands.has('ping')) {
      console.log('📡 Testing ping command...');
      const pingResult = await connectionResult.client.commands.ping();
      console.log('✅ Ping successful:', pingResult);
    }
    
    console.log('🎉 Simple connection test completed successfully!');
    
  } catch (error) {
    console.error('❌ Simple connection test failed:', error);
    process.exit(1);
  }
}

testSimpleConnection().then(() => {
  console.log('✅ All tests passed!');
  process.exit(0);
}).catch((error) => {
  console.error('❌ Test failed:', error);
  process.exit(1);
});