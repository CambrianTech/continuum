/**
 * Worker Thread Skeleton Integration Test
 * =========================================
 *
 * Tests bidirectional communication, latency, and reliability
 * of PersonaUser worker threads.
 *
 * Success Criteria:
 * - Worker starts reliably (<5s)
 * - Ping-pong latency <10ms
 * - Multiple rapid pings without errors
 * - Clean shutdown without hangs
 *
 * This is Phase 1: THE HARD PART (threading/IPC)
 * Once this passes, everything else is easy normal code.
 */

import { PersonaWorkerThread } from '../../shared/workers/PersonaWorkerThread';

interface TestResult {
  scenario: string;
  passed: boolean;
  metrics: {
    latency?: number;
    throughput?: number;
    errorRate?: number;
  };
  notes: string;
}

/**
 * Scenario 1: Worker Startup
 * Test that worker starts and signals ready within 5 seconds
 */
async function testScenario_WorkerStartup(): Promise<TestResult> {
  console.log('\n📋 Scenario 1: Worker Startup');
  console.log('='.repeat(60));

  const startTime = Date.now();

  try {
    // Create worker
    const worker = new PersonaWorkerThread('test-persona-123');

    // Wait for ready signal (should complete within 5s)
    await worker.start();

    const startupTime = Date.now() - startTime;
    const passed = startupTime < 5000;

    // Clean up
    await worker.shutdown();

    return {
      scenario: 'Worker Startup',
      passed,
      metrics: { latency: startupTime },
      notes: passed
        ? `✅ Worker started in ${startupTime}ms`
        : `❌ Worker took ${startupTime}ms (>5s limit)`
    };

  } catch (error) {
    return {
      scenario: 'Worker Startup',
      passed: false,
      metrics: { latency: Date.now() - startTime },
      notes: `❌ Startup failed: ${error instanceof Error ? error.message : String(error)}`
    };
  }
}

/**
 * Scenario 2: Ping-Pong Communication
 * Test bidirectional message passing with 10 ping-pong exchanges
 */
async function testScenario_PingPong(): Promise<TestResult> {
  console.log('\n📋 Scenario 2: Ping-Pong Communication');
  console.log('='.repeat(60));

  try {
    const worker = new PersonaWorkerThread('test-persona-123');
    await worker.start();

    const latencies: number[] = [];

    // Test 10 pings
    console.log('   Sending 10 pings...');
    for (let i = 0; i < 10; i++) {
      const latency = await worker.ping();
      latencies.push(latency);
      console.log(`   Ping ${i + 1}: ${latency}ms`);
    }

    const avgLatency = latencies.reduce((a, b) => a + b, 0) / latencies.length;
    const maxLatency = Math.max(...latencies);
    const minLatency = Math.min(...latencies);
    const passed = avgLatency < 10;

    await worker.shutdown();

    return {
      scenario: 'Ping-Pong Communication',
      passed,
      metrics: {
        latency: avgLatency,
        throughput: 10 / (latencies.reduce((a, b) => a + b, 0) / 1000)
      },
      notes: passed
        ? `✅ Avg: ${avgLatency.toFixed(2)}ms, Min: ${minLatency}ms, Max: ${maxLatency}ms`
        : `❌ Avg latency ${avgLatency.toFixed(2)}ms (>10ms limit)`
    };

  } catch (error) {
    return {
      scenario: 'Ping-Pong Communication',
      passed: false,
      metrics: { latency: 0 },
      notes: `❌ Ping-pong failed: ${error instanceof Error ? error.message : String(error)}`
    };
  }
}

/**
 * Scenario 3: Rapid Fire Stress Test
 * Send 100 pings concurrently to test queue handling and stability
 */
async function testScenario_RapidFire(): Promise<TestResult> {
  console.log('\n📋 Scenario 3: Rapid Fire Stress Test (100 concurrent pings)');
  console.log('='.repeat(60));

  try {
    const worker = new PersonaWorkerThread('test-persona-123');
    await worker.start();

    const startTime = Date.now();
    const promises = [];

    console.log('   Sending 100 pings concurrently...');

    // Send 100 pings concurrently
    for (let i = 0; i < 100; i++) {
      promises.push(worker.ping().catch(() => -1));
    }

    const results = await Promise.all(promises);
    const elapsed = Date.now() - startTime;

    const errorCount = results.filter(r => r === -1).length;
    const successCount = results.filter(r => r !== -1).length;
    const errorRate = errorCount / results.length;
    const avgLatency = successCount > 0
      ? results.filter(r => r !== -1).reduce((a, b) => a + b, 0) / successCount
      : 0;
    const passed = errorRate < 0.01; // <1% error rate

    await worker.shutdown();

    return {
      scenario: 'Rapid Fire Stress Test',
      passed,
      metrics: {
        throughput: 100 / (elapsed / 1000),
        errorRate,
        latency: avgLatency
      },
      notes: passed
        ? `✅ ${successCount}/100 successful, ${(errorRate * 100).toFixed(1)}% errors, ${(100 / (elapsed / 1000)).toFixed(1)} pings/sec`
        : `❌ ${errorCount}/100 errors (${(errorRate * 100).toFixed(1)}% >1% limit)`
    };

  } catch (error) {
    return {
      scenario: 'Rapid Fire Stress Test',
      passed: false,
      metrics: { throughput: 0, errorRate: 1 },
      notes: `❌ Stress test failed: ${error instanceof Error ? error.message : String(error)}`
    };
  }
}

