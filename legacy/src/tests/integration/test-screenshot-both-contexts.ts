/**
 * Comprehensive Screenshot Test - Both Server and Browser Contexts
 * Tests screenshot functionality with TRUE browser-side and server-side execution
 * 
 * REFACTORED: Uses ScreenshotTesting utilities to eliminate code duplication
 */

import {
  ScreenshotTestingEngine,
  captureTestScreenshot,
  assertScreenshotValid
} from '../shared/ScreenshotTesting';

import {
  TestExecutionEngine,
  runTestSuite
} from '../shared/TestExecution';

import {
  DOM_SELECTORS,
  TEST_TIMEOUTS
} from '../shared/TestConstants';

interface CrossContextScreenshotResult {
  testName: string;
  context: 'server' | 'browser';
  success: boolean;
  filepath?: string;
  fileSize?: number;
  error?: string;
}

async function testServerSideScreenshot(): Promise<CrossContextScreenshotResult> {
  console.log('🖥️  SERVER CONTEXT: Server-initiated screenshot');
  
  try {
    // Use ScreenshotTesting utilities - eliminates all duplicate connection/error handling
    const screenshotResult = await captureTestScreenshot(
      'Server Context Screenshot',
      DOM_SELECTORS.BODY,
      {
        filename: 'server-context-test.png',
        timeout: TEST_TIMEOUTS.STANDARD_OPERATION,
        validateFile: true
      }
    );
    
    // Validate result using shared assertions
    if (screenshotResult.success && screenshotResult.data) {
      try {
        assertScreenshotValid(screenshotResult.data, 'Server Screenshot');
        
        return {
          testName: 'serverScreenshot',
          context: 'server',
          success: true,
          filepath: screenshotResult.data.filepath,
          fileSize: screenshotResult.data.fileSize
        };
      } catch (validationError) {
        return {
          testName: 'serverScreenshot',
          context: 'server',
          success: false,
          error: `Validation failed: ${validationError}`
        };
      }
    } else {
      return {
        testName: 'serverScreenshot',
        context: 'server',
        success: false,
        error: screenshotResult.error || 'Screenshot capture failed'
      };
    }
    
  } catch (error) {
    return {
      testName: 'serverScreenshot',
      context: 'server',
      success: false,
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

async function testBrowserSideScreenshot(): Promise<CrossContextScreenshotResult> {
  console.log('🌐 BROWSER CONTEXT: Browser-initiated screenshot via exec');
  
  try {
    // Use TestExecution utilities for browser code execution
    const engine = new TestExecutionEngine();
    
    const browserResult = await engine.executeTest(
      'Browser Context Exec Screenshot',
      async (client) => {
        // Have browser execute screenshot via exec command
        const execResult = await client.commands.exec({
          code: `
            console.log('🌐 BROWSER INITIATED: Starting browser-side screenshot');
            
            // Try to call screenshot function if available
            if (typeof window !== 'undefined' && window.jtag && window.jtag.screenshot) {
              console.log('🌐 BROWSER INITIATED: Using jtag.screenshot');
              return window.jtag.screenshot('browser-initiated-test.png');
            } else {
              console.log('❌ BROWSER INITIATED: jtag.screenshot not available');
              return { success: false, error: 'Browser screenshot API not available' };
            }
          `
        });
        
        return execResult;
      },
      {
        timeout: TEST_TIMEOUTS.COMPLEX_OPERATION,
        cleanupAfter: true
      }
    );
    
    if (browserResult.success && browserResult.data?.success) {
      return {
        testName: 'browserScreenshot',
        context: 'browser',
        success: true
      };
    } else {
      return {
        testName: 'browserScreenshot',
        context: 'browser',
        success: false,
        error: browserResult.error || 'Browser execution failed'
      };
    }
    
  } catch (error) {
    return {
      testName: 'browserScreenshot',
      context: 'browser',
      success: false,
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

async function runComprehensiveScreenshotTests(): Promise<void> {
  console.log('📸 COMPREHENSIVE SCREENSHOT TESTS - Both Contexts (REFACTORED)');
  console.log('='.repeat(65));
  
  try {
    // Use TestExecution utilities for structured test suite
    const suiteResult = await runTestSuite(
      'Cross-Context Screenshot Tests',
      [
        {
          name: 'Server Context Screenshot',
          testFunction: async () => {
            const result = await testServerSideScreenshot();
            if (!result.success) {
              throw new Error(result.error || 'Server screenshot failed');
            }
            return result;
          },
          options: { timeout: TEST_TIMEOUTS.STANDARD_OPERATION }
        },
        {
          name: 'Browser Context Screenshot',
          testFunction: async () => {
            const result = await testBrowserSideScreenshot();
            if (!result.success) {
              throw new Error(result.error || 'Browser screenshot failed');
            }
            return result;
          },
          options: { timeout: TEST_TIMEOUTS.COMPLEX_OPERATION }
        }
      ]
    );
    
    // Enhanced results reporting
    console.log('🎯 CROSS-CONTEXT SCREENSHOT TEST RESULTS');
    console.log('='.repeat(45));
    
    const { summary } = suiteResult;
    console.log(`📊 Results: ${summary.passed}/${summary.total} tests passed (${summary.successRate}%)`);
    console.log(`⏱️ Total Duration: ${summary.totalDuration}ms`);
    
    // Detailed results from individual tests
    const serverResult = await testServerSideScreenshot();
    const browserResult = await testBrowserSideScreenshot();
    
    [serverResult, browserResult].forEach((result, index) => {
      const status = result.success ? '✅ PASSED' : '❌ FAILED';
      const context = result.context === 'server' ? '🖥️ ' : '🌐';
      console.log(`${index + 1}. ${context} ${result.testName}: ${status}`);
      
      if (result.success) {
        if (result.filepath) console.log(`   📁 File: ${result.filepath}`);
        if (result.fileSize) console.log(`   📏 Size: ${result.fileSize} bytes`);
      } else if (result.error) {
        console.log(`   ❌ Error: ${result.error}`);
      }
    });
    
    if (summary.passed === summary.total) {
      console.log('\n🎉 ALL CROSS-CONTEXT SCREENSHOT TESTS PASSED!');
      console.log('✅ Both server-side and browser-side screenshot functionality confirmed');
      console.log('✅ Refactored utilities eliminated code duplication successfully');
    } else {
      console.log('\n❌ SOME SCREENSHOT TESTS FAILED');
      console.error('Test failures detected in cross-context screenshot tests');
      process.exit(1);
    }
    
  } catch (error) {
    console.error('💥 Cross-context screenshot test suite failed:', error);
    process.exit(1);
  }
}

runComprehensiveScreenshotTests().catch(error => {
  console.error('💥 Comprehensive test error:', error);
  process.exit(1);
});