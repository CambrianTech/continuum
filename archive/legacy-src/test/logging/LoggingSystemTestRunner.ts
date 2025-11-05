#!/usr/bin/env tsx
/**
 * Comprehensive Logging System Test Runner
 * Validates all console forwarding and logging functionality
 */

import { execSync } from 'child_process';
import { existsSync } from 'fs';
import { readFile, readdir, stat, writeFile, unlink } from 'fs/promises';
import * as path from 'path';

interface TestResult {
  name: string;
  passed: boolean;
  duration: number;
  error?: string;
}

interface TestSuite {
  name: string;
  tests: TestResult[];
  duration: number;
  passed: boolean;
}

class LoggingSystemTestRunner {
  private results: TestSuite[] = [];

  async runAllTests(): Promise<void> {
    console.log('🧪 Running Comprehensive Logging System Tests\n');
    
    // Run all test suites
    await this.runUnitTests();
    await this.runIntegrationTests();
    await this.runSystemValidation();
    await this.runPerformanceTests();
    
    // Report results
    this.reportResults();
  }

  private async runUnitTests(): Promise<void> {
    console.log('🔬 Running Unit Tests...');
    
    const testSuite: TestSuite = {
      name: 'Unit Tests',
      tests: [],
      duration: 0,
      passed: true
    };

    const startTime = Date.now();
    
    try {
      // WebSocket daemon unit tests
      await this.runSingleTest(
        'WebSocket Daemon Units',
        'npx tsx --test src/integrations/websocket/test/unit/WebSocketDaemon.test.ts',
        testSuite
      );

      // Logger daemon unit tests (skipped - LoggerDaemon was stashed in favor of working console forwarding)
      // const loggerUnitTests = 'src/daemons/logger/test/unit/LoggerDaemon.test.ts';
      // if (existsSync(loggerUnitTests)) {
      //   await this.runSingleTest(
      //     'Logger Daemon Units',
      //     `npx tsx --test ${loggerUnitTests}`,
      //     testSuite
      //   );
      // }

    } catch (error) {
      testSuite.passed = false;
      testSuite.tests.push({
        name: 'Unit Tests',
        passed: false,
        duration: 0,
        error: error instanceof Error ? error.message : String(error)
      });
    }

    testSuite.duration = Date.now() - startTime;
    this.results.push(testSuite);
  }

  private async runIntegrationTests(): Promise<void> {
    console.log('🔗 Running Integration Tests...');
    
    const testSuite: TestSuite = {
      name: 'Integration Tests',
      tests: [],
      duration: 0,
      passed: true
    };

    const startTime = Date.now();
    
    try {
      // WebSocket console forwarding integration tests
      await this.runSingleTest(
        'Console Forwarding Integration',
        'npx tsx --test src/integrations/websocket/test/integration/ConsoleForwarding.integration.test.ts',
        testSuite
      );

      // Complete logging system integration tests
      await this.runSingleTest(
        'Complete Logging System Integration',
        'npx tsx --test src/test/integration/ConsoleLoggingSystem.integration.test.ts',
        testSuite
      );

    } catch (error) {
      testSuite.passed = false;
      testSuite.tests.push({
        name: 'Integration Tests',
        passed: false,
        duration: 0,
        error: error instanceof Error ? error.message : String(error)
      });
    }

    testSuite.duration = Date.now() - startTime;
    this.results.push(testSuite);
  }

  private async runSystemValidation(): Promise<void> {
    console.log('🔍 Running System Validation...');
    
    const testSuite: TestSuite = {
      name: 'System Validation',
      tests: [],
      duration: 0,
      passed: true
    };

    const startTime = Date.now();
    
    try {
      // Check if logging system is operational
      await this.validateLoggingSystem(testSuite);
      
      // Check for recent session logs
      await this.validateRecentLogs(testSuite);
      
      // Check for gigabyte files (regression test)
      await this.validateNoGigabyteFiles(testSuite);

    } catch (error) {
      testSuite.passed = false;
      testSuite.tests.push({
        name: 'System Validation',
        passed: false,
        duration: 0,
        error: error instanceof Error ? error.message : String(error)
      });
    }

    testSuite.duration = Date.now() - startTime;
    this.results.push(testSuite);
  }

  private async runPerformanceTests(): Promise<void> {
    console.log('⚡ Running Performance Tests...');
    
    const testSuite: TestSuite = {
      name: 'Performance Tests',
      tests: [],
      duration: 0,
      passed: true
    };

    const startTime = Date.now();
    
    try {
      // Check log file sizes
      await this.validateLogFileSizes(testSuite);
      
      // Check system responsiveness
      await this.validateSystemResponsiveness(testSuite);

    } catch (error) {
      testSuite.passed = false;
      testSuite.tests.push({
        name: 'Performance Tests',
        passed: false,
        duration: 0,
        error: error instanceof Error ? error.message : String(error)
      });
    }

    testSuite.duration = Date.now() - startTime;
    this.results.push(testSuite);
  }

