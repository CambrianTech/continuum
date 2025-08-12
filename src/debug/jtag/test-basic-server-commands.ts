/**
 * Test basic server client commands - connect and list
 */

import { jtag } from './server-index';

async function testBasicServerCommands() {
  console.log('🧪 Testing basic server client commands...');
  
  try {
    // Test 1: Get client
    console.log('🔌 Getting client...');
    const client = await jtag.connect();
    console.log('✅ Client obtained');
    
    // Test 2: List commands
    console.log('📋 Testing list command...');
    const listResult = await client.commands.list({});
    console.log('✅ List result:', JSON.stringify(listResult, null, 2));
    
  } catch (error) {
    console.error('❌ Basic server commands test failed:', error);
    process.exit(1);
  }
}

testBasicServerCommands();