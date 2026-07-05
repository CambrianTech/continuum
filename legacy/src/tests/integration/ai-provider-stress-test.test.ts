/**
 * AI Provider Stress Test - Performance & Reliability Under Load
 * ==============================================================
 *
 * Configurable stress testing for any AI provider (Candle, OpenAI, Anthropic, etc.)
 * Tests concurrency limits, timeout rates, response times, and throughput.
 *
 * Usage:
 *   npx tsx tests/integration/ai-provider-stress-test.test.ts
 *   TEST_PROVIDER=openai npx tsx tests/integration/ai-provider-stress-test.test.ts
 *   TEST_CONCURRENCY=10 npx tsx tests/integration/ai-provider-stress-test.test.ts
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';

// Test configuration from environment or defaults
interface StressTestConfig {
  provider: string;
  model: string;
  concurrentRequests: number;
  totalRequests: number;
  temperature: number;
  maxTokens: number;
  timeoutMs: number;
}

// Test result metrics
interface RequestMetrics {
  requestId: string;
  prompt: string;
  startTime: number;
  endTime: number;
  duration: number;
  success: boolean;
  isTimeout: boolean;
  error?: string;
  tokensGenerated?: number;
}

interface StressTestResults {
  config: StressTestConfig;
  totalRequests: number;
  successfulRequests: number;
  failedRequests: number;
  timeoutRequests: number;
  totalDuration: number;
  averageResponseTime: number;
  minResponseTime: number;
  maxResponseTime: number;
  p50ResponseTime: number;
  p95ResponseTime: number;
  p99ResponseTime: number;
  successRate: number;
  timeoutRate: number;
  requestsPerSecond: number;
  metrics: RequestMetrics[];
}

// Provider-specific configurations
const PROVIDER_CONFIGS: Record<string, Partial<StressTestConfig>> = {
  candle: {
    provider: 'candle',
    model: 'phi3:mini',
    concurrentRequests: 6,
    totalRequests: 12,
    temperature: 0.7,
    maxTokens: 50,
    timeoutMs: 30000
  },
  openai: {
    provider: 'openai',
    model: 'gpt-4o-mini',
    concurrentRequests: 10,
    totalRequests: 20,
    temperature: 0.7,
    maxTokens: 50,
    timeoutMs: 30000
  },
  anthropic: {
    provider: 'anthropic',
    model: 'claude-3-haiku-20240307',
    concurrentRequests: 10,
    totalRequests: 20,
    temperature: 0.7,
    maxTokens: 50,
    timeoutMs: 30000
  }
};

// Diverse test prompts to simulate real usage
const TEST_PROMPTS = [
  'What is TypeScript?',
  'Explain async/await in JavaScript',
  'What are promises and how do they work?',
  'Define REST API architecture',
  'What is GraphQL and when should I use it?',
  'Explain microservices design patterns',
  'How does OAuth 2.0 authentication work?',
  'What is the difference between SQL and NoSQL?',
  'Explain Docker containerization benefits',
  'What are WebSockets used for?',
  'How does JWT token authentication work?',
  'What is the purpose of Redis caching?'
];

/**
 * Calculate percentile from sorted array
 */
function calculatePercentile(sortedValues: number[], percentile: number): number {
  if (sortedValues.length === 0) return 0;
  const index = Math.ceil((percentile / 100) * sortedValues.length) - 1;
  return sortedValues[Math.max(0, index)];
}

/**
 * Run single AI generation request with timing
 */
