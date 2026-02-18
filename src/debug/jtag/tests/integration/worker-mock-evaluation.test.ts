/**
 * Worker Thread Mock Evaluation Test
 * ====================================
 *
 * Tests message evaluation flow with mock processing.
 * No real AI inference - just verify result structure works.
 *
 * Success Criteria:
 * - Worker receives evaluation request
 * - Worker returns result with correct messageId
 * - Multiple evaluations work in sequence
 * - Processing time reasonable (<500ms for mock)
 * - Timeout handling works
 *
 * Phase 2: Verify evaluation flow before adding real inference
 */

import { PersonaWorkerThread } from '../../shared/workers/PersonaWorkerThread';

interface TestResult {
  scenario: string;
  passed: boolean;
  metrics: {
    latency?: number;
    throughput?: number;
    accuracy?: number;
  };
  notes: string;
}

interface EvaluationResult {
  messageId: string;
  confidence: number;
  shouldRespond: boolean;
  reasoning: string;
  processingTime: number;
}

/**
 * Scenario 1: Single Evaluation
 * Test that worker evaluates message and returns structured result
 */
async function testScenario_SingleEvaluation(): Promise<TestResult> {
  console.log('\n📋 Scenario 1: Single Message Evaluation');
  console.log('='.repeat(60));

  try {
    const worker = new PersonaWorkerThread('test-persona-123');
    await worker.start();

    const message = {
      id: 'test-msg-001',
      content: 'What is TypeScript?',
      senderId: 'test-user',
      timestamp: Date.now()
    };

    console.log(`   Evaluating message: "${message.content}"`);
    const startTime = Date.now();

    const result = await worker.evaluateMessage(message);
    const latency = Date.now() - startTime;

    console.log(`   Result: confidence=${result.confidence}, shouldRespond=${result.shouldRespond}`);
    console.log(`   Reasoning: ${result.reasoning}`);
    console.log(`   Processing time: ${result.processingTime}ms`);

    // Verify result structure
    const hasCorrectStructure =
      result.messageId === message.id &&
      typeof result.confidence === 'number' &&
      result.confidence >= 0 && result.confidence <= 1 &&
      typeof result.shouldRespond === 'boolean' &&
      typeof result.reasoning === 'string' &&
      typeof result.processingTime === 'number';

    const passed = hasCorrectStructure && latency < 1000;

    await worker.shutdown();

    return {
      scenario: 'Single Evaluation',
      passed,
      metrics: { latency },
      notes: passed
        ? `✅ Evaluation returned correct structure in ${latency}ms`
        : `❌ Invalid result structure or too slow (${latency}ms)`
    };

  } catch (error) {
    return {
      scenario: 'Single Evaluation',
      passed: false,
      metrics: { latency: 0 },
      notes: `❌ Evaluation failed: ${error instanceof Error ? error.message : String(error)}`
    };
  }
}

/**
 * Scenario 2: Sequential Evaluations
 * Test multiple evaluations in sequence
 */
async function testScenario_SequentialEvaluations(): Promise<TestResult> {
  console.log('\n📋 Scenario 2: Sequential Evaluations (5 messages)');
  console.log('='.repeat(60));

  try {
    const worker = new PersonaWorkerThread('test-persona-123');
    await worker.start();

    const messages = [
      { id: 'msg-1', content: 'Hello', senderId: 'user', timestamp: Date.now() },
      { id: 'msg-2', content: 'How are you?', senderId: 'user', timestamp: Date.now() },
      { id: 'msg-3', content: 'Explain async/await', senderId: 'user', timestamp: Date.now() },
      { id: 'msg-4', content: 'What is a promise?', senderId: 'user', timestamp: Date.now() },
      { id: 'msg-5', content: 'Goodbye', senderId: 'user', timestamp: Date.now() }
    ];

    const results: EvaluationResult[] = [];
    const startTime = Date.now();

    console.log('   Processing messages sequentially...');
    for (const message of messages) {
      const result = await worker.evaluateMessage(message);
      results.push(result);
      console.log(`   ${message.id}: confidence=${result.confidence.toFixed(2)}, shouldRespond=${result.shouldRespond}`);
    }

    const totalTime = Date.now() - startTime;
    const avgTime = totalTime / messages.length;

    // Verify all results have correct messageIds
    const allCorrect = results.every((result, i) =>
      result.messageId === messages[i].id
    );

    const passed = allCorrect && avgTime < 500;

    await worker.shutdown();

    return {
      scenario: 'Sequential Evaluations',
      passed,
      metrics: {
        latency: avgTime,
        throughput: messages.length / (totalTime / 1000)
      },
      notes: passed
        ? `✅ Processed ${messages.length} messages, avg ${avgTime.toFixed(0)}ms each`
        : `❌ ${allCorrect ? 'Too slow' : 'MessageId mismatch'} (avg ${avgTime.toFixed(0)}ms)`
    };

  } catch (error) {
    return {
      scenario: 'Sequential Evaluations',
      passed: false,
      metrics: { latency: 0 },
      notes: `❌ Sequential evaluation failed: ${error instanceof Error ? error.message : String(error)}`
    };
  }
}

/**
 * Scenario 3: Confidence Variation
 * Test that mock evaluation varies confidence based on content
 */