  private async runSingleTest(name: string, command: string, testSuite: TestSuite): Promise<void> {
    const testStart = Date.now();
    
    try {
      const output = execSync(command, { 
        encoding: 'utf8',
        timeout: 60000 // 60 second timeout
      });
      
      const testDuration = Date.now() - testStart;
      
      // Check if test passed (look at Node.js test output)
      const failCount = output.match(/ℹ fail (\d+)/)?.[1];
      const passed = !failCount || parseInt(failCount) === 0;
      
      const testResult: TestResult = {
        name,
        passed,
        duration: testDuration
      };
      
      if (!passed) {
        testResult.error = 'Test execution indicated failures';
      }
      
      testSuite.tests.push(testResult);
      
      if (!passed) {
        testSuite.passed = false;
      }
      
    } catch (error) {
      const testDuration = Date.now() - testStart;
      
      testSuite.tests.push({
        name,
        passed: false,
        duration: testDuration,
        error: error instanceof Error ? error.message : String(error)
      });
      
      testSuite.passed = false;
    }
  }

  private async validateLoggingSystem(testSuite: TestSuite): Promise<void> {
    const testStart = Date.now();
    
    try {
      // Check if sessions directory exists
      const sessionsDir = '.continuum/sessions/user/shared';
      if (!existsSync(sessionsDir)) {
        throw new Error('Sessions directory does not exist');
      }
      
      // Check if any sessions have logs
      const sessions = await readdir(sessionsDir);
      let hasLogs = false;
      
      for (const session of sessions) {
        const logDir = path.join(sessionsDir, session, 'logs');
        if (existsSync(logDir)) {
          const logFiles = await readdir(logDir);
          if (logFiles.length > 0) {
            hasLogs = true;
            break;
          }
        }
      }
      
      if (!hasLogs) {
        throw new Error('No session logs found');
      }
      
      testSuite.tests.push({
        name: 'Logging System Operational',
        passed: true,
        duration: Date.now() - testStart
      });
      
    } catch (error) {
      testSuite.tests.push({
        name: 'Logging System Operational',
        passed: false,
        duration: Date.now() - testStart,
        error: error instanceof Error ? error.message : String(error)
      });
      testSuite.passed = false;
    }
  }

  private async validateRecentLogs(testSuite: TestSuite): Promise<void> {
    const testStart = Date.now();
    
    try {
      const sessionsDir = '.continuum/sessions/user/shared';
      const sessions = await readdir(sessionsDir);
      
      let hasRecentLogs = false;
      const oneHourAgo = Date.now() - (60 * 60 * 1000);
      
      for (const session of sessions) {
        const logDir = path.join(sessionsDir, session, 'logs');
        if (existsSync(logDir)) {
          const logFiles = await readdir(logDir);
          
          for (const logFile of logFiles) {
            const logPath = path.join(logDir, logFile);
            const stats = await stat(logPath);
            
            if (stats.mtimeMs > oneHourAgo && stats.size > 0) {
              hasRecentLogs = true;
              break;
            }
          }
        }
        
        if (hasRecentLogs) break;
      }
      
      const testResult: TestResult = {
        name: 'Recent Logs Present',
        passed: hasRecentLogs,
        duration: Date.now() - testStart
      };
      
      if (!hasRecentLogs) {
        testResult.error = 'No recent logs found';
      }
      
      testSuite.tests.push(testResult);
      
      if (!hasRecentLogs) {
        testSuite.passed = false;
      }
      
    } catch (error) {
      testSuite.tests.push({
        name: 'Recent Logs Present',
        passed: false,
        duration: Date.now() - testStart,
        error: error instanceof Error ? error.message : String(error)
      });
      testSuite.passed = false;
    }
  }

  private async validateNoGigabyteFiles(testSuite: TestSuite): Promise<void> {
    const testStart = Date.now();
    
    try {
      const sessionsDir = '.continuum/sessions/user/shared';
      const sessions = await readdir(sessionsDir);
      
      let hasGigabyteFiles = false;
      const maxSizeBytes = 100 * 1024 * 1024; // 100MB threshold
      
      for (const session of sessions) {
        const logDir = path.join(sessionsDir, session, 'logs');
        if (existsSync(logDir)) {
          const logFiles = await readdir(logDir);
          
          for (const logFile of logFiles) {
            const logPath = path.join(logDir, logFile);
            const stats = await stat(logPath);
            
            if (stats.size > maxSizeBytes) {
              hasGigabyteFiles = true;
              break;
            }
          }
        }
        
        if (hasGigabyteFiles) break;
      }
      
      const testResult: TestResult = {
        name: 'No Gigabyte Files (Regression)',
        passed: !hasGigabyteFiles,
        duration: Date.now() - testStart
      };
      
      if (hasGigabyteFiles) {
        testResult.error = 'Large log files detected';
      }
      
      testSuite.tests.push(testResult);
      
      if (hasGigabyteFiles) {
        testSuite.passed = false;
      }
      
    } catch (error) {
      testSuite.tests.push({
        name: 'No Gigabyte Files (Regression)',
        passed: false,
        duration: Date.now() - testStart,
        error: error instanceof Error ? error.message : String(error)
      });
      testSuite.passed = false;
    }
  }

