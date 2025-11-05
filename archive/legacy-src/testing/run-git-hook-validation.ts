#!/usr/bin/env npx tsx
/**
 * Simple Git Hook Validation Test Runner
 * 
 * Follows the intelligent modular testing framework structure
 * Usage: npx tsx src/testing/run-git-hook-validation.ts
 */

import { GitHookValidationTest } from './GitHookValidationTest';

async function main() {
  try {
    console.log('🔧 Git Hook Validation Test');
    console.log('============================\n');
    
    const tester = new GitHookValidationTest();
    
    // Run the basic validation test
    const result = await tester.runBasicValidationTest();
    
    if (result.success) {
      console.log('✅ Git hook validation test passed!');
      console.log(`📸 Screenshot: ${result.screenshotPath}`);
      console.log(`🔍 Validation ID: ${result.validationId}`);
      
      // Get commit hash for reference
      const commitHash = await tester.getCurrentCommitHash();
      console.log(`📝 Commit: ${commitHash.substring(0, 7)}`);
      
      process.exit(0);
    } else {
      console.error('❌ Git hook validation test failed!');
      if (result.error) {
        console.error(`Error: ${result.error}`);
      }
      process.exit(1);
    }
    
  } catch (error) {
    console.error('🚨 Test runner failed:', error);
    process.exit(1);
  }
}

// Run if this file is executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}