async function testScenario_ConfidenceVariation(): Promise<TestResult> {
  console.log('\n📋 Scenario 3: Confidence Variation');
  console.log('='.repeat(60));

  try {
    const worker = new PersonaWorkerThread('test-persona-123');
    await worker.start();

    const messages = [
      { id: 'msg-1', content: 'test message', senderId: 'test', timestamp: Date.now() },
      { id: 'msg-2', content: 'What is TypeScript?', senderId: 'user', timestamp: Date.now() },
      { id: 'msg-3', content: 'Explain async programming', senderId: 'user', timestamp: Date.now() }
    ];

    const results: EvaluationResult[] = [];

    console.log('   Evaluating different message types...');
    for (const message of messages) {
      const result = await worker.evaluateMessage(message);
      results.push(result);
      console.log(`   "${message.content.substring(0, 30)}": conf=${result.confidence.toFixed(2)}`);
    }

    // Check for confidence variation (not all same)
    const confidences = results.map(r => r.confidence);
    const allSame = confidences.every(c => c === confidences[0]);
    const hasVariation = !allSame;

    // Check reasonable confidence range (0-1)
    const inRange = confidences.every(c => c >= 0 && c <= 1);

    const passed = hasVariation && inRange;

    await worker.shutdown();

    return {
      scenario: 'Confidence Variation',
      passed,
      metrics: {
        accuracy: hasVariation ? 1.0 : 0.0
      },
      notes: passed
        ? `✅ Confidence varies naturally: ${confidences.map(c => c.toFixed(2)).join(', ')}`
        : `❌ ${!hasVariation ? 'No variation' : 'Out of range'}`
    };

  } catch (error) {
    return {
      scenario: 'Confidence Variation',
      passed: false,
      metrics: { accuracy: 0 },
      notes: `❌ Confidence test failed: ${error instanceof Error ? error.message : String(error)}`
    };
  }
}

/**
 * Scenario 4: Timeout Handling
 * Test that evaluation respects timeout
 */
async function testScenario_TimeoutHandling(): Promise<TestResult> {
  console.log('\n📋 Scenario 4: Timeout Handling');
  console.log('='.repeat(60));

  try {
    const worker = new PersonaWorkerThread('test-persona-123');
    await worker.start();

    const message = {
      id: 'msg-timeout',
      content: 'This should timeout',
      senderId: 'user',
      timestamp: Date.now()
    };

    console.log('   Testing timeout with 1s limit...');
    const startTime = Date.now();

    try {
      // This should complete within timeout for mock (100-500ms)
      const result = await worker.evaluateMessage(message, 1000);
      const elapsed = Date.now() - startTime;

      const passed = elapsed < 1000;

      await worker.shutdown();

      return {
        scenario: 'Timeout Handling',
        passed,
        metrics: { latency: elapsed },
        notes: passed
          ? `✅ Completed within timeout (${elapsed}ms)`
          : `❌ Too slow (${elapsed}ms > 1000ms)`
      };

    } catch (timeoutError) {
      // If it times out, that's also valid behavior to test
      const elapsed = Date.now() - startTime;

      await worker.shutdown();

      return {
        scenario: 'Timeout Handling',
        passed: false,
        metrics: { latency: elapsed },
        notes: `❌ Unexpected timeout: ${timeoutError instanceof Error ? timeoutError.message : String(timeoutError)}`
      };
    }

  } catch (error) {
    return {
      scenario: 'Timeout Handling',
      passed: false,
      metrics: { latency: 0 },
      notes: `❌ Timeout test failed: ${error instanceof Error ? error.message : String(error)}`
    };
  }
}

/**
 * Main test runner
 */
async function runMockEvaluationTests() {
  console.log('\n🧪 WORKER THREAD MOCK EVALUATION TEST SUITE');
  console.log('='.repeat(60));
  console.log('Phase 2: Testing evaluation flow (mock processing)');
  console.log('Verifies result structure before adding real Candle inference.\n');

  const results: TestResult[] = [];

  try {
    // Run all scenarios
    results.push(await testScenario_SingleEvaluation());
    await new Promise(resolve => setTimeout(resolve, 1000));

    results.push(await testScenario_SequentialEvaluations());
    await new Promise(resolve => setTimeout(resolve, 1000));

    results.push(await testScenario_ConfidenceVariation());
    await new Promise(resolve => setTimeout(resolve, 1000));

    results.push(await testScenario_TimeoutHandling());

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

  if (!isNaN(avgLatency)) {
    console.log(`Average Latency: ${avgLatency.toFixed(2)}ms`);
  }

  // Save results
  const resultsSummary = {
    timestamp: new Date().toISOString(),
    phase: 'Phase 2: Mock Evaluation',
    passRate: `${passRate}%`,
    passed,
    total,
    metrics: {
      avgLatency: avgLatency.toFixed(2)
    },
    details: results
  };

  const fs = await import('fs');
  const path = await import('path');
  const resultsDir = path.join(process.cwd(), '.continuum/sessions/validation');
  const resultsFile = path.join(resultsDir, 'worker-mock-evaluation-results-latest.json');

  await fs.promises.mkdir(resultsDir, { recursive: true });
  await fs.promises.writeFile(resultsFile, JSON.stringify(resultsSummary, null, 2));

  console.log('\n💾 Results saved to:', resultsFile);

  console.log('\n' + '='.repeat(60));

  if (passRate === '100') {
    console.log('✅ ALL TESTS PASSED - Ready for Phase 3 (real inference)');
    console.log('   Evaluation flow verified with mock processing');
    process.exit(0);
  } else {
    console.log('❌ SOME TESTS FAILED - Fix evaluation flow before proceeding');
    console.log(`   ${total - passed} test(s) failed`);
    process.exit(1);
  }
}

// Run tests
runMockEvaluationTests().catch(error => {
  console.error('❌ Test runner failed:', error);
  process.exit(1);
});
