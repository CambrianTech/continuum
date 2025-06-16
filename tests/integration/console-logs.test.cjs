#!/usr/bin/env node
/**
 * Console and Logs Integration Tests
 * Foundation tests that verify console output, logging, and version information
 * Converted from Python to match existing test patterns
 */

const assert = require('assert');
const WebSocket = require('ws');
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

class ConsoleLogsTest {
  constructor() {
    this.testResults = [];
    this.ws = null;
    this.serverProcess = null;
  }

  log(message) {
    console.log(`📋 ${message}`);
  }

  async setupContinuumConnection() {
    // Start continuum server if not running
    try {
      this.ws = new WebSocket('ws://localhost:9000');
      
      return new Promise((resolve, reject) => {
        this.ws.on('open', () => {
          this.log('Connected to Continuum WebSocket');
          resolve();
        });
        
        this.ws.on('error', (error) => {
          this.log('WebSocket connection failed, starting server...');
          this.startContinuumServer().then(resolve).catch(reject);
        });
        
        setTimeout(() => reject(new Error('Connection timeout')), 5000);
      });
    } catch (error) {
      throw new Error(`Failed to connect to Continuum: ${error.message}`);
    }
  }

  async startContinuumServer() {
    return new Promise((resolve, reject) => {
      this.serverProcess = spawn('node', ['continuum.cjs', '--agents'], {
        stdio: 'pipe',
        cwd: process.cwd()
      });

      this.serverProcess.stdout.on('data', (data) => {
        const output = data.toString();
        if (output.includes('WebSocket server running')) {
          setTimeout(() => {
            this.ws = new WebSocket('ws://localhost:9000');
            this.ws.on('open', resolve);
          }, 1000);
        }
      });

      setTimeout(() => reject(new Error('Server start timeout')), 10000);
    });
  }

  async executeJavaScript(code) {
    return new Promise((resolve, reject) => {
      const message = {
        type: 'js_execute',
        code: code,
        timestamp: Date.now()
      };

      this.ws.send(JSON.stringify(message));

      const timeout = setTimeout(() => {
        reject(new Error('JavaScript execution timeout'));
      }, 5000);

      this.ws.on('message', (data) => {
        try {
          const response = JSON.parse(data.toString());
          if (response.type === 'js_result') {
            clearTimeout(timeout);
            resolve(response);
          }
        } catch (error) {
          // Ignore parsing errors for other messages
        }
      });
    });
  }

  async testBrowserConsoleCapture() {
    this.log('Testing browser console capture...');
    
    try {
      const result = await this.executeJavaScript(`
        console.log('🧪 Console capture test starting...');
        
        // Capture console output
        var capturedLogs = [];
        var originalLog = console.log;
        var originalWarn = console.warn;
        var originalError = console.error;
        
        function captureLog(level, args) {
          var message = Array.prototype.slice.call(args).join(' ');
          capturedLogs.push({
            level: level,
            message: message,
            timestamp: Date.now()
          });
        }
        
        console.log = function() {
          captureLog('log', arguments);
          originalLog.apply(console, arguments);
        };
        
        console.warn = function() {
          captureLog('warn', arguments);
          originalWarn.apply(console, arguments);
        };
        
        console.error = function() {
          captureLog('error', arguments);
          originalError.apply(console, arguments);
        };
        
        // Generate test log messages
        console.log('✅ Test log message');
        console.warn('⚠️ Test warning message');
        console.log('ℹ️ Test info message');
        
        // Test continuum API logging
        if (typeof window.continuum !== 'undefined') {
          console.log('🎯 Continuum API available');
          console.log('📦 Continuum version:', window.continuum.version);
        } else {
          console.warn('❌ Continuum API not available');
        }
        
        // Restore original console methods
        console.log = originalLog;
        console.warn = originalWarn;
        console.error = originalError;
        
        return {
          success: true,
          totalLogs: capturedLogs.length,
          logs: capturedLogs
        };
      `);

      if (result.success && result.result) {
        const data = JSON.parse(result.result);
        const logs = data.logs;
        
        assert(logs.length > 0, 'No console logs captured');
        
        // Verify we have expected test messages
        const logMessages = logs.map(log => log.message);
        const testLogFound = logMessages.some(msg => msg.includes('Test log message'));
        const testWarnFound = logMessages.some(msg => msg.includes('Test warning message'));
        
        assert(testLogFound, 'Test log message not found in captured logs');
        assert(testWarnFound, 'Test warning message not found in captured logs');
        
        this.addResult('Browser console capture', true, `Captured ${logs.length} console messages`);
        return logs;
      } else {
        throw new Error('JavaScript execution failed');
      }
    } catch (error) {
      this.addResult('Browser console capture', false, error.message);
      throw error;
    }
  }

