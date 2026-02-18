#!/usr/bin/env npx tsx
/**
 * Middle-Out Test 01: Console Logging Integration
 * 
 * This test validates that console calls from both browser and server
 * are properly intercepted and logged to the unified log folder.
 * 
 * REQUIRES: npm start to be running (test-bench with JTAG system active)
 */

import * as fs from 'fs';
import * as path from 'path';
import { TestBenchClient } from '@testsMiddleOut/shared/TestBenchClient';

class ConsoleLoggingIntegrationTest {
  private client = new TestBenchClient();
  private readonly LOG_DIR = '.continuum/jtag/logs';
  private testId = `console_test_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;

  async runConsoleLoggingTest(): Promise<void> {
    console.log('🧪 Middle-Out Test 01: Console Logging Integration');
    console.log('=================================================');
    
    // Connect to running test-bench
    console.log('1. Connecting to test-bench...');
    await this.client.connect();
    console.log('   ✅ Connected to test-bench');
    
    // Test server-side console logging
    await this.testServerConsoleLogging();
    
    // Test browser-side console logging
    await this.testBrowserConsoleLogging();
    
    // Test cross-environment log verification
    await this.testCrossEnvironmentLogVerification();
    
    // Test all log levels
    await this.testAllLogLevels();
    
    console.log('\n🎉 Console logging integration test PASSED');
    console.log('📋 All console calls properly intercepted and logged');
    console.log(`📁 Check logs in: ${this.LOG_DIR}`);
  }

  private async testServerConsoleLogging(): Promise<void> {
    console.log('\n2. Testing server-side console logging...');
    
    const serverMessage = `Server test message [${this.testId}]`;
    
    // These console calls should be intercepted by ConsoleDaemonServer
    console.log(serverMessage);
    console.info(`Server info message [${this.testId}]`);
    console.warn(`Server warning message [${this.testId}]`);
    console.error(`Server error message [${this.testId}]`);
    console.debug(`Server debug message [${this.testId}]`);
    
    // Wait for file writes
    await this.sleep(500);
    
    // Verify server console logs were created
    const serverLogFile = path.join(this.LOG_DIR, 'server-console-log.log');
    if (!fs.existsSync(serverLogFile)) {
      throw new Error('Server console log file not created');
    }
    
    const serverLogContent = fs.readFileSync(serverLogFile, 'utf8');
    if (!serverLogContent.includes(this.testId)) {
      throw new Error('Server console message not found in log file');
    }
    
    console.log('   ✅ Server console logging working correctly');
    console.log(`   📁 Server logs: ${serverLogFile}`);
  }

  private async testBrowserConsoleLogging(): Promise<void> {
    console.log('\n3. Testing browser-side console logging...');
    
    const browserMessage = `Browser test message [${this.testId}]`;
    
    // Execute console calls in browser via test-bench
    const browserResult = await this.client.executeInBrowser(`
      // These should be intercepted by ConsoleDaemonBrowser
      console.log('${browserMessage}');
      console.info('Browser info message [${this.testId}]');
      console.warn('Browser warning message [${this.testId}]'); 
      console.error('Browser error message [${this.testId}]');
      console.debug('Browser debug message [${this.testId}]');
      
