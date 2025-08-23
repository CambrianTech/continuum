/**
 * Universal Transport Test Framework
 * 
 * Clean, well-typed, modular architecture for testing all transports
 * across all environments (browser, server, mobile) with zero repetition.
 * 
 * Eliminates code duplication through proper abstraction layers.
 */

import type { JTAGMessage, JTAGPayload } from '../../system/core/types/JTAGTypes';
import type { JTAGTransport, TransportSendResult } from '../../system/transports/shared/TransportTypes';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

/**
 * Transport Test Environment
 */
export enum TestEnvironment {
  SERVER = 'server',
  BROWSER = 'browser', 
  MOBILE = 'mobile',
  CROSS_ENVIRONMENT = 'cross-environment'
}

/**
 * Transport Test Category 
 */
export enum TestCategory {
  INITIALIZATION = 'initialization',
  CONNECTION = 'connection',
  MESSAGE_PASSING = 'message-passing',
  DISCOVERY = 'discovery',
  ROUTING = 'routing',
  FAULT_TOLERANCE = 'fault-tolerance',
  PERFORMANCE = 'performance',
  SECURITY = 'security'
}

/**
 * Transport Test Configuration
 */
export interface TransportTestConfig<T = unknown> {
  readonly name: string;
  readonly environment: TestEnvironment;
  readonly category: TestCategory;
  readonly timeout: number;
  readonly retries: number;
  readonly config: T;
}

/**
 * Test Result with rich typing
 */
export interface TestResult<T = unknown> {
  readonly testId: string;
  readonly name: string;
  readonly category: TestCategory;
  readonly environment: TestEnvironment;
  readonly success: boolean;
  readonly duration: number;
  readonly error?: string;
  readonly metrics?: T;
  readonly timestamp: string;
}

/**
 * Transport Test Metrics
 */
export interface TransportMetrics {
  readonly latency?: number;
  readonly throughput?: number;
  readonly memoryUsage?: number;
  readonly cpuUsage?: number;
  readonly messagesSent?: number;
  readonly messagesReceived?: number;
  readonly bytesTransferred?: number;
  readonly connectionsEstablished?: number;
  readonly errorsEncountered?: number;
}

/**
 * Test Scenario - Reusable test logic
 */
export abstract class TransportTestScenario<TConfig = unknown, TMetrics = TransportMetrics> {
  public readonly name: string;
  public readonly category: TestCategory;
  public readonly timeout: number;
  protected readonly config: TConfig;

  constructor(
    name: string,
    category: TestCategory,
    config: TConfig,
    timeout = 30000
  ) {
    this.name = name;
    this.category = category;
    this.config = config;
    this.timeout = timeout;
  }

  /**
   * Execute the test scenario
   */
  abstract execute(transport: JTAGTransport): Promise<TestResult<TMetrics>>;

  /**
   * Setup before test execution
   */
  protected async setup(transport: JTAGTransport): Promise<void> {
    // Default: no setup required
  }

  /**
   * Cleanup after test execution  
   */
  protected async cleanup(transport: JTAGTransport): Promise<void> {
    // Default: no cleanup required
  }

  /**
   * Create test result
   */
  protected createResult(
    success: boolean,
    duration: number,
    metrics?: TMetrics,
    error?: string
  ): TestResult<TMetrics> {
    return {
      testId: generateUUID(),
      name: this.name,
      category: this.category,
      environment: TestEnvironment.SERVER, // Will be set by runner
      success,
      duration,
      error,
      metrics,
      timestamp: new Date().toISOString()
    };
  }
}

/**
 * Transport Factory - Creates transports for testing
 */
export interface TransportFactory<TTransport extends JTAGTransport, TConfig = unknown> {
  readonly name: string;
  readonly supportedEnvironments: readonly TestEnvironment[];
  
  create(config: TConfig): Promise<TTransport>;
  cleanup(transport: TTransport): Promise<void>;
}

/**
 * Test Runner - Executes test scenarios against transports
 */
export class TransportTestRunner<TTransport extends JTAGTransport = JTAGTransport> {
  private readonly factory: TransportFactory<TTransport>;
  private readonly environment: TestEnvironment;
  private activeTransports: Set<TTransport> = new Set();

