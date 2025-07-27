#!/usr/bin/env node

/**
 * Test Script: Verify Screenshot Command Proxy Fix
 * 
 * This script tests that jtagSystem.commands.screenshot() now routes through
 * the message system instead of throwing "Direct command invocation requires session context"
 */

const { JTAGSystem } = require('./dist/server/JTAGSystemServer.js');

async function testScreenshotProxy() {
  console.log('🚀 Testing Screenshot Command Proxy Fix');
  console.log('=====================================');
  
  try {
    // Initialize JTAG system
    console.log('1️⃣ Initializing JTAG system...');
    const jtag = await JTAGSystem.connect();
    console.log('✅ JTAG system connected');
    
    // Test that commands interface exists
    console.log('\n2️⃣ Checking commands interface...');
    console.log('Available commands:', Object.keys(jtag.commands));
    console.log('Screenshot command type:', typeof jtag.commands.screenshot);
    
    // Test the actual screenshot command
    console.log('\n3️⃣ Testing screenshot command proxy...');
    const result = await jtag.commands.screenshot({ 
      filename: 'proxy-test.png',
      selector: 'body' 
    });
    
    console.log('✅ Screenshot command executed successfully!');
    console.log('Result:', JSON.stringify(result, null, 2));
    
    // Cleanup
    await jtag.shutdown();
    console.log('\n🎉 Test completed successfully - Proxy fix works!');
    
  } catch (error) {
    console.error('❌ Test failed:', error.message);
    
    if (error.message.includes('Direct command invocation requires session context')) {
      console.error('💥 The proxy fix did not work - still getting the old error');
    } else {
      console.error('🔍 Different error occurred - investigate further');
    }
    
    process.exit(1);
  }
}

testScreenshotProxy().catch(console.error);