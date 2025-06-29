/**
 * SelfTest Command - TypeScript Implementation
 * JTAG System Verification with Browser/DevTools Testing
 */

import { CommandDefinition, CommandResult, CommandContext } from '../../core/BaseCommand';

export interface SelfTestParams {
  verbose?: boolean;
  mode?: 'simple' | 'browser' | 'devtools' | 'full';
}

export interface SelfTestResult {
  success: boolean;
  message: string;
  tests: TestResults;
  mode: string;
  error?: string;
}

export interface TestResults {
  simple: boolean;
  browser: boolean;
  devtools: boolean;
  screenshot: boolean;
}

export interface SelfTestContext {
  continuum?: any; // TODO: Type this properly when Continuum types are available
}

export class SelfTestCommand {
  private static readonly DEFAULT_MODE = 'simple';
  private static readonly DEFAULT_VERBOSE = true;

  static getDefinition(): CommandDefinition {
    return {
      name: 'selftest',
      description: 'Verify JTAG system health using portal sessions with browser/DevTools testing',
      icon: '🔧',
      category: 'development',
      params: JSON.stringify({
        verbose: {
          type: 'boolean',
          description: 'Show detailed JTAG output',
          default: SelfTestCommand.DEFAULT_VERBOSE
        },
        mode: {
          type: 'string',
          description: 'Test mode: simple, browser, devtools, full',
          default: SelfTestCommand.DEFAULT_MODE,
          enum: ['simple', 'browser', 'devtools', 'full']
        }
      }),
      examples: [
        'selftest --verbose=true --mode=simple',
        'selftest --mode=browser',
        'selftest --mode=devtools',
        'selftest --mode=full'
      ],
      usage: 'selftest [--verbose] [--mode=<simple|browser|devtools|full>]'
    };
  }

  static async execute(params: SelfTestParams, context?: SelfTestContext): Promise<SelfTestResult> {
    try {
      console.log(`🔧 SelfTest: Starting health verification using existing session system`);
      
      const parsedParams = this.parseParams(params);
      const {
        verbose = SelfTestCommand.DEFAULT_VERBOSE,
        mode = SelfTestCommand.DEFAULT_MODE
      } = parsedParams;

      console.log(`🔧 SelfTest: Using current session - mode=${mode}, verbose=${verbose}`);

      const testResults: TestResults = {
        simple: false,
        browser: false,
        devtools: false,
        screenshot: false
      };

      // Execute tests as fluent API chain - all in sequence, one promise
      if (verbose) console.log(`🔧 Executing JTAG test chain for mode: ${mode}`);
      
      if (context?.continuum) {
        try {
          // Execute the full JTAG test chain
          const jtag = await this.executeJTAGTestChain(mode, verbose, context.continuum);
          
          // Parse results from chain execution
          testResults.simple = jtag.simple;
          testResults.browser = jtag.browser;
          testResults.devtools = jtag.devtools;
          testResults.screenshot = jtag.screenshot;
          
        } catch (chainError) {
          console.log(`❌ JTAG test chain failed: ${chainError.message}`);
          // All tests fail if chain breaks
          testResults.simple = false;
          testResults.browser = false;
          testResults.devtools = false;
          testResults.screenshot = false;
        }
      } else {
        // Fallback to simple test only
        if (verbose) console.log(`⚠️ No continuum context - running simple test only`);
        testResults.simple = await this.testSimpleSystem(verbose);
      }

      // Calculate passed tests based on what was actually run
      const ranTests = [];
      
      // Simple test always runs
      ranTests.push(testResults.simple);
      
      if (mode === 'browser' || mode === 'devtools' || mode === 'full') {
        ranTests.push(testResults.browser);
      }
      if (mode === 'devtools' || mode === 'full') {
        ranTests.push(testResults.devtools);
      }
      if (mode === 'full') {
        ranTests.push(testResults.screenshot);
      }
      
      const allPassed = ranTests.every(result => result === true);
      const passedCount = ranTests.filter(result => result === true).length;
      const totalCount = ranTests.length;

      console.log(`🎯 SELFTEST RESULT: ${allPassed ? '✅' : '⚠️'} ${passedCount}/${totalCount} tests passed`);

      return {
        success: allPassed,
        message: `SelfTest ${mode} mode: ${passedCount}/${totalCount} tests passed`,
        tests: testResults,
        mode: mode
      };

    } catch (error) {
      console.log(`❌ SelfTest execution error: ${error.message}`);
      return {
        success: false,
        message: `SelfTest error: ${error.message}`,
        tests: {
          simple: false,
          browser: false,
          devtools: false,
          screenshot: false
        },
        mode: (params as any).mode || SelfTestCommand.DEFAULT_MODE,
        error: error.stack
      };
    }
  }