  constructor(factory: TransportFactory<TTransport>, environment: TestEnvironment) {
    this.factory = factory;
    this.environment = environment;
    
    if (!factory.supportedEnvironments.includes(environment)) {
      throw new Error(`Transport factory ${factory.name} does not support environment ${environment}`);
    }
  }

  /**
   * Run a single test scenario
   */
  async runScenario<TMetrics>(
    scenario: TransportTestScenario<unknown, TMetrics>,
    transportConfig?: unknown
  ): Promise<TestResult<TMetrics>> {
    const startTime = Date.now();
    
    try {
      // Create transport
      const transport = await this.factory.create(transportConfig || {});
      this.activeTransports.add(transport);
      
      // Execute test with timeout
      const result = await Promise.race([
        scenario.execute(transport),
        this.createTimeoutPromise<TestResult<TMetrics>>(scenario.timeout)
      ]);
      
      // Update result with correct environment
      return {
        ...result,
        environment: this.environment
      };
      
    } catch (error: any) {
      const duration = Date.now() - startTime;
      return scenario['createResult'](false, duration, undefined, error.message) as TestResult<TMetrics>;
    }
  }

  /**
   * Run multiple test scenarios
   */
  async runScenarios(
    scenarios: TransportTestScenario[],
    transportConfig?: unknown
  ): Promise<TestResult[]> {
    const results: TestResult[] = [];
    
    for (const scenario of scenarios) {
      console.log(`🧪 Running: ${scenario.name} [${scenario.category}]`);
      const result = await this.runScenario(scenario, transportConfig);
      
      const status = result.success ? '✅ PASSED' : '❌ FAILED';
      const duration = result.duration.toFixed(0);
      console.log(`   ${status} (${duration}ms)${result.error ? ': ' + result.error : ''}`);
      
      results.push(result);
    }
    
    return results;
  }

  /**
   * Create timeout promise
   */
  private createTimeoutPromise<T>(timeout: number): Promise<T> {
    return new Promise((_, reject) => {
      setTimeout(() => {
        reject(new Error(`Test timed out after ${timeout}ms`));
      }, timeout);
    });
  }

  /**
   * Cleanup all active transports
   */
  async cleanup(): Promise<void> {
    console.log(`🧹 Cleaning up ${this.activeTransports.size} active transports...`);
    
    const cleanupPromises = Array.from(this.activeTransports).map(async transport => {
      try {
        await this.factory.cleanup(transport);
      } catch (error: any) {
        console.warn(`⚠️ Error cleaning up transport: ${error.message}`);
      }
    });
    
    await Promise.all(cleanupPromises);
    this.activeTransports.clear();
    console.log('✅ Transport cleanup complete');
  }
}

/**
 * Test Suite - Organized collection of test scenarios
 */
export class TransportTestSuite {
  private readonly scenarios: Map<TestCategory, TransportTestScenario[]> = new Map();
  public readonly name: string;

  constructor(name: string) {
    this.name = name;
  }

  /**
   * Add test scenario to suite
   */
  addScenario(scenario: TransportTestScenario): this {
    if (!this.scenarios.has(scenario.category)) {
      this.scenarios.set(scenario.category, []);
    }
    
    this.scenarios.get(scenario.category)!.push(scenario);
    return this;
  }

  /**
   * Add multiple scenarios
   */
  addScenarios(scenarios: TransportTestScenario[]): this {
    scenarios.forEach(scenario => this.addScenario(scenario));
    return this;
  }

  /**
   * Get scenarios by category
   */
  getScenarios(category?: TestCategory): TransportTestScenario[] {
    if (category) {
      return this.scenarios.get(category) || [];
    }
    
    // Return all scenarios
    return Array.from(this.scenarios.values()).flat();
  }

  /**
   * Get all categories
   */
  getCategories(): TestCategory[] {
    return Array.from(this.scenarios.keys());
  }

