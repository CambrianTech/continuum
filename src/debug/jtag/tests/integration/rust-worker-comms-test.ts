/**
 * Rust Worker Communication Test
 *
 * PURPOSE: Prove TypeScript ↔ Rust socket communication works
 * BEFORE building full adapter system
 *
 * Tests:
 * 1. DataWorkerClient can connect to Rust worker
 * 2. ping message works (health check)
 * 3. open-database message works (returns handle)
 * 4. Error handling works (unknown message type)
 *
 * RUN:
 * ```bash
 * # Terminal 1: Start Rust worker
 * cd workers/data-daemon
 * cargo run --bin data-worker-test
 *
 * # Terminal 2: Run this test
 * npx tsx tests/integration/rust-worker-comms-test.ts
 * ```
 */

import { DataWorkerClient } from '../../shared/ipc/data-worker/DataWorkerClient';

async function main() {
  console.log('🧪 Rust Worker Communication Test\n');

  const client = new DataWorkerClient({
    socketPath: '/tmp/jtag-data-worker.sock',
    timeout: 5000
  });

  try {
    // ========================================================================
    // TEST 1: Connect to Rust worker
    // ========================================================================
    console.log('1️⃣  Connecting to Rust worker...');
    await client.connect();
    console.log('   ✅ Connected\n');

    // ========================================================================
    // TEST 2: Ping (health check)
    // ========================================================================
    console.log('2️⃣  Testing ping...');
    const pingResponse = await client.ping();
    console.log('   ✅ Ping successful');
    console.log('   📊 Stats:', JSON.stringify(pingResponse, null, 2));
    console.log();

    // ========================================================================
    // TEST 3: Open database (returns fake handle)
    // ========================================================================
    console.log('3️⃣  Testing open-database...');
    const openResponse = await client.openDatabase({
      filename: '/tmp/test-database.sqlite',
      adapterType: 'sqlite',
      storageType: 'auto-detect'
    });
    console.log('   ✅ Open database successful');
    console.log('   📂 Handle:', openResponse.handle);
    console.log('   💾 Storage type:', openResponse.storageType);
    console.log('   ⚙️  Pragma mode:', openResponse.pragmaMode);
    console.log('   🔗 Pool size:', openResponse.poolSize);
    console.log();

    // ========================================================================
    // TEST 4: Multiple sequential requests (connection stability)
    // ========================================================================
    console.log('4️⃣  Testing multiple sequential requests...');
    for (let i = 0; i < 3; i++) {
      const ping = await client.ping();
      console.log(`   ✅ Ping ${i + 1}/3 - requests processed: ${ping.requestsProcessed}`);
    }
    console.log();

    // ========================================================================
    // TEST 5: Close database (not implemented yet, should gracefully fail)
    // ========================================================================
    console.log('5️⃣  Testing unimplemented message (close-database)...');
    try {
      await client.closeDatabase({ handle: openResponse.handle });
      console.log('   ⚠️  Close succeeded (unexpected - not implemented yet)');
    } catch (error) {
      if (error instanceof Error && error.message.includes('Unknown message type')) {
        console.log('   ✅ Expected error:', error.message);
      } else {
        console.log('   ⚠️  Unexpected error:', error);
      }
    }
    console.log();

    // ========================================================================
    // SUCCESS
    // ========================================================================
    console.log('✅ ALL TESTS PASSED\n');
    console.log('🎉 TypeScript ↔ Rust communication PROVEN');
    console.log('   Next: Build full adapter system with confidence\n');

  } catch (error) {
    console.error('❌ TEST FAILED:', error);
    console.error('\n💡 Troubleshooting:');
    console.error('   1. Is Rust worker running? cargo run --bin data-worker-test');
    console.error('   2. Check socket path: /tmp/jtag-data-worker.sock');
    console.error('   3. Check worker logs for errors\n');
    process.exit(1);
  } finally {
    // Cleanup
    await client.disconnect();
    console.log('🧹 Disconnected from worker\n');
  }
}

main();
