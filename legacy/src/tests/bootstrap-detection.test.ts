#!/usr/bin/env npx tsx
/**
 * Bootstrap Detection Test Module
 * 
 * Tests the bootstrap detection logic that's been causing false negatives.
 * Ensures signal detection matches actual system state for autonomous development.
 */

import { promises as fs } from 'fs';
import { exec } from 'child_process';
import { promisify } from 'util';
import { JTAG_LOG_PATTERNS } from '../system/core/client/shared/JTAGClientConstants';

const execAsync = promisify(exec);

interface BootstrapTestResult {
  testName: string;
  passed: boolean;
  details: string;
  actualValue?: any;
  expectedValue?: any;
}

class BootstrapDetectionTester {
  
  async runAllTests(): Promise<BootstrapTestResult[]> {
    const results: BootstrapTestResult[] = [];
    
    console.log('🧪 BOOTSTRAP DETECTION TEST SUITE');
    console.log('='.repeat(50));
    
    // Test 1: Check if bootstrap message exists in logs
    results.push(await this.testBootstrapMessageExists());
    
    // Test 2: Test command count extraction
    results.push(await this.testCommandCountExtraction());
    
    // Test 3: Test pattern matching
    results.push(await this.testPatternMatching());
    
    // Test 4: Test actual detection function
    results.push(await this.testBootstrapDetection());
    
    // Test 5: Test command count detection
    results.push(await this.testCommandCountDetection());
    
    return results;
  }
  
  private async testBootstrapMessageExists(): Promise<BootstrapTestResult> {
    try {
      const logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      const content = await fs.readFile(logPath, 'utf8');
      const hasBootstrap = content.includes(JTAG_LOG_PATTERNS.BOOTSTRAP_STRING);
      
      return {
        testName: 'Bootstrap message exists in logs',
        passed: hasBootstrap,
        details: hasBootstrap ? 'Bootstrap message found' : 'Bootstrap message NOT found',
        actualValue: hasBootstrap,
        expectedValue: true
      };
    } catch (error: any) {
      return {
        testName: 'Bootstrap message exists in logs',
        passed: false,
        details: `Error reading log file: ${error.message}`,
      };
    }
  }
  
  private async testCommandCountExtraction(): Promise<BootstrapTestResult> {
    try {
      const logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      const { stdout } = await execAsync(`tail -50 ${logPath} | grep "${JTAG_LOG_PATTERNS.BOOTSTRAP_STRING}" 2>/dev/null || echo ""`);
      
      const match = stdout.match(JTAG_LOG_PATTERNS.COMMAND_COUNT_PATTERN);
      const commandCount = match ? parseInt(match[1]) : 0;
      
      return {
        testName: 'Command count extraction from bootstrap message',
        passed: commandCount > 0,
        details: `Extracted ${commandCount} commands from logs`,
        actualValue: commandCount,
        expectedValue: 18 // We know there should be 18 commands
      };
    } catch (error: any) {
      return {
        testName: 'Command count extraction from bootstrap message',
        passed: false,
        details: `Error extracting command count: ${error.message}`,
      };
    }
  }
  
  private async testPatternMatching(): Promise<BootstrapTestResult> {
    const testString = "✅ JTAGClient: Bootstrap complete! Discovered 18 commands";
    const bootstrapMatch = JTAG_LOG_PATTERNS.BOOTSTRAP_PATTERN.test(testString);
    const commandMatch = testString.match(JTAG_LOG_PATTERNS.COMMAND_COUNT_PATTERN);
    
    return {
      testName: 'Pattern matching on known bootstrap string',
      passed: bootstrapMatch && commandMatch !== null,
      details: `Bootstrap pattern: ${bootstrapMatch}, Command count: ${commandMatch?.[1] || 'none'}`,
      actualValue: { bootstrap: bootstrapMatch, commandCount: commandMatch?.[1] },
      expectedValue: { bootstrap: true, commandCount: '18' }
    };
  }
  
  private async testBootstrapDetection(): Promise<BootstrapTestResult> {
    try {
      const logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      const { stdout } = await execAsync(`tail -50 ${logPath} | grep "${JTAG_LOG_PATTERNS.BOOTSTRAP_STRING}" 2>/dev/null || echo ""`);
      const detected = stdout.includes(JTAG_LOG_PATTERNS.BOOTSTRAP_STRING);
      
      return {
        testName: 'Bootstrap detection (mimics signal system)',
        passed: detected,
        details: detected ? 'Detection successful' : 'Detection failed - bootstrap not found',
        actualValue: detected,
        expectedValue: true
      };
    } catch (error: any) {
      return {
        testName: 'Bootstrap detection (mimics signal system)',
        passed: false,
        details: `Detection error: ${error.message}`,
      };
    }
  }
  
  private async testCommandCountDetection(): Promise<BootstrapTestResult> {
    try {
      const logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      const { stdout } = await execAsync(`tail -50 ${logPath} | grep "${JTAG_LOG_PATTERNS.BOOTSTRAP_STRING}" 2>/dev/null || echo ""`);
      const match = stdout.match(JTAG_LOG_PATTERNS.COMMAND_COUNT_PATTERN);
      const count = match ? parseInt(match[1]) : 0;
      
      return {
        testName: 'Command count detection (mimics signal system)',
        passed: count === 18,
        details: `Detected ${count} commands (expected 18)`,
        actualValue: count,
        expectedValue: 18
      };
    } catch (error: any) {
      return {
        testName: 'Command count detection (mimics signal system)',
        passed: false,
        details: `Count detection error: ${error.message}`,
      };
    }
  }
  
  displayResults(results: BootstrapTestResult[]): void {
    console.log('');
    console.log('📊 TEST RESULTS:');
    console.log('-'.repeat(50));
    
    let passed = 0;
    let failed = 0;
    
    for (const result of results) {
      const status = result.passed ? '✅ PASS' : '❌ FAIL';
      console.log(`${status} ${result.testName}`);
      console.log(`     ${result.details}`);
      
      if (!result.passed) {
        if (result.expectedValue !== undefined && result.actualValue !== undefined) {
          console.log(`     Expected: ${JSON.stringify(result.expectedValue)}`);
          console.log(`     Actual:   ${JSON.stringify(result.actualValue)}`);
        }
      }
      console.log('');
      
      result.passed ? passed++ : failed++;
    }
    
    console.log('='.repeat(50));
    console.log(`📈 SUMMARY: ${passed} passed, ${failed} failed`);
    
    if (failed > 0) {
      console.log('');
      console.log('🔍 DIAGNOSIS: Bootstrap detection has issues that need fixing');
      console.log('💡 This explains why signal system reports false negatives');
    } else {
      console.log('');
      console.log('✅ All bootstrap detection tests passed!');
    }
  }
}

// Run tests if called directly
if (require.main === module) {
  const tester = new BootstrapDetectionTester();
  tester.runAllTests()
    .then(results => {
      tester.displayResults(results);
      process.exit(results.every(r => r.passed) ? 0 : 1);
    })
    .catch(console.error);
}

export { BootstrapDetectionTester, type BootstrapTestResult };