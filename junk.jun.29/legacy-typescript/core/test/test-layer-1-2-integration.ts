#!/usr/bin/env npx tsx
/**
 * Layer 1+2 Integration Test: Core ContinuumOS + Bootstrap
 * JTAG methodology: logs as stimulus-response validation
 */

import { BootstrapSystem } from '../bootstrap/BootstrapSystem.js';

async function testLayer1And2Integration() {
  console.log('🧅 ONION TEST: Layer 1+2 Integration (Core + Bootstrap)');
  console.log('=' .repeat(60));
  
  let success = true;
  const results: string[] = [];
  
  try {
    // Layer 1+2: Test Bootstrap System Initialization
    console.log('🏷️ MARKER: Testing Bootstrap System startup...');
    const bootstrap = new BootstrapSystem();
    
    console.log('🏷️ MARKER: Starting bootstrap system...');
    await bootstrap.start();
    
    // JTAG: Verify system is ready
    const isReady = bootstrap.isReady();
    console.log(`🏷️ MARKER: Bootstrap ready: ${isReady}`);
    
    if (!isReady) {
      success = false;
      results.push('❌ Bootstrap system failed to initialize');
    } else {
      results.push('✅ Bootstrap system initialized successfully');
    }
    
    // JTAG: Test system state
    const systemState = bootstrap.getSystemState();
    console.log(`🏷️ MARKER: System state: ${JSON.stringify(systemState, null, 2)}`);
    
    if (systemState.systemReady) {
      results.push('✅ System state shows ready');
    } else {
      success = false;
      results.push('❌ System state not ready');
    }
    
    // JTAG: Test command execution capability
    console.log('🏷️ MARKER: Testing command execution...');
    const testResult = await bootstrap.execute('help', {});
    console.log(`🏷️ MARKER: Command result: ${JSON.stringify(testResult, null, 2)}`);
    
    if (testResult.success) {
      results.push('✅ Command execution working');
    } else {
      success = false;
      results.push('❌ Command execution failed');
    }
    
  } catch (error) {
    success = false;
    results.push(`❌ Integration test failed: ${error}`);
    console.error('🏷️ MARKER: Error:', error);
  }
  
  // JTAG: Final results
  console.log('\n🧅 ONION TEST RESULTS (Layer 1+2):');
  console.log('=' .repeat(40));
  
  results.forEach(result => console.log(result));
  
  if (success) {
    console.log('\n🎉 Layer 1+2 Integration: PASSED');
    console.log('🧅 Ready to test Layer 3 (Daemons)');
    process.exit(0);
  } else {
    console.log('\n❌ Layer 1+2 Integration: FAILED');
    console.log('🧅 Must fix inner layers before proceeding');
    process.exit(1);
  }
}

// Run the integration test
testLayer1And2Integration().catch(console.error);