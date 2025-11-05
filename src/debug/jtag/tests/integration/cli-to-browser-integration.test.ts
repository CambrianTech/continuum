/**
 * CLI → Browser Integration Test
 * 
 * Automated version of the manual integration testing that discovered
 * the critical gap: CLI messages not appearing in browser widgets.
 * 
 * This test validates the complete flow: CLI → Server → WebSocket → Browser → Widget
 */

import { execSync } from 'child_process';
import path from 'path';

interface TestResult {
  testName: string;
  success: boolean;
  details: any;
  timestamp: string;
  error?: string;
}

class CLIToBrowserIntegrationTest {
  private testMessage: string;
  private messageId: string | null = null;
  private results: TestResult[] = [];

  constructor() {
    this.testMessage = `CLI_INTEGRATION_TEST_${Date.now()}_${Math.random().toString(36).substring(2, 8)}`;
  }

  /**
   * Execute JTAG command and parse JSON result
   */
  private executeJTAGCommand(command: string): any {
    try {
      const fullCommand = `./jtag ${command}`;
      const output = execSync(fullCommand, { 
        cwd: path.resolve('.'),
        encoding: 'utf-8',
        timeout: 30000
      });
      
      // Extract JSON result from CLI output
      const lines = output.split('\n');
      const resultStartIndex = lines.findIndex(line => line.includes('COMMAND RESULT:'));
      if (resultStartIndex === -1) {
        throw new Error('No COMMAND RESULT found in output');
      }
      
      const jsonLines = lines.slice(resultStartIndex + 1);
      const jsonEndIndex = jsonLines.findIndex(line => line.includes('===='));
      const jsonContent = jsonLines.slice(0, jsonEndIndex).join('\n');
      
      return JSON.parse(jsonContent);
    } catch (error) {
      throw new Error(`JTAG command failed: ${command}\nError: ${error.message}`);
    }
  }

