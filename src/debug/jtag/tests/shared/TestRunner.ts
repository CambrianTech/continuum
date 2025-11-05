/**
 * Intelligent Middle-Out Test Runner
 * Executes tests based on architectural layers and importance
 */

import { execSync } from 'child_process';
import { TestLevel, TestImportance, TestCategory, generateExecutionPlan, getAllTests } from './TestDecorators';

export interface TestRunConfig {
  level?: TestLevel[];
  importance?: TestImportance[];
  category?: TestCategory[];
  maxConcurrency?: number;
  failFast?: boolean;
  verbose?: boolean;
  skipSystem?: boolean;
}

export interface TestResult {
  name: string;
  success: boolean;
  duration: number;
  output?: string;
  error?: string;
}

export class MiddleOutTestRunner {
  private config: TestRunConfig;
  
  constructor(config: TestRunConfig = {}) {
    this.config = {
      maxConcurrency: 1,
      failFast: true,
      verbose: true,
      ...config
    };
  }

  /**
   * Run tests based on middle-out strategy
   */
  async runTests(): Promise<TestResult[]> {
    console.log('🧅 MIDDLE-OUT TEST EXECUTION STRATEGY');
    console.log('🏗️  Building from foundation → system → e2e');
    
    const executionPlan = generateExecutionPlan();
    const results: TestResult[] = [];
    
    // Execute layer by layer (middle-out approach)
    const layerOrder = [
      TestLevel.FOUNDATION,
      TestLevel.UNIT,
      TestLevel.INTEGRATION, 
      TestLevel.SYSTEM,
      TestLevel.E2E
    ];
    
    for (const level of layerOrder) {
      const testsInLevel = executionPlan[level];
      if (testsInLevel.length === 0) continue;
      
      console.log(`\n🧅 LAYER: ${level.toUpperCase()}`);
      console.log(`📋 Tests: ${testsInLevel.length}`);
      
      // Filter tests based on config
      const filteredTests = this.filterTests(testsInLevel, level);
      if (filteredTests.length === 0) {
        console.log(`⏭️  Skipped - no tests match criteria`);
        continue;
      }
      
      // Execute tests in this layer
      const layerResults = await this.executeTestsInLayer(filteredTests, level);
      results.push(...layerResults);
      
      // Check if we should stop early
      if (this.config.failFast && layerResults.some(r => !r.success)) {
        console.log(`❌ FAIL FAST: Stopping due to failures in ${level} layer`);
        break;
      }
    }
    
    this.printSummary(results);
    return results;
  }

  private filterTests(testNames: string[], level: TestLevel): string[] {
    const allTests = getAllTests();
    
    return testNames.filter(name => {
      const metadata = allTests.get(name);
      if (!metadata) return false;
      
      // Filter by level
      if (this.config.level && !this.config.level.includes(metadata.level)) {
        return false;
      }
      
      // Filter by importance
      if (this.config.importance && !this.config.importance.includes(metadata.importance)) {
        return false;
      }
      
      // Filter by category
      if (this.config.category && !this.config.category.includes(metadata.category)) {
        return false;
      }
      
      // Skip system tests if requested
      if (this.config.skipSystem && metadata.requiresSystem) {
        return false;
      }
      
      return true;
    });
  }

  private async executeTestsInLayer(testNames: string[], level: TestLevel): Promise<TestResult[]> {
    const results: TestResult[] = [];
    
    for (const testName of testNames) {
      const result = await this.runSingleTest(testName);
      results.push(result);
      
      if (this.config.verbose) {
        const status = result.success ? '✅' : '❌';
        const duration = result.duration.toFixed(0);
        console.log(`${status} ${testName} (${duration}ms)`);
        
        if (!result.success && result.error) {
          console.log(`   Error: ${result.error.substring(0, 100)}...`);
        }
      }
      
      if (this.config.failFast && !result.success) {
        break;
      }
    }
    
    return results;
  }

  private async runSingleTest(testName: string): Promise<TestResult> {
    const startTime = Date.now();
    
    try {
      // Map test name to actual file path
      const testFile = this.resolveTestFile(testName);
      
      console.log(`🧪 Running: ${testFile}`);
      const output = execSync(`npx tsx ${testFile}`, { 
        encoding: 'utf8',
        timeout: 60000 // 60 second timeout
      });
      
      return {
        name: testName,
        success: true,
        duration: Date.now() - startTime,
        output: output.toString()
      };
    } catch (error: any) {
      return {
        name: testName,
        success: false,
        duration: Date.now() - startTime,
        error: error.message || error.toString()
      };
    }
  }

  private resolveTestFile(testName: string): string {
    // This is a simplified mapping - in practice, you'd have a registry
    // mapping test names to actual file paths
    const commonPaths = [
      `tests/${testName}.test.ts`,
      `tests/unit/${testName}.test.ts`,
      `tests/integration/${testName}.test.ts`,
      `tests/system/${testName}.test.ts`,
      `tests/e2e/${testName}.test.ts`,
      `tests/foundation/${testName}.test.ts`
    ];
    
    // For now, return the first common pattern
    // In practice, you'd check filesystem or use a registry
    return commonPaths[0];
  }

  private printSummary(results: TestResult[]): void {
    const successful = results.filter(r => r.success).length;
    const failed = results.filter(r => !r.success).length;
    const totalTime = results.reduce((sum, r) => sum + r.duration, 0);
    
    console.log('\n🏆 TEST EXECUTION SUMMARY');
    console.log(`✅ Passed: ${successful}`);
    console.log(`❌ Failed: ${failed}`);
    console.log(`⏱️  Total Time: ${(totalTime / 1000).toFixed(1)}s`);
    
    if (failed > 0) {
      console.log('\n❌ FAILED TESTS:');
      results
        .filter(r => !r.success)
        .forEach(r => {
          console.log(`   • ${r.name}: ${r.error?.substring(0, 80)}...`);
        });
    }
  }
}

/**
 * Command-line interface for running tests
 */
export async function runMiddleOutTests(config: TestRunConfig = {}) {
  const runner = new MiddleOutTestRunner(config);
  const results = await runner.runTests();
  
  const hasFailures = results.some(r => !r.success);
  process.exit(hasFailures ? 1 : 0);
}