#!/usr/bin/env tsx
/**
 * Console Logging Failure Detection Test
 * 
 * CRITICAL: Tests that Error objects are properly serialized and captured in logs
 * This test was created after discovering that our logging system was capturing
 * "❌ BROWSER EXEC: Failed: {}" instead of the actual error details.
 */

import { promises as fs } from 'fs';
import { exec } from 'child_process';
import { promisify } from 'util';
import path from 'path';

const execAsync = promisify(exec);

interface LoggingTestResult {
  passed: boolean;
  details: string;
  actualLogContent?: string;
  expectedPattern?: string;
}

class ConsoleLoggingFailureTest {
  private logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-error.log';
  
  async runConsoleLoggingTests(): Promise<void> {
    console.log('🧪 CONSOLE LOGGING FAILURE DETECTION TESTS');
    console.log('=' .repeat(60));
    console.log('🎯 Mission: Ensure Error objects are properly captured in logs');
    console.log('');
    
    const results = {
      passed: 0,
      failed: 0,
      tests: [] as LoggingTestResult[]
    };
    
    // Test 1: Error Object Serialization
    const errorSerializationTest = await this.testErrorObjectSerialization();
    results.tests.push(errorSerializationTest);
    if (errorSerializationTest.passed) results.passed++;
    else results.failed++;
    
    // Test 2: Current System Error Detection
    const currentSystemTest = await this.testCurrentSystemErrorCapture();
    results.tests.push(currentSystemTest);
    if (currentSystemTest.passed) results.passed++;
    else results.failed++;
    
    // Test 3: Error Stack Trace Capture
    const stackTraceTest = await this.testErrorStackTraceCapture();
    results.tests.push(stackTraceTest);
    if (stackTraceTest.passed) results.passed++;
    else results.failed++;
    
    // Test 4: Warning Capture
    const warningTest = await this.testWarningCapture();
    results.tests.push(warningTest);
    if (warningTest.passed) results.passed++;
    else results.failed++;
    
    this.displayResults(results);
  }
  
  private async testErrorObjectSerialization(): Promise<LoggingTestResult> {
    console.log('🔬 Test 1: Error Object Serialization');
    
    try {
      // Test how Error objects serialize
      const testError = new Error("Command 'log' not available. Available commands: click, compile-typescript, exec");
      testError.stack = `Error: Command 'log' not available
    at Object.get (index.js:6239:17)
    at testCrossContext (index.js:6789:23)`;
      
      // Test different serialization approaches
      const jsonStringify = JSON.stringify(testError);
      const errorToString = testError.toString();
      const errorMessage = testError.message;
      const errorStack = testError.stack;
      
      console.log(`   - JSON.stringify(error): "${jsonStringify}"`);
      console.log(`   - error.toString(): "${errorToString}"`);
      console.log(`   - error.message: "${errorMessage}"`);
      console.log(`   - error.stack length: ${errorStack?.length || 0} chars`);
      
      // The issue: JSON.stringify(Error) returns "{}" 
      if (jsonStringify === '{}') {
        console.log('   ❌ CONFIRMED: JSON.stringify(Error) returns "{}" - THIS IS THE BUG!');
        return {
          passed: false,
          details: 'Error objects serialize to {} with JSON.stringify - need custom serialization',
          actualLogContent: jsonStringify,
          expectedPattern: 'Error message and stack trace'
        };
      } else {
        console.log('   ✅ Error serialization working properly');
        return {
          passed: true,
          details: 'Error objects serialize properly'
        };
      }
      
    } catch (error) {
      return {
        passed: false,
        details: `Test setup failed: ${error}`
      };
    }
  }
  
  private async testCurrentSystemErrorCapture(): Promise<LoggingTestResult> {
    console.log('🔬 Test 2: Current System Error Log Analysis');
    
    try {
      // Read the actual error log from our recent test run
      const exists = await fs.access(this.logPath).then(() => true).catch(() => false);
      if (!exists) {
        return {
          passed: false,
          details: 'Error log file not found - system may not be capturing errors'
        };
      }
      
      const logContent = await fs.readFile(this.logPath, 'utf-8');
      console.log(`   - Log file size: ${logContent.length} bytes`);
      console.log(`   - Recent error entries: ${logContent.split('\\n').filter(l => l.trim()).length}`);
      
      // Check for empty error objects in logs
      const emptyErrorPattern = /Failed: {}/g;
      const emptyErrorMatches = logContent.match(emptyErrorPattern);
      
      if (emptyErrorMatches && emptyErrorMatches.length > 0) {
        console.log(`   ❌ FOUND ${emptyErrorMatches.length} instances of "Failed: {}" - ERROR INFO LOST!`);
        console.log(`   📋 Sample error entry: ${logContent.split('\\n')[0]}`);
        
        return {
          passed: false,
          details: `Found ${emptyErrorMatches.length} instances of empty error objects in logs`,
          actualLogContent: logContent.slice(0, 200) + '...',
          expectedPattern: 'Error messages with actual error details'
        };
      } else {
        console.log('   ✅ No empty error objects found in logs');
        return {
          passed: true,
          details: 'Error logs contain proper error information'
        };
      }
      
    } catch (error) {
      return {
        passed: false,
        details: `Failed to analyze error logs: ${error}`
      };
    }
  }
  
