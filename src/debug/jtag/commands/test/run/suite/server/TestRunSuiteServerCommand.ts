/**
 * Test Run Suite Server Command
 * Replaces shell script-based test runners (run-categorized-tests.sh)
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import { TestRunSuiteParams, TestRunSuiteResult, TestFileResult, DEFAULT_TEST_PROFILES, TestProfile, createTestRunSuiteResult } from '../shared/TestRunSuiteTypes';
import { execSync, exec } from 'child_process';
import { promisify } from 'util';
import { glob } from 'glob';
import * as path from 'path';

const execAsync = promisify(exec);

export class TestRunSuiteServerCommand extends CommandBase<TestRunSuiteParams, TestRunSuiteResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('test-run-suite', context, subpath, commander);
  }

  async execute(params: TestRunSuiteParams): Promise<TestRunSuiteResult> {
    const startTime = Date.now();
    const results: TestFileResult[] = [];

    try {
      // Determine test configuration
      const profile = this.getTestProfile(params);
      console.log(`🧪 Running test suite: ${profile.name}`);
      console.log(`📋 Description: ${profile.description}`);
      console.log(`⚙️ Config: ${profile.tests.length} patterns, timeout=${profile.timeout}ms, parallel=${profile.parallelism}`);

      // Find all test files matching the profile patterns
      const testFiles = await this.findTestFiles(profile.tests);
      console.log(`🔍 Found ${testFiles.length} test files`);

      if (testFiles.length === 0) {
        return createTestRunSuiteResult(params, {
          success: false,
          profile: profile.name,
          testsRun: 0,
          testsPassed: 0,
          testsFailed: 0,
          totalExecutionTime: Date.now() - startTime,
          results: [],
          error: `No test files found for profile '${profile.name}'`
        });
      }

      // Execute tests
      if (params.parallel && profile.parallelism > 1) {
        results.push(...await this.executeTestsParallel(testFiles, profile, params));
      } else {
        results.push(...await this.executeTestsSequential(testFiles, profile, params));
      }

      // Calculate summary
      const testsPassed = results.filter(r => r.success).length;
      const testsFailed = results.filter(r => !r.success).length;
      const allSuccess = testsFailed === 0;

      const finalResult = createTestRunSuiteResult(params, {
        success: allSuccess,
        profile: profile.name,
        testsRun: results.length,
        testsPassed,
        testsFailed,
        totalExecutionTime: Date.now() - startTime,
        results
      });

      // Output summary
      this.printSummary(finalResult, params);

      return finalResult;

    } catch (error) {
      return createTestRunSuiteResult(params, {
        success: false,
        profile: params.profile || 'unknown',
        testsRun: 0,
        testsPassed: 0,
        testsFailed: 0,
        totalExecutionTime: Date.now() - startTime,
        results,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  private getTestProfile(params: TestRunSuiteParams): TestProfile {
    if (params.tests) {
      // Custom test configuration
      const testPatterns = params.tests.split(',').map(t => t.trim());
      return {
        name: params.name || 'custom',
        description: 'Custom test configuration',
        tests: testPatterns,
        deployBrowser: true, // Safe default
        parallelism: Number(params.parallelism) || (params.parallel ? 2 : 1),
        timeout: Number(params.timeout) || 120000
      };
    }

    const profileName = params.profile || 'comprehensive';
    const profile = DEFAULT_TEST_PROFILES[profileName];

    if (!profile) {
      throw new Error(`Unknown test profile: ${profileName}. Available: ${Object.keys(DEFAULT_TEST_PROFILES).join(', ')}`);
    }

    // Override with command parameters
    return {
      ...profile,
      parallelism: Number(params.parallelism) || profile.parallelism,
      timeout: Number(params.timeout) || profile.timeout
    };
  }

  private async findTestFiles(patterns: string[]): Promise<string[]> {
    const allFiles: string[] = [];
    const cwd = process.cwd();

    for (const pattern of patterns) {
      try {
        const files = await glob(pattern, {
          cwd,
          absolute: false,
          ignore: ['**/node_modules/**', '**/dist/**', '**/.continuum/**']
        });
        allFiles.push(...files);
      } catch (error) {
        console.warn(`⚠️ Pattern '${pattern}' failed: ${error instanceof Error ? error.message : error}`);
      }
    }

    // Remove duplicates and sort
    return Array.from(new Set(allFiles)).sort();
  }

  private async executeTestsSequential(
    testFiles: string[],
    profile: TestProfile,
    params: TestRunSuiteParams
  ): Promise<TestFileResult[]> {
    const results: TestFileResult[] = [];

    for (let i = 0; i < testFiles.length; i++) {
      const file = testFiles[i];
      console.log(`🧪 [${i + 1}/${testFiles.length}] Running: ${file}`);

      const result = await this.executeTestFile(file, profile.timeout, params);
      results.push(result);

      if (!result.success && params.failFast) {
        console.log(`❌ Stopping due to failure (failFast enabled)`);
        break;
      }
    }

    return results;
  }

  private async executeTestsParallel(
    testFiles: string[],
    profile: TestProfile,
    params: TestRunSuiteParams
  ): Promise<TestFileResult[]> {
    const chunkSize = profile.parallelism;
    const results: TestFileResult[] = [];

    // Process files in parallel batches
    for (let i = 0; i < testFiles.length; i += chunkSize) {
      const chunk = testFiles.slice(i, i + chunkSize);
      console.log(`🧪 [Batch ${Math.floor(i / chunkSize) + 1}] Running ${chunk.length} tests in parallel`);

      const promises = chunk.map(file => this.executeTestFile(file, profile.timeout, params));
      const batchResults = await Promise.all(promises);
      results.push(...batchResults);

      // Check for failures if failFast is enabled
      if (params.failFast && batchResults.some(r => !r.success)) {
        console.log(`❌ Stopping due to failure in batch (failFast enabled)`);
        break;
      }
    }

    return results;
  }

  private async executeTestFile(
    file: string,
    timeout: number,
    params: TestRunSuiteParams
  ): Promise<TestFileResult> {
    const startTime = Date.now();
    const command = `npx tsx ${file}`;

    try {
      if (params.verbose) {
        console.log(`   Command: ${command}`);
      }

      const { stdout, stderr } = await execAsync(command, {
        timeout: Number(timeout) || 120000,
        maxBuffer: 1024 * 1024 * 10, // 10MB
        cwd: process.cwd()
      });

      const output = stdout + (stderr ? '\n' + stderr : '');
      const executionTime = Date.now() - startTime;

      if (params.verbose) {
        console.log(`   ✅ Completed in ${executionTime}ms`);
      }

      return {
        file,
        success: true,
        output,
        executionTime
      };

    } catch (error: any) {
      const executionTime = Date.now() - startTime;
      const output = (error.stdout || '') + (error.stderr || '');

      if (params.verbose) {
        console.log(`   ❌ Failed in ${executionTime}ms: ${error.message}`);
      }

      return {
        file,
        success: false,
        output,
        executionTime,
        error: error.message || String(error)
      };
    }
  }

  private printSummary(result: TestRunSuiteResult, params: TestRunSuiteParams): void {
    console.log('\n📊 Test Suite Results:');
    console.log('=====================');
    console.log(`Profile: ${result.profile}`);
    console.log(`Tests: ${result.testsPassed}/${result.testsRun} passed`);
    console.log(`Time: ${(result.totalExecutionTime / 1000).toFixed(1)}s`);

    if (result.testsFailed > 0) {
      console.log('\n❌ Failed Tests:');
      result.results.filter(r => !r.success).forEach(r => {
        console.log(`   ${r.file}: ${r.error}`);
      });
    }

    const successRate = ((result.testsPassed / result.testsRun) * 100).toFixed(1);
    if (result.success) {
      console.log(`\n🎉 ALL TESTS PASSED! (${successRate}%)`);
    } else {
      console.log(`\n⚠️ ${result.testsFailed} tests failed (${successRate}% success)`);
    }
  }

  getCommandName(): string {
    return 'test/run/suite';
  }

  getDescription(): string {
    return 'Run test suites by profile or custom configuration, replacing shell script runners';
  }

  getUsageExamples(): string[] {
    return [
      'test/run/suite --profile="comprehensive"',
      'test/run/suite --profile="chat" --parallel --timeout=60000',
      'test/run/suite --tests="crud,chat,screenshots" --name="custom-precommit" --save',
      'test/run/suite --profile="integration" --format="json" --verbose',
      'test/run/suite --profile="precommit" --fail-fast'
    ];
  }
}