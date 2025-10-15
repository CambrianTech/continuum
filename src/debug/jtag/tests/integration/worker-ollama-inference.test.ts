/**
 * Worker Thread Ollama Inference Test
 * ====================================
 *
 * Tests real Ollama inference with small model (llama3.2:1b).
 * Replaces mock processing with actual AI evaluation.
 *
 * Success Criteria:
 * - Worker initializes OllamaAdapter successfully
 * - Worker completes real inference (3-30s expected)
 * - Worker returns confidence parsed from AI response
 * - Provider health monitoring works
 * - Worker survives Ollama restart (if tested)
 *
 * Phase 3: Verify real inference before scaling to multiple workers
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
 * Check if Ollama is running and has llama3.2:1b model
 */
async function checkOllamaAvailability(): Promise<boolean> {
  try {
    const response = await fetch('http://localhost:11434/api/tags');
    if (!response.ok) {
      return false;
    }
    const data = await response.json();
    const hasModel = data.models?.some((m: any) => m.name.includes('llama3.2:1b'));
    return hasModel;
  } catch (error) {
    return false;
  }
}

/**
 * Scenario 1: Single Real Inference
 * Test that worker completes real Ollama inference
 */
async function testScenario_SingleRealInference(): Promise<TestResult> {
  console.log('\n📋 Scenario 1: Single Real Ollama Inference');
  console.log('='.repeat(60));

  try {
    const worker = new PersonaWorkerThread('test-persona-ollama', {
      providerType: 'ollama',
      providerConfig: {
        baseUrl: 'http://localhost:11434',
        maxConcurrent: 1
      }
    });

    console.log('   Starting worker with OllamaAdapter...');
    await worker.start();

    const message = {
      id: 'test-msg-ollama-001',
      content: 'Should I respond to this technical question about TypeScript?',
      senderId: 'test-user',
      timestamp: Date.now()
    };

    console.log(`   Evaluating message: "${message.content}"`);
    console.log('   ⏳ Waiting for real inference (may take 3-30s)...');
    const startTime = Date.now();

    // Real inference with longer timeout (30s)
    const result = await worker.evaluateMessage(message, 30000);
    const latency = Date.now() - startTime;

    console.log(`   Result: confidence=${result.confidence.toFixed(2)}, shouldRespond=${result.shouldRespond}`);
    console.log(`   Reasoning: ${result.reasoning.substring(0, 100)}...`);
    console.log(`   Processing time: ${result.processingTime}ms (real inference)`);

    // Verify result structure
    const hasCorrectStructure =
      result.messageId === message.id &&
      typeof result.confidence === 'number' &&
      result.confidence >= 0 && result.confidence <= 1 &&
      typeof result.shouldRespond === 'boolean' &&
      typeof result.reasoning === 'string' &&
      typeof result.processingTime === 'number';

    // Real inference should take 100ms-30s (lower bound allows fast models like llama3.2:1b)
    const reasonableTime = latency >= 100 && latency <= 30000;

    const passed = hasCorrectStructure && reasonableTime;

    await worker.shutdown();

    return {
      scenario: 'Single Real Inference',
      passed,
      metrics: { latency },
      notes: passed
        ? `✅ Real inference completed in ${(latency / 1000).toFixed(1)}s with conf=${result.confidence.toFixed(2)}`
        : `❌ ${!hasCorrectStructure ? 'Invalid structure' : `Unreasonable time (${latency}ms)`}`
    };

  } catch (error) {
    return {
      scenario: 'Single Real Inference',
      passed: false,
      metrics: { latency: 0 },
      notes: `❌ Real inference failed: ${error instanceof Error ? error.message : String(error)}`
    };
  }
}

/**
 * Scenario 2: Sequential Real Inferences
 * Test multiple real evaluations in sequence
 */
