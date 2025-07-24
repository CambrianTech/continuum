#!/usr/bin/env tsx
/**
 * Test screenshot functionality with existing running system
 * 
 * This test connects to the already-running JTAG system instead of creating a new one,
 * avoiding port conflicts and testing against the real browser connection.
 */

async function testScreenshotWithExistingSystem() {
  console.log('🧪 Testing screenshot with existing JTAG system...');
  
  try {
    // Import and connect to existing system
    const { jtag } = await import('./index');
    
    // Connect to the already-running system 
    console.log('🔌 Connecting to existing JTAG system...');
    const system = await jtag.connect();
    
    console.log('✅ Connected successfully!');
    console.log('🔍 System info:', system.getSystemInfo());
    
    // Test screenshot command
    console.log('📸 Testing screenshot command...');
    const { ScreenshotParams } = await import('./daemons/command-daemon/commands/screenshot/shared/ScreenshotTypes');
    const params = new ScreenshotParams(); // Will auto-generate timestamped filename
    const result = await system.commands.screenshot(params);
    
    console.log('📸 Screenshot result:', result);
    
    if (result.success) {
      console.log('✅ Screenshot test PASSED - system is fully operational');
      console.log(`   File saved: ${result.filepath}`);
    } else {
      console.log('❌ Screenshot test FAILED:', result.error);
      process.exit(1);
    }
    
    // Cleanup
    await system.shutdown();
    
  } catch (error: unknown) {
    console.error('❌ Test failed:', error instanceof Error ? error.message : String(error));
    console.error('   Make sure system is running with: npm start');
    process.exit(1);
  }
}

// Run the test
testScreenshotWithExistingSystem();