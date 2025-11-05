#!/usr/bin/env npx tsx
/**
 * Auto-register classified tests for middle-out execution
 * Discovers test files and registers them in the test system
 */

import { TestSpec, TestLevel, TestImportance, TestCategory } from '../tests/shared/TestDecorators';
import { readdirSync, statSync } from 'fs';
import { join } from 'path';

// Import our classified test classes to trigger their registration
import { TransportBlockerTest } from '../tests/classified/TransportBlockerTest';
import { ScreenshotCriticalTest } from '../tests/classified/ScreenshotCriticalTest';
import { ChatHighTest } from '../tests/classified/ChatHighTest';
import { PerformanceMediumTest } from '../tests/classified/PerformanceMediumTest';
import { ProfessionalDataArchitectureTest } from '../tests/classified/ProfessionalDataArchitectureTest';

/**
 * Test registry mapping test names to file paths and metadata
 */
export const CLASSIFIED_TESTS = {
  'TransportBlockerTest': {
    filePath: 'tests/classified/TransportBlockerTest.ts',
    testClass: TransportBlockerTest,
    metadata: {
      level: TestLevel.FOUNDATION,
      importance: TestImportance.BLOCKER,
      category: TestCategory.TRANSPORT,
      description: 'WebSocket transport connection - core JTAG dependency',
      timeout: 10000,
      requiresSystem: false
    }
  },
  
  'ScreenshotCriticalTest': {
    filePath: 'tests/classified/ScreenshotCriticalTest.ts',
    testClass: ScreenshotCriticalTest,
    metadata: {
      level: TestLevel.INTEGRATION,
      importance: TestImportance.CRITICAL,
      category: TestCategory.SCREENSHOT,
      description: 'Screenshot command functionality - core JTAG visual debugging',
      timeout: 30000,
      requiresSystem: true
    }
  },
  
  'ChatHighTest': {
    filePath: 'tests/classified/ChatHighTest.ts',
    testClass: ChatHighTest,
    metadata: {
      level: TestLevel.SYSTEM,
      importance: TestImportance.HIGH,
      category: TestCategory.CHAT,
      description: 'Multi-user chat system - enhanced JTAG collaboration',
      timeout: 60000,
      requiresSystem: true
    }
  },
  
  'PerformanceMediumTest': {
    filePath: 'tests/classified/PerformanceMediumTest.ts',
    testClass: PerformanceMediumTest,
    metadata: {
      level: TestLevel.INTEGRATION,
      importance: TestImportance.MEDIUM,
      category: TestCategory.PERFORMANCE,
      description: 'Performance metrics monitoring - system optimization',
      timeout: 15000,
      requiresSystem: false
    }
  },
  
  'ProfessionalDataArchitectureTest': {
    filePath: 'tests/classified/ProfessionalDataArchitectureTest.ts',
    testClass: ProfessionalDataArchitectureTest,
    metadata: {
      level: TestLevel.INTEGRATION,
      importance: TestImportance.CRITICAL,
      category: TestCategory.DATA,
      description: 'Professional data architecture - Rust-like typing, DataService, HybridAdapter',
      timeout: 30000,
      requiresSystem: false
    }
  }
};

/**
 * Register tests in the decorator system
 */
function registerClassifiedTests() {
  console.log('📋 Registering classified tests...');
  
  Object.entries(CLASSIFIED_TESTS).forEach(([testName, testInfo]) => {
    console.log(`  📝 ${testName}: ${testInfo.metadata.importance}/${testInfo.metadata.category}`);
  });
  
  console.log(`✅ Registered ${Object.keys(CLASSIFIED_TESTS).length} classified tests`);
}

/**
 * Get tests by filter criteria
 */
export function getFilteredTests(criteria: {
  level?: TestLevel[];
  importance?: TestImportance[];
  category?: TestCategory[];
  requiresSystem?: boolean;
}): string[] {
  return Object.entries(CLASSIFIED_TESTS).filter(([_, testInfo]) => {
    const meta = testInfo.metadata;
    
    // Filter by level
    if (criteria.level && !criteria.level.includes(meta.level)) {
      return false;
    }
    
    // Filter by importance
    if (criteria.importance && !criteria.importance.includes(meta.importance)) {
      return false;
    }
    
    // Filter by category
    if (criteria.category && !criteria.category.includes(meta.category)) {
      return false;
    }
    
    // Filter by system requirement
    if (criteria.requiresSystem !== undefined && meta.requiresSystem !== criteria.requiresSystem) {
      return false;
    }
    
    return true;
  }).map(([testName, _]) => testName);
}

/**
 * Execute tests by filter
 */
