/**
 * ProcessPool Integration Test
 *
 * Tests basic process lifecycle management:
 * - Spawn worker processes
 * - Verify ready state
 * - Terminate processes cleanly
 * - Pool statistics
 *
 * Phase 2.1: Basic spawn/kill verification
 */

import { ProcessPool } from '../../system/genome/server/ProcessPool';
import * as path from 'path';

async function testProcessPoolLifecycle() {
  console.log('🔧 PROCESS POOL LIFECYCLE TEST');
  console.log('=================================');

  const workerPath = path.join(__dirname, '../../system/genome/server/inference-worker.ts');
  console.log(`📂 Worker script: ${workerPath}`);

  // Test 1: Initialize pool
  console.log('\n1️⃣  Testing pool initialization...');
  const pool = new ProcessPool(workerPath, {
    minProcesses: 2,
    maxProcesses: 5,
    hotPoolSize: 2,
    warmPoolSize: 3,
  });

  await pool.initialize();

  let stats = pool.getStats();
  console.log(`✅ Pool initialized:`);
  console.log(`   Total processes: ${stats.total}`);
  console.log(`   By state: ${JSON.stringify(stats.byState)}`);
  console.log(`   By tier: ${JSON.stringify(stats.byTier)}`);

  if (stats.total !== 2) {
    throw new Error(`Expected 2 processes, got ${stats.total}`);
  }

  // Test 2: Spawn additional processes
  console.log('\n2️⃣  Testing manual process spawn...');
  const process1 = await pool.spawnProcess('hot');
  const process2 = await pool.spawnProcess('warm');

  if (!process1 || !process2) {
    throw new Error('Failed to spawn processes');
  }

  console.log(`✅ Spawned 2 additional processes:`);
  console.log(`   Process 1: ${process1.id} (${process1.poolTier}) PID: ${process1.pid}`);
  console.log(`   Process 2: ${process2.id} (${process2.poolTier}) PID: ${process2.pid}`);

  stats = pool.getStats();
  console.log(`   Total processes now: ${stats.total}`);

  if (stats.total !== 4) {
    throw new Error(`Expected 4 processes, got ${stats.total}`);
  }

  // Test 3: Check process limits
  console.log('\n3️⃣  Testing max process limit...');
  const process3 = await pool.spawnProcess('warm');

  if (!process3) {
    throw new Error('Failed to spawn 5th process');
  }

  stats = pool.getStats();
  console.log(`✅ Spawned 5th process (at max limit)`);
  console.log(`   Total processes: ${stats.total}`);

  // Try to exceed limit
  const process4 = await pool.spawnProcess('warm');
  if (process4) {
    throw new Error('Should not be able to exceed max processes');
  }

  console.log(`✅ Correctly rejected 6th process (max=5)`);

  // Test 4: Terminate specific process
  console.log('\n4️⃣  Testing process termination...');
  const terminated = await pool.terminateProcess(process1.id, 'test-termination');

  if (!terminated) {
    throw new Error('Failed to terminate process');
  }

  stats = pool.getStats();
  console.log(`✅ Terminated process ${process1.id}`);
  console.log(`   Total processes now: ${stats.total}`);

  if (stats.total !== 4) {
    throw new Error(`Expected 4 processes after termination, got ${stats.total}`);
  }

  // Test 5: Pool shutdown
  console.log('\n5️⃣  Testing pool shutdown...');
  await pool.shutdown();

  stats = pool.getStats();
  console.log(`✅ Pool shutdown complete`);
  console.log(`   Total processes: ${stats.total}`);

  if (stats.total !== 0) {
    throw new Error(`Expected 0 processes after shutdown, got ${stats.total}`);
  }

  console.log('\n🎉 PROCESS POOL LIFECYCLE TEST PASSED');
  console.log('✅ All tests completed successfully:');
  console.log('  - Pool initialization with min processes');
  console.log('  - Manual process spawning');
  console.log('  - Max process limit enforcement');
  console.log('  - Individual process termination');
  console.log('  - Clean pool shutdown');
}

// Run the test
testProcessPoolLifecycle()
  .then(() => {
    console.log('\n✅ Test suite completed successfully');
    process.exit(0);
  })
  .catch((error) => {
    console.error('\n❌ Test suite failed:', error);
    process.exit(1);
  });
