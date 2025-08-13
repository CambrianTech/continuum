#!/usr/bin/env npx tsx
/**
 * Comprehensive Bootstrap Detection Test Suite
 * 
 * Tests ALL scenarios that could cause false negatives in bootstrap detection.
 * This is mission-critical - false negatives break autonomous development.
 */

import { promises as fs } from 'fs';
import { exec } from 'child_process';
import { promisify } from 'util';
import { JTAG_LOG_PATTERNS, JTAG_BOOTSTRAP_MESSAGES } from '../system/core/client/shared/JTAGClientConstants';
import path from 'path';

const execAsync = promisify(exec);

interface TestScenario {
  name: string;
  description: string;
  logContent: string;
  expectedBootstrap: boolean;
  expectedCommandCount: number;
  category: 'positive' | 'negative' | 'edge-case';
}

class ComprehensiveBootstrapTester {
  
  private scenarios: TestScenario[] = [
    // POSITIVE CASES - Should detect bootstrap
    {
      name: 'Standard Bootstrap Message',
      description: 'Normal successful bootstrap with 18 commands',
      logContent: '2025-08-13T00:53:35.899Z [BROWSER_CONSOLE] ✅ JTAGClient: Bootstrap complete! Discovered 18 commands',
      expectedBootstrap: true,
      expectedCommandCount: 18,
      category: 'positive'
    },
    {
      name: 'Different Command Count',
      description: 'Bootstrap with different number of commands',
      logContent: '2025-08-13T00:53:35.899Z [BROWSER_CONSOLE] ✅ JTAGClient: Bootstrap complete! Discovered 22 commands',
      expectedBootstrap: true,
      expectedCommandCount: 22,
      category: 'positive'
    },
    {
      name: 'Bootstrap with Extra Log Lines',
      description: 'Bootstrap message buried in other log entries',
      logContent: `2025-08-13T00:53:35.897Z [BROWSER_CONSOLE] 📋 JTAGClient: Discovering available commands...
2025-08-13T00:53:35.898Z [BROWSER_CONSOLE] ✅ BROWSER: Found 18 available commands
2025-08-13T00:53:35.899Z [BROWSER_CONSOLE] ✅ JTAGClient: Bootstrap complete! Discovered 18 commands
2025-08-13T00:53:35.900Z [BROWSER_CONSOLE] ✅ JTAG Demo: UI initialized`,
      expectedBootstrap: true,
      expectedCommandCount: 18,
      category: 'positive'
    },
    {
      name: 'Multiple Bootstrap Messages',
      description: 'Multiple bootstrap messages (should use latest)',
      logContent: `2025-08-13T00:50:35.899Z [BROWSER_CONSOLE] ✅ JTAGClient: Bootstrap complete! Discovered 15 commands
2025-08-13T00:53:35.899Z [BROWSER_CONSOLE] ✅ JTAGClient: Bootstrap complete! Discovered 18 commands`,
      expectedBootstrap: true,
      expectedCommandCount: 18,
      category: 'positive'
    },
    
    // NEGATIVE CASES - Should NOT detect bootstrap
    {
      name: 'No Bootstrap Message',
      description: 'Log file with no bootstrap message',
      logContent: `2025-08-13T00:53:35.897Z [BROWSER_CONSOLE] 📋 JTAGClient: Discovering available commands...
2025-08-13T00:53:35.898Z [BROWSER_CONSOLE] ✅ BROWSER: Found 18 available commands
2025-08-13T00:53:35.900Z [BROWSER_CONSOLE] ✅ JTAG Demo: UI initialized`,
      expectedBootstrap: false,
      expectedCommandCount: 0,
      category: 'negative'
    },
    {
      name: 'Incomplete Bootstrap Message',
      description: 'Truncated or incomplete bootstrap message',
      logContent: '2025-08-13T00:53:35.899Z [BROWSER_CONSOLE] ✅ JTAGClient: Bootstrap complete! Discovered',
      expectedBootstrap: false,
      expectedCommandCount: 0,
      category: 'negative'
    },
    {
      name: 'Empty Log File',
      description: 'Completely empty log file',
      logContent: '',
      expectedBootstrap: false,
      expectedCommandCount: 0,
      category: 'negative'
    },
    
    // EDGE CASES - Tricky scenarios
    {
      name: 'Bootstrap with Zero Commands',
      description: 'Bootstrap message but zero commands discovered',
      logContent: '2025-08-13T00:53:35.899Z [BROWSER_CONSOLE] ✅ JTAGClient: Bootstrap complete! Discovered 0 commands',
      expectedBootstrap: true,
      expectedCommandCount: 0,
      category: 'edge-case'
    },
    {
      name: 'Bootstrap with Very High Command Count',
      description: 'Bootstrap with unusually high command count',
      logContent: '2025-08-13T00:53:35.899Z [BROWSER_CONSOLE] ✅ JTAGClient: Bootstrap complete! Discovered 999 commands',
      expectedBootstrap: true,
      expectedCommandCount: 999,
      category: 'edge-case'
    },
    {
      name: 'Bootstrap in Different Log Format',
      description: 'Bootstrap message with slightly different formatting',
      logContent: '2025-08-13T00:53:35.899Z [CONSOLE] ✅ JTAGClient: Bootstrap complete! Discovered 18 commands',
      expectedBootstrap: true,
      expectedCommandCount: 18,
      category: 'edge-case'
    },
    {
      name: 'Bootstrap with Extra Whitespace',
      description: 'Bootstrap message with extra spaces/tabs',
      logContent: '2025-08-13T00:53:35.899Z [BROWSER_CONSOLE]   ✅ JTAGClient: Bootstrap complete! Discovered 18 commands  ',
      expectedBootstrap: true,
      expectedCommandCount: 18,
      category: 'edge-case'
    },
    {
      name: 'Similar But Wrong Message',
      description: 'Message that looks like bootstrap but isn\'t',
      logContent: '2025-08-13T00:53:35.899Z [BROWSER_CONSOLE] ✅ JTAGClient: Connection complete! Found 18 commands',
      expectedBootstrap: false,
      expectedCommandCount: 0,
      category: 'edge-case'
    }
  ];