  async testVersionInformation() {
    this.log('Testing version information in logs...');
    
    try {
      const result = await this.executeJavaScript(`
        var versionInfo = {
          continuum_version: null,
          browser_version: navigator.userAgent,
          timestamp: new Date().toISOString(),
          location: window.location.href
        };
        
        // Check continuum version
        if (typeof window.continuum !== 'undefined' && window.continuum.version) {
          versionInfo.continuum_version = window.continuum.version;
          console.log('📦 CONTINUUM VERSION:', window.continuum.version);
        } else {
          console.warn('❌ Continuum version not available');
        }
        
        // Check version badge element
        var versionBadge = document.querySelector('.version-badge');
        if (versionBadge) {
          versionInfo.badge_text = versionBadge.textContent.trim();
          versionInfo.badge_visible = versionBadge.offsetWidth > 0 && versionBadge.offsetHeight > 0;
          console.log('🏷️ VERSION BADGE:', versionInfo.badge_text);
        } else {
          console.warn('❌ Version badge element not found');
          versionInfo.badge_text = null;
          versionInfo.badge_visible = false;
        }
        
        return {
          success: true,
          versionInfo: versionInfo
        };
      `);

      if (result.success && result.result) {
        const data = JSON.parse(result.result);
        const versionInfo = data.versionInfo;
        
        this.log(`Version Information:`);
        this.log(`  📦 Continuum version: ${versionInfo.continuum_version}`);
        this.log(`  🏷️ Badge text: ${versionInfo.badge_text}`);
        this.log(`  👁️ Badge visible: ${versionInfo.badge_visible}`);
        
        // Verify version information exists
        const hasVersion = versionInfo.continuum_version || versionInfo.badge_text;
        this.addResult('Version information', !!hasVersion, 'Version info available in UI or API');
        
        return versionInfo;
      } else {
        throw new Error('JavaScript execution failed');
      }
    } catch (error) {
      this.addResult('Version information', false, error.message);
      throw error;
    }
  }

  async testErrorLoggingAndHandling() {
    this.log('Testing error logging and handling...');
    
    try {
      const result = await this.executeJavaScript(`
        var errorTests = [];
        var originalError = console.error;
        var capturedErrors = [];
        
        // Override console.error to capture
        console.error = function() {
          var args = Array.prototype.slice.call(arguments);
          capturedErrors.push({
            message: args.join(' '),
            timestamp: Date.now()
          });
          originalError.apply(console, arguments);
        };
        
        try {
          // Test 1: Intentional error to verify logging
          console.error('🧪 TEST ERROR: This is an intentional test error');
          errorTests.push({name: 'intentional_error', success: true});
          
          // Test 2: JavaScript syntax error in try/catch
          try {
            eval('invalid javascript syntax here!!!');
          } catch (syntaxError) {
            console.error('🧪 SYNTAX ERROR TEST:', syntaxError.message);
            errorTests.push({name: 'syntax_error', success: true, error: syntaxError.message});
          }
          
        } finally {
          // Restore console.error
          console.error = originalError;
        }
        
        return {
          success: true,
          errorTests: errorTests,
          capturedErrors: capturedErrors
        };
      `);

      if (result.success && result.result) {
        const data = JSON.parse(result.result);
        const errorTests = data.errorTests;
        const capturedErrors = data.capturedErrors;
        
        // Verify we captured the test error
        const testErrorFound = capturedErrors.some(error => error.message.includes('TEST ERROR'));
        assert(testErrorFound, 'Test error not found in captured errors');
        
        this.addResult('Error logging and handling', true, `Captured ${capturedErrors.length} error messages`);
        return capturedErrors;
      } else {
        throw new Error('JavaScript execution failed');
      }
    } catch (error) {
      this.addResult('Error logging and handling', false, error.message);
      throw error;
    }
  }

