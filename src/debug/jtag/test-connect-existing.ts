#!/usr/bin/env tsx
/**
 * Test Connect to Existing System - Connect to user's running system
 * 
 * Since the user mentioned they have npm start running in another tab with browser connected,
 * let's try to connect to that existing system and test commands.
 */

import { JTAGClientServer } from './server/JTAGClientServer';

async function testExistingSystem() {
  try {
    console.log('🎯 Testing connection to existing JTAG system...');
    
    // Try connecting without starting anything new
    const { client: jtag, listResult } = await JTAGClientServer.connect({
      enableFallback: false // Force remote connection to existing system
    });
    
    console.log(`🆔 Connected! Session: ${jtag.sessionId}`);
    console.log(`📋 Available commands: ${listResult.totalCount}`);
    
    // List all available commands
    if (listResult.commands) {
      console.log('📝 Commands available:');
      listResult.commands.forEach((cmd, i) => {
        console.log(`  ${i + 1}. ${cmd}`);
      });
    }
    
    // Try a simple server-side command (list doesn't need browser)
    console.log('\n🧪 Testing LIST command (server-side)...');
    const listResult2 = await jtag.commands.list();
    console.log('✅ LIST command worked!', listResult2);
    
    // Try screenshot command (needs browser)
    console.log('\n📸 Testing SCREENSHOT command (browser required)...');
    try {
      const screenshotResult = await jtag.commands.screenshot({
        querySelector: 'body',
        filename: 'test-existing-system.png'
      });
      console.log('✅ SCREENSHOT command worked!', screenshotResult);
    } catch (error) {
      console.log('⚠️ Screenshot failed (expected if no browser):', error.message);
    }
    
    console.log('\n🎉 Connection test completed successfully!');
    
  } catch (error) {
    console.error('❌ Connection test failed:', error);
    console.log('\n💡 This might mean:');
    console.log('  1. No JTAG system is running on ports 9001/9002');
    console.log('  2. The RemoteConnection correlation system needs implementation');
    console.log('  3. The transport factory configuration needs adjustment');
  }
}

testExistingSystem();