/**
 * Grid Distributed Extreme Capacity Integration Test
 * 
 * STRICT TYPING PROTOCOLS - NO `any` TYPES, NO MAGIC STRINGS
 * Uses actual API constants and types to prevent test failures from
 * improper constants. Ready for ERROR-level linting enforcement.
 * 
 * Tests distributed Grid capacity under extreme load while measuring
 * performance and generating optimization recommendations.
 */

import * as path from 'path';
import { 
  GridDistributedTester, 
  GridTestNodeFactory, 
  GRID_TEST_CONSTANTS
} from '../shared/GridTestFramework';
import { PerformanceTester } from '../shared/PerformanceTester';

// Import types differently for compatibility
import type { 
  GridTestConfig,
  GridTestResult
} from '../shared/GridTestFramework';
import type { TestScorecard } from '../shared/PerformanceTester';

// Strict typing - NO magic numbers or strings
const EXTREME_TEST_CONFIG: Readonly<GridTestConfig> = {
  nodeCount: 5,
  messagesPerNode: 100, 
  testTimeoutMs: 180000, // 3 minutes
  enableBroadcastTest: true,
  enablePartitionTest: true,
  performanceLogDir: path.resolve(__dirname, '../../.continuum/jtag/performance')
} as const;

// Performance thresholds - strictly typed constants
const PERFORMANCE_THRESHOLDS = {
  MAX_AVERAGE_LATENCY_MS: 200,
  MIN_SUCCESS_RATE_PERCENT: 95,
  MIN_OVERALL_SCORE: 70,
  MAX_MEMORY_GROWTH_MB: 150,
  MIN_THROUGHPUT_OPS_PER_SEC: 50
} as const;

// Test grades mapping - strict typing
const ACCEPTABLE_GRADES = ['A', 'B', 'C'] as const;
type AcceptableGrade = typeof ACCEPTABLE_GRADES[number];

