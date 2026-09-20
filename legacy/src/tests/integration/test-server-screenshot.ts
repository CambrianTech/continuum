#!/usr/bin/env npx tsx

/**
 * Test Server Screenshot Functionality
 * Specifically tests the server-side screenshot command that user noticed is missing
 */

console.log('🧪 Testing server-side screenshot functionality...');

async function testServerScreenshot() {
  try {
    console.log('📡 Connecting to JTAG server...');
    
    // Import the server client
    const { jtag } = await import('./server-index.js');
    
    console.log('🔗 Creating JTAG connection...');
    const jtagSystem = await jtag.connect();
    
    console.log('📸 Testing server screenshot command...');
    const result = await jtagSystem.commands.screenshot({
      filename: 'server-test-screenshot.png',
      context: 'server',
      sessionId: 'test-session-' + Date.now()
    });
    
    console.log('✅ Server screenshot result:', JSON.stringify(result, null, 2));
    
    // Also test with querySelector from server (should delegate to browser)
    console.log('🌐 Testing server screenshot with browser delegation...');
    const browserDelegationResult = await jtagSystem.commands.screenshot({
      filename: 'server-browser-delegation.png',
      querySelector: 'body',
      context: 'server',
      sessionId: 'test-delegation-' + Date.now()
    });
    
    console.log('✅ Server browser delegation result:', JSON.stringify(browserDelegationResult, null, 2));
    
  } catch (error) {
    console.error('❌ Server screenshot test failed:', error);
    console.error('Stack:', error.stack);
  }
}

testServerScreenshot().then(() => {
  console.log('🏁 Server screenshot test completed');
}).catch(error => {
  console.error('💥 Test script failed:', error);
  process.exit(1);
});