  /**
   * Test 1: Verify Shadow DOM implementation works (baseline)
   */
  async testShadowDOMBaseline(): Promise<TestResult> {
    const testName = 'Shadow DOM Baseline - Body Element Detection';
    
    try {
      const result = this.executeJTAGCommand('get-text --selector="body" --trim=true');
      
      const success = result.success && 
                     result.commandResult?.success && 
                     result.commandResult?.found &&
                     result.commandResult?.shadowDOMData?.totalShadowRoots >= 0;
      
      const testResult: TestResult = {
        testName,
        success,
        details: {
          shadowRoots: result.commandResult?.shadowDOMData?.totalShadowRoots,
          elementFound: result.commandResult?.found,
          textLength: result.commandResult?.text?.length || 0
        },
        timestamp: new Date().toISOString()
      };
      
      if (!success) {
        testResult.error = 'Shadow DOM baseline test failed - get-text on body element failed';
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: TestResult = {
        testName,
        success: false,
        details: {},
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Test 2: Send message via CLI
   */
  async testCLIMessageSend(): Promise<TestResult> {
    const testName = 'CLI Message Send';
    
    try {
      const result = this.executeJTAGCommand(
        `chat/send-message --message="${this.testMessage}" --userId="integration_test_user" --roomId="general"`
      );
      
      const success = result.success && result.messageId;
      
      if (success) {
        this.messageId = result.messageId;
      }
      
      const testResult: TestResult = {
        testName,
        success,
        details: {
          messageId: result.messageId,
          message: this.testMessage,
          serverResponse: result.success
        },
        timestamp: new Date().toISOString()
      };
      
      if (!success) {
        testResult.error = 'CLI message send failed - server did not accept message';
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: TestResult = {
        testName,
        success: false,
        details: { testMessage: this.testMessage },
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Test 3: Check if message appears in browser DOM
   */
  async testBrowserMessageAppearance(): Promise<TestResult> {
    const testName = 'Browser Message Appearance';
    
    try {
      // Wait a moment for message propagation
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      const result = this.executeJTAGCommand(
        `exec --code "return document.body.innerHTML.includes('${this.testMessage}')" --environment browser`
      );
      
      const messageFound = result.commandResult?.result === true;
      
      const testResult: TestResult = {
        testName,
        success: messageFound,
        details: {
          messageFound,
          testMessage: this.testMessage,
          executionSuccess: result.commandResult?.success
        },
        timestamp: new Date().toISOString()
      };
      
      if (!messageFound) {
        testResult.error = 'INTEGRATION GAP: CLI message not found in browser DOM - event propagation broken';
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: TestResult = {
        testName,
        success: false,
        details: { testMessage: this.testMessage },
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Test 4: Check widget content via Shadow DOM
   */
  async testWidgetContentViaShadowDOM(): Promise<TestResult> {
    const testName = 'Widget Content via Shadow DOM';
    
    try {
      const result = this.executeJTAGCommand('get-text --selector="continuum-widget" --trim=true');
      
      const widgetFound = result.commandResult?.found;
      const hasContent = result.commandResult?.text && result.commandResult.text.length > 0;
      const messageInWidget = hasContent && result.commandResult.text.includes(this.testMessage);
      
      const testResult: TestResult = {
        testName,
        success: messageInWidget,
        details: {
          widgetFound,
          hasContent,
          messageInWidget,
          textLength: result.commandResult?.text?.length || 0,
          shadowDOMRoots: result.commandResult?.shadowDOMData?.totalShadowRoots
        },
        timestamp: new Date().toISOString()
      };
      
      if (!messageInWidget) {
        if (!widgetFound) {
          testResult.error = 'Widget not found in DOM';
        } else if (!hasContent) {
          testResult.error = 'Widget found but empty - data binding broken';
        } else {
          testResult.error = 'Widget has content but test message not present - event routing issue';
        }
      }
      
      this.results.push(testResult);
      return testResult;
    } catch (error) {
      const errorResult: TestResult = {
        testName,
        success: false,
        details: {},
        timestamp: new Date().toISOString(),
        error: error.message
      };
      this.results.push(errorResult);
      return errorResult;
    }
  }

  /**
   * Run complete integration test suite
   */
  async runFullIntegrationTest(): Promise<{
    overallSuccess: boolean;
    results: TestResult[];
    summary: {
      total: number;
      passed: number;
      failed: number;
      criticalGaps: string[];
    };
  }> {
    console.log('🚀 Starting CLI → Browser Integration Test Suite');
    console.log(`📧 Test Message: ${this.testMessage}`);
    
    // Run all tests in sequence
    await this.testShadowDOMBaseline();
    await this.testCLIMessageSend();  
    await this.testBrowserMessageAppearance();
    await this.testWidgetContentViaShadowDOM();
    
    // Calculate summary
    const passed = this.results.filter(r => r.success).length;
    const failed = this.results.filter(r => !r.success).length;
    const overallSuccess = failed === 0;
    
    // Identify critical gaps
    const criticalGaps: string[] = [];
    this.results.forEach(result => {
      if (!result.success && result.error?.includes('INTEGRATION GAP')) {
        criticalGaps.push(result.error);
      }
    });
    
    const summary = {
      total: this.results.length,
      passed,
      failed,
      criticalGaps
    };
    
    // Report results
    console.log('\n📊 INTEGRATION TEST RESULTS:');
    this.results.forEach((result, index) => {
      const status = result.success ? '✅' : '❌';
      console.log(`   ${index + 1}. ${status} ${result.testName}`);
      if (!result.success && result.error) {
        console.log(`      Error: ${result.error}`);
      }
    });
    
    console.log(`\n🎯 Summary: ${passed}/${this.results.length} tests passed`);
    
    if (criticalGaps.length > 0) {
      console.log('\n🚨 CRITICAL INTEGRATION GAPS DETECTED:');
      criticalGaps.forEach((gap, index) => {
        console.log(`   ${index + 1}. ${gap}`);
      });
    }
    
    return {
      overallSuccess,
      results: this.results,
      summary
    };
  }
}

/**
 * Main test execution
 */
async function main() {
  try {
    const integrationTest = new CLIToBrowserIntegrationTest();
    const results = await integrationTest.runFullIntegrationTest();
    
    if (results.overallSuccess) {
      console.log('\n🎉 ALL INTEGRATION TESTS PASSED - CLI → Browser flow working correctly!');
      process.exit(0);
    } else {
      console.log('\n❌ INTEGRATION TESTS FAILED - CLI → Browser flow broken');
      console.log('See INTEGRATION_GAP_REPORT.md for detailed analysis');
      process.exit(1);
    }
  } catch (error) {
    console.error('💥 Integration test execution failed:', error);
    process.exit(1);
  }
}

// Run test if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}

export { CLIToBrowserIntegrationTest };