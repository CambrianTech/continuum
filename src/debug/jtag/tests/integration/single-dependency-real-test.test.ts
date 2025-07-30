/**
 * Real Single Dependency Pattern Test
 * Tests the actual single dependency pattern with real system
 */

import { jtag } from '../../index';
import { SmartSystemStartup } from '../../scripts/smart-system-startup';

async function runTests() {
  console.log('🧪 Single Dependency Pattern - Real System Integration');
  
  try {
    // Setup
    console.log('🔄 Setting up system...');
    const startup = new SmartSystemStartup();
    await startup.ensureSystemRunning();
    
    // Test 1: Connect should return JTAG system with commands
    console.log('\n📋 TEST 1: Connect and discover commands');
    const connectPromise = jtag.connect();
    const timeoutPromise = new Promise((_, reject) => 
      setTimeout(() => reject(new Error('Connection timeout after 15s')), 15000)
    );
    
    const jtagSystem = await Promise.race([connectPromise, timeoutPromise]) as any;
    
    if (!jtagSystem || !jtagSystem.commands) {
      throw new Error('Connection failed - missing JTAG system or commands');
    }
    
    // Test list command (single dependency)
    console.log('✅ Connected! Testing list command...');
    const listResult = await jtagSystem.commands.list();
    
    if (!Array.isArray(listResult.commands) || listResult.commands.length === 0) {
      throw new Error('No commands discovered via list');
    }
    
    console.log('✅ Commands found:', listResult.commands.length);
    console.log('📋 Available commands:', listResult.commands.map((c: any) => c.name));
    
    // Test 2: List command should be available (single dependency)
    console.log('\n🔑 TEST 2: List command availability');
    
    if (typeof jtagSystem.commands.list !== 'function') {
      throw new Error('List command not available');
    }
    
    // Test calling list again
    const listResult2 = await jtagSystem.commands.list();
    if (!Array.isArray(listResult2.commands) || listResult2.commands.length === 0) {
      throw new Error('List command failed on second call');
    }
    
    console.log('✅ List command works, found', listResult2.commands.length, 'commands');
    
    // Test 3: Screenshot command should work with proper sessionId
    console.log('\n📸 TEST 3: Screenshot command execution');
    const screenshotResult = await jtagSystem.commands.screenshot({
      querySelector: 'body',
      filename: 'test-single-dependency.png'
    });
    
    if (!screenshotResult || !screenshotResult.success) {
      throw new Error('Screenshot command failed');
    }
    
    console.log('✅ Screenshot result:', screenshotResult);
    
    console.log('\n🎉 ALL TESTS PASSED! Single dependency pattern working correctly.');
    
  } catch (error) {
    console.error('\n❌ TEST FAILED:', error.message);
    if (error.stack) {
      console.error('Stack:', error.stack);
    }
    process.exit(1);
  }
}

runTests();