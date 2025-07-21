#!/usr/bin/env tsx
/**
 * Test Current Broken State
 * 
 * Documents exactly what's broken before fixing anything.
 * This test should PASS when the system is properly fixed.
 */

import * as fs from 'fs';
import * as path from 'path';

const testLogDir = path.resolve(process.cwd(), '../../../.continuum/jtag/logs');

console.log('🧪 TESTING CURRENT BROKEN STATE');
console.log('================================\n');

async function testBasicSystemLoad() {
  console.log('1. Testing basic system load...');
  
  try {
    // This should work without throwing
    const { jtag } = require('../dist/index.js');
    console.log('   ✅ System loads without errors');
    
    // Basic UUID generation should work
    const uuid = jtag.getUUID();
    console.log(`   ✅ UUID generation: ${uuid.uuid}`);
    
    return true;
  } catch (error: any) {
    console.log(`   ❌ System load failed: ${error.message}`);
    return false;
  }
}

async function testLoggingNaming() {
  console.log('\n2. Testing log file naming...');
  
  try {
    const { jtag } = require('../dist/index.js');
    
    // Clean slate - remove any existing undefined files
    const existingUndefinedFiles = fs.readdirSync(testLogDir)
      .filter(f => f.includes('undefined'));
    
    console.log(`   📋 Existing undefined files: ${existingUndefinedFiles.length}`);
    
    // Trigger logging that should create proper file names
    jtag.log('TEST_NAMING', 'Test proper log file naming');
    jtag.error('TEST_NAMING', 'Test error file naming');
    jtag.warn('TEST_NAMING', 'Test warn file naming');
    
    // Wait for async file operations
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Check results
    const logFiles = fs.readdirSync(testLogDir);
    const undefinedFiles = logFiles.filter(f => f.includes('undefined'));
    const properFiles = logFiles.filter(f => 
      f.match(/^(server|browser)\.(log|error|warn|info|critical)\.(json|txt)$/)
    );
    
    console.log(`   📁 Total log files: ${logFiles.length}`);
    console.log(`   ❌ Undefined files: ${undefinedFiles.length}`);
    console.log(`   ✅ Properly named files: ${properFiles.length}`);
    
    if (undefinedFiles.length > 0) {
      console.log('   🐛 Undefined files found:', undefinedFiles.slice(0, 3));
      return false;
    } else {
      console.log('   🎉 No undefined files created!');
      return true;
    }
    
  } catch (error: any) {
    console.log(`   ❌ Logging test failed: ${error.message}`);
    return false;
  }
}

async function testRouterFunctionality() {
  console.log('\n3. Testing router functionality...');
  
  try {
    const { jtagRouter } = require('../shared/JTAGRouter');
    
    // This should not throw "getActiveTransports is not a function"
    const transports = jtagRouter.getActiveTransports?.();
    
    if (transports) {
      console.log(`   ✅ Router has ${transports.length} active transports`);
      return true;
    } else {
      console.log('   ❌ getActiveTransports method not found');
      return false;
    }
    
  } catch (error: any) {
    console.log(`   ❌ Router test failed: ${error.message}`);
    return false;
  }
}

async function runBrokenStateTests() {
  console.log('Running broken state validation tests...\n');
  
  const results = {
    systemLoad: await testBasicSystemLoad(),
    logNaming: await testLoggingNaming(), 
    router: await testRouterFunctionality()
  };
  
  const totalTests = Object.keys(results).length;
  const passedTests = Object.values(results).filter(Boolean).length;
  
  console.log('\n📊 BROKEN STATE TEST RESULTS');
  console.log('============================');
  console.log(`✅ Passed: ${passedTests}/${totalTests}`);
  console.log(`❌ Failed: ${totalTests - passedTests}/${totalTests}`);
  
  if (passedTests === totalTests) {
    console.log('\n🎉 ALL TESTS PASSED - System is fixed!');
    process.exit(0);
  } else {
    console.log('\n🐛 System still broken - tests document current issues');
    console.log('Fix these issues one by one and re-run this test');
    process.exit(1);
  }
}

// Ensure log directory exists
if (!fs.existsSync(testLogDir)) {
  fs.mkdirSync(testLogDir, { recursive: true });
}

runBrokenStateTests().catch(console.error);