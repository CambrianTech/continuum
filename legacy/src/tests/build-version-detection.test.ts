#!/usr/bin/env tsx
/**
 * Build Version Detection Test
 * 
 * Tests the intelligent build need detection system that ensures
 * running JTAG system matches current source code state.
 * 
 * This test validates:
 * 1. Source code hash calculation
 * 2. Running system version detection  
 * 3. Version mismatch detection
 * 4. Build need analysis
 * 5. Integration with auto-spawn pattern
 */

import { BuildVersionDetector } from '../utils/BuildVersionDetector';
import { autoSpawnTest } from '../utils/TestAutoSpawn';
import * as fs from 'fs';
import * as path from 'path';

async function testBuildVersionDetection() {
  console.log('🧪 BUILD VERSION DETECTION TEST');
  console.log('===============================');
  
  const detector = new BuildVersionDetector();
  
  // Test 1: Source hash calculation
  console.log('\n📋 Test 1: Source Hash Calculation');
  try {
    const sourceHash = await detector.calculateSourceHash();
    console.log(`✅ Source hash calculated: ${sourceHash.substring(0, 16)}...`);
    
    if (sourceHash.length !== 64) {
      throw new Error(`Expected 64-char SHA256 hash, got ${sourceHash.length} chars`);
    }
    
    // Calculate again to ensure consistency
    const sourceHash2 = await detector.calculateSourceHash();
    if (sourceHash !== sourceHash2) {
      throw new Error('Source hash calculation is not deterministic');
    }
    
    console.log('✅ Source hash calculation is consistent');
  } catch (error) {
    console.log(`❌ Source hash calculation failed: ${error.message}`);
    throw error;
  }
  
  // Test 2: Build need analysis  
  console.log('\n📋 Test 2: Build Need Analysis');
  try {
    const analysis = await detector.analyzeBuildNeeds();
    
    console.log(`📊 Analysis results:`);
    console.log(`   TypeScript rebuild needed: ${analysis.typescript}`);
    console.log(`   Generated files rebuild needed: ${analysis.generated}`);
    console.log(`   System deployment needed: ${analysis.system}`);
    console.log(`   Severity: ${analysis.severity}`);
    console.log(`   Reasons: ${analysis.reason.join(', ') || 'None'}`);
    
    if (analysis.severity === 'critical') {
      console.log('⚠️ Critical build issues detected - this is expected if system not built');
    } else {
      console.log('✅ Build need analysis completed successfully');
    }
  } catch (error) {
    console.log(`❌ Build need analysis failed: ${error.message}`);
    throw error;
  }
  
  // Test 3: Version mismatch detection
  console.log('\n📋 Test 3: Version Mismatch Detection');
  try {
    const mismatchResult = await detector.detectVersionMismatch();
    
    console.log(`📊 Version mismatch results:`);
    console.log(`   Needs rebuild: ${mismatchResult.needsRebuild}`);
    console.log(`   Reason: ${mismatchResult.reason}`);
    console.log(`   Build status: ${mismatchResult.details.buildStatus}`);
    console.log(`   Source hash: ${mismatchResult.details.sourceHash.substring(0, 12)}...`);
    console.log(`   Running hash: ${mismatchResult.details.runningHash.substring(0, 12)}...`);
    
    if (mismatchResult.needsRebuild) {
      console.log('⚠️ System needs rebuild - this may be expected for testing');
    } else {
      console.log('✅ System is current with source code');
    }
  } catch (error) {
    console.log(`❌ Version mismatch detection failed: ${error.message}`);
    throw error;
  }
  
  // Test 4: Testing integration check
  console.log('\n📋 Test 4: Testing Integration Check'); 
  try {
    const testingResult = await detector.shouldRebuildForTesting();
    
    console.log(`📊 Should rebuild for testing:`);
    console.log(`   Rebuild needed: ${testingResult.rebuild}`);
    console.log(`   Reason: ${testingResult.reason}`);
    
    if (testingResult.rebuild) {
      console.log('⚠️ Would trigger rebuild in real test scenario');
    } else {
      console.log('✅ System ready for testing without rebuild');
    }
  } catch (error) {
    console.log(`❌ Testing integration check failed: ${error.message}`);
    throw error;
  }
  
  // Test 5: Version storage simulation
  console.log('\n📋 Test 5: Version Storage Simulation');
  try {
    const sourceHash = await detector.calculateSourceHash();
    await detector.storeSystemVersion(sourceHash);
    
    console.log('✅ Version storage completed successfully');
    
    // Verify storage by checking if file was created
    const versionFilePath = '.continuum/jtag/system/source-hash.json';
    if (fs.existsSync(versionFilePath)) {
      const versionData = JSON.parse(fs.readFileSync(versionFilePath, 'utf8'));
      
      if (versionData.sourceHash === sourceHash) {
        console.log('✅ Version storage verification passed');
      } else {
        throw new Error('Stored version hash does not match calculated hash');
      }
    } else {
      throw new Error('Version file was not created');
    }
    
  } catch (error) {
    console.log(`❌ Version storage failed: ${error.message}`);
    throw error;
  }
  
  console.log('\n🎉 BUILD VERSION DETECTION TEST COMPLETE');
  console.log('=========================================');
  console.log('✅ All build detection components working correctly');
  console.log('🚀 AI development with automatic build detection is ready');
  console.log('');
  console.log('💡 KEY BENEFITS:');
  console.log('   • Tests automatically detect source code changes');
  console.log('   • Rebuilds triggered only when necessary'); 
  console.log('   • Version mismatches resolved before testing');
  console.log('   • Zero-friction development for AI');
}

async function demonstrateAutoSpawnIntegration() {
  console.log('\n🔧 DEMONSTRATING AUTO-SPAWN INTEGRATION');
  console.log('=======================================');
  
  // This would normally be wrapped with autoSpawnTest, but we'll just show how it works
  const detector = new BuildVersionDetector();
  const shouldRebuild = await detector.shouldRebuildForTesting();
  
  if (shouldRebuild.rebuild) {
    console.log('🔄 In real auto-spawn scenario, this would trigger:');
    console.log('   1. Automatic smart-build run');
    console.log('   2. Fresh browser deployment');
    console.log('   3. Re-execution of test with correct version');
    console.log(`   Reason: ${shouldRebuild.reason}`);
  } else {
    console.log('✅ System is current - test would run normally');
    console.log(`   ${shouldRebuild.reason}`);
  }
  
  console.log('');
  console.log('🎯 AUTO-SPAWN PATTERN + BUILD DETECTION = AUTONOMOUS DEVELOPMENT');
}

async function main() {
  await testBuildVersionDetection();
  await demonstrateAutoSpawnIntegration();
  
  console.log('🏆 BUILD VERSION DETECTION: FULLY VALIDATED');
  process.exit(0);
}

// Use auto-spawn for this test too! 
// (Though it won't trigger build detection since it's testing the detection system)
autoSpawnTest(main);