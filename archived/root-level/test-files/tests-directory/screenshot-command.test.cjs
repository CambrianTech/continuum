#!/usr/bin/env node
/**
 * Screenshot Command Tests
 * Comprehensive testing before implementing universal API
 */

const assert = require('assert');
const path = require('path');
const fs = require('fs');

class ScreenshotCommandTest {
  constructor() {
    this.testResults = [];
    this.screenshotDir = path.join(process.cwd(), '.continuum', 'screenshots');
  }

  log(message) {
    console.log(`📋 ${message}`);
  }

  async run() {
    this.log('Starting Screenshot Command Tests...');
    
    await this.testDirectoryExists();
    await this.testScreenshotServiceExists();
    await this.testCommandDefinition();
    await this.testWebSocketIntegration();
    
    this.printResults();
  }

  async testDirectoryExists() {
    try {
      const exists = fs.existsSync(this.screenshotDir);
      this.addResult('Screenshot directory exists', exists, 'Directory should exist for file operations');
      
      if (!exists) {
        fs.mkdirSync(this.screenshotDir, { recursive: true });
        this.log(`Created screenshot directory: ${this.screenshotDir}`);
      }
    } catch (error) {
      this.addResult('Screenshot directory test', false, error.message);
    }
  }

  async testScreenshotServiceExists() {
    try {
      const ScreenshotService = require('../src/services/ScreenshotService.cjs');
      const service = new ScreenshotService({ screenshotDir: this.screenshotDir });
      
      this.addResult('ScreenshotService loads', true, 'Service class imported successfully');
      this.addResult('ScreenshotService instantiates', !!service, 'Service creates instance');
      
      // Test saveBrowserScreenshot method exists
      const hasMethod = typeof service.saveBrowserScreenshot === 'function';
      this.addResult('saveBrowserScreenshot method exists', hasMethod, 'Core method available');
      
    } catch (error) {
      this.addResult('ScreenshotService test', false, error.message);
    }
  }

  async testCommandDefinition() {
    try {
      const ScreenshotCommand = require('../src/commands/core/ScreenshotCommand.cjs');
      
      this.addResult('ScreenshotCommand loads', true, 'Command class imported successfully');
      
      const definition = ScreenshotCommand.getDefinition();
      this.addResult('Command has definition', !!definition, 'getDefinition() returns object');
      this.addResult('Command name is SCREENSHOT', definition.name === 'SCREENSHOT', 'Proper command name');
      
      const hasExecute = typeof ScreenshotCommand.execute === 'function';
      this.addResult('Execute method exists', hasExecute, 'Core execute method available');
      
    } catch (error) {
      this.addResult('ScreenshotCommand test', false, error.message);
    }
  }

  async testWebSocketIntegration() {
    try {
      const WebSocketServer = require('../src/integrations/WebSocketServer.cjs');
      this.addResult('WebSocketServer loads', true, 'WebSocket integration available');
      
      // Test that screenshot_data handler exists in the code
      const wsCode = fs.readFileSync(path.join(__dirname, '../src/integrations/WebSocketServer.cjs'), 'utf8');
      const hasScreenshotHandler = wsCode.includes('screenshot_data');
      this.addResult('screenshot_data handler exists', hasScreenshotHandler, 'WebSocket handles screenshot messages');
      
      const hasServiceCall = wsCode.includes('screenshotService');
      this.addResult('ScreenshotService integration', hasServiceCall, 'WebSocket calls ScreenshotService');
      
    } catch (error) {
      this.addResult('WebSocket integration test', false, error.message);
    }
  }

  addResult(testName, passed, details) {
    const result = { testName, passed, details };
    this.testResults.push(result);
    
    const status = passed ? '✅' : '❌';
    this.log(`${status} ${testName}: ${details}`);
  }

  printResults() {
    const passed = this.testResults.filter(r => r.passed).length;
    const total = this.testResults.length;
    
    console.log(`\n🎯 Test Results: ${passed}/${total} passed`);
    
    if (passed === total) {
      console.log('🎉 All tests passed! Safe to proceed with implementation.');
    } else {
      console.log('⚠️ Some tests failed. Fix issues before proceeding.');
      
      const failures = this.testResults.filter(r => !r.passed);
      console.log('\n❌ Failures:');
      failures.forEach(f => console.log(`   - ${f.testName}: ${f.details}`));
    }
  }
}

// Run tests if called directly
if (require.main === module) {
  const test = new ScreenshotCommandTest();
  test.run().catch(console.error);
}

module.exports = ScreenshotCommandTest;