async function runTimedRequest(
  config: StressTestConfig,
  prompt: string,
  requestIndex: number
): Promise<RequestMetrics> {
  const requestId = `stress-${Date.now()}-${requestIndex}`;
  const startTime = Date.now();

  try {
    // Build messages parameter as required by ai/generate command
    const messages = JSON.stringify([
      { role: 'user', content: prompt }
    ]);

    const result = await runJtagCommand(
      `ai/generate --preferredProvider=${config.provider} --model=${config.model} --messages='${messages}' --temperature=${config.temperature} --maxTokens=${config.maxTokens}`
    );

    const endTime = Date.now();
    const duration = endTime - startTime;

    // Check if request succeeded
    const success = Boolean(result?.success && result?.text);
    const isTimeout = duration >= config.timeoutMs;

    return {
      requestId,
      prompt: prompt.substring(0, 50),
      startTime,
      endTime,
      duration,
      success,
      isTimeout,
      tokensGenerated: result?.usage?.outputTokens as number | undefined
    };
  } catch (error) {
    const endTime = Date.now();
    const duration = endTime - startTime;
    const errorMsg = error instanceof Error ? error.message : String(error);

    return {
      requestId,
      prompt: prompt.substring(0, 50),
      startTime,
      endTime,
      duration,
      success: false,
      isTimeout: errorMsg.includes('timeout') || duration >= config.timeoutMs,
      error: errorMsg
    };
  }
}

/**
 * Run batch of concurrent requests
 */
async function runConcurrentBatch(
  config: StressTestConfig,
  prompts: string[],
  startIndex: number
): Promise<RequestMetrics[]> {
  const batchPromises = prompts.map((prompt, idx) =>
    runTimedRequest(config, prompt, startIndex + idx)
  );

  return Promise.all(batchPromises);
}

/**
 * Analyze results and calculate statistics
 */
function analyzeResults(
  config: StressTestConfig,
  metrics: RequestMetrics[],
  totalDuration: number
): StressTestResults {
  const successfulRequests = metrics.filter((m) => m.success).length;
  const failedRequests = metrics.filter((m) => !m.success).length;
  const timeoutRequests = metrics.filter((m) => m.isTimeout).length;

  // Calculate response time statistics
  const responseTimes = metrics.map((m) => m.duration).sort((a, b) => a - b);
  const averageResponseTime =
    responseTimes.reduce((sum, t) => sum + t, 0) / responseTimes.length;
  const minResponseTime = responseTimes[0] ?? 0;
  const maxResponseTime = responseTimes[responseTimes.length - 1] ?? 0;

  const p50ResponseTime = calculatePercentile(responseTimes, 50);
  const p95ResponseTime = calculatePercentile(responseTimes, 95);
  const p99ResponseTime = calculatePercentile(responseTimes, 99);

  const successRate = successfulRequests / metrics.length;
  const timeoutRate = timeoutRequests / metrics.length;
  const requestsPerSecond = (metrics.length / totalDuration) * 1000;

  return {
    config,
    totalRequests: metrics.length,
    successfulRequests,
    failedRequests,
    timeoutRequests,
    totalDuration,
    averageResponseTime,
    minResponseTime,
    maxResponseTime,
    p50ResponseTime,
    p95ResponseTime,
    p99ResponseTime,
    successRate,
    timeoutRate,
    requestsPerSecond,
    metrics
  };
}

/**
 * Print detailed test results
 */
