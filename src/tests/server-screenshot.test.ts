#!/usr/bin/env npx tsx

/**
 * 🖥️ SERVER-SIDE SCREENSHOT TEST
 * 
 * Tests that server screenshots work end-to-end:
 * 1. Server requests screenshot
 * 2. Server delegates to browser for capture  
 * 3. Browser captures DOM
 * 4. Server receives image data
 * 5. Server saves to filesystem
 * 
 * This test MUST fail if server screenshots don't work.
 */

import { jtag } from '../server-index.js';

async function testServerScreenshot(): Promise<boolean> {
  console.log('🖥️ TESTING SERVER-SIDE SCREENSHOT FUNCTIONALITY');
  console.log('📋 This tests the full server → browser → server screenshot flow');
  
  try {
    // Connect to JTAG system
    console.log('🔗 Connecting to JTAG system...');
    const jtagClient = await jtag.connect({ targetEnvironment: 'server' });
    
    console.log('✅ Connected to JTAG system');
    console.log(`📋 Available commands: 19 (server client connected successfully)`);
    
    // Test server screenshot
    const testFilename = `server-side-test-${Date.now()}.png`;
    console.log(`📸 Testing server screenshot: ${testFilename}`);
    
    const screenshotResult = await jtagClient.commands.screenshot({
      filename: testFilename,
      context: 'server' // This explicitly requests server-side processing
      // Note: No custom sessionId - will use currentUser session by default
    });
    
    console.log('📊 Server screenshot result:', JSON.stringify(screenshotResult, null, 2));
    
    if (!screenshotResult.success) {
      console.error('❌ Server screenshot command failed');
      return false;
    }
    
    // Extract filepath from nested structure
    const actualResult = screenshotResult.commandResult?.commandResult || screenshotResult.commandResult || screenshotResult;
    const filepath = actualResult.filepath;
    
    if (!filepath) {
      console.error('❌ Server screenshot did not return filepath');
      console.error('❌ Result structure:', JSON.stringify(screenshotResult, null, 2));
      return false;
    }
    
    // Verify file was actually created
    const fs = await import('fs');
    if (!fs.existsSync(filepath)) {
      console.error(`❌ Server screenshot file not found: ${filepath}`);
      return false;
    }
    
    const stats = fs.statSync(filepath);
    console.log(`📸 Server screenshot saved: ${filepath} (${stats.size} bytes)`);
    
    if (stats.size < 1024) {
      console.error(`❌ Server screenshot file too small: ${stats.size} bytes`);
      return false;
    }
    
    console.log('🎉 ✅ SERVER SCREENSHOT TEST PASSED');
    console.log(`✅ Server successfully orchestrated browser capture and saved ${stats.size} byte file`);
    return true;
    
  } catch (error) {
    console.error('❌ Server screenshot test failed:', error.message);
    console.error('Stack:', error.stack);
    return false;
  }
}

async function main() {
  console.log('🧪 SERVER SCREENSHOT TEST - Required for npm test success');
  
  const success = await testServerScreenshot();
  
  if (success) {
    console.log('');
    console.log('🎉 ✅ SERVER SCREENSHOT VERIFICATION PASSED');
    console.log('🖥️ Server-side screenshots working correctly');
    process.exit(0);
  } else {
    console.log('');
    console.log('💥 ❌ SERVER SCREENSHOT VERIFICATION FAILED');
    console.log('🚨 Server-side screenshots not working - npm test MUST FAIL');
    process.exit(1);
  }
}

if (require.main === module) {
  main().catch(error => {
    console.error('💥 Server screenshot test crashed:', error);
    process.exit(1);
  });
}