  async runComprehensiveTests(): Promise<void> {
    console.log('🧪 COMPREHENSIVE BOOTSTRAP DETECTION TEST SUITE');
    console.log('='.repeat(70));
    console.log('🎯 Mission: Ensure NO false negatives in autonomous development');
    console.log('');

    const results = {
      passed: 0,
      failed: 0,
      categories: {
        'positive': { passed: 0, failed: 0 },
        'negative': { passed: 0, failed: 0 },
        'edge-case': { passed: 0, failed: 0 }
      }
    };

    for (const scenario of this.scenarios) {
      const result = await this.runScenarioTest(scenario);
      
      if (result.passed) {
        results.passed++;
        results.categories[scenario.category].passed++;
        console.log(`✅ PASS: ${scenario.name}`);
      } else {
        results.failed++;
        results.categories[scenario.category].failed++;
        console.log(`❌ FAIL: ${scenario.name}`);
        console.log(`   Expected: bootstrap=${scenario.expectedBootstrap}, count=${scenario.expectedCommandCount}`);
        console.log(`   Actual:   bootstrap=${result.actualBootstrap}, count=${result.actualCommandCount}`);
        console.log(`   Details:  ${result.details}`);
      }
      console.log('');
    }

    this.displaySummary(results);
  }

  private async runScenarioTest(scenario: TestScenario): Promise<{
    passed: boolean;
    actualBootstrap: boolean;
    actualCommandCount: number;
    details: string;
  }> {
    try {
      // Create temporary test log file
      const tempLogPath = path.join(__dirname, '../.temp-test-log.txt');
      await fs.writeFile(tempLogPath, scenario.logContent);

      // Test bootstrap detection using the full pattern to match real logic
      const { stdout: bootstrapStdout } = await execAsync(`tail -50 ${tempLogPath} | grep -E "${JTAG_BOOTSTRAP_MESSAGES.BOOTSTRAP_COMPLETE_PREFIX}.*commands" 2>/dev/null || echo ""`);
      const actualBootstrap = bootstrapStdout.trim().length > 0;

      // Test command count extraction - take LAST occurrence
      let actualCommandCount = 0;
      if (bootstrapStdout.trim()) {
        const lines = bootstrapStdout.trim().split('\n');
        const lastLine = lines[lines.length - 1];
        const match = lastLine.match(JTAG_LOG_PATTERNS.COMMAND_COUNT_PATTERN);
        actualCommandCount = match ? parseInt(match[1]) : 0;
      }

      // Clean up temp file
      await fs.unlink(tempLogPath).catch(() => {}); // Ignore cleanup errors

      const passed = actualBootstrap === scenario.expectedBootstrap && 
                    actualCommandCount === scenario.expectedCommandCount;

      return {
        passed,
        actualBootstrap,
        actualCommandCount,
        details: passed ? 'Test passed as expected' : 'Test failed - detection mismatch'
      };

    } catch (error: any) {
      return {
        passed: false,
        actualBootstrap: false,
        actualCommandCount: 0,
        details: `Test error: ${error.message}`
      };
    }
  }