  /**
   * Execute the entire test suite
   */
  async execute<TTransport extends JTAGTransport>(
    runner: TransportTestRunner<TTransport>,
    transportConfig?: unknown,
    categories?: TestCategory[]
  ): Promise<TestSuiteResult> {
    const startTime = Date.now();
    const selectedCategories = categories || this.getCategories();
    const results: TestResult[] = [];
    
    console.log(`🚀 Executing Transport Test Suite: ${this.name}`);
    console.log(`📋 Categories: ${selectedCategories.join(', ')}`);
    console.log('='.repeat(60));
    
    try {
      for (const category of selectedCategories) {
        const categoryScenarios = this.getScenarios(category);
        if (categoryScenarios.length === 0) continue;
        
        console.log(`\\n📂 Category: ${category} (${categoryScenarios.length} tests)`);
        console.log('-'.repeat(40));
        
        const categoryResults = await runner.runScenarios(categoryScenarios, transportConfig);
        results.push(...categoryResults);
      }
      
    } finally {
      await runner.cleanup();
    }
    
    const duration = Date.now() - startTime;
    return this.createSuiteResult(results, duration);
  }

  /**
   * Create test suite result summary
   */
  private createSuiteResult(results: TestResult[], duration: number): TestSuiteResult {
    const passed = results.filter(r => r.success).length;
    const failed = results.length - passed;
    const successRate = results.length > 0 ? (passed / results.length) * 100 : 0;
    
    const categoryStats = new Map<TestCategory, { passed: number; total: number }>();
    
    results.forEach(result => {
      if (!categoryStats.has(result.category)) {
        categoryStats.set(result.category, { passed: 0, total: 0 });
      }
      const stats = categoryStats.get(result.category)!;
      stats.total++;
      if (result.success) stats.passed++;
    });
    
    return {
      suiteName: this.name,
      totalTests: results.length,
      passed,
      failed,
      successRate,
      duration,
      results,
      categoryStats: Object.fromEntries(categoryStats),
      timestamp: new Date().toISOString()
    };
  }
}

/**
 * Test Suite Result
 */
export interface TestSuiteResult {
  readonly suiteName: string;
  readonly totalTests: number;
  readonly passed: number;
  readonly failed: number;
  readonly successRate: number;
  readonly duration: number;
  readonly results: readonly TestResult[];
  readonly categoryStats: Record<TestCategory, { passed: number; total: number }>;
  readonly timestamp: string;
}

/**
 * Result Formatter - Pretty print test results
 */
export class TestResultFormatter {
  
  static formatSuiteResult(result: TestSuiteResult): void {
    console.log('\\n' + '='.repeat(60));
    console.log(`📊 TEST SUITE RESULTS: ${result.suiteName}`);
    console.log('='.repeat(60));
    
    // Overall stats
    console.log(`📈 Overall: ${result.passed}/${result.totalTests} passed (${result.successRate.toFixed(1)}%)`);
    console.log(`⏱️  Duration: ${result.duration}ms`);
    console.log(`📅 Completed: ${result.timestamp}`);
    
    // Category breakdown
    console.log('\\n📋 Results by Category:');
    Object.entries(result.categoryStats).forEach(([category, stats]) => {
      const rate = stats.total > 0 ? (stats.passed / stats.total) * 100 : 0;
      const status = rate === 100 ? '✅' : rate >= 80 ? '⚠️' : '❌';
      console.log(`   ${status} ${category}: ${stats.passed}/${stats.total} (${rate.toFixed(1)}%)`);
    });
    
    // Failed tests detail
    const failedTests = result.results.filter(r => !r.success);
    if (failedTests.length > 0) {
      console.log('\\n❌ Failed Tests:');
      failedTests.forEach(test => {
        console.log(`   • ${test.name} [${test.category}]: ${test.error}`);
      });
    }
    
    // Success summary
    if (result.successRate === 100) {
      console.log('\\n🎉 ALL TESTS PASSED! Transport is fully functional.');
    } else if (result.successRate >= 80) {
      console.log('\\n⚠️  MOSTLY PASSING with some issues to address.');
    } else {
      console.log('\\n💥 SIGNIFICANT FAILURES - transport needs attention.');
    }
  }
}

// Note: TestEnvironment and TestCategory are already exported above
export type {
  TransportTestConfig,
  TestResult,
  TransportMetrics,
  TransportFactory,
  TestSuiteResult
};