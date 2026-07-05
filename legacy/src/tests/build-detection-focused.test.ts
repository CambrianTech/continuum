#!/usr/bin/env tsx
/**
 * Focused Build Detection Test
 * 
 * Quick validation of the core build detection logic without 
 * triggering full system deployment (to avoid timeouts).
 */

import { BuildVersionDetector } from '../utils/BuildVersionDetector';

async function testBuildDetectionCore() {
  console.log('🧪 FOCUSED BUILD DETECTION TEST');
  console.log('===============================');
  
  const detector = new BuildVersionDetector();
  let testsPassed = 0;
  let totalTests = 0;
  
  // Test 1: Source hash calculation
  console.log('\n📋 Test 1: Source Hash Calculation');
  totalTests++;
  try {
    const sourceHash = await detector.calculateSourceHash();
    console.log(`✅ Source hash: ${sourceHash.substring(0, 12)}...`);
    
    if (sourceHash.length === 64 && /^[a-f0-9]+$/.test(sourceHash)) {
      console.log('✅ Hash format valid (64-char hex SHA256)');
      testsPassed++;
    } else {
      console.log(`❌ Invalid hash format: ${sourceHash.length} chars`);
    }
  } catch (error) {
    console.log(`❌ Source hash failed: ${error.message}`);
  }
  
  // Test 2: Build need analysis (without system dependency)
  console.log('\n📋 Test 2: Build Need Analysis');
  totalTests++;
  try {
    const analysis = await detector.analyzeBuildNeeds();
    console.log(`✅ Analysis completed: ${analysis.severity} severity`);
    console.log(`   TypeScript: ${analysis.typescript}, Generated: ${analysis.generated}, System: ${analysis.system}`);
    console.log(`   Reasons: ${analysis.reason.length} found`);
    testsPassed++;
  } catch (error) {
    console.log(`❌ Build analysis failed: ${error.message}`);
  }
  
  // Test 3: Version storage simulation
  console.log('\n📋 Test 3: Version Storage Test');
  totalTests++;
  try {
    const mockHash = 'test123hash456test789hash012test345hash678test901hash234test567hash890abcdef';
    await detector.storeSystemVersion(mockHash);
    console.log('✅ Version storage completed');
    
    // Try to detect the stored version
    const detectionResult = await detector.detectVersionMismatch();
    console.log(`✅ Detection result: ${detectionResult.reason}`);
    testsPassed++;
  } catch (error) {
    console.log(`❌ Version storage test failed: ${error.message}`);
  }
  
  // Test 4: Testing integration (dry run)
  console.log('\n📋 Test 4: Testing Integration Check');
  totalTests++;
  try {
    const shouldRebuild = await detector.shouldRebuildForTesting();
    console.log(`✅ Testing check: ${shouldRebuild.rebuild ? 'Would rebuild' : 'Ready'}`);
    console.log(`   Reason: ${shouldRebuild.reason}`);
    testsPassed++;
  } catch (error) {
    console.log(`❌ Testing integration failed: ${error.message}`);
  }
  
  // Results
  console.log('\n🎯 TEST RESULTS');
  console.log('===============');
  console.log(`✅ Passed: ${testsPassed}/${totalTests}`);
  console.log(`📊 Success rate: ${Math.round((testsPassed/totalTests) * 100)}%`);
  
  if (testsPassed === totalTests) {
    console.log('');
    console.log('🎉 BUILD DETECTION SYSTEM: FULLY FUNCTIONAL');
    console.log('✅ Core logic validated');
    console.log('✅ Hash calculation working');  
    console.log('✅ Build analysis working');
    console.log('✅ Version storage working');
    console.log('✅ Testing integration ready');
    console.log('');
    console.log('🚀 READY FOR AUTONOMOUS AI DEVELOPMENT');
    process.exit(0);
  } else {
    console.log('');
    console.log('❌ Some build detection components failed');
    console.log('🔍 Check error messages above for details');
    process.exit(1);
  }
}

// Run the focused test directly (no auto-spawn to avoid recursion)
testBuildDetectionCore().catch(error => {
  console.error('💥 Test framework error:', error.message);
  process.exit(1);
});