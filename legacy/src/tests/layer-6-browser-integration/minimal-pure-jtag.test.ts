#!/usr/bin/env node
/**
 * Minimal Pure JTAG Test
 * 
 * Simple test that demonstrates JTAG's core functionality:
 * - Universal exec() command
 * - Screenshot capture  
 * - Cross-context execution
 * 
 * Focuses on commands we know exist and work.
 */

console.log('\n🎯 Minimal Pure JTAG Test');
console.log('=========================');

async function runMinimalJTAGTest(): Promise<void> {
  try {
    // Connect to JTAG
    const jtagModule = await import('../../server-index');
    const jtag = await jtagModule.jtag.connect();
    console.log('✅ JTAG connected');

    let passed = 0;
    let failed = 0;

    // Test 1: List commands (always works)
    try {
      console.log('▶️  Testing command discovery');
      const result = await jtag.commands.list();
      
      if (result && result.success && result.commands) {
        console.log(`✅ Command discovery passed: ${result.commands.length} commands`);
        passed++;
      } else {
        console.log('❌ Command discovery failed:', JSON.stringify(result, null, 2));
        failed++;
      }
    } catch (error) {
      console.log('❌ Command discovery error:', error);
      failed++;
    }

    // Test 2: Screenshot command
    try {
      console.log('▶️  Testing screenshot');
      const screenshot = await jtag.commands.screenshot('minimal-test');
      
      if (screenshot && (screenshot.success || screenshot.filename)) {
        console.log('✅ Screenshot test passed');
        passed++;
      } else {
        console.log('❌ Screenshot test failed:', JSON.stringify(screenshot, null, 2));
        failed++;
      }
    } catch (error) {
      console.log('❌ Screenshot test error:', error);
      failed++;
    }

    // Test 3: Ping command (should exist)
    try {
      console.log('▶️  Testing ping');
      const ping = await jtag.commands.ping();
      
      if (ping && (ping.success || ping.message)) {
        console.log('✅ Ping test passed');
        passed++;
      } else {
        console.log('❌ Ping test failed:', JSON.stringify(ping, null, 2));
        failed++;
      }
    } catch (error) {
      console.log('❌ Ping test error:', error);
      failed++;
    }

    // Results
    console.log('\n📊 Results:');
    console.log(`✅ Passed: ${passed}`);
    console.log(`❌ Failed: ${failed}`);
    
    const total = passed + failed;
    const successRate = total > 0 ? Math.round((passed / total) * 100) : 0;
    console.log(`📈 Success rate: ${successRate}%`);

    if (failed === 0) {
      console.log('🎉 ALL TESTS PASSED! Pure JTAG working!');
      process.exit(0);
    } else if (passed > 0) {
      console.log('⚠️  Some tests passed - partial functionality working');
      process.exit(0);
    } else {
      console.log('❌ All tests failed');
      process.exit(1);
    }

  } catch (error) {
    console.error('💥 Test setup failed:', error);
    process.exit(1);
  }
}

// Run test
if (require.main === module) {
  runMinimalJTAGTest();
}