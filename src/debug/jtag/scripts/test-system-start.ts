#!/usr/bin/env tsx
/**
 * Test System Startup Script
 * 
 * Starts the JTAG system, launches browser, and exits cleanly.
 * Used by the test runner to start the system without hanging.
 */

import { SystemOrchestrator } from '../system/orchestration/SystemOrchestrator';
import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';

async function main(): Promise<void> {
  console.log('🚀 TEST SYSTEM STARTUP - Starting system for testing...');
  
  try {
    // Force test-bench example for npm test
    const testWorkingDir = 'examples/test-bench';
    WorkingDirConfig.setWorkingDir(testWorkingDir);
    console.log(`📂 Test working directory: ${testWorkingDir}`);
    
    const orchestrator = new SystemOrchestrator();
    
    // Start system with browser launch for testing
    const result = await orchestrator.orchestrate('npm-start', {
      testMode: true,
      verbose: true,
      browserUrl: `http://localhost:${(await require('../examples/shared/ExampleConfig').getActivePorts()).http_server}`
    });
    
    if (result.success) {
      console.log('✅ System startup completed successfully');
      console.log(`📊 Completed milestones: ${result.completedMilestones.join(' → ')}`);
      console.log('🌐 Browser interface is ready for interaction');
      console.log('🎉 Test system startup complete - test runner can now proceed');
      
      // Exit cleanly so test runner can continue
      process.exit(0);
    } else {
      console.error('❌ System startup failed');
      console.error(`Failed milestone: ${result.failedMilestone}`);
      console.error(`Error: ${result.error}`);
      process.exit(1);
    }
  } catch (error: any) {
    console.error('💥 Test system startup crashed:', error.message);
    if (error.stack) {
      console.error('Stack:', error.stack);
    }
    process.exit(1);
  }
}

// Handle CTRL+C gracefully
process.on('SIGINT', () => {
  console.log('\n🛑 Test system startup interrupted');
  process.exit(130);
});

process.on('SIGTERM', () => {
  console.log('\n🛑 Test system startup terminated');
  process.exit(143);
});

if (require.main === module) {
  main();
}