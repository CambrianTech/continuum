#!/usr/bin/env npx tsx
/**
 * Launch Active Example Script
 * 
 * MIGRATED TO MILESTONE-BASED ORCHESTRATION
 * 
 * Uses SystemOrchestrator to ensure proper milestone execution order.
 * CRITICAL FIX: Browser now opens ONLY after SERVER_READY milestone.
 */

import { systemOrchestrator } from '../system/orchestration/SystemOrchestrator';
import { getActiveExampleName } from "../examples/server/ExampleConfigServer";
import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';

async function launchActiveExample(): Promise<void> {
  try {
    console.log('🚀 MILESTONE-BASED JTAG SYSTEM STARTUP');

    // Set up working directory context
    const activeExample = getActiveExampleName();
    const workingDir = `examples/${activeExample}`;

    // Use milestone-based orchestration for 'system-start' entry point
    // This ensures proper milestone order: BUILD → SERVER → BROWSER → SYSTEM
    // Browser check happens inside orchestrator using ./jtag ping after server is ready
    const systemState = await systemOrchestrator.orchestrate('system-start', {
      workingDir,
      verbose: true,
      browserUrl: undefined, // Use default from configuration
      skipBrowser: process.env.CONTINUUM_DEFER_BROWSER === '1' || process.env.CONTINUUM_DEFER_BROWSER === 'true'
    });
    
    if (!systemState.success) {
      console.error(`❌ System startup failed at milestone: ${systemState.failedMilestone}`);
      console.error(`❌ Error: ${systemState.error}`);
      process.exit(1);
    }
    
    console.log('🎉 Complete JTAG system started successfully via milestone orchestration');
    console.log(`✅ Milestones completed: ${systemState.completedMilestones.join(' → ')}`);
    
    if (systemState.browserOpened) {
      console.log('🌐 Browser interface is ready for interaction');
    }
    
    // Setup cleanup handlers for graceful shutdown
    process.on('SIGINT', () => {
      console.log('\n⚡ Shutting down milestone-based JTAG system...');
      cleanup();
      process.exit(0);
    });
    
    process.on('SIGTERM', () => {
      console.log('\n⚡ Terminating milestone-based JTAG system...');
      cleanup();
      process.exit(0);
    });
    
    // Keep running - prevent Node.js from exiting
    console.log('📡 Milestone-based JTAG system running - press Ctrl+C to stop');
    
    // Keep process alive (orchestrator manages the actual server processes)
    await new Promise(() => {}); // Infinite wait
    
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    console.error('❌ Failed to launch milestone-based JTAG system:', errorMsg);
    cleanup();
    process.exit(1);
  }
}

function cleanup() {
  console.log('🧹 Cleaning up milestone-based system...');
  systemOrchestrator.cleanup().catch(console.error);
}

// Run the launcher
launchActiveExample();
