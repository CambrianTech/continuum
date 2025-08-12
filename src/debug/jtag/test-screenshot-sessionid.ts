/**
 * Test screenshot command with sessionId fix
 */

import { jtag } from './server-index';

async function testScreenshotSessionId(): Promise<void> {
  console.log('🧪 Testing screenshot command sessionId fix...');
  
  try {
    // Connect to server client
    console.log('🔌 Getting client...');
    const result = await jtag.connect();
    const client = result.client;
    console.log('✅ Client obtained');
    console.log('🔍 Client sessionId:', client.sessionId);
    
    // Take a screenshot via server client
    console.log('📸 Testing screenshot command...');
    const screenshotResult = await client.commands.screenshot({
      sessionId: client.sessionId,
      context: client.context,
      filename: 'sessionid-test.png'
    });
    console.log('✅ Screenshot command completed:', screenshotResult);
    
    // Check which session directory it saved to
    console.log('📂 Checking where screenshot was saved...');
    
  } catch (error) {
    console.error('❌ Screenshot sessionId test failed:', error);
    process.exit(1);
  }
}

testScreenshotSessionId();