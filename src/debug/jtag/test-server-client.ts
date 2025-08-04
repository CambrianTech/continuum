#!/usr/bin/env tsx

/**
 * Test server-side JTAGClient to verify it can route commands to browser
 */

import { jtag } from './server-index';

async function testServerClient() {
  console.log('🧪 Testing server-side JTAGClient...');
  
  try {
    // Connect to the running JTAG server system
    console.log('🔌 Connecting to running JTAG server system...');
    const connectionResult = await jtag.connect({ 
      targetEnvironment: 'server'  // Connect to the server system
    });
    const { client } = connectionResult;
    
    console.log('✅ Connected! Available commands:', connectionResult.listResult.totalCount);
    
    // Test screenshot command routing from server -> browser
    console.log('📸 Testing screenshot command from server -> browser...');
    const screenshotResult = await client.commands.screenshot({ 
      filename: 'server-to-browser-test.png' 
    });
    
    console.log('✅ Screenshot command successful:', screenshotResult);
    
    // Test another command to verify routing
    console.log('📝 Testing log command from server -> browser...');
    const logResult = await client.commands.log({
      category: 'SERVER_TO_BROWSER_TEST',
      message: 'This message was sent from server-side JTAGClient to browser'
    });
    
    console.log('✅ Log command successful:', logResult);
    
    console.log('🎉 Server-side JTAGClient test completed successfully!');
    
  } catch (error) {
    console.error('❌ Server-side JTAGClient test failed:', error);
    
    if (error instanceof Error && error.message.includes('timeout')) {
      console.log('\n💡 This likely means the JTAG system is not running or not ready.');
      console.log('🚀 To start the system:');
      console.log('   cd src/debug/jtag');
      console.log('   npm run system:start');
      console.log('   sleep 45  # Wait for full build');
      console.log('   npx tsx test-server-client.ts');
    }
    
    process.exit(1);
  }
}

// Run the test
testServerClient().then(() => {
  console.log('✅ All server client tests passed!');
  process.exit(0);
}).catch((error) => {
  console.error('❌ Server client test failed:', error);
  process.exit(1);
});