/**
 * Scenario 4: Clean Shutdown
 * Test that worker terminates cleanly without hanging
 */
async function testScenario_CleanShutdown(): Promise<TestResult> {
  console.log('\n📋 Scenario 4: Clean Shutdown');
  console.log('='.repeat(60));

  try {
    const worker = new PersonaWorkerThread('test-persona-123');
    await worker.start();

    console.log('   Sending shutdown signal...');
    const startTime = Date.now();
    await worker.shutdown();
    const shutdownTime = Date.now() - startTime;

    const passed = shutdownTime < 1000;

    return {
      scenario: 'Clean Shutdown',
      passed,
      metrics: { latency: shutdownTime },
      notes: passed
        ? `✅ Shutdown in ${shutdownTime}ms`
        : `❌ Shutdown took ${shutdownTime}ms (>1s limit)`
    };

  } catch (error) {
    return {
      scenario: 'Clean Shutdown',
      passed: false,
      metrics: { latency: 0 },
      notes: `❌ Shutdown failed: ${error instanceof Error ? error.message : String(error)}`
    };
  }
}

/**
 * Main test runner
 */
async function runWorkerSkeletonTests() {
  console.log('\n🧪 WORKER THREAD SKELETON TEST SUITE');
  console.log('='.repeat(60));
  console.log('Phase 1: Testing bidirectional communication (THE HARD PART)');
  console.log('Once this passes, everything else is easy normal code.\n');

  const results: TestResult[] = [];

  try {
    // Run all scenarios
    results.push(await testScenario_WorkerStartup());
    await new Promise(resolve => setTimeout(resolve, 1000));

    results.push(await testScenario_PingPong());
    await new Promise(resolve => setTimeout(resolve, 1000));

    results.push(await testScenario_RapidFire());
    await new Promise(resolve => setTimeout(resolve, 1000));

    results.push(await testScenario_CleanShutdown());

  } catch (error) {
    console.error('\n❌ Test suite failed with exception:', error);
    process.exit(1);
  }

  // Summary
  console.log('\n\n📊 TEST RESULTS SUMMARY');
  console.log('='.repeat(60));

  const passed = results.filter(r => r.passed).length;
  const total = results.length;
  const passRate = (passed / total * 100).toFixed(0);

  results.forEach(r => {
    const status = r.passed ? '✅' : '❌';
    console.log(`${status} ${r.scenario}`);
    console.log(`   ${r.notes}`);
  });

  console.log('\n📈 AGGREGATE METRICS');
  console.log('='.repeat(60));
  console.log(`Pass Rate: ${passed}/${total} (${passRate}%)`);

  // Calculate aggregate metrics
  const avgLatency = results
    .filter(r => r.metrics.latency !== undefined)
    .reduce((sum, r) => sum + (r.metrics.latency || 0), 0) /
    results.filter(r => r.metrics.latency !== undefined).length;

  const avgThroughput = results
    .filter(r => r.metrics.throughput !== undefined)
    .reduce((sum, r) => sum + (r.metrics.throughput || 0), 0) /
    results.filter(r => r.metrics.throughput !== undefined).length;

  if (!isNaN(avgLatency)) {
    console.log(`Average Latency: ${avgLatency.toFixed(2)}ms`);
  }
  if (!isNaN(avgThroughput)) {
    console.log(`Average Throughput: ${avgThroughput.toFixed(1)} ops/sec`);
  }

  // Save results for comparison
  const resultsSummary = {
    timestamp: new Date().toISOString(),
    phase: 'Phase 1: Skeleton Communication',
    passRate: `${passRate}%`,
    passed,
    total,
    metrics: {
      avgLatency: avgLatency.toFixed(2),
      avgThroughput: avgThroughput.toFixed(1)
    },
    details: results
  };

  const fs = await import('fs');
  const path = await import('path');
  const resultsDir = path.join(process.cwd(), '.continuum/sessions/validation');
  const resultsFile = path.join(resultsDir, 'worker-skeleton-results-latest.json');

  await fs.promises.mkdir(resultsDir, { recursive: true });
  await fs.promises.writeFile(resultsFile, JSON.stringify(resultsSummary, null, 2));

  console.log('\n💾 Results saved to:', resultsFile);

  console.log('\n' + '='.repeat(60));

  if (passRate === '100') {
    console.log('✅ ALL TESTS PASSED - THE HARD PART IS DONE!');
    console.log('   Ready to proceed to Phase 2 (mock evaluation)');
    console.log('   Everything from here is easy normal code.');
    process.exit(0);
  } else {
    console.log('❌ SOME TESTS FAILED - Fix threading/IPC issues before proceeding');
    console.log(`   ${total - passed} test(s) failed`);
    process.exit(1);
  }
}

// Run tests
runWorkerSkeletonTests().catch(error => {
  console.error('❌ Test runner failed:', error);
  process.exit(1);
});