  private displaySummary(results: any): void {
    console.log('📊 COMPREHENSIVE TEST RESULTS');
    console.log('='.repeat(50));
    console.log(`Overall:     ${results.passed} passed, ${results.failed} failed`);
    console.log(`Positive:    ${results.categories.positive.passed} passed, ${results.categories.positive.failed} failed`);
    console.log(`Negative:    ${results.categories.negative.passed} passed, ${results.categories.negative.failed} failed`);
    console.log(`Edge Cases:  ${results.categories['edge-case'].passed} passed, ${results.categories['edge-case'].failed} failed`);
    console.log('');

    if (results.failed === 0) {
      console.log('🎉 ALL TESTS PASSED - Bootstrap detection is robust!');
      console.log('✅ System is ready for reliable autonomous development');
    } else {
      console.log('🚨 CRITICAL: Bootstrap detection has failures');
      console.log('⚠️ False negatives will break autonomous development');
      console.log('🔧 Fix required before deploying to AI agents');
    }

    console.log('');
    console.log('🎯 Why this matters:');
    console.log('  - False negatives → AI thinks system broken when it works');
    console.log('  - False positives → AI thinks system ready when it\'s not');
    console.log('  - Both scenarios break autonomous development flows');
  }

  // Integration test with actual system
  async testRealSystemDetection(): Promise<void> {
    console.log('🔗 INTEGRATION TEST - Real System Detection');
    console.log('-'.repeat(50));

    try {
      const logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      
      // Check if log exists
      const exists = await fs.access(logPath).then(() => true).catch(() => false);
      if (!exists) {
        console.log('⚠️ SKIP: No active system detected - log file missing');
        return;
      }

      // Test current system state using the same logic as signal system
      const { stdout } = await execAsync(`tail -50 ${logPath} | grep -E "${JTAG_BOOTSTRAP_MESSAGES.BOOTSTRAP_COMPLETE_PREFIX}.*commands" 2>/dev/null || echo ""`);
      const hasBootstrap = stdout.trim().length > 0;
      
      let commandCount = 0;
      if (stdout.trim()) {
        const lines = stdout.trim().split('\n');
        const lastLine = lines[lines.length - 1];
        const match = lastLine.match(JTAG_LOG_PATTERNS.COMMAND_COUNT_PATTERN);
        commandCount = match ? parseInt(match[1]) : 0;
      }

      console.log(`Bootstrap detected: ${hasBootstrap ? '✅ YES' : '❌ NO'}`);
      console.log(`Command count: ${commandCount}`);

      // Cross-check with actual JTAG ping
      try {
        const { stdout: pingResult } = await execAsync('./jtag ping --timeout 5000 2>/dev/null || echo "FAILED"');
        const systemWorks = pingResult.includes('SUCCESS');
        
        console.log(`JTAG ping test: ${systemWorks ? '✅ SUCCESS' : '❌ FAILED'}`);
        
        // This is the critical test: detection should match reality
        const detectionAccurate = hasBootstrap === systemWorks;
        console.log(`Detection accuracy: ${detectionAccurate ? '✅ ACCURATE' : '❌ INACCURATE'}`);
        
        if (!detectionAccurate) {
          console.log('🚨 CRITICAL: Detection doesn\'t match system reality!');
        }

      } catch {
        console.log('⚠️ Could not test JTAG ping - system may not be running');
      }

    } catch (error: any) {
      console.log(`❌ Integration test failed: ${error.message}`);
    }
  }
}

// Run comprehensive tests if called directly
if (require.main === module) {
  const tester = new ComprehensiveBootstrapTester();
  
  async function runAllTests() {
    await tester.runComprehensiveTests();
    console.log('');
    await tester.testRealSystemDetection();
  }
  
  runAllTests().catch(console.error);
}

export { ComprehensiveBootstrapTester };