function printResults(results: StressTestResults): void {
  console.log('\n📊 STRESS TEST RESULTS');
  console.log('=====================\n');

  console.log('Configuration:');
  console.log(`  Provider: ${results.config.provider}`);
  console.log(`  Model: ${results.config.model}`);
  console.log(`  Concurrency: ${results.config.concurrentRequests}`);
  console.log(`  Total Requests: ${results.totalRequests}`);
  console.log(`  Max Tokens: ${results.config.maxTokens}`);
  console.log(`  Timeout: ${results.config.timeoutMs}ms\n`);

  console.log('Request Results:');
  console.log(`  Total: ${results.totalRequests}`);
  console.log(`  Successful: ${results.successfulRequests} (${(results.successRate * 100).toFixed(1)}%)`);
  console.log(`  Failed: ${results.failedRequests} (${((results.failedRequests / results.totalRequests) * 100).toFixed(1)}%)`);
  console.log(`  Timeouts: ${results.timeoutRequests} (${(results.timeoutRate * 100).toFixed(1)}%)\n`);

  console.log('Response Time Statistics:');
  console.log(`  Average: ${results.averageResponseTime.toFixed(0)}ms`);
  console.log(`  Min: ${results.minResponseTime.toFixed(0)}ms`);
  console.log(`  Max: ${results.maxResponseTime.toFixed(0)}ms`);
  console.log(`  P50 (median): ${results.p50ResponseTime.toFixed(0)}ms`);
  console.log(`  P95: ${results.p95ResponseTime.toFixed(0)}ms`);
  console.log(`  P99: ${results.p99ResponseTime.toFixed(0)}ms\n`);

  console.log('Throughput:');
  console.log(`  Total Duration: ${results.totalDuration.toFixed(0)}ms`);
  console.log(`  Requests/sec: ${results.requestsPerSecond.toFixed(2)}\n`);

  // Status indicators
  if (results.timeoutRate === 0) {
    console.log('✅ EXCELLENT: Zero timeout rate');
  } else if (results.timeoutRate < 0.1) {
    console.log('✅ GOOD: Low timeout rate (<10%)');
  } else if (results.timeoutRate < 0.3) {
    console.log('⚠️  WARNING: Moderate timeout rate (10-30%)');
  } else {
    console.log('❌ CRITICAL: High timeout rate (>30%)');
  }

  if (results.successRate >= 0.95) {
    console.log('✅ EXCELLENT: High success rate (≥95%)');
  } else if (results.successRate >= 0.8) {
    console.log('⚠️  WARNING: Acceptable success rate (80-95%)');
  } else {
    console.log('❌ CRITICAL: Low success rate (<80%)');
  }

  if (results.p95ResponseTime < 5000) {
    console.log('✅ EXCELLENT: Fast P95 response time (<5s)');
  } else if (results.p95ResponseTime < 10000) {
    console.log('⚠️  WARNING: Acceptable P95 response time (5-10s)');
  } else {
    console.log('❌ CRITICAL: Slow P95 response time (>10s)');
  }

  // Failed request details
  if (results.failedRequests > 0) {
    console.log('\n❌ Failed Requests:');
    const failedMetrics = results.metrics.filter((m) => !m.success);
    failedMetrics.forEach((metric, idx) => {
      console.log(`  ${idx + 1}. [${metric.duration}ms] ${metric.prompt}...`);
      if (metric.error) {
        console.log(`     Error: ${metric.error}`);
      }
    });
  }
}

/**
 * Run single provider/model stress test
 */
async function runSingleStressTest(config: StressTestConfig): Promise<StressTestResults> {
  console.log('\nTesting:', config.provider, '-', config.model);
  console.log(`Concurrency: ${config.concurrentRequests} | Total: ${config.totalRequests}\n`);

  const allMetrics: RequestMetrics[] = [];
  const testStartTime = Date.now();

  // Run requests in batches of configured concurrency
  for (let i = 0; i < config.totalRequests; i += config.concurrentRequests) {
    const batchSize = Math.min(config.concurrentRequests, config.totalRequests - i);
    const batchPrompts = TEST_PROMPTS.slice(i % TEST_PROMPTS.length, (i % TEST_PROMPTS.length) + batchSize);

    // Pad if needed
    while (batchPrompts.length < batchSize) {
      batchPrompts.push(TEST_PROMPTS[batchPrompts.length % TEST_PROMPTS.length]);
    }

    console.log(`📤 Batch ${Math.floor(i / config.concurrentRequests) + 1}: Sending ${batchSize} concurrent requests...`);

    const batchMetrics = await runConcurrentBatch(config, batchPrompts, i);
    allMetrics.push(...batchMetrics);

    const batchSuccess = batchMetrics.filter((m) => m.success).length;
    console.log(`   Completed: ${batchSuccess}/${batchSize} successful`);
  }

  const totalDuration = Date.now() - testStartTime;

  // Analyze results
  return analyzeResults(config, allMetrics, totalDuration);
}

/**
 * Main stress test runner
 */
