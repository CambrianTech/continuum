/**
 * Promise/Event System Test Suite Runner
 * 
 * Systematically executes all tests for the type-safe message system,
 * promise chain preservation, and cross-context communication.
 * 
 * Follows the 6-layer JTAG testing methodology with proper sequencing.
 */

import { execSync } from 'child_process';
import { existsSync } from 'fs';
import * as path from 'path';

interface TestResult {
  name: string;
  passed: boolean;
  duration: number;
  error?: string;
  output?: string;
}

interface TestSuite {
  name: string;
  description: string;
  tests: string[];
  dependencies?: string[];
}

class PromiseEventTestRunner {
  private results: TestResult[] = [];
  private startTime = Date.now();

  private testSuites: TestSuite[] = [
    {
      name: 'Unit Tests',
      description: 'Core component unit tests for promise/event system',
      tests: [
        'tests/unit/ResponseCorrelator.test.ts',
        'tests/unit/JTAGMessageTypes.test.ts', 
        'tests/unit/JTAGMessageQueue.test.ts'
      ]
    },
    {
      name: 'Integration Tests',
      description: 'Component integration tests with mock transports',
      tests: [
        'tests/integration/PromiseChainIntegration.test.ts'
      ],
      dependencies: ['Unit Tests']
    },
    {
      name: 'Layer 4 System Integration',
      description: 'Cross-context promise chain tests with real browser/server',
      tests: [
        'tests/layer-4-system-integration/promise-chain-cross-context.test.ts'
      ],
      dependencies: ['Unit Tests', 'Integration Tests']
    }
  ];

  async runAllTests(): Promise<void> {
    console.log('🚀 Starting Promise/Event System Test Suite');
    console.log('=' .repeat(60));
    
    // Check prerequisites
    await this.checkPrerequisites();
    
    // Run test suites in dependency order
    for (const suite of this.testSuites) {
      await this.runTestSuite(suite);
    }
    
    // Generate final report
    this.generateFinalReport();
  }

  private async checkPrerequisites(): Promise<void> {
    console.log('🔍 Checking prerequisites...');
    
    // Check if npm start has been run (JTAG system should be running)
    try {
      const response = await fetch('http://localhost:9001/health');
      if (!response.ok) {
        throw new Error('JTAG system not responding');
      }
      console.log('✅ JTAG system running on port 9001');
    } catch (error) {
      console.error('❌ JTAG system not running. Please run `npm start` first.');
      console.error('   This is CRITICAL for cross-context testing.');
      process.exit(1);
    }

    // Check if test-bench server is running
    try {
      const response = await fetch('http://localhost:9002/status');
      if (!response.ok) {
        throw new Error('Test-bench server not responding');
      }
      console.log('✅ Test-bench server running on port 9002');
    } catch (error) {
      console.warn('⚠️ Test-bench server not running - some integration tests may fail');
    }

    // Check test files exist
    const missingFiles: string[] = [];
    for (const suite of this.testSuites) {
      for (const testFile of suite.tests) {
        const fullPath = path.join(__dirname, '..', testFile);
        if (!existsSync(fullPath)) {
          missingFiles.push(testFile);
        }
      }
    }

    if (missingFiles.length > 0) {
      console.error('❌ Missing test files:');
      missingFiles.forEach(file => console.error(`   - ${file}`));
      process.exit(1);
    }

    console.log('✅ All prerequisites met');
    console.log('');
  }

  private async runTestSuite(suite: TestSuite): Promise<void> {
    console.log(`📦 ${suite.name}: ${suite.description}`);
    console.log('-'.repeat(40));

    // Check dependencies
    if (suite.dependencies) {
      for (const dep of suite.dependencies) {
        const depPassed = this.results
          .filter(r => r.name.includes(dep))
          .every(r => r.passed);
        
        if (!depPassed) {
          console.error(`❌ Dependency '${dep}' failed - skipping ${suite.name}`);
          return;
        }
      }
    }

    let suiteSuccess = true;

    for (const testFile of suite.tests) {
      const result = await this.runSingleTest(testFile);
      this.results.push(result);
      
      if (!result.passed) {
        suiteSuccess = false;
      }
      
      // Print immediate result
      const status = result.passed ? '✅' : '❌';
      const duration = `${result.duration}ms`;
      console.log(`${status} ${path.basename(testFile)} (${duration})`);
      
      if (!result.passed && result.error) {
        console.error(`   Error: ${result.error}`);
      }
    }

    const suiteStatus = suiteSuccess ? '✅ PASSED' : '❌ FAILED';
    console.log(`\n${suiteStatus} ${suite.name}`);
    console.log('');
  }