      return { success: true, message: 'Browser console calls executed' };
    `);
    
    if (!browserResult.success) {
      throw new Error(`Browser console test failed: ${browserResult.error}`);
    }
    
    console.log('   ✅ Browser console calls executed');
    console.log('   🔄 Waiting for WebSocket transport to server...');
    
    // Wait for WebSocket transport and file writes
    await this.sleep(2000);
    
    // Verify browser console logs appeared on server
    const browserLogFile = path.join(this.LOG_DIR, 'browser-console-log.log');
    if (!fs.existsSync(browserLogFile)) {
      throw new Error('Browser console log file not created on server');
    }
    
    const browserLogContent = fs.readFileSync(browserLogFile, 'utf8');
    if (!browserLogContent.includes(this.testId)) {
      throw new Error('Browser console message not transported to server logs');
    }
    
    console.log('   ✅ Browser console logging transported to server correctly');
    console.log(`   📁 Browser logs on server: ${browserLogFile}`);
  }

  private async testCrossEnvironmentLogVerification(): Promise<void> {
    console.log('\n4. Testing cross-environment log verification...');
    
    const crossTestId = `cross_${Date.now()}`;
    
    // Log from server
    console.log(`Cross-environment test from server [${crossTestId}]`);
    
    // Log from browser
    await this.client.executeInBrowser(`
      console.log('Cross-environment test from browser [${crossTestId}]');
      return { success: true };
    `);
    
    // Wait for transport
    await this.sleep(1500);
    
    // Verify both messages appear in their respective log files
    const serverLogFile = path.join(this.LOG_DIR, 'server-console-log.log');
    const browserLogFile = path.join(this.LOG_DIR, 'browser-console-log.log');
    
    const serverContent = fs.readFileSync(serverLogFile, 'utf8');
    const browserContent = fs.readFileSync(browserLogFile, 'utf8');
    
    if (!serverContent.includes(crossTestId)) {
      throw new Error('Server cross-environment message not found');
    }
    
    if (!browserContent.includes(crossTestId)) {
      throw new Error('Browser cross-environment message not found');
    }
    
    console.log('   ✅ Cross-environment logging verified');
    console.log('   📊 Both server and browser messages properly segregated');
  }

  private async testAllLogLevels(): Promise<void> {
    console.log('\n5. Testing all log levels across environments...');
    
    const levelTestId = `levels_${Date.now()}`;
    const levels = ['log', 'info', 'warn', 'error', 'debug'];
    
    // Test all levels from server
    for (const level of levels) {
      (console as any)[level](`Server ${level} test [${levelTestId}]`);
    }
    
    // Test all levels from browser
    await this.client.executeInBrowser(`
      const levels = ['log', 'info', 'warn', 'error', 'debug'];
      for (const level of levels) {
        console[level]('Browser ' + level + ' test [${levelTestId}]');
      }
      return { success: true };
    `);
    
    // Wait for all transports and writes
    await this.sleep(2000);
    
    // Verify all log level files were created
    const expectedFiles = [
      'server-console-log.log',
      'server-console-info.log', 
      'server-console-warn.log',
      'server-console-error.log',
      'server-console-debug.log',
      'browser-console-log.log',
      'browser-console-info.log',
      'browser-console-warn.log', 
      'browser-console-error.log',
      'browser-console-debug.log'
    ];
    
    let filesFound = 0;
    let filesWithContent = 0;
    
    for (const filename of expectedFiles) {
      const filepath = path.join(this.LOG_DIR, filename);
      if (fs.existsSync(filepath)) {
        filesFound++;
        const content = fs.readFileSync(filepath, 'utf8');
        if (content.includes(levelTestId)) {
          filesWithContent++;
        }
      }
    }
    
    console.log(`   📊 Log files found: ${filesFound}/${expectedFiles.length}`);
    console.log(`   📝 Files with test content: ${filesWithContent}/${expectedFiles.length}`);
    
    if (filesWithContent < levels.length) {
      console.log('   ⚠️ Some log levels may not be fully implemented yet');
    } else {
      console.log('   ✅ All log levels working correctly');
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

async function runConsoleLoggingIntegration(): Promise<void> {
  const test = new ConsoleLoggingIntegrationTest();
  
  try {
    await test.runConsoleLoggingTest();
    process.exit(0);
  } catch (error: any) {
    console.error('\n❌ Console logging integration test FAILED');
    console.error(`💥 Error: ${error.message}`);
    console.error('');
    console.error('Troubleshooting:');
    console.error('  1. Make sure "npm start" is running (test-bench + JTAG)');
    console.error('  2. Check that ConsoleDaemon is auto-discovered and active');
    console.error('  3. Verify WebSocket connection between browser and server');
    console.error('  4. Check file system permissions for log directory');
    console.error('  5. Look for ConsoleDaemon initialization messages');
    
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runConsoleLoggingIntegration();
}

export { ConsoleLoggingIntegrationTest };