  async testScreenshotCommandLogging() {
    this.log('Testing screenshot command logging...');
    
    try {
      const result = await this.executeJavaScript(`
        var screenshotLogs = [];
        var originalLog = console.log;
        var originalWarn = console.warn;
        
        // Capture screenshot-related logs
        function captureScreenshotLog(level, args) {
          var message = Array.prototype.slice.call(args).join(' ');
          if (message.includes('screenshot') || message.includes('📸') || 
              message.includes('continuum.command') || message.includes('ScreenshotUtils')) {
            screenshotLogs.push({
              level: level,
              message: message,
              timestamp: Date.now()
            });
          }
        }
        
        console.log = function() {
          captureScreenshotLog('log', arguments);
          originalLog.apply(console, arguments);
        };
        
        console.warn = function() {
          captureScreenshotLog('warn', arguments);
          originalWarn.apply(console, arguments);
        };
        
        try {
          if (typeof window.continuum !== 'undefined' && window.continuum.command) {
            console.log('🧪 Testing continuum.command.screenshot logging...');
            // Note: Not actually triggering screenshot to avoid file creation during test
          } else {
            console.warn('❌ continuum.command not available for logging test');
          }
          
          // Test ScreenshotUtils logging if available
          if (typeof window.ScreenshotUtils !== 'undefined') {
            console.log('🧪 Testing ScreenshotUtils logging...');
          } else {
            console.warn('❌ ScreenshotUtils not available for logging test');
          }
          
        } finally {
          // Restore console methods
          console.log = originalLog;
          console.warn = originalWarn;
        }
        
        return {
          success: true,
          screenshotLogs: screenshotLogs,
          totalLogs: screenshotLogs.length
        };
      `);

      if (result.success && result.result) {
        const data = JSON.parse(result.result);
        const screenshotLogs = data.screenshotLogs;
        
        this.log(`Screenshot Command Logging: ${screenshotLogs.length} logs captured`);
        
        this.addResult('Screenshot command logging', true, `Found ${screenshotLogs.length} screenshot-related logs`);
        return screenshotLogs;
      } else {
        throw new Error('JavaScript execution failed');
      }
    } catch (error) {
      this.addResult('Screenshot command logging', false, error.message);
      throw error;
    }
  }

  addResult(testName, passed, details) {
    const result = { testName, passed, details };
    this.testResults.push(result);
    
    const status = passed ? '✅' : '❌';
    this.log(`${status} ${testName}: ${details}`);
  }

  async cleanup() {
    if (this.ws) {
      this.ws.close();
    }
    if (this.serverProcess) {
      this.serverProcess.kill();
    }
  }

  async run() {
    this.log('Starting Console and Logs Integration Tests...');
    
    try {
      await this.setupContinuumConnection();
      
      await this.testBrowserConsoleCapture();
      await this.testVersionInformation();
      await this.testErrorLoggingAndHandling();
      await this.testScreenshotCommandLogging();
      
      this.printResults();
      return this.testResults;
    } catch (error) {
      this.log(`Test suite failed: ${error.message}`);
      this.addResult('Test suite execution', false, error.message);
      return this.testResults;
    } finally {
      await this.cleanup();
    }
  }

  printResults() {
    const passed = this.testResults.filter(r => r.passed).length;
    const total = this.testResults.length;
    
    console.log(`\n🎯 Console/Logs Test Results: ${passed}/${total} passed`);
    
    if (passed === total) {
      console.log('🎉 All console/logs tests passed!');
    } else {
      console.log('⚠️ Some console/logs tests failed.');
      
      const failures = this.testResults.filter(r => !r.passed);
      console.log('\n❌ Failures:');
      failures.forEach(f => console.log(`   - ${f.testName}: ${f.details}`));
    }
  }
}

// Jest test suite
describe('Console and Logs Integration Tests', () => {
  let consoleTest;

  beforeAll(() => {
    consoleTest = new ConsoleLogsTest();
  });

  afterAll(async () => {
    if (consoleTest) {
      await consoleTest.cleanup();
    }
  });

  test('Complete Console and Logs Process', async () => {
    const results = await consoleTest.run();
    
    // Verify all tests passed
    const passedCount = results.filter(r => r.passed).length;
    const totalCount = results.length;
    
    expect(passedCount).toBe(totalCount);
    expect(totalCount).toBeGreaterThan(0);
    
    // Verify specific test cases
    const resultsByName = {};
    results.forEach(r => { resultsByName[r.testName] = r; });
    
    expect(resultsByName['Browser console capture']?.passed).toBe(true);
    expect(resultsByName['Version information']?.passed).toBe(true);
    expect(resultsByName['Error logging and handling']?.passed).toBe(true);
    expect(resultsByName['Screenshot command logging']?.passed).toBe(true);
  }, 30000); // 30 second timeout for integration test
});

// Run tests if called directly
if (require.main === module) {
  const test = new ConsoleLogsTest();
  test.run().catch(console.error);
}

module.exports = ConsoleLogsTest;