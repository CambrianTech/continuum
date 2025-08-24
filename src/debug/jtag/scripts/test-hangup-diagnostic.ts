#!/usr/bin/env tsx
/**
 * Hangup Diagnostic Tool
 * 
 * Tests system responsiveness and identifies potential performance bottlenecks.
 */

import { SystemReadySignaler } from './signaling/server/SystemReadySignaler';
import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';

async function main() {
  console.log('🔍 HANGUP DIAGNOSTIC - Testing iteration speed...');
  console.time('Full diagnostic');

  try {
    // Set context
    WorkingDirConfig.setWorkingDir('examples/widget-ui');

    const signaler = new SystemReadySignaler();

    // Test fast signal check (should be instant if system ready)
    console.log('⚡ Testing fast system check...');
    const start = Date.now();
    const signal = await signaler.checkSystemReady(100); // Very short timeout
    const duration = Date.now() - start;

    if (signal) {
      console.log('✅ System ready detected in', duration, 'ms');
      console.log('📊 Health:', signal.systemHealth);
      console.log('📊 Commands:', signal.commandCount);
      console.log('📊 Ports:', signal.portsActive.join(', '));
      
      // Test multiple quick iterations to check for consistency
      console.log('🔄 Testing iteration consistency...');
      const iterationTimes = [];
      for (let i = 0; i < 5; i++) {
        const iterStart = Date.now();
        const iterSignal = await signaler.checkSystemReady(50);
        const iterTime = Date.now() - iterStart;
        iterationTimes.push(iterTime);
        
        if (!iterSignal) {
          console.log(`⚠️ Iteration ${i + 1} failed in ${iterTime}ms`);
        }
      }
      
      const avgTime = iterationTimes.reduce((a, b) => a + b, 0) / iterationTimes.length;
      console.log(`📊 Average iteration time: ${avgTime.toFixed(1)}ms`);
      console.log(`📊 Iteration times: [${iterationTimes.join(', ')}]ms`);
      
      if (avgTime > 200) {
        console.log('⚠️ WARNING: Iterations taking longer than expected (>200ms)');
        console.log('🔍 Potential hangup detected - check file watchers or event loops');
      } else if (avgTime < 50) {
        console.log('✅ EXCELLENT: Fast iteration times (<50ms average)');
      } else {
        console.log('✅ GOOD: Reasonable iteration times (50-200ms average)');
      }
      
    } else {
      console.log('❌ System not ready or timed out in', duration, 'ms');
      console.log('🔍 System may be starting up or have issues');
      
      // Check if signal file exists
      try {
        const fs = await import('fs/promises');
        const signalFile = 'examples/widget-ui/.continuum/jtag/signals/system-ready.json';
        const stats = await fs.stat(signalFile);
        const ageMs = Date.now() - stats.mtimeMs;
        console.log(`📄 Signal file age: ${Math.round(ageMs / 1000)}s`);
        
        if (ageMs > 300000) { // 5 minutes
          console.log('⚠️ Signal file is very stale (>5min) - system may be down');
        }
      } catch (error) {
        console.log('❌ No signal file found - system not started');
      }
    }

    console.timeEnd('Full diagnostic');
    console.log('🚀 Diagnostic complete - ready for iteration');

  } catch (error) {
    console.error('❌ Diagnostic failed:', error.message);
    console.timeEnd('Full diagnostic');
    process.exit(1);
  }
}

if (require.main === module) {
  main().catch(console.error);
}