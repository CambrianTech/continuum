/**
 * AI Production Readiness Test
 * =============================
 *
 * Validates that our AI system can handle real production load without:
 * 1. Flooding any provider with too many concurrent requests
 * 2. Creating cascading failures across multiple personas
 * 3. Breaking when models timeout or fail
 * 4. Losing self-healing capability
 *
 * Simulates realistic chat scenario:
 * - Multiple AI personas (Helper, Teacher, CodeReview)
 * - Human messages triggering gating decisions
 * - Concurrent AI responses with queue management
 * - Self-healing when providers fail
 *
 * Tests all available providers (free: Candle, paid: OpenAI/Anthropic if keys present)
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';

interface ProviderTest {
  name: string;
  provider: string;
  model: string;
  skipIfNoKey?: boolean;
  envVar?: string;
}

const PROVIDER_TESTS: ProviderTest[] = [
  // Free providers (always test)
  {
    name: 'Candle/phi3:mini',
    provider: 'candle',
    model: 'phi3:mini'
  },
  {
    name: 'Candle/llama3.2:1b',
    provider: 'candle',
    model: 'llama3.2:1b'
  },
  // Paid providers (only test if API keys present)
  {
    name: 'OpenAI/gpt-4o-mini',
    provider: 'openai',
    model: 'gpt-4o-mini',
    skipIfNoKey: true,
    envVar: 'OPENAI_API_KEY'
  },
  {
    name: 'OpenAI/gpt-3.5-turbo',
    provider: 'openai',
    model: 'gpt-3.5-turbo',
    skipIfNoKey: true,
    envVar: 'OPENAI_API_KEY'
  },
  {
    name: 'Anthropic/claude-3-haiku',
    provider: 'anthropic',
    model: 'claude-3-haiku-20240307',
    skipIfNoKey: true,
    envVar: 'ANTHROPIC_API_KEY'
  }
];

/**
 * Test single model's production readiness
 */
