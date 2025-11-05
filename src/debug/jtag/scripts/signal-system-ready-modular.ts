#!/usr/bin/env tsx
/**
 * System Ready Signal Generator (Modular Entry Point)
 * 
 * Refactored, modular implementation of the system readiness signaling system.
 * Maintains compatibility with existing interfaces while providing cleaner architecture.
 * 
 * Usage:
 *   npm run signal:ready          # Called by system startup
 *   npx tsx scripts/signal-system-ready-modular.ts --check  # Check if system is ready
 */

import { SystemReadySignaler } from './signaling/server/SystemReadySignaler';

// Export the main class for external usage
export { SystemReadySignaler } from './signaling/server/SystemReadySignaler';
export * from './signaling/shared/SystemSignalingTypes';
export * from './signaling/shared/MilestoneConfiguration';
export * from './signaling/shared/ProgressCalculator';

// Main CLI entry point
async function main() {
  const signaler = new SystemReadySignaler();
  
  // Parse command line arguments
  const args = process.argv.slice(2);
  const isCheckMode = args.includes('--check');
  
  try {
    if (isCheckMode) {
      console.log('🔍 Checking system readiness...');
      const signal = await signaler.checkSystemReady(30000); // 30s timeout for check mode
      
      if (signal) {
        console.log('✅ System ready signal detected');
        console.log(`📊 Health: ${signal.systemHealth}, Commands: ${signal.commandCount}, Ports: ${signal.portsActive.join(', ')}`);
        process.exit(0);
      } else {
        console.log('❌ System readiness check timed out');
        console.log('🔧 Try: npm run system:restart');
        process.exit(1);
      }
    } else {
      // Generate mode (default)
      const signal = await signaler.generateReadySignal();
      console.log('✅ System ready signal generated');
      process.exit(0);
    }
  } catch (error: any) {
    console.error('❌ Signal operation failed:', error.message);
    process.exit(1);
  }
}

// Run main function when executed directly
if (require.main === module) {
  main();
}