async function testScenario_SequentialRealInferences(): Promise<TestResult> {
  console.log('\n📋 Scenario 2: Sequential Real Inferences (3 messages)');
  console.log('='.repeat(60));

  try {
    const worker = new PersonaWorkerThread('test-persona-ollama', {
      providerType: 'ollama',
      providerConfig: {
        baseUrl: 'http://localhost:11434',
        maxConcurrent: 1
      }
    });

    await worker.start();

    const messages = [
      { id: 'msg-1', content: 'Should I respond to greetings like "hello"?', senderId: 'user', timestamp: Date.now() },
      { id: 'msg-2', content: 'Should I respond to questions like "What is async/await?"', senderId: 'user', timestamp: Date.now() },
      { id: 'msg-3', content: 'Should I respond to test messages?', senderId: 'test', timestamp: Date.now() }
    ];

    const results: EvaluationResult[] = [];
    const startTime = Date.now();

    console.log('   Processing messages sequentially with real inference...');
    console.log('   ⏳ This may take 10-90 seconds total...');

    for (const message of messages) {
      console.log(`   → Evaluating: "${message.content.substring(0, 50)}..."`);
      const result = await worker.evaluateMessage(message, 30000);
      results.push(result);
      console.log(`     ✓ conf=${result.confidence.toFixed(2)}, shouldRespond=${result.shouldRespond}, took ${(result.processingTime / 1000).toFixed(1)}s`);
    }

    const totalTime = Date.now() - startTime;
    const avgTime = totalTime / messages.length;

    // Verify all results have correct messageIds
    const allCorrect = results.every((result, i) =>
      result.messageId === messages[i].id
    );

    // Real inference: each should take 100ms-30s (lower bound allows fast models like llama3.2:1b)
    const reasonableAvg = avgTime >= 100 && avgTime <= 30000;

    const passed = allCorrect && reasonableAvg;

    await worker.shutdown();

    return {
      scenario: 'Sequential Real Inferences',
      passed,
      metrics: {
        latency: avgTime,
        throughput: messages.length / (totalTime / 1000)
      },
      notes: passed
        ? `✅ Processed ${messages.length} messages, avg ${(avgTime / 1000).toFixed(1)}s each`
        : `❌ ${!allCorrect ? 'MessageId mismatch' : `Unreasonable time (avg ${avgTime}ms)`}`
    };

  } catch (error) {
    return {
      scenario: 'Sequential Real Inferences',
      passed: false,
      metrics: { latency: 0 },
      notes: `❌ Sequential real inference failed: ${error instanceof Error ? error.message : String(error)}`
    };
  }
}

/**
 * Scenario 3: Confidence Variation with Real AI
 * Test that real AI produces varied confidence based on content
 */
async function testScenario_RealConfidenceVariation(): Promise<TestResult> {
  console.log('\n📋 Scenario 3: Real AI Confidence Variation');
  console.log('='.repeat(60));

  try {
    const worker = new PersonaWorkerThread('test-persona-ollama', {
      providerType: 'ollama',
      providerConfig: {
        baseUrl: 'http://localhost:11434',
        maxConcurrent: 1
      }
    });

    await worker.start();

    const messages = [
      { id: 'msg-1', content: 'Should I respond to test messages from test users?', senderId: 'test', timestamp: Date.now() },
      { id: 'msg-2', content: 'Should I respond to important technical questions about TypeScript?', senderId: 'user', timestamp: Date.now() }
    ];

    const results: EvaluationResult[] = [];

    console.log('   Evaluating different message types with real AI...');
    console.log('   ⏳ This may take 6-60 seconds...');

    for (const message of messages) {
      console.log(`   → "${message.content.substring(0, 50)}..."`);
      const result = await worker.evaluateMessage(message, 30000);
      results.push(result);
      console.log(`     ✓ conf=${result.confidence.toFixed(2)}`);
    }

    // Check for confidence variation (not all same)
    const confidences = results.map(r => r.confidence);
    const allSame = confidences.every(c => Math.abs(c - confidences[0]) < 0.01);
    const hasVariation = !allSame;

    // Check reasonable confidence range (0-1)
    const inRange = confidences.every(c => c >= 0 && c <= 1);

    const passed = hasVariation && inRange;

    await worker.shutdown();

    return {
      scenario: 'Real AI Confidence Variation',
      passed,
      metrics: {
        accuracy: hasVariation ? 1.0 : 0.0
      },
      notes: passed
        ? `✅ Real AI confidence varies: ${confidences.map(c => c.toFixed(2)).join(', ')}`
        : `❌ ${!hasVariation ? 'No variation' : 'Out of range'}`
    };

  } catch (error) {
    return {
      scenario: 'Real AI Confidence Variation',
      passed: false,
      metrics: { accuracy: 0 },
      notes: `❌ Real AI confidence test failed: ${error instanceof Error ? error.message : String(error)}`
    };
  }
}

