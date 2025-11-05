#!/usr/bin/env npx tsx
/**
 * Git Hook Test Profiles - Middle-Out Architecture
 * Different test execution strategies for different git events
 */

import { MiddleOutTestRunner, TestRunConfig } from '../tests/shared/TestRunner';
import { TestLevel, TestImportance, TestCategory } from '../tests/shared/TestDecorators';

// Test Profiles for different git hooks and CI/CD stages
export const TEST_PROFILES = {
  /**
   * PRE-COMMIT: BLOCKER TESTS ONLY - Nothing that breaks JTAG debugging
   * Purpose: Prevent commits that break core JTAG functionality
   * Time limit: ~15 seconds | Focus: Transport, messaging, routing, session
   */
  'pre-commit': {
    level: [TestLevel.FOUNDATION, TestLevel.UNIT],
    importance: [TestImportance.BLOCKER], // ONLY blockers - nothing else can commit
    category: [TestCategory.TRANSPORT, TestCategory.MESSAGING, TestCategory.ROUTING, TestCategory.SESSION],
    maxConcurrency: 5,
    failFast: true,
    skipSystem: true, // Fast, no system startup
    verbose: false
  } as TestRunConfig,

  /**
   * PRE-PUSH: CRITICAL + HIGH (skip BLOCKER since we ran those)
   * Purpose: Full JTAG functionality + enhanced features before sharing
   * Time limit: ~2-3 minutes | Focus: Screenshots, commands, exec, chat, events
   */
  'pre-push': {
    level: [TestLevel.FOUNDATION, TestLevel.UNIT, TestLevel.INTEGRATION, TestLevel.SYSTEM],
    importance: [TestImportance.CRITICAL, TestImportance.HIGH], // Skip BLOCKER (already tested in pre-commit)
    category: [TestCategory.SCREENSHOT, TestCategory.COMMANDS, TestCategory.EXEC, TestCategory.DATA, TestCategory.CHAT, TestCategory.EVENTS],
    maxConcurrency: 2,
    failFast: true,
    skipSystem: false, // Allow system tests for full integration
    verbose: true
  } as TestRunConfig,

  /**
   * CI-PULL-REQUEST: Full integration validation
   * Purpose: Complete validation before merge
   * Time limit: ~5-10 minutes
   */
  'ci-pr': {
    level: [TestLevel.FOUNDATION, TestLevel.UNIT, TestLevel.INTEGRATION, TestLevel.SYSTEM],
    importance: [TestImportance.CRITICAL, TestImportance.HIGH, TestImportance.MEDIUM],
    maxConcurrency: 1,
    failFast: false, // Want to see all failures
    skipSystem: false,
    verbose: true
  } as TestRunConfig,

  /**
   * CI-MAIN: Full test suite including experimental
   * Purpose: Complete system validation and future-proofing
   * Time limit: ~15-20 minutes
   */
  'ci-main': {
    level: [TestLevel.FOUNDATION, TestLevel.UNIT, TestLevel.INTEGRATION, TestLevel.SYSTEM, TestLevel.E2E],
    importance: [TestImportance.CRITICAL, TestImportance.HIGH, TestImportance.MEDIUM, TestImportance.LOW],
    maxConcurrency: 1,
    failFast: false,
    skipSystem: false,
    verbose: true
  } as TestRunConfig,

  /**
   * EXPERIMENTAL: New features and edge cases
   * Purpose: Testing experimental features without blocking development
   * Time limit: No limit (can be slow)
   */
  'experimental': {
    level: [TestLevel.FOUNDATION, TestLevel.UNIT, TestLevel.INTEGRATION, TestLevel.SYSTEM, TestLevel.E2E],
    importance: [TestImportance.EXPERIMENTAL],
    maxConcurrency: 1,
    failFast: false,
    skipSystem: false,
    verbose: true
  } as TestRunConfig,

  /**
   * QUICK: Developer-friendly fast feedback
   * Purpose: Quick smoke test during development
   * Time limit: ~10-15 seconds
   */
  'quick': {
    level: [TestLevel.FOUNDATION],
    importance: [TestImportance.CRITICAL],
    maxConcurrency: 5,
    failFast: true,
    skipSystem: true,
    verbose: false
  } as TestRunConfig,

  /**
   * CATEGORY-SPECIFIC: Test specific feature areas
   * Purpose: Focused testing for specific development work
   */
  'chat-only': {
    category: [TestCategory.CHAT, TestCategory.EVENTS, TestCategory.MESSAGING],
    importance: [TestImportance.CRITICAL, TestImportance.HIGH],
    maxConcurrency: 1,
    failFast: false,
    verbose: true
  } as TestRunConfig,

  'transport-only': {
    category: [TestCategory.TRANSPORT, TestCategory.MESSAGING, TestCategory.ROUTING],
    importance: [TestImportance.CRITICAL, TestImportance.HIGH],
    maxConcurrency: 1,
    failFast: false,
    verbose: true
  } as TestRunConfig,

  'performance': {
    category: [TestCategory.PERFORMANCE],
    importance: [TestImportance.HIGH, TestImportance.MEDIUM, TestImportance.LOW],
    maxConcurrency: 1,
    failFast: false,
    verbose: true
  } as TestRunConfig
};

/**
 * CLI interface for running test profiles
 */
async function main() {
  const profileName = process.argv[2] || 'pre-commit';
  const profile = TEST_PROFILES[profileName as keyof typeof TEST_PROFILES];
  
  if (!profile) {
    console.error(`❌ Unknown test profile: ${profileName}`);
    console.error(`Available profiles: ${Object.keys(TEST_PROFILES).join(', ')}`);
    process.exit(1);
  }
  
  console.log(`🧅 MIDDLE-OUT TEST PROFILE: ${profileName.toUpperCase()}`);
  
  // Show profile configuration
  if (profile.level) {
    console.log(`📊 Levels: ${profile.level.join(', ')}`);
  }
  if (profile.importance) {
    console.log(`⚡ Importance: ${profile.importance.join(', ')}`);
  }
  if (profile.category) {
    console.log(`📂 Categories: ${profile.category.join(', ')}`);
  }
  
  console.log(`🚀 Concurrency: ${profile.maxConcurrency}`);
  console.log(`⏹️  Fail Fast: ${profile.failFast}`);
  console.log(`🖥️  Skip System: ${profile.skipSystem}`);
  console.log('');
  
  const runner = new MiddleOutTestRunner(profile);
  await runner.runTests();
}

if (require.main === module) {
  main().catch(error => {
    console.error('💥 Test profile execution failed:', error);
    process.exit(1);
  });
}