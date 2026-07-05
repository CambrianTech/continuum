#!/usr/bin/env npx tsx

/**
 * Test Demo UI Screenshot Functionality
 * Tests both browser and server screenshot functionality that should be available in the demo UI
 */

console.log('🧪 Testing demo UI screenshot functionality...');

async function testDemoUIScreenshots() {
  try {
    console.log('📡 Connecting to JTAG via browser interface (same as demo UI)...');
    
    // Import the browser client (same as demo UI uses)  
    const { jtag } = await import('./browser-index.js');
    
    console.log('🔗 Creating JTAG connection (browser mode)...');
    const jtagSystem = await jtag.connect();
    
    console.log('📋 Available commands:', Object.keys(jtagSystem.commands || {}));
    
    // Test 1: Browser screenshot (should work)
    console.log('🌐 Testing browser screenshot (same as demo UI "Browser Screenshot" button)...');
    try {
      const browserResult = await jtagSystem.commands.screenshot({
        filename: 'demo-ui-browser-test.png',
        querySelector: 'body',
        context: 'browser',
        sessionId: 'demo-ui-test-' + Date.now()
      });
      
      console.log('✅ Browser screenshot result:', JSON.stringify(browserResult, null, 2));
    } catch (error) {
      console.error('❌ Browser screenshot failed:', error.message);
    }
    
    // Test 2: Server screenshot (the missing functionality)
    console.log('🖥️  Testing server screenshot (same as demo UI "Server Screenshot" button)...');
    try {
      const serverResult = await jtagSystem.commands.screenshot({
        filename: 'demo-ui-server-test.png',
        context: 'server', // This should route to server for processing
        sessionId: 'demo-ui-server-test-' + Date.now()
      });
      
      console.log('✅ Server screenshot result:', JSON.stringify(serverResult, null, 2));
    } catch (error) {
      console.error('❌ Server screenshot failed:', error.message);
      console.error('❌ Server screenshot stack:', error.stack);
    }
    
    // Check if screenshots were created
    console.log('📂 Checking screenshot outputs...');
    
  } catch (error) {
    console.error('❌ Demo UI screenshot test failed:', error);
    console.error('Stack:', error.stack);
  }
}

testDemoUIScreenshots().then(() => {
  console.log('🏁 Demo UI screenshot test completed');
}).catch(error => {
  console.error('💥 Test script failed:', error);
  process.exit(1);
});