  private async validateLogFileSizes(testSuite: TestSuite): Promise<void> {
    const testStart = Date.now();
    
    try {
      const sessionsDir = '.continuum/sessions/user/shared';
      const sessions = await readdir(sessionsDir);
      
      let totalSize = 0;
      let fileCount = 0;
      
      for (const session of sessions) {
        const logDir = path.join(sessionsDir, session, 'logs');
        if (existsSync(logDir)) {
          const logFiles = await readdir(logDir);
          
          for (const logFile of logFiles) {
            const logPath = path.join(logDir, logFile);
            const stats = await stat(logPath);
            totalSize += stats.size;
            fileCount++;
          }
        }
      }
      
      const totalSizeMB = totalSize / (1024 * 1024);
      
      const reasonable = totalSizeMB < 100; // Less than 100MB total
      
      const testResult: TestResult = {
        name: 'Log File Sizes Reasonable',
        passed: reasonable,
        duration: Date.now() - testStart
      };
      
      if (!reasonable) {
        testResult.error = `Total log size: ${totalSizeMB.toFixed(2)}MB`;
      }
      
      testSuite.tests.push(testResult);
      
      if (!reasonable) {
        testSuite.passed = false;
      }
      
    } catch (error) {
      testSuite.tests.push({
        name: 'Log File Sizes Reasonable',
        passed: false,
        duration: Date.now() - testStart,
        error: error instanceof Error ? error.message : String(error)
      });
      testSuite.passed = false;
    }
  }

  private async validateSystemResponsiveness(testSuite: TestSuite): Promise<void> {
    const testStart = Date.now();
    
    try {
      // Simple responsiveness test - check if we can create and read a file quickly
      const testFile = path.join('.continuum', 'test-responsiveness.tmp');
      
      const writeStart = Date.now();
      await writeFile(testFile, 'responsiveness test');
      const writeTime = Date.now() - writeStart;
      
      const readStart = Date.now();
      await readFile(testFile, 'utf8');
      const readTime = Date.now() - readStart;
      
      // Clean up
      await unlink(testFile);
      
      const responsive = writeTime < 100 && readTime < 100;
      
      const testResult: TestResult = {
        name: 'System Responsiveness',
        passed: responsive,
        duration: Date.now() - testStart
      };
      
      if (!responsive) {
        testResult.error = `Write: ${writeTime}ms, Read: ${readTime}ms`;
      }
      
      testSuite.tests.push(testResult);
      
      if (!responsive) {
        testSuite.passed = false;
      }
      
    } catch (error) {
      testSuite.tests.push({
        name: 'System Responsiveness',
        passed: false,
        duration: Date.now() - testStart,
        error: error instanceof Error ? error.message : String(error)
      });
      testSuite.passed = false;
    }
  }

  private reportResults(): void {
    console.log('\n📊 Test Results Summary');
    console.log('========================');
    
    let totalTests = 0;
    let passedTests = 0;
    let totalDuration = 0;
    
    for (const suite of this.results) {
      totalTests += suite.tests.length;
      passedTests += suite.tests.filter(t => t.passed).length;
      totalDuration += suite.duration;
      
      const suiteStatus = suite.passed ? '✅' : '❌';
      console.log(`\n${suiteStatus} ${suite.name} (${suite.duration}ms)`);
      
      for (const test of suite.tests) {
        const testStatus = test.passed ? '  ✅' : '  ❌';
        console.log(`${testStatus} ${test.name} (${test.duration}ms)`);
        
        if (!test.passed && test.error) {
          console.log(`     Error: ${test.error}`);
        }
      }
    }
    
    console.log('\n📈 Overall Results');
    console.log('==================');
    console.log(`Tests: ${passedTests}/${totalTests} passed`);
    console.log(`Success Rate: ${((passedTests / totalTests) * 100).toFixed(1)}%`);
    console.log(`Total Duration: ${totalDuration}ms`);
    
    if (passedTests === totalTests) {
      console.log('\n🎉 All logging system tests passed!');
    } else {
      console.log('\n❌ Some logging system tests failed');
      process.exit(1);
    }
  }
}

// Run the tests if this file is executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const runner = new LoggingSystemTestRunner();
  runner.runAllTests().catch(error => {
    console.error('Test runner failed:', error);
    process.exit(1);
  });
}