/**
 * Main test runner
 */
async function runOllamaInferenceTests() {
  console.log('\n🧪 WORKER THREAD OLLAMA INFERENCE TEST SUITE');
  console.log('='.repeat(60));
  console.log('Phase 3: Testing real Ollama inference');
  console.log('Replaces mock processing with actual AI evaluation.\n');

  // Check Ollama availability first
  console.log('🔍 Checking Ollama availability...');
  const ollamaAvailable = await checkOllamaAvailability();

  if (!ollamaAvailable) {
    console.error('❌ SKIPPING TESTS: Ollama not running or llama3.2:1b not installed');
    console.error('   Please ensure:');
    console.error('   1. Ollama is running: ollama serve');
    console.error('   2. Model is installed: ollama pull llama3.2:1b');
    process.exit(1);
  }

  console.log('✅ Ollama is available with llama3.2:1b model\n');

  const results: TestResult[] = [];

  try {
    // Run all scenarios
    results.push(await testScenario_SingleRealInference());
    await new Promise(resolve => setTimeout(resolve, 2000));

    results.push(await testScenario_SequentialRealInferences());
    await new Promise(resolve => setTimeout(resolve, 2000));

    results.push(await testScenario_RealConfidenceVariation());

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
    .filter(r => r.metrics.latency !== undefined && r.metrics.latency > 0)
    .reduce((sum, r) => sum + (r.metrics.latency || 0), 0) /
    results.filter(r => r.metrics.latency !== undefined && r.metrics.latency > 0).length;

  if (!isNaN(avgLatency)) {
    console.log(`Average Latency: ${(avgLatency / 1000).toFixed(2)}s (real inference)`);
  }

  // Save results
  const resultsSummary = {
    timestamp: new Date().toISOString(),
    phase: 'Phase 3: Real Ollama Inference',
    passRate: `${passRate}%`,
    passed,
    total,
    metrics: {
      avgLatency: avgLatency ? (avgLatency / 1000).toFixed(2) + 's' : 'N/A'
    },
    details: results
  };

  const fs = await import('fs');
  const path = await import('path');
  const resultsDir = path.join(process.cwd(), '.continuum/sessions/validation');
  const resultsFile = path.join(resultsDir, 'worker-ollama-inference-results-latest.json');

  await fs.promises.mkdir(resultsDir, { recursive: true });
  await fs.promises.writeFile(resultsFile, JSON.stringify(resultsSummary, null, 2));

  console.log('\n💾 Results saved to:', resultsFile);

  console.log('\n' + '='.repeat(60));

  if (passRate === '100') {
    console.log('✅ ALL TESTS PASSED - Ready for Phase 4 (multiple workers)');
    console.log('   Real Ollama inference verified with worker threads');
    process.exit(0);
  } else {
    console.log('❌ SOME TESTS FAILED - Fix real inference before proceeding');
    console.log(`   ${total - passed} test(s) failed`);
    process.exit(1);
  }
}

// Run tests
runOllamaInferenceTests().catch(error => {
  console.error('❌ Test runner failed:', error);
  process.exit(1);
});