  private async testErrorStackTraceCapture(): Promise<LoggingTestResult> {
    console.log('🔬 Test 3: Error Stack Trace Capture');
    
    try {
      // Check if our logs capture stack traces from browser devtools
      const logContent = await fs.readFile(this.logPath, 'utf-8').catch(() => '');
      
      // Look for stack trace patterns
      const stackTracePatterns = [
        /at Object\.get \(index\.js:/,
        /at testCrossContext/,
        /at testBrowserLogging/,
        /at eval \(eval at execute/
      ];
      
      let stackTracesCaptured = 0;
      for (const pattern of stackTracePatterns) {
        if (pattern.test(logContent)) {
          stackTracesCaptured++;
        }
      }
      
      console.log(`   - Stack trace patterns found: ${stackTracesCaptured}/${stackTracePatterns.length}`);
      
      if (stackTracesCaptured === 0) {
        console.log('   ❌ No stack traces captured in error logs');
        return {
          passed: false,
          details: 'Stack traces from browser errors not captured in logs',
          actualLogContent: logContent.slice(0, 300) + '...',
          expectedPattern: 'Stack traces like "at Object.get (index.js:6239:17)"'
        };
      } else {
        console.log('   ✅ Stack traces found in error logs');
        return {
          passed: true,
          details: `${stackTracesCaptured} stack trace patterns captured`
        };
      }
      
    } catch (error) {
      return {
        passed: false,
        details: `Stack trace test failed: ${error}`
      };
    }
  }
  
  private async testWarningCapture(): Promise<LoggingTestResult> {
    console.log('🔬 Test 4: Warning Message Capture');
    
    try {
      // Check for browser warning logs
      const warningLogPath = this.logPath.replace('error.log', 'warn.log');
      const warningExists = await fs.access(warningLogPath).then(() => true).catch(() => false);
      
      if (!warningExists) {
        console.log('   ❌ No warning log file found');
        return {
          passed: false,
          details: 'Warning log file does not exist - warnings not being captured',
          expectedPattern: 'browser-console-warn.log file with warning messages'
        };
      }
      
      const warningContent = await fs.readFile(warningLogPath, 'utf-8');
      
      // Look for the specific warning we saw in browser devtools
      const expectedWarning = 'removeEventListener not fully implemented for browser WebSocket adapter';
      const hasExpectedWarning = warningContent.includes(expectedWarning);
      
      console.log(`   - Warning log size: ${warningContent.length} bytes`);
      console.log(`   - Expected warning found: ${hasExpectedWarning ? '✅' : '❌'}`);
      
      if (!hasExpectedWarning && warningContent.length === 0) {
        return {
          passed: false,
          details: 'Warning log exists but is empty - warnings not being captured',
          actualLogContent: `Empty file (${warningContent.length} bytes)`,
          expectedPattern: expectedWarning
        };
      } else if (hasExpectedWarning) {
        return {
          passed: true,
          details: 'Expected warning found in warning logs'
        };
      } else {
        return {
          passed: false,
          details: 'Warning log has content but missing expected warning',
          actualLogContent: warningContent.slice(0, 200) + '...',
          expectedPattern: expectedWarning
        };
      }
      
    } catch (error) {
      return {
        passed: false,
        details: `Warning capture test failed: ${error}`
      };
    }
  }
  
  private displayResults(results: { passed: number, failed: number, tests: LoggingTestResult[] }): void {
    console.log('');
    console.log('📊 CONSOLE LOGGING TEST RESULTS');
    console.log('=' .repeat(50));
    console.log(`Overall: ${results.passed} passed, ${results.failed} failed`);
    console.log('');
    
    results.tests.forEach((test, index) => {
      const status = test.passed ? '✅ PASS' : '❌ FAIL';
      console.log(`${status}: Test ${index + 1} - ${test.details}`);
      
      if (!test.passed && test.actualLogContent) {
        console.log(`   Actual: ${test.actualLogContent}`);
      }
      if (!test.passed && test.expectedPattern) {
        console.log(`   Expected: ${test.expectedPattern}`);
      }
    });
    
    console.log('');
    
    if (results.failed === 0) {
      console.log('🎉 ALL LOGGING TESTS PASSED - Console logging is working properly!');
    } else {
      console.log('🚨 CRITICAL LOGGING FAILURES DETECTED');
      console.log('⚠️ Error information is being lost in our logging system');
      console.log('🔧 IMMEDIATE ACTION REQUIRED:');
      console.log('   1. Fix Error object serialization');
      console.log('   2. Ensure warning capture is working');
      console.log('   3. Add proper error handling for console routing');
    }
  }
}

// Run the test if called directly
if (require.main === module) {
  const tester = new ConsoleLoggingFailureTest();
  tester.runConsoleLoggingTests().catch(console.error);
}

export { ConsoleLoggingFailureTest };