#!/usr/bin/env tsx
/**
 * Screenshot Hang Debug Test
 * 
 * This test reveals exactly WHY screenshot tests hang by:
 * 1. Making all error states immediately visible
 * 2. Tracking each operation step-by-step
 * 3. Showing live diagnostics during execution
 * 4. Providing instant feedback on failures
 */

import { jtag } from '../server-index';
import { withHangDetection, HangingTestDetector } from '../utils/HangingTestDetector';

async function debugScreenshotHang() {
  const detector = new HangingTestDetector('Screenshot Debug');
  
  console.log('🔍 SCREENSHOT HANG DEBUG TEST');
  console.log('============================\n');
  
  try {
    // Step 1: Connect to JTAG system
    detector.trackOperation('jtag-connect', 'Connecting to JTAG system');
    console.log('📋 Step 1: Connecting to JTAG system...');
    
    const jtagClient = await jtag.connect({ targetEnvironment: 'server' });
    detector.completeOperation('jtag-connect');
    console.log('✅ JTAG client connected successfully');
    
    // Step 2: Test basic screenshot with detailed tracking
    detector.trackOperation('screenshot-basic', 'Basic screenshot command');
    console.log('\n📋 Step 2: Attempting basic screenshot...');
    console.log('🔍 Calling: jtagClient.commands.screenshot()');
    console.log('⏱️ Starting timer - watching for hang...');
    
    const screenshotPromise = jtagClient.commands.screenshot('hang-debug-test');
    
    // Race the screenshot against a timeout to catch hangs immediately  
    const timeoutPromise = new Promise((_, reject) => {
      setTimeout(() => {
        console.log('🚨 SCREENSHOT TIMEOUT: Command hung after 15 seconds!');
        console.log('📊 This reveals the exact hang point');
        console.log('💥 FORCING EXIT - no more silent hanging!');
        reject(new Error('Screenshot command timeout - this is where it hangs!'));
      }, 15000); // 15 second timeout for immediate feedback
    });
    
    const result = await Promise.race([screenshotPromise, timeoutPromise]);
    detector.completeOperation('screenshot-basic');
    
    console.log('📸 Screenshot result:', result);
    console.log('✅ Screenshot completed without hanging');
    
    detector.stop();
    return true;
    
  } catch (error) {
    console.log('\n💥 HANG DETECTED - ERROR ANALYSIS:');
    console.log('================================');
    console.log(`❌ Error: ${error.message}`);
    console.log(`📊 Error type: ${error.constructor.name}`);
    
    if (error.message.includes('timeout')) {
      console.log('\n🎯 HANG DIAGNOSIS:');
      console.log('• Screenshot command is hanging (not returning)');
      console.log('• Likely causes:');
      console.log('  - WebSocket connection broken');
      console.log('  - Browser not responding');
      console.log('  - Transport layer failure');
      console.log('  - Command routing failure');
      
      console.log('\n🔧 IMMEDIATE DEBUG STEPS:');
      console.log('1. Check system health: npm run signal:check');
      console.log('2. Check WebSocket logs: tail -f examples/test-bench/.continuum/jtag/currentUser/logs/server.log');
      console.log('3. Check browser logs: tail -f examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log');
      console.log('4. Check active ports: lsof -iTCP -sTCP:LISTEN | grep 900');
    }
    
    detector.stop();
    return false;
  }
}

// Run with hang detection
withHangDetection('Screenshot Hang Debug', debugScreenshotHang).then(success => {
  if (success) {
    console.log('\n🎉 Screenshot test completed successfully - no hang detected');
    process.exit(0);
  } else {
    console.log('\n❌ Screenshot test revealed hanging behavior');
    console.log('💡 Error has been made immediately apparent for debugging');
    process.exit(1);
  }
}).catch(error => {
  console.log('\n💥 Hang detection test crashed:', error.message);
  console.log('🔍 This itself reveals a systemic issue');
  process.exit(1);
});