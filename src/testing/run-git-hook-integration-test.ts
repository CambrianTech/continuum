#!/usr/bin/env npx tsx
/**
 * Git Hook Integration Test Runner
 * 
 * Executable script to run git hook validation tests
 * Usage: npx tsx src/testing/run-git-hook-integration-test.ts
 */

import { GitHookIntegrationTest } from './GitHookIntegrationTest';
import { ContinuumContext } from '../types/shared/core/ContinuumTypes';

async function main() {
  try {
    console.log('🔧 Git Hook Integration Test Runner');
    console.log('=====================================\n');
    
    // Create context for testing
    const context: ContinuumContext = {
      sessionId: `test-session-${Date.now()}-${Math.random().toString(36).substr(2, 9)}` as any,
      sessionPaths: {
        base: process.cwd(),
        screenshots: '.continuum/screenshots',
        logs: '.continuum/logs', 
        recordings: '.continuum/recordings',
        files: '.continuum/files',
        devtools: '.continuum/devtools'
      }
    };

    const tester = new GitHookIntegrationTest(context);
    
    // Verify validation directory structure
    console.log('📁 Checking validation directory structure...');
    const structureExists = await tester.verifyValidationDirectoryStructure();
    if (!structureExists) {
      console.error('❌ Validation directory structure not found');
      process.exit(1);
    }
    console.log('✅ Validation directory structure verified\n');
    
    // Run the complete test suite
    const results = await tester.runCompleteTestSuite();
    
    // Exit with appropriate code
    process.exit(results.overall ? 0 : 1);
    
  } catch (error) {
    console.error('🚨 Test runner failed:', error);
    process.exit(1);
  }
}

// Run if this file is executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}