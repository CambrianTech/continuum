#!/usr/bin/env npx tsx
/**
 * SIGNAL SYSTEM TESTS
 * 
 * Test the ACTUAL signal generation logic that's failing
 */

import { SystemReadySignaler } from '../scripts/signal-system-ready';

async function testSignalSystem() {
  console.log('🧪 TESTING ACTUAL SIGNAL SYSTEM');
  console.log('='.repeat(50));
  
  const signaler = new SystemReadySignaler();
  
  try {
    console.log('1. Testing signal generation...');
    const signal = await signaler.generateReadySignal();
    
    console.log('2. Return value type:', typeof signal);
    console.log('3. Return value:', signal);
    
    if (!signal) {
      console.log('❌ Method returned undefined/null');
      return;
    }
    
    console.log('4. Signal properties:', {
      bootstrapComplete: signal.bootstrapComplete,
      commandCount: signal.commandCount,
      systemHealth: signal.systemHealth
    });
    
  } catch (error: any) {
    console.log('❌ Signal system error:', error.message);
    console.log('Stack:', error.stack);
  }
}

testSignalSystem().catch(console.error);