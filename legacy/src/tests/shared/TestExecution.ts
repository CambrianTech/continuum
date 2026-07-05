/**
 * Test Execution - Core test running and orchestration patterns
 * 
 * Provides standardized test execution with timing, error handling,
 * and result collection across all test suites.
 */

import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import { JTAGClientFactory } from './JTAGClientFactory';
import { TEST_TIMEOUTS, TestTimeout } from './TestConstants';

// =============================================================================
// TEST EXECUTION INTERFACES - Comprehensive typing for test operations
// =============================================================================

export interface TestExecutionOptions {
  timeout?: TestTimeout;
  retryAttempts?: number;
  validateResults?: boolean;
  logProgress?: boolean;
  cleanupAfter?: boolean;
}

export interface TestExecutionResult {
  testName: string;
  success: boolean;
  duration: number;
  error?: string;
  data?: any;
  screenshots?: string[];
  logs?: string[];
}

export interface TestSuiteResult {
  suiteName: string;
  results: TestExecutionResult[];
  summary: {
    passed: number;
    failed: number;
    total: number;
    successRate: number;
    totalDuration: number;
  };
}

// =============================================================================
// TEST EXECUTION ENGINE - Standardized test orchestration
// =============================================================================

export class TestExecutionEngine {
  private clientFactory: JTAGClientFactory;
  private results: TestExecutionResult[] = [];
  private activeClient: JTAGClient | null = null;
  
  constructor() {
    this.clientFactory = JTAGClientFactory.getInstance();
  }
  
  /**
   * Execute single test with comprehensive error handling and timing
   */
  async executeTest<T>(
    testName: string,
    testFunction: (client: JTAGClient) => Promise<T>,
    options: TestExecutionOptions = {}
  ): Promise<TestExecutionResult> {
    const startTime = Date.now();
    const timeout = options.timeout || TEST_TIMEOUTS.STANDARD_OPERATION;
    
    if (options.logProgress !== false) {
      console.log(`🧪 TestExecution: Starting "${testName}" (timeout: ${timeout}ms)`);
    }
    
    try {
      // Reuse client if available, create new if needed
      if (!this.activeClient) {
        const connection = await this.clientFactory.createClient({ timeout });
        this.activeClient = connection.client;
      }
      
      // Execute test function with timeout
      const result = await Promise.race([
        testFunction(this.activeClient),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error(`Test timeout after ${timeout}ms`)), timeout)
        )
      ]);
      
      const duration = Date.now() - startTime;
      
      const testResult: TestExecutionResult = {
        testName,
        success: true,
        duration,
        data: result
      };
      
      this.results.push(testResult);
      
      if (options.logProgress !== false) {
        console.log(`✅ TestExecution: "${testName}" completed (${duration}ms)`);
      }
      
      return testResult;
      
    } catch (error) {
      const duration = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      const testResult: TestExecutionResult = {
        testName,
        success: false,
        duration,
        error: errorMessage
      };
      
      this.results.push(testResult);
      
      console.error(`❌ TestExecution: "${testName}" failed (${duration}ms):`, errorMessage);
      
      return testResult;
      
    } finally {
      // Cleanup if requested
      if (options.cleanupAfter) {
        await this.cleanup();
      }
    }
  }
  
  /**
   * Execute test suite with comprehensive reporting
   */
  async executeTestSuite(
    suiteName: string,
    tests: Array<{
      name: string;
      testFunction: (client: JTAGClient) => Promise<any>;
      options?: TestExecutionOptions;
    }>
  ): Promise<TestSuiteResult> {
    console.log(`\n🎯 TestExecution: Starting test suite "${suiteName}" (${tests.length} tests)`);
    
    const suiteStartTime = Date.now();
    const suiteResults: TestExecutionResult[] = [];
    
    try {
      // Execute all tests in sequence
      for (const test of tests) {
        const result = await this.executeTest(
          test.name,
          test.testFunction,
          test.options || {}
        );
        suiteResults.push(result);
      }
      
      const suiteDuration = Date.now() - suiteStartTime;
      const passed = suiteResults.filter(r => r.success).length;
      const failed = suiteResults.filter(r => !r.success).length;
      const total = suiteResults.length;
      const successRate = total > 0 ? Math.round((passed / total) * 100) : 0;
      
      const suiteResult: TestSuiteResult = {
        suiteName,
        results: suiteResults,
        summary: {
          passed,
          failed,
          total,
          successRate,
          totalDuration: suiteDuration
        }
      };
      
      console.log(`\n📊 TestExecution: Suite "${suiteName}" completed`);
      console.log(`   ✅ Passed: ${passed}/${total} (${successRate}%)`);
      console.log(`   ⏱️ Duration: ${suiteDuration}ms`);
      
      if (failed > 0) {
        console.log(`\n❌ Failed Tests in "${suiteName}":`);
        suiteResults
          .filter(r => !r.success)
          .forEach(result => {
            console.log(`   - ${result.testName}: ${result.error}`);
          });
      }
      
      return suiteResult;
      
    } finally {
      await this.cleanup();
    }
  }
  
  /**
   * Get all execution results
   */
  getResults(): TestExecutionResult[] {
    return [...this.results];
  }
  
  /**
   * Get execution summary
   */
  getSummary(): { passed: number; failed: number; total: number; successRate: number } {
    const passed = this.results.filter(r => r.success).length;
    const failed = this.results.filter(r => !r.success).length;
    const total = this.results.length;
    const successRate = total > 0 ? Math.round((passed / total) * 100) : 0;
    
    return { passed, failed, total, successRate };
  }
  
  /**
   * Clear all results (for fresh test runs)
   */
  clearResults(): void {
    this.results = [];
  }
  
  /**
   * Cleanup all resources
   */
  async cleanup(): Promise<void> {
    if (this.activeClient) {
      await this.clientFactory.cleanupClient(this.activeClient);
      this.activeClient = null;
    }
  }
}

// =============================================================================
// CONVENIENCE FUNCTIONS - Simple interfaces for common patterns
// =============================================================================

/**
 * Execute single test without managing engine instance
 */
export async function runSingleTest<T>(
  testName: string,
  testFunction: (client: JTAGClient) => Promise<T>,
  options?: TestExecutionOptions
): Promise<TestExecutionResult> {
  const engine = new TestExecutionEngine();
  try {
    return await engine.executeTest(testName, testFunction, options);
  } finally {
    await engine.cleanup();
  }
}

/**
 * Execute test suite without managing engine instance
 */
export async function runTestSuite(
  suiteName: string,
  tests: Array<{
    name: string;
    testFunction: (client: JTAGClient) => Promise<any>;
    options?: TestExecutionOptions;
  }>
): Promise<TestSuiteResult> {
  const engine = new TestExecutionEngine();
  return engine.executeTestSuite(suiteName, tests);
}

// Export singleton engine for advanced usage
export const testExecutionEngine = new TestExecutionEngine();