  private async runSingleTest(testFile: string): Promise<TestResult> {
    const testName = path.basename(testFile, '.test.ts');
    const fullPath = path.join(__dirname, '..', testFile);
    const startTime = Date.now();

    try {
      // Use tsx for TypeScript execution (same as other JTAG tests)
      const command = `npx tsx "${fullPath}"`;
      
      console.log(`   Running: ${testName}...`);
      
      const output = execSync(command, {
        encoding: 'utf8',
        timeout: 120000, // 2 minute timeout per test file
        stdio: 'pipe',
        cwd: path.join(__dirname, '..', '..')  // Set working directory to jtag root
      });

      return {
        name: testName,
        passed: true,
        duration: Date.now() - startTime,
        output: output.toString()
      };

    } catch (error: any) {
      return {
        name: testName,
        passed: false,
        duration: Date.now() - startTime,
        error: error.message,
        output: error.stdout?.toString() || error.stderr?.toString()
      };
    }
  }

  private generateFinalReport(): void {
    const totalDuration = Date.now() - this.startTime;
    const totalTests = this.results.length;
    const passedTests = this.results.filter(r => r.passed).length;
    const failedTests = totalTests - passedTests;

    console.log('📊 Promise/Event System Test Suite Report');
    console.log('=' .repeat(60));
    console.log(`Total Tests: ${totalTests}`);
    console.log(`Passed: ${passedTests}`);
    console.log(`Failed: ${failedTests}`);
    console.log(`Success Rate: ${((passedTests / totalTests) * 100).toFixed(1)}%`);
    console.log(`Total Duration: ${totalDuration}ms`);
    console.log('');

    // Detailed results
    console.log('📋 Detailed Results:');
    console.log('-'.repeat(40));
    
    for (const result of this.results) {
      const status = result.passed ? '✅' : '❌';
      const duration = `${result.duration}ms`;
      console.log(`${status} ${result.name.padEnd(30)} ${duration}`);
      
      if (!result.passed && result.error) {
        console.log(`   Error: ${result.error}`);
      }
    }

    // Coverage assessment
    console.log('');
    console.log('🎯 Test Coverage Assessment:');
    console.log('-'.repeat(40));
    
    const coverageAreas = [
      { area: 'ResponseCorrelator', tested: this.results.some(r => r.name.includes('ResponseCorrelator') && r.passed) },
      { area: 'Message Types', tested: this.results.some(r => r.name.includes('MessageTypes') && r.passed) },
      { area: 'Message Queue', tested: this.results.some(r => r.name.includes('MessageQueue') && r.passed) },
      { area: 'Promise Chain Integration', tested: this.results.some(r => r.name.includes('Integration') && r.passed) },
      { area: 'Cross-Context Communication', tested: this.results.some(r => r.name.includes('cross-context') && r.passed) }
    ];

    coverageAreas.forEach(area => {
      const status = area.tested ? '✅' : '❌';
      console.log(`${status} ${area.area}`);
    });

    // Final assessment
    console.log('');
    if (failedTests === 0) {
      console.log('🎉 ALL TESTS PASSED! Promise/Event system is ready for production.');
      console.log('   ✅ Type-safe message system validated');
      console.log('   ✅ Promise chain preservation confirmed');
      console.log('   ✅ Cross-context communication working');
      console.log('   ✅ Deduplication and queuing operational');
    } else {
      console.log('⚠️ Some tests failed - Promise/Event system needs attention.');
      console.log(`   ${failedTests} test(s) need to be fixed before production use.`);
    }

    // Exit code for CI/CD
    process.exit(failedTests > 0 ? 1 : 0);
  }
}

// Self-executing test runner
if (require.main === module) {
  const runner = new PromiseEventTestRunner();
  runner.runAllTests().catch(error => {
    console.error('💥 Test suite runner failed:', error);
    process.exit(1);
  });
}

export { PromiseEventTestRunner };