describe('🌐 Grid Distributed Extreme Capacity Integration Tests', () => {
  let tester: GridDistributedTester;
  let testResult: GridTestResult;
  let scorecard: TestScorecard;

  beforeEach(() => {
    // Create tester with strict configuration typing
    tester = new GridDistributedTester(
      EXTREME_TEST_CONFIG,
      EXTREME_TEST_CONFIG.performanceLogDir
    );
  });

  afterEach(async () => {
    // Ensure cleanup with proper error handling
    if (tester) {
      try {
        // Cleanup handled internally by tester
      } catch (error) {
        console.warn(`Test cleanup warning: ${error}`);
      }
    }
  });

  test('Extreme distributed Grid capacity with comprehensive performance analysis', async () => {
    console.log('🎯 GRID EXTREME DISTRIBUTED CAPACITY TEST');
    console.log('════════════════════════════════════════════════════════════');
    console.log(`📊 Configuration:`);
    console.log(`   Nodes: ${EXTREME_TEST_CONFIG.nodeCount}`);
    console.log(`   Messages per node: ${EXTREME_TEST_CONFIG.messagesPerNode}`);
    console.log(`   Total messages: ${EXTREME_TEST_CONFIG.nodeCount * EXTREME_TEST_CONFIG.messagesPerNode}`);
    console.log(`   Timeout: ${EXTREME_TEST_CONFIG.testTimeoutMs / 1000}s`);

    // Execute distributed test with strict error handling
    testResult = await tester.executeDistributedTest();

    // Generate performance scorecard with strict typing
    scorecard = await tester.generateScorecard();

    console.log('');
    console.log('📊 TEST RESULTS:');
    console.log(`   Nodes Created: ${testResult.nodeCount}/${EXTREME_TEST_CONFIG.nodeCount}`);
    console.log(`   Messages Routed: ${testResult.messagesRouted}/${testResult.totalMessages}`);
    console.log(`   Average Latency: ${testResult.averageLatencyMs.toFixed(2)}ms`);
    console.log(`   Success Rate: ${testResult.successRate.toFixed(1)}%`);
    console.log(`   Errors: ${testResult.errors.length}`);

    console.log('');
    console.log('🏆 PERFORMANCE SCORECARD:');
    console.log(`   Overall Score: ${scorecard.overallScore}/100`);
    console.log(`   Grade: ${scorecard.grade}`);
    console.log(`   Test Duration: ${(scorecard.duration / 1000).toFixed(1)}s`);
    
    console.log('');
    console.log('⚡ DETAILED METRICS:');
    console.log(`   Latency - Min: ${scorecard.metrics.latency.min.toFixed(2)}ms`);
    console.log(`   Latency - Avg: ${scorecard.metrics.latency.avg.toFixed(2)}ms`);
    console.log(`   Latency - P95: ${scorecard.metrics.latency.p95.toFixed(2)}ms`);
    console.log(`   Latency - P99: ${scorecard.metrics.latency.p99.toFixed(2)}ms`);
    console.log(`   Latency - Max: ${scorecard.metrics.latency.max.toFixed(2)}ms`);
    console.log(`   Throughput: ${scorecard.metrics.throughput.operationsPerSecond.toFixed(1)} ops/sec`);
    console.log(`   Memory Peak: ${scorecard.metrics.resources.peakMemoryMB.toFixed(1)}MB`);
    console.log(`   Memory Growth: ${scorecard.metrics.resources.memoryGrowthMB.toFixed(1)}MB`);
    console.log(`   Success Rate: ${scorecard.metrics.reliability.successRate.toFixed(1)}%`);

    if (scorecard.optimizations.length > 0) {
      console.log('');
      console.log('🎯 OPTIMIZATION SUGGESTIONS:');
      scorecard.optimizations.forEach((opt, index) => {
        console.log(`   ${index + 1}. [${opt.severity.toUpperCase()}] ${opt.issue}`);
        console.log(`      → ${opt.suggestion}`);
        console.log(`      → Expected: ${opt.expectedImprovement}`);
      });
    }

    console.log('');
    console.log('📁 RESULTS SAVED TO:');
    console.log(`   Performance logs: ${EXTREME_TEST_CONFIG.performanceLogDir}`);

    // Strict assertions with proper error messages
    assertNodeCreation(testResult);
    assertNodeDiscovery(testResult);
    assertMessageRouting(testResult);
    assertLatencyPerformance(scorecard);
    assertThroughputPerformance(scorecard);
    assertReliability(scorecard);
    assertMemoryUsage(scorecard);
    assertOverallPerformance(scorecard);

    console.log('');
    console.log('🎉 EXTREME CAPACITY TEST PASSED!');
    console.log(`🏆 Grade: ${scorecard.grade} (${scorecard.overallScore}/100)`);

  }, EXTREME_TEST_CONFIG.testTimeoutMs);

  test('Grid performance optimization validation', async () => {
    console.log('🔬 GRID PERFORMANCE OPTIMIZATION VALIDATION');
    console.log('════════════════════════════════════════════════════════════');

    // Run smaller test focused on performance patterns
    const optimizationConfig: Readonly<GridTestConfig> = {
      ...EXTREME_TEST_CONFIG,
      nodeCount: 3,
      messagesPerNode: 50,
      enableBroadcastTest: false,
      enablePartitionTest: false
    };

    const optimizationTester = new GridDistributedTester(
      optimizationConfig,
      optimizationConfig.performanceLogDir
    );

    const result = await optimizationTester.executeDistributedTest();
    const optimizationScorecard = await optimizationTester.generateScorecard();

    console.log('📈 OPTIMIZATION METRICS:');
    console.log(`   Code abstraction improving latency: ${result.averageLatencyMs < PERFORMANCE_THRESHOLDS.MAX_AVERAGE_LATENCY_MS ? '✅' : '❌'}`);
    console.log(`   Memory efficiency: ${optimizationScorecard.metrics.resources.memoryGrowthMB < 50 ? '✅' : '❌'}`);
    console.log(`   Protocol consistency: ${result.errors.length === 0 ? '✅' : '❌'}`);

    // Validate that abstractions are improving performance
    expect(result.averageLatencyMs).toBeLessThan(PERFORMANCE_THRESHOLDS.MAX_AVERAGE_LATENCY_MS);
    expect(optimizationScorecard.metrics.resources.memoryGrowthMB).toBeLessThan(100);
    expect(result.errors).toHaveLength(0);

    console.log('🎯 OPTIMIZATION VALIDATION PASSED!');
  }, 60000);
});

// Strict assertion functions with detailed error messages
function assertNodeCreation(result: GridTestResult): void {
  if (result.nodeCount !== EXTREME_TEST_CONFIG.nodeCount) {
    throw new Error(
      `Node creation failed: created ${result.nodeCount}, expected ${EXTREME_TEST_CONFIG.nodeCount}`
    );
  }
  expect(result.nodeCount).toBe(EXTREME_TEST_CONFIG.nodeCount);
}

function assertNodeDiscovery(result: GridTestResult): void {
  const expectedDiscovered = EXTREME_TEST_CONFIG.nodeCount;
  if (result.discoveredNodes < expectedDiscovered) {
    throw new Error(
      `Node discovery failed: discovered ${result.discoveredNodes}, expected ${expectedDiscovered}`
    );
  }
  expect(result.discoveredNodes).toBeGreaterThanOrEqual(expectedDiscovered);
}