  private static async executeJTAGTestChain(mode: string, verbose: boolean, continuum: any): Promise<TestResults> {
    const testId = `jtag_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    if (verbose) console.log(`🔧 JTAG Chain starting with ID: ${testId}`);

    const results: TestResults = {
      simple: false,
      browser: false,
      devtools: false,
      screenshot: false
    };

    try {
      // Chain 1: Basic system check
      if (verbose) console.log(`   1️⃣ Simple system test...`);
      results.simple = await this.testSimpleSystem(verbose);
      
      // Chain 2: Browser JavaScript execution (all modes except simple)
      if (mode !== 'simple') {
        if (verbose) console.log(`   2️⃣ Browser JavaScript execution test...`);
        
        const jsTestScript = `
          console.log('🔧 JTAG CHAIN ${testId} - Browser JavaScript Test');
          console.log('✅ JavaScript execution confirmed in browser');
          console.log('🌐 Browser:', navigator.userAgent.substring(0, 30));
          console.error('🧪 Intentional error log test');
          console.warn('⚠️ Intentional warning log test');
          
          window.jtagResults = window.jtagResults || {};
          window.jtagResults.jsTest = {
            testId: '${testId}',
            timestamp: Date.now(),
            success: true
          };
          
          console.log('🎯 JavaScript test complete:', window.jtagResults.jsTest);
          true;
        `;
        
        const jsResult = await continuum.executeCommand('browser_js', { script: jsTestScript });
        results.browser = jsResult && jsResult.success;
        
        if (verbose && results.browser) {
          console.log(`   ✅ Browser JavaScript chain completed`);
        }
      }
      
      // Chain 3: DevTools integration (devtools/full modes)
      if (mode === 'devtools' || mode === 'full') {
        if (verbose) console.log(`   3️⃣ DevTools integration test...`);
        results.devtools = await this.testDevToolsIntegration(verbose, continuum);
      }
      
      // Chain 4: Screenshot with visual verification (full mode only)
      if (mode === 'full') {
        if (verbose) console.log(`   4️⃣ Screenshot capture with visual marker...`);
        
        // Add visual marker first
        const markerScript = `
          console.log('📸 JTAG CHAIN ${testId} - Adding screenshot marker');
          const marker = document.createElement('div');
          marker.id = 'jtag-marker-${testId}';
          marker.style.cssText = 'position: fixed; top: 10px; right: 10px; z-index: 99999; background: lime; color: black; padding: 10px; border: 3px solid red; font-weight: bold; font-family: monospace; border-radius: 5px;';
          marker.textContent = 'JTAG: ${testId}';
          document.body.appendChild(marker);
          
          setTimeout(() => {
            const el = document.getElementById('jtag-marker-${testId}');
            if (el) el.remove();
          }, 5000);
          
          console.log('✅ JTAG screenshot marker ready');
          true;
        `;
        
        await continuum.executeCommand('browser_js', { script: markerScript });
        
        // Wait for marker to render
        await new Promise(resolve => setTimeout(resolve, 500));
        
        // Capture screenshot
        const screenshotResult = await continuum.executeCommand('screenshot', {
          filename: `jtag_${testId}.png`,
          selector: 'body'
        });
        
        results.screenshot = screenshotResult && screenshotResult.success;
        
        if (verbose && results.screenshot) {
          console.log(`   ✅ Screenshot captured: jtag_${testId}.png`);
        }
      }
      
      if (verbose) {
        console.log(`🎯 JTAG Chain ${testId} completed:`, results);
      }
      
      return results;
      
    } catch (error) {
      console.log(`❌ JTAG Chain ${testId} failed: ${error.message}`);
      throw error;
    }
  }

  private static async testSimpleSystem(verbose: boolean): Promise<boolean> {
    try {
      if (verbose) console.log(`   🧪 Testing basic system components...`);
      
      // Test basic Node.js functionality
      const nodeVersion = process.version;
      if (verbose) console.log(`   ✅ Node.js version: ${nodeVersion}`);
      
      // Test memory usage
      const memUsage = process.memoryUsage();
      if (verbose) console.log(`   ✅ Memory usage: ${Math.round(memUsage.heapUsed / 1024 / 1024)}MB`);
      
      return true;
    } catch (error) {
      console.log(`   ❌ Simple system test failed: ${error.message}`);
      return false;
    }
  }

  private static async testBrowserWithJavaScript(verbose: boolean, continuum?: any): Promise<boolean> {
    try {
      if (verbose) console.log(`   🌐 Testing browser JavaScript execution...`);
      
      if (!continuum) {
        if (verbose) console.log(`   ⚠️ No continuum context available for browser test`);
        return false;
      }

      // Generate unique test ID to verify round-trip
      const testId = `selftest_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      // Create comprehensive test that shows in browser console
      const testScript = `
        console.log('🔧 SELFTEST BROWSER EXECUTION - ID: ${testId}');
        console.log('✅ JavaScript execution confirmed');
        console.log('🌐 Browser environment detected:', navigator.userAgent.substring(0, 50));
        console.error('🧪 Testing error logging - this is intentional');
        console.warn('⚠️ Testing warning logging - this is intentional');
        
        // Test DOM interaction
        if (document.body) {
          document.body.style.borderTop = '2px solid lime';
          setTimeout(() => document.body.style.borderTop = '', 1000);
          console.log('✅ DOM interaction successful');
        }
        
        // Return test results
        window.selftestResult = {
          testId: '${testId}',
          timestamp: Date.now(),
          success: true,
          browserReady: true
        };
        
        console.log('🎯 SELFTEST COMPLETE - Results:', window.selftestResult);
        true;
      `;
      
      try {
        if (verbose) console.log(`   🎯 Executing test ID: ${testId}`);
        const result = await continuum.executeCommand('browser_js', {
          script: testScript
        });
        
        if (result && result.success) {
          if (verbose) console.log(`   ✅ Browser JavaScript execution successful - check browser console for test ID: ${testId}`);
          return true;
        } else {
          if (verbose) console.log(`   ❌ Browser JavaScript execution failed`);
          return false;
        }
      } catch (jsError) {
        if (verbose) console.log(`   ❌ Browser JavaScript test failed: ${jsError.message}`);
        return false;
      }
    } catch (error) {
      console.log(`   ❌ Browser JavaScript test failed: ${error.message}`);
      return false;
    }
  }

  private static async testBrowserManagement(verbose: boolean, continuum?: any): Promise<boolean> {
    try {
      if (verbose) console.log(`   🌐 Testing browser management...`);
      
      // Test if we can access browser detection services
      if (continuum?.browserDetector) {
        if (verbose) console.log(`   ✅ Browser detector available`);
        
        // Test browser detection
        try {
          const browsers = await continuum.browserDetector.getAvailableBrowsers();
          if (verbose) console.log(`   ✅ Found ${browsers.length} browsers`);
        } catch (detectError) {
          if (verbose) console.log(`   ⚠️ Browser detection failed: ${detectError.message}`);
        }
      } else {
        if (verbose) console.log(`   ⚠️ Browser detector not available`);
      }
      
      // Test DevTools port accessibility
      try {
        const fetch = await import('node-fetch').then(mod => mod.default);
        const response = await fetch('http://localhost:9222/json');
        if (response.ok) {
          const tabs = await response.json();
          if (verbose) console.log(`   ✅ DevTools port responsive: ${tabs.length} tabs`);
          return true;
        } else {
          if (verbose) console.log(`   ⚠️ DevTools port not responding`);
          return false;
        }
      } catch (fetchError) {
        if (verbose) console.log(`   ⚠️ DevTools connection failed: ${fetchError.message}`);
        return false;
      }
    } catch (error) {
      console.log(`   ❌ Browser management test failed: ${error.message}`);
      return false;
    }
  }

  private static async testDevToolsIntegration(verbose: boolean, continuum?: any): Promise<boolean> {
    try {
      if (verbose) console.log(`   🔧 Testing DevTools integration...`);
      
      // Test WebSocket connection to DevTools
      try {
        const fetch = await import('node-fetch').then(mod => mod.default);
        const response = await fetch('http://localhost:9222/json');
        
        if (!response.ok) {
          if (verbose) console.log(`   ❌ DevTools not available`);
          return false;
        }
        
        const tabs = await response.json();
        if (tabs.length === 0) {
          if (verbose) console.log(`   ⚠️ No browser tabs found`);
          return false;
        }
        
        // Find Continuum tab
        const continuumTab = tabs.find(tab => 
          tab.url.includes('localhost:9000') || 
          tab.title.toLowerCase().includes('continuum')
        );
        
        if (continuumTab) {
          if (verbose) console.log(`   ✅ Continuum tab found: ${continuumTab.title}`);
          
          // Test WebSocket connectivity
          if (continuumTab.webSocketDebuggerUrl) {
            if (verbose) console.log(`   ✅ WebSocket debugger URL available`);
            return true;
          } else {
            if (verbose) console.log(`   ⚠️ No WebSocket debugger URL`);
            return false;
          }
        } else {
          if (verbose) console.log(`   ⚠️ Continuum tab not found`);
          return false;
        }
      } catch (devtoolsError) {
        if (verbose) console.log(`   ❌ DevTools integration failed: ${devtoolsError.message}`);
        return false;
      }
    } catch (error) {
      console.log(`   ❌ DevTools integration test failed: ${error.message}`);
      return false;
    }
  }

  private static async testScreenshotCapability(verbose: boolean, continuum?: any): Promise<boolean> {
    try {
      if (verbose) console.log(`   📸 Testing screenshot capability with visual verification...`);
      
      if (!continuum) {
        if (verbose) console.log(`   ⚠️ No continuum context available for screenshot test`);
        return false;
      }

      const screenshotId = `selftest_screenshot_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      try {
        // First, add visual marker to the page
        const markerScript = `
          console.log('📸 SELFTEST: Adding visual screenshot marker');
          const marker = document.createElement('div');
          marker.id = 'selftest-marker';
          marker.style.cssText = 'position: fixed; top: 10px; right: 10px; z-index: 9999; background: lime; color: black; padding: 8px; border: 2px solid red; font-weight: bold; font-family: monospace;';
          marker.textContent = 'SELFTEST: ${screenshotId}';
          document.body.appendChild(marker);
          
          // Remove after screenshot
          setTimeout(() => {
            const el = document.getElementById('selftest-marker');
            if (el) el.remove();
          }, 3000);
          
          console.log('✅ Screenshot marker added for verification');
          true;
        `;
        
        if (verbose) console.log(`   🎯 Adding visual marker for screenshot: ${screenshotId}`);
        await continuum.executeCommand('browser_js', { script: markerScript });
        
        // Small delay to ensure marker is rendered
        await new Promise(resolve => setTimeout(resolve, 500));
        
        // Take screenshot with the marker visible
        const result = await continuum.executeCommand('screenshot', {
          filename: `${screenshotId}.png`,
          selector: 'body'
        });
        
        if (result && result.success) {
          if (verbose) console.log(`   ✅ Screenshot captured with marker - saved as ${screenshotId}.png`);
          return true;
        } else {
          if (verbose) console.log(`   ❌ Screenshot capture failed`);
          return false;
        }
      } catch (screenshotError) {
        if (verbose) console.log(`   ❌ Screenshot test failed: ${screenshotError.message}`);
        return false;
      }
    } catch (error) {
      console.log(`   ❌ Screenshot capability test failed: ${error.message}`);
      return false;
    }
  }

  private static createErrorResult(message: string): SelfTestResult {
    return {
      success: false,
      message,
      tests: {
        simple: false,
        browser: false,
        devtools: false,
        screenshot: false
      },
      mode: 'error'
    };
  }

  private static isValidMode(mode: string): mode is SelfTestParams['mode'] {
    return ['simple', 'browser', 'devtools', 'full'].includes(mode);
  }

  protected static parseParams<T = any>(params: any): T {
    if (typeof params === 'string') {
      try {
        return JSON.parse(params) as T;
      } catch (error) {
        console.warn(`Failed to parse JSON params: ${params}`, error);
        return params as T;
      }
    }
    return params as T;
  }
}

export default SelfTestCommand;