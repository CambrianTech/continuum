#!/usr/bin/env tsx
/**
 * System Ready Signal Generator (Compatibility Wrapper)
 * 
 * This file has been refactored into modular components for better maintainability.
 * This wrapper maintains backward compatibility while forwarding to the new modular implementation.
 * 
 * New modular architecture:
 * - shared/SystemSignalingTypes.ts - Core types and interfaces
 * - shared/MilestoneConfiguration.ts - Configurable milestone definitions  
 * - shared/ProgressCalculator.ts - Progress tracking and visualization
 * - server/SystemMetricsCollector.ts - Server-side metrics collection
 * - server/SystemReadySignaler.ts - Main signaler with event-driven detection
 * 
 * Benefits of modular approach:
 * - Easily extensible milestone configuration
 * - Testable components in isolation
 * - Reusable progress calculation logic
 * - Clear separation of concerns
 * - Future-ready for additional milestones
 * 
 * Usage:
 *   npm run signal:ready          # Called by system startup
 *   npx tsx scripts/signal-system-ready.ts --check  # Check if system is ready
 */

// Forward all exports from the modular implementation
export { SystemReadySignaler } from './signaling/server/SystemReadySignaler';
export * from './signaling/shared/SystemSignalingTypes';
export * from './signaling/shared/MilestoneConfiguration';
export * from './signaling/shared/ProgressCalculator';

// Import the main implementation
import { SystemReadySignaler } from './signaling/server/SystemReadySignaler';

// Maintain exact CLI compatibility
async function main() {
  const signaler = new SystemReadySignaler();
  const args = process.argv.slice(2);

  try {
    if (args.includes('--check')) {
      // Fast check for autonomous testing - only 5 seconds
      const signal = await signaler.checkSystemReady(30000);
      if (signal) {
        console.log('✅ System is ready');
        console.log(JSON.stringify(signal, null, 2));
        
        // Signal will be cleared on next system start or by the generator process
        
        // TIMEOUT ELIMINATION: Force exit to prevent hanging on background resources
        // This prevents npm test from hanging after successful signal check
        setTimeout(() => {
          console.error('⚠️ Forcing exit - background resources preventing clean exit');
          process.exit(0);
        }, 1000); // Give 1 second for output to flush, then force exit
        
        process.exit(0);
      } else {
        console.log('❌ System is not ready');
        process.exit(1);
      }
    } else if (args.includes('--clear')) {
      await signaler.clearSignals();
      process.exit(0);
    } else {
      // Generate ready signal (default)
      await signaler.generateReadySignal();
      process.exit(0);
    }
  } catch (error: any) {
    console.error('❌ Signal operation failed:', error.message);
    process.exit(1);
  }
}

// Maintain exact CLI behavior
if (require.main === module) {
  main().catch(console.error);
}