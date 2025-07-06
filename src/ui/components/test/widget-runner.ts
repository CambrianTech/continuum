#!/usr/bin/env tsx
/**
 * Widget Discovery Test Runner
 * Moved from root to proper module location
 */

import { AllWidgetsTestRunner } from './AllWidgetsTest';

async function main() {
  const runner = new AllWidgetsTestRunner();
  
  try {
    console.log('🔍 Running widget discovery and validation...');
    await runner.quickComplianceCheck();
    
    console.log('\n🧪 Running comprehensive widget tests...');
    const results = await runner.runAllWidgetTests();
    
    if (results.summary.failed === 0) {
      console.log('\n✅ All widget tests passed!');
      process.exit(0);
    } else {
      console.log('\n❌ Some widget tests failed');
      process.exit(1);
    }
  } catch (error) {
    console.error('❌ Widget testing failed:', error);
    process.exit(1);
  }
}

main();