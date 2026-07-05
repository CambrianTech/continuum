#!/usr/bin/env npx tsx
/**
 * Signal System Debug Test
 * 
 * Directly tests the SystemReadySignaler to find the disconnect between
 * working detection logic and false negative signals.
 */

import { SystemReadySignaler } from '../scripts/signal-system-ready';

async function debugSignalGeneration() {
  console.log('🔍 SIGNAL SYSTEM DEBUG TEST');
  console.log('='.repeat(50));
  
  const signaler = new SystemReadySignaler();
  
  try {
    console.log('📊 Generating signal directly...');
    const signal = await signaler.generateSignal();
    
    console.log('');
    console.log('📋 SIGNAL RESULTS:');
    console.log('-'.repeat(30));
    console.log(`Bootstrap Complete: ${signal.bootstrapComplete}`);
    console.log(`Command Count: ${signal.commandCount}`);
    console.log(`System Health: ${signal.systemHealth}`);
    console.log(`Browser Ready: ${signal.browserReady}`);
    console.log('');
    
    if (signal.errors && signal.errors.length > 0) {
      console.log('❌ ERRORS:');
      signal.errors.forEach(err => console.log(`  - ${err}`));
      console.log('');
    }
    
    if (signal.nodeErrors && signal.nodeErrors.length > 0) {
      console.log('🔍 NODE ERRORS:');
      signal.nodeErrors.forEach(err => console.log(`  - ${err}`));
      console.log('');
    }
    
    // The key test: Are we getting false negatives?
    const shouldBeHealthy = signal.bootstrapComplete && signal.commandCount > 0;
    const actuallyHealthy = signal.systemHealth === 'healthy' || signal.systemHealth === 'degraded';
    
    console.log('🎯 ANALYSIS:');
    console.log('-'.repeat(30));
    console.log(`Should be ready: ${shouldBeHealthy}`);
    console.log(`Actually ready: ${actuallyHealthy}`);
    console.log(`False negative: ${shouldBeHealthy && !actuallyHealthy ? 'YES ❌' : 'NO ✅'}`);
    
  } catch (error: any) {
    console.error('❌ Signal generation failed:', error.message);
  }
}

// Run debug if called directly
if (require.main === module) {
  debugSignalGeneration().catch(console.error);
}