function assertMessageRouting(result: GridTestResult): void {
  const minExpectedMessages = Math.floor(result.totalMessages * (PERFORMANCE_THRESHOLDS.MIN_SUCCESS_RATE_PERCENT / 100));
  
  if (result.messagesRouted < minExpectedMessages) {
    throw new Error(
      `Message routing failed: routed ${result.messagesRouted}, minimum expected ${minExpectedMessages} (${PERFORMANCE_THRESHOLDS.MIN_SUCCESS_RATE_PERCENT}% of ${result.totalMessages})`
    );
  }
  
  expect(result.messagesRouted).toBeGreaterThanOrEqual(minExpectedMessages);
  expect(result.successRate).toBeGreaterThanOrEqual(PERFORMANCE_THRESHOLDS.MIN_SUCCESS_RATE_PERCENT);
}

function assertLatencyPerformance(scorecard: TestScorecard): void {
  if (scorecard.metrics.latency.avg > PERFORMANCE_THRESHOLDS.MAX_AVERAGE_LATENCY_MS) {
    throw new Error(
      `Latency performance failed: average ${scorecard.metrics.latency.avg.toFixed(2)}ms exceeds threshold ${PERFORMANCE_THRESHOLDS.MAX_AVERAGE_LATENCY_MS}ms`
    );
  }
  
  expect(scorecard.metrics.latency.avg).toBeLessThan(PERFORMANCE_THRESHOLDS.MAX_AVERAGE_LATENCY_MS);
}

function assertThroughputPerformance(scorecard: TestScorecard): void {
  if (scorecard.metrics.throughput.operationsPerSecond < PERFORMANCE_THRESHOLDS.MIN_THROUGHPUT_OPS_PER_SEC) {
    throw new Error(
      `Throughput performance failed: ${scorecard.metrics.throughput.operationsPerSecond.toFixed(1)} ops/sec below threshold ${PERFORMANCE_THRESHOLDS.MIN_THROUGHPUT_OPS_PER_SEC} ops/sec`
    );
  }
  
  expect(scorecard.metrics.throughput.operationsPerSecond).toBeGreaterThanOrEqual(PERFORMANCE_THRESHOLDS.MIN_THROUGHPUT_OPS_PER_SEC);
}

function assertReliability(scorecard: TestScorecard): void {
  if (scorecard.metrics.reliability.successRate < PERFORMANCE_THRESHOLDS.MIN_SUCCESS_RATE_PERCENT) {
    throw new Error(
      `Reliability failed: success rate ${scorecard.metrics.reliability.successRate.toFixed(1)}% below threshold ${PERFORMANCE_THRESHOLDS.MIN_SUCCESS_RATE_PERCENT}%`
    );
  }
  
  expect(scorecard.metrics.reliability.successRate).toBeGreaterThanOrEqual(PERFORMANCE_THRESHOLDS.MIN_SUCCESS_RATE_PERCENT);
}

function assertMemoryUsage(scorecard: TestScorecard): void {
  if (scorecard.metrics.resources.memoryGrowthMB > PERFORMANCE_THRESHOLDS.MAX_MEMORY_GROWTH_MB) {
    throw new Error(
      `Memory usage failed: growth ${scorecard.metrics.resources.memoryGrowthMB.toFixed(1)}MB exceeds threshold ${PERFORMANCE_THRESHOLDS.MAX_MEMORY_GROWTH_MB}MB`
    );
  }
  
  expect(scorecard.metrics.resources.memoryGrowthMB).toBeLessThan(PERFORMANCE_THRESHOLDS.MAX_MEMORY_GROWTH_MB);
}

function assertOverallPerformance(scorecard: TestScorecard): void {
  if (scorecard.overallScore < PERFORMANCE_THRESHOLDS.MIN_OVERALL_SCORE) {
    throw new Error(
      `Overall performance failed: score ${scorecard.overallScore}/100 below threshold ${PERFORMANCE_THRESHOLDS.MIN_OVERALL_SCORE}/100`
    );
  }
  
  if (!ACCEPTABLE_GRADES.includes(scorecard.grade as AcceptableGrade)) {
    throw new Error(
      `Performance grade failed: received ${scorecard.grade}, acceptable grades: ${ACCEPTABLE_GRADES.join(', ')}`
    );
  }
  
  expect(scorecard.overallScore).toBeGreaterThanOrEqual(PERFORMANCE_THRESHOLDS.MIN_OVERALL_SCORE);
  expect(ACCEPTABLE_GRADES).toContain(scorecard.grade);
}