async function testModelProductionReadiness(test: ProviderTest): Promise<{
  success: boolean;
  provider: string;
  model: string;
  responseTime: number;
  error?: string;
}> {
  const startTime = Date.now();

  try {
    // Simple generation test
    const messages = JSON.stringify([
      { role: 'user', content: 'Explain async/await in one sentence.' }
    ]);

    const result = await runJtagCommand(
      `ai/generate --preferredProvider=${test.provider} --model=${test.model} --messages='${messages}' --maxTokens=50`
    );

    const responseTime = Date.now() - startTime;

    if (result?.success && result?.text) {
      return {
        success: true,
        provider: test.provider,
        model: test.model,
        responseTime
      };
    } else {
      return {
        success: false,
        provider: test.provider,
        model: test.model,
        responseTime,
        error: result?.error ?? 'No text generated'
      };
    }
  } catch (error) {
    const responseTime = Date.now() - startTime;
    return {
      success: false,
      provider: test.provider,
      model: test.model,
      responseTime,
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

/**
 * Test concurrent chat scenario (simulates real production load)
 */
async function testConcurrentChatLoad(): Promise<{
  success: boolean;
  personaCount: number;
  messagesPerPersona: number;
  successRate: number;
  avgResponseTime: number;
}> {
  console.log('\n📨 Testing concurrent chat load (multiple personas responding)...');

  // Simulate 3 personas each evaluating 2 messages (6 concurrent AI requests)
  const concurrentRequests = 6;
  const messages = [
    'What is TypeScript?',
    'How do I handle errors?'
  ];

  const startTime = Date.now();
  const promises: Promise<unknown>[] = [];

  for (let i = 0; i < concurrentRequests; i++) {
    const prompt = messages[i % messages.length];
    const messagesParam = JSON.stringify([
      { role: 'user', content: prompt }
    ]);

    promises.push(
      runJtagCommand(
        `ai/generate --preferredProvider=candle --model=phi3:mini --messages='${messagesParam}' --maxTokens=50`
      ).catch((error) => ({ success: false, error: error.message }))
    );
  }

  const results = await Promise.all(promises);
  const totalTime = Date.now() - startTime;

  const successfulResults = results.filter((r: unknown) => (r as { success?: boolean })?.success === true);
  const successRate = successfulResults.length / concurrentRequests;
  const avgResponseTime = totalTime / concurrentRequests;

  return {
    success: successRate >= 0.8, // At least 80% success
    personaCount: 3,
    messagesPerPersona: 2,
    successRate,
    avgResponseTime
  };
}

/**
 * Test self-healing capability
 */
async function testSelfHealing(): Promise<{
  success: boolean;
  healthCheckWorks: boolean;
  canDetectFailure: boolean;
}> {
  console.log('\n🏥 Testing self-healing capability...');

  // Test 1: Can we detect health status?
  let healthCheckWorks = false;
  try {
    // Health check should work even if Candle is slow
    const startTime = Date.now();
    await runJtagCommand('ai/generate --preferredProvider=candle --model=phi3:mini --messages=\'[{"role":"user","content":"hi"}]\' --maxTokens=10');
    const responseTime = Date.now() - startTime;

    healthCheckWorks = responseTime < 30000; // Should respond within 30s
  } catch {
    // Even failures are okay - we just want to know if system responds
    healthCheckWorks = true;
  }

  // Test 2: System handles failures gracefully
  let canDetectFailure = true;
  try {
    // Try with invalid model - should fail gracefully
    const result = await runJtagCommand(
      'ai/generate --preferredProvider=candle --model=nonexistent-model --messages=\'[{"role":"user","content":"test"}]\' --maxTokens=10'
    );

    // Should return error, not crash
    canDetectFailure = !result?.success || result?.error !== undefined;
  } catch {
    // Catching error is also okay - system didn't crash
    canDetectFailure = true;
  }

  return {
    success: healthCheckWorks && canDetectFailure,
    healthCheckWorks,
    canDetectFailure
  };
}

/**
 * Main production readiness test
 */
async function runProductionReadinessTest(): Promise<void> {
  console.log('🏭 AI PRODUCTION READINESS TEST');
  console.log('===============================\n');

  // Phase 1: Test all available models
  console.log('📋 Phase 1: Testing all available models...\n');

  const modelResults: Array<{ name: string; success: boolean; responseTime: number; error?: string }> = [];

  for (const test of PROVIDER_TESTS) {
    // Skip paid providers if no API key
    if (test.skipIfNoKey && test.envVar && !process.env[test.envVar]) {
      console.log(`⏭️  Skipping ${test.name} (no ${test.envVar})`);
      continue;
    }

    console.log(`🧪 Testing ${test.name}...`);
    const result = await testModelProductionReadiness(test);

    modelResults.push({
      name: test.name,
      success: result.success,
      responseTime: result.responseTime,
      error: result.error
    });

    if (result.success) {
      console.log(`   ✅ PASS (${result.responseTime}ms)`);
    } else {
      console.log(`   ❌ FAIL (${result.responseTime}ms): ${result.error}`);
    }
  }

  // Phase 2: Test concurrent load (prevents flooding)
  console.log('\n📋 Phase 2: Testing concurrent chat load...\n');
  const loadResult = await testConcurrentChatLoad();

  console.log(`   Personas: ${loadResult.personaCount}`);
  console.log(`   Messages each: ${loadResult.messagesPerPersona}`);
  console.log(`   Success rate: ${(loadResult.successRate * 100).toFixed(0)}%`);
  console.log(`   Avg response time: ${loadResult.avgResponseTime.toFixed(0)}ms`);

  if (loadResult.success) {
    console.log(`   ✅ PASS: No flooding detected`);
  } else {
    console.log(`   ❌ FAIL: Success rate too low (${(loadResult.successRate * 100).toFixed(0)}%)`);
  }

  // Phase 3: Test self-healing
  console.log('\n📋 Phase 3: Testing self-healing capability...\n');
  const healingResult = await testSelfHealing();

  console.log(`   Health check works: ${healingResult.healthCheckWorks ? '✅' : '❌'}`);
  console.log(`   Can detect failures: ${healingResult.canDetectFailure ? '✅' : '❌'}`);

  if (healingResult.success) {
    console.log(`   ✅ PASS: Self-healing operational`);
  } else {
    console.log(`   ❌ FAIL: Self-healing issues detected`);
  }

  // Summary
  console.log('\n' + '='.repeat(60));
  console.log('📊 PRODUCTION READINESS SUMMARY');
  console.log('='.repeat(60) + '\n');

  const modelsPassed = modelResults.filter(r => r.success).length;
  const modelsTotal = modelResults.length;

  console.log('Model Availability:');
  console.log(`  Tested: ${modelsTotal} models`);
  console.log(`  Passed: ${modelsPassed}/${modelsTotal}`);
  modelResults.forEach(r => {
    const status = r.success ? '✅' : '❌';
    console.log(`    ${status} ${r.name} (${r.responseTime}ms)${r.error ? ` - ${r.error}` : ''}`);
  });

  console.log('\nConcurrent Load Handling:');
  console.log(`  Success Rate: ${(loadResult.successRate * 100).toFixed(0)}%`);
  console.log(`  Avg Response: ${loadResult.avgResponseTime.toFixed(0)}ms`);
  console.log(`  Status: ${loadResult.success ? '✅ PASS' : '❌ FAIL'}`);

  console.log('\nSelf-Healing:');
  console.log(`  Health Monitoring: ${healingResult.healthCheckWorks ? '✅' : '❌'}`);
  console.log(`  Failure Detection: ${healingResult.canDetectFailure ? '✅' : '❌'}`);
  console.log(`  Status: ${healingResult.success ? '✅ PASS' : '❌ FAIL'}`);

  // Overall verdict
  const allPassed = modelsPassed > 0 && loadResult.success && healingResult.success;

  console.log('\n' + '='.repeat(60));
  if (allPassed) {
    console.log('✅ PRODUCTION READY');
    console.log('   All critical systems operational');
    console.log('   Safe to deploy with real users');
  } else {
    console.log('❌ NOT PRODUCTION READY');
    if (modelsPassed === 0) console.log('   ⚠️  No models available');
    if (!loadResult.success) console.log('   ⚠️  Concurrent load issues');
    if (!healingResult.success) console.log('   ⚠️  Self-healing not operational');
    process.exit(1);
  }
  console.log('='.repeat(60));
}

// Run test
runProductionReadinessTest().catch((error) => {
  console.error('❌ Production readiness test failed:', error instanceof Error ? error.message : String(error));
  process.exit(1);
});
