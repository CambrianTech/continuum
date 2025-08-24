/**
 * System Startup Script - Milestone-Based Orchestration Entry Point
 * 
 * Uses SystemOrchestrator to ensure proper milestone execution order.
 * Provides clean startup process with proper signaling and exits cleanly.
 */

import { systemOrchestrator } from '../system/orchestration/SystemOrchestrator';
import { getActiveExampleName } from '../system/shared/ExampleConfig';

export async function startSystem(entryPoint: string = 'npm-start'): Promise<void> {
  console.log(`🎯 MILESTONE-BASED JTAG SYSTEM STARTUP (${entryPoint.toUpperCase()})`);
  
  try {
    // Set up working directory context for per-project isolation  
    const activeExample = getActiveExampleName();
    const workingDir = `examples/${activeExample}`;
    
    // Use milestone-based orchestration with specified entry point
    // This ensures proper milestone order and fixes browser timing
    const systemState = await systemOrchestrator.orchestrate(entryPoint as any, {
      workingDir,
      verbose: true,
      skipBrowser: entryPoint === 'npm-test' ? false : false // Both npm-start and npm-test need browser
    });
    
    if (!systemState.success) {
      console.error(`❌ System startup failed at milestone: ${systemState.failedMilestone}`);
      console.error(`❌ Error: ${systemState.error}`);
      process.exit(1);
    }
    
    console.log(`🎉 MILESTONE-BASED system startup complete (${entryPoint})!`);
    console.log(`✅ Milestones completed: ${systemState.completedMilestones.join(' → ')}`);
    console.log('🚀 Server running in background - ready for use');
    
    if (systemState.browserOpened) {
      console.log('🌐 Browser interface opened and ready for interaction');
    }
    
  } catch (error) {
    console.error('💥 Milestone-based system startup failed:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

// If called directly, run startup
if (require.main === module) {
  startSystem();
}