async function runStressTest(): Promise<void> {
  console.log('🧪 AI PROVIDER STRESS TEST');
  console.log('=========================\n');

  // Check if multi-model test mode
  const testAll = process.env.TEST_ALL === 'true';

  if (testAll) {
    // Test all configured providers/models
    console.log('🔥 MULTI-MODEL STRESS TEST MODE');
    console.log('Testing all configured providers...\n');

    const allResults: StressTestResults[] = [];

    for (const [name, configDefaults] of Object.entries(PROVIDER_CONFIGS)) {
      console.log(`\n${'='.repeat(50)}`);
      console.log(`Testing: ${name.toUpperCase()}`);
      console.log('='.repeat(50));

      const config: StressTestConfig = {
        provider: configDefaults.provider ?? 'candle',
        model: configDefaults.model ?? 'phi3:mini',
        concurrentRequests: configDefaults.concurrentRequests ?? 6,
        totalRequests: configDefaults.totalRequests ?? 12,
        temperature: configDefaults.temperature ?? 0.7,
        maxTokens: configDefaults.maxTokens ?? 50,
        timeoutMs: configDefaults.timeoutMs ?? 30000
      };

      try {
        const results = await runSingleStressTest(config);
        allResults.push(results);
        printResults(results);
      } catch (error) {
        console.log(`\n❌ ${name} test failed: ${error instanceof Error ? error.message : String(error)}`);
      }
    }

    // Print comparison summary
    console.log('\n\n' + '='.repeat(60));
    console.log('📊 MULTI-MODEL COMPARISON');
    console.log('='.repeat(60) + '\n');

    console.log('Provider/Model              | Success | Timeout | Avg Time | P95 Time | RPS');
    console.log('-'.repeat(85));

    for (const result of allResults) {
      const label = `${result.config.provider}/${result.config.model}`.padEnd(27);
      const success = `${(result.successRate * 100).toFixed(0)}%`.padStart(7);
      const timeout = `${(result.timeoutRate * 100).toFixed(0)}%`.padStart(7);
      const avgTime = `${result.averageResponseTime.toFixed(0)}ms`.padStart(8);
      const p95Time = `${result.p95ResponseTime.toFixed(0)}ms`.padStart(8);
      const rps = result.requestsPerSecond.toFixed(2).padStart(5);

      console.log(`${label} | ${success} | ${timeout} | ${avgTime} | ${p95Time} | ${rps}`);
    }

    // Overall pass/fail
    const allPassed = allResults.every(r => r.successRate >= 0.8 && r.timeoutRate <= 0.3);
    if (allPassed) {
      console.log('\n✅ ALL PROVIDERS PASSED');
    } else {
      console.log('\n❌ SOME PROVIDERS FAILED');
      process.exit(1);
    }

  } else {
    // Single provider test mode
    const providerName = process.env.TEST_PROVIDER ?? 'candle';
    const defaultConfig = PROVIDER_CONFIGS[providerName] ?? PROVIDER_CONFIGS.candle;

    const config: StressTestConfig = {
      provider: process.env.TEST_PROVIDER ?? defaultConfig.provider ?? 'candle',
      model: process.env.TEST_MODEL ?? defaultConfig.model ?? 'phi3:mini',
      concurrentRequests: parseInt(process.env.TEST_CONCURRENCY ?? String(defaultConfig.concurrentRequests ?? 6)),
      totalRequests: parseInt(process.env.TEST_TOTAL ?? String(defaultConfig.totalRequests ?? 12)),
      temperature: parseFloat(process.env.TEST_TEMPERATURE ?? String(defaultConfig.temperature ?? 0.7)),
      maxTokens: parseInt(process.env.TEST_MAX_TOKENS ?? String(defaultConfig.maxTokens ?? 50)),
      timeoutMs: parseInt(process.env.TEST_TIMEOUT ?? String(defaultConfig.timeoutMs ?? 30000))
    };

    const results = await runSingleStressTest(config);
    printResults(results);

    // Exit with appropriate code
    if (results.successRate < 0.8 || results.timeoutRate > 0.3) {
      console.log('\n❌ TEST FAILED: Performance below acceptable thresholds');
      process.exit(1);
    } else {
      console.log('\n✅ TEST PASSED: Performance within acceptable thresholds');
    }
  }
}

// Run test
runStressTest().catch((error) => {
  console.error('❌ Stress test failed:', error instanceof Error ? error.message : String(error));
  process.exit(1);
});