export async function runFilteredTests(criteria: {
  level?: TestLevel[];
  importance?: TestImportance[];
  category?: TestCategory[];
  requiresSystem?: boolean;
}, options: {
  verbose?: boolean;
  failFast?: boolean;
} = {}): Promise<{ passed: number; failed: number; results: any[] }> {
  
  const filteredTests = getFilteredTests(criteria);
  console.log(`\n🎯 Running ${filteredTests.length} classified tests...`);
  
  const results = [];
  let passed = 0;
  let failed = 0;
  
  for (const testName of filteredTests) {
    const testInfo = CLASSIFIED_TESTS[testName];
    const startTime = Date.now();
    
    try {
      console.log(`\n🧪 ${testName} (${testInfo.metadata.importance}/${testInfo.metadata.category})`);
      
      const success = await testInfo.testClass.run();
      const duration = Date.now() - startTime;
      
      if (success) {
        passed++;
        if (options.verbose) {
          console.log(`✅ ${testName} passed (${duration}ms)`);
        }
      } else {
        failed++;
        console.log(`❌ ${testName} failed (${duration}ms)`);
        if (options.failFast) {
          console.log('🛑 Fail fast enabled - stopping execution');
          break;
        }
      }
      
      results.push({
        name: testName,
        success,
        duration,
        metadata: testInfo.metadata
      });
      
    } catch (error) {
      failed++;
      const duration = Date.now() - startTime;
      console.error(`💥 ${testName} crashed (${duration}ms):`, error);
      
      results.push({
        name: testName,
        success: false,
        duration,
        error: error.toString(),
        metadata: testInfo.metadata
      });
      
      if (options.failFast) {
        console.log('🛑 Fail fast enabled - stopping execution');
        break;
      }
    }
  }
  
  return { passed, failed, results };
}

// Auto-register when this module is imported
registerClassifiedTests();

// CLI interface
async function main() {
  const profileArg = process.argv[2] || 'all';
  
  let criteria: any = {};
  let options = { verbose: true, failFast: true };
  
  switch (profileArg) {
    // Importance filters
    case 'blocker':
      criteria = { importance: [TestImportance.BLOCKER] };
      console.log('🚨 BLOCKER TESTS ONLY - Blocks commits');
      break;
    case 'critical':
      criteria = { importance: [TestImportance.CRITICAL] };
      console.log('📸 CRITICAL TESTS ONLY - Core JTAG features');
      break;
    case 'high':
      criteria = { importance: [TestImportance.HIGH] };
      console.log('📈 HIGH PRIORITY TESTS ONLY - Enhanced features');
      break;
    case 'medium':
      criteria = { importance: [TestImportance.MEDIUM] };
      console.log('📊 MEDIUM PRIORITY TESTS ONLY - System quality');
      break;
    
    // Level filters
    case 'unit':
      criteria = { level: [TestLevel.UNIT] };
      console.log('🧪 UNIT TESTS ONLY - Individual components');
      break;
    case 'integration':
      criteria = { level: [TestLevel.INTEGRATION] };
      console.log('🔗 INTEGRATION TESTS ONLY - Component interactions');
      break;
    case 'system':
      criteria = { level: [TestLevel.SYSTEM] };
      console.log('🏗️  SYSTEM TESTS ONLY - Full system validation');
      break;
    case 'e2e':
      criteria = { level: [TestLevel.E2E] };
      console.log('🎯 E2E TESTS ONLY - End-to-end workflows');
      break;
    case 'foundation':
      criteria = { level: [TestLevel.FOUNDATION] };
      console.log('🏛️  FOUNDATION TESTS ONLY - Core infrastructure');
      break;
    
    // Category filters  
    case 'transport-category':
      criteria = { category: [TestCategory.TRANSPORT, TestCategory.MESSAGING, TestCategory.ROUTING] };
      console.log('🔗 TRANSPORT TESTS - WebSocket, messaging, routing');
      break;
    case 'screenshot-category':
      criteria = { category: [TestCategory.SCREENSHOT] };
      console.log('📸 SCREENSHOT TESTS - Visual debugging capability');
      break;
    case 'chat-category':
      criteria = { category: [TestCategory.CHAT] };
      console.log('💬 CHAT TESTS - Multi-user collaboration');
      break;
    case 'events-category':
      criteria = { category: [TestCategory.EVENTS] };
      console.log('📡 EVENT TESTS - Real-time event system');
      break;
    case 'routing-category':
      criteria = { category: [TestCategory.ROUTING] };
      console.log('🔀 ROUTING TESTS - Command routing system');
      break;
    case 'logging-category':
      criteria = { category: [TestCategory.HEALTH] }; // Health includes logging/monitoring
      console.log('📝 LOGGING TESTS - Console capture and routing');
      break;
    
    // System requirement filters
    case 'no-system':
      criteria = { requiresSystem: false };
      console.log('⚡ NO SYSTEM REQUIRED - Fast tests');
      break;
    
    case 'all':
    default:
      console.log('🎯 ALL CLASSIFIED TESTS');
      break;
  }
  
  const { passed, failed, results } = await runFilteredTests(criteria, options);
  
  console.log(`\n🏆 CLASSIFIED TEST SUMMARY:`);
  console.log(`✅ Passed: ${passed}`);
  console.log(`❌ Failed: ${failed}`);
  console.log(`⏱️  Total: ${results.length} tests`);
  
  if (failed > 0) {
    console.log('\n❌ FAILED TESTS:');
    results.filter(r => !r.success).forEach(r => {
      console.log(`   • ${r.name} (${r.metadata.importance}/${r.metadata.category})`);
    });
  }
  
  process.exit(failed > 0 ? 1 : 0);
}

if (require.main === module) {
  main().catch(error => {
    console.error('💥 Classified test execution failed:', error);
    process.exit(1);
  });
}