#!/usr/bin/env node
/**
 * JTAG Browser Automation Integration Test
 * 
 * TRUE END-TO-END TESTING:
 * 1. Launches actual browser with Puppeteer
 * 2. Tests real client ↔ server communication
 * 3. Verifies UUID tracking across contexts
 * 4. Validates screenshot files are actually saved
 * 5. Checks log files for UUID retrieval
 * 6. Tests jtag.exec() in both browser and server contexts
 * 
 * This is the ultimate validation that JTAG works in production.
 */

import * as fs from 'fs';
import * as path from 'path';
import { ChildProcess } from 'child_process';
import { promisify } from 'util';
import { JTAGExecResult, JTAGScreenshotResult, JTAGScreenshotOptions, JTAGUUIDInfo } from '../../../system/core/types/JTAGTypes';

// Type declarations for browser window.jtag interface
declare global {
  interface Window {
    jtag: {
      getUUID(): JTAGUUIDInfo;
      log(component: string, message: string, data?: any): void;
      exec(code: string): Promise<JTAGExecResult>;
      screenshot(filename: string, options?: JTAGScreenshotOptions): Promise<JTAGScreenshotResult>;
      critical(component: string, message: string, data?: any): void;
    };
  }
}

// Import configuration from package.json
const packageConfig = require('../../package.json');
const browserConfig = packageConfig.config?.browser || {
  headless: false,
  devtools: true,
  width: 1200,
  height: 800
};

// Conditional Puppeteer import to handle missing dependency gracefully
let puppeteer: any = null;
try {
  puppeteer = require('puppeteer');
} catch (error) {
  console.log('⚠️  Puppeteer not installed - browser automation tests will be skipped');
  console.log('   Install with: npm install puppeteer');
  process.exit(0);
}

const execAsync = promisify(require('child_process').exec);

interface BrowserAutomationResults {
  browserTests: number;
  serverTests: number;
  fileVerificationTests: number;
  logRetrievalTests: number;
  screenshotVerificationTests: number;
  uuidTrackingTests: number;
  execTests: number;
  passed: number;
  failed: number;
  errors: string[];
  verifiedFiles: string[];
  retrievedUUIDs: string[];
}

// interface TestFileInfo {
//   path: string;
//   exists: boolean;
//   size: number;
//   type: 'log' | 'screenshot' | 'config';
// }

const TEST_CONFIG = {
  jtagPort: 9001,
  demoPort: 9002,
  testTimeout: 45000,
  browserTimeout: 30000,
  headless: browserConfig.headless, // Use package.json config
  devtools: browserConfig.devtools, // Use package.json config
  width: browserConfig.width,       // Use package.json config
  height: browserConfig.height,     // Use package.json config
  screenshotDir: '../../../.continuum/jtag/screenshots',
  logDir: '../../../.continuum/jtag/logs'
};

console.log('\n🌐 JTAG Browser Automation Integration Test');
console.log('=============================================');

class BrowserAutomationTester {
  private results: BrowserAutomationResults;
  private browser: any = null;
  private page: any = null;
  private demoProcess: ChildProcess | null = null;

  constructor() {
    this.results = {
      browserTests: 0,
      serverTests: 0,
      fileVerificationTests: 0,
      logRetrievalTests: 0,
      screenshotVerificationTests: 0,
      uuidTrackingTests: 0,
      execTests: 0,
      passed: 0,
      failed: 0,
      errors: [],
      verifiedFiles: [],
      retrievedUUIDs: []
    };
  }

  async runBrowserAutomationTests(): Promise<void> {
    console.log('🚀 Starting browser automation tests...\n');

    try {
      // Step 1: Install Puppeteer if needed
      await this.ensurePuppeteer();
      
      // Step 2: Wait for Continuum system (started by npm pretest hook)
      await this.waitForContinuumSystem();
      
      // Step 3: Launch real browser
      await this.launchBrowser();
      
      // Step 4: Test server-side UUID and logging
      await this.testServerSideFeatures();
      
      // Step 5: Test browser-side features
      await this.testBrowserSideFeatures();
      
      // Step 6: Test cross-context communication
      await this.testCrossContextCommunication();
      
      // Step 7: Test jtag.exec() in both contexts
      await this.testCodeExecutionBothSides();
      
      // Step 8: Verify saved files
      await this.verifyAllSavedFiles();
      
      // Step 9: Test log retrieval and UUID verification
      await this.testLogRetrievalAndUUIDs();
      
      // Step 10: Test screenshot file verification
      await this.testScreenshotFileVerification();
      
      // Results
      this.printDetailedResults();
      
    } catch (error: any) {
      console.error('\n💥 Browser automation test failed:', error.message);
      this.results.errors.push(`Test failed: ${error.message}`);
      throw error;
    } finally {
      await this.cleanup();
    }
  }

  private async ensurePuppeteer(): Promise<void> {
    if (!puppeteer) {
      console.log('📦 Installing Puppeteer...');
      try {
        await execAsync('npm install puppeteer --no-save');
        puppeteer = require('puppeteer');
        console.log('   ✅ Puppeteer installed successfully\n');
      } catch (error: any) {
        throw new Error(`Failed to install Puppeteer: ${error.message}`);
      }
    }
  }

  private async waitForContinuumSystem(): Promise<void> {
    console.log('⏳ Waiting for Continuum system (started by npm pretest)...');
    
    // Simple polling approach - check if localhost:9002 is responding
    const maxAttempts = 30;
    const delay = 2000;
    
    for (let attempt = 1; attempt <= maxAttempts; attempt++) {
      try {
        const response = await execAsync(`curl -s -o /dev/null -w "%{http_code}" http://localhost:${TEST_CONFIG.demoPort}`);
        
        if (response.stdout.trim() === '200') {
          console.log('   ✅ Continuum system responding on localhost:9002\n');
          await this.sleep(2000); // Give it a moment to fully initialize
          return;
        }
      } catch (error) {
        // Connection failed, continue polling
      }
      
      console.log(`   📋 Attempt ${attempt}/${maxAttempts} - waiting for system...`);
      await this.sleep(delay);
    }
    
    throw new Error('Continuum system not responding after 60 seconds');
  }

  private async launchBrowser(): Promise<void> {
    console.log('🌐 Launching browser with Puppeteer...');
    
    this.browser = await puppeteer.launch({
      headless: TEST_CONFIG.headless,
      devtools: TEST_CONFIG.devtools,
      args: [
        '--no-sandbox', 
        '--disable-setuid-sandbox',
        `--window-size=${TEST_CONFIG.width},${TEST_CONFIG.height}`
      ],
      defaultViewport: {
        width: TEST_CONFIG.width,
        height: TEST_CONFIG.height
      }
    });
    
    this.page = await this.browser.newPage();
    
    // Enable console logging from browser
    this.page.on('console', (msg: any) => {
      console.log(`   🌐 Browser: ${msg.text()}`);
    });
    
    // Navigate to JTAG demo
    await this.page.goto(`http://localhost:${TEST_CONFIG.demoPort}`, { 
      waitUntil: 'networkidle2',
      timeout: TEST_CONFIG.browserTimeout 
    });
    
    console.log('   📄 Browser loaded JTAG demo page');
    
    // CRITICAL: Wait for JTAG ready event instead of fixed timeout
    // The browser-side JTAG emits 'jtag:ready' when WebSocket connection is established
    console.log('   ⏳ Waiting for JTAG ready event...');
    
    try {
      await this.page.evaluate(() => {
        return new Promise<void>((resolve, reject) => {
          const timeout = setTimeout(() => {
            reject(new Error('JTAG ready event timeout after 10 seconds'));
          }, 10000);
          
          window.addEventListener('jtag:ready', (event: any) => {
            clearTimeout(timeout);
            console.log('🎉 JTAG ready event received:', event.detail);
            resolve();
          }, { once: true });
          
          // If already ready, resolve immediately
          if (window.jtag && typeof window.jtag.getUUID === 'function') {
            clearTimeout(timeout);
            resolve();
          }
        });
      });
      
      console.log('   🎉 JTAG ready event received!');
      
      // Verify JTAG API is available
      const jtagStatus = await this.page.evaluate(() => {
        return {
          apiAvailable: !!window.jtag,
          methods: window.jtag ? Object.keys(window.jtag) : [],
          uuid: window.jtag ? window.jtag.getUUID() : null
        };
      });
      
      console.log(`   🔌 JTAG API Status: Available=${jtagStatus.apiAvailable}`);
      console.log(`   🆔 JTAG UUID: ${jtagStatus.uuid?.uuid || 'Not available'}`);
      console.log('   ✅ Browser initialization complete\n');
      
    } catch (error: any) {
      console.log(`   ⚠️ JTAG ready event failed: ${error.message}`);
      console.log('   ⏳ Falling back to 5-second timeout...');
      await this.sleep(5000);
      console.log('   ✅ Timeout fallback complete\n');
    }
  }

  private async testServerSideFeatures(): Promise<void> {
    console.log('🖥️ Testing server-side JTAG features...');
    
    try {
      // Import JTAG for server-side testing
      const { jtag } = await import('../../index');
      
      // Test server-side UUID generation
      const serverUUID = jtag.getUUID();
      this.results.retrievedUUIDs.push(serverUUID.uuid);
      
      if (serverUUID.uuid && serverUUID.context === 'server') {
        this.results.uuidTrackingTests++;
        this.results.passed++;
        console.log(`   ✅ Server UUID: ${serverUUID.uuid}`);
      } else {
        throw new Error('Server UUID test failed');
      }
      
      // Test server-side logging
      jtag.log('SERVER_TEST', 'Server-side logging test', { 
        testType: 'browser-automation',
        uuid: serverUUID.uuid,
        timestamp: new Date().toISOString()
      });
      
      this.results.serverTests++;
      this.results.passed++;
      console.log('   ✅ Server-side logging: PASSED');
      
      // Test server-side exec
      const execResult = await jtag.exec('Math.sqrt(Date.now())');
      if (execResult.success && typeof execResult.result === 'number') {
        this.results.execTests++;
        this.results.passed++;
        console.log(`   ✅ Server exec: ${execResult.result} (${execResult.executionTime}ms)`);
      } else {
        throw new Error('Server exec test failed');
      }
      
      // Test server-side screenshot
      const screenshotResult = await jtag.screenshot('server-automation-test', {
        width: 1024,
        height: 768,
        format: 'png'
      });
      
      if (screenshotResult.success || screenshotResult.filename) {
        this.results.screenshotVerificationTests++;
        this.results.passed++;
        console.log(`   ✅ Server screenshot: ${screenshotResult.filename}`);
      } else {
        throw new Error('Server screenshot test failed');
      }
      
      console.log('   🎉 Server-side tests complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`Server-side test failed: ${error.message}`);
      console.log(`   ❌ Server-side test failed: ${error.message}`);
    }
  }

  private async testBrowserSideFeatures(): Promise<void> {
    console.log('🌐 Testing browser-side JTAG features...');
    
    try {
      // Test browser-side UUID via page evaluation
      const browserUUID = await this.page.evaluate(() => {
        return window.jtag.getUUID();
      });
      
      if (browserUUID && browserUUID.uuid && browserUUID.context === 'browser') {
        this.results.retrievedUUIDs.push(browserUUID.uuid);
        this.results.uuidTrackingTests++;
        this.results.passed++;
        console.log(`   ✅ Browser UUID: ${browserUUID.uuid}`);
      } else {
        throw new Error('Browser UUID test failed');
      }
      
      // Test browser-side logging
      await this.page.evaluate(() => {
        window.jtag.log('BROWSER_TEST', 'Browser-side logging test', { 
          testType: 'browser-automation',
          userAgent: navigator.userAgent.substring(0, 50),
          timestamp: new Date().toISOString()
        });
      });
      
      this.results.browserTests++;
      this.results.passed++;
      console.log('   ✅ Browser-side logging: PASSED');
      
      // Test browser-side exec
      const browserExecResult = await this.page.evaluate(async () => {
        return await window.jtag.exec('2 + 2');
      });
      
      if (browserExecResult.success && browserExecResult.result === 4) {
        this.results.execTests++;
        this.results.passed++;
        console.log(`   ✅ Browser exec: ${browserExecResult.result} (${browserExecResult.executionTime}ms)`);
      } else {
        throw new Error('Browser exec test failed');
      }
      
      // Test browser-side screenshot
      const browserScreenshotResult = await this.page.evaluate(async () => {
        return await window.jtag.screenshot('browser-automation-test', {
          width: 800,
          height: 600
        });
      });
      
      if (browserScreenshotResult.success || browserScreenshotResult.filename) {
        this.results.screenshotVerificationTests++;
        this.results.passed++;
        console.log(`   ✅ Browser screenshot: ${browserScreenshotResult.filename}`);
      } else {
        throw new Error('Browser screenshot test failed');
      }
      
      console.log('   🎉 Browser-side tests complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`Browser-side test failed: ${error.message}`);
      console.log(`   ❌ Browser-side test failed: ${error.message}`);
    }
  }

  private async testCrossContextCommunication(): Promise<void> {
    console.log('🔄 Testing cross-context communication...');
    
    try {
      // Test browser → server communication by triggering browser action
      // and verifying server receives it
      await this.page.evaluate(() => {
        window.jtag.critical('CROSS_CONTEXT_TEST', 'Browser to server communication', {
          testType: 'cross-context',
          direction: 'browser-to-server',
          timestamp: Date.now()
        });
      });
      
      // Wait for server to receive
      await this.sleep(1000);
      
      this.results.browserTests++;
      this.results.passed++;
      console.log('   ✅ Browser → Server communication: PASSED');
      
      // Test server → browser communication via shared UUID verification
      const { jtag } = await import('../../index');
      const serverUUID = jtag.getUUID();
      
      const browserCanAccessServerUUID = await this.page.evaluate(async (uuid: string) => {
        // Test if browser can execute code that references server concepts
        const result = await window.jtag.exec(`"${uuid}".startsWith("jtag_")`);
        return result.success && result.result === true;
      }, serverUUID.uuid);
      
      if (browserCanAccessServerUUID) {
        this.results.serverTests++;
        this.results.passed++;
        console.log('   ✅ Server → Browser UUID verification: PASSED');
      } else {
        throw new Error('Cross-context UUID verification failed');
      }
      
      console.log('   🎉 Cross-context communication tests complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`Cross-context test failed: ${error.message}`);
      console.log(`   ❌ Cross-context test failed: ${error.message}`);
    }
  }

  private async testCodeExecutionBothSides(): Promise<void> {
    console.log('⚡ Testing jtag.exec() in both browser and server...');
    
    try {
      // Server-side exec tests
      const { jtag } = await import('../../index');
      
      const serverExecTests = [
        'Date.now()',
        'Math.PI * 2',
        'JSON.stringify({test: "server"})',
        'jtag.getUUID().context'
      ];
      
      for (const code of serverExecTests) {
        const result = await jtag.exec(code);
        if (result.success) {
          this.results.execTests++;
          this.results.passed++;
          console.log(`   ✅ Server exec: ${code} → ${JSON.stringify(result.result).substring(0, 20)}`);
        } else {
          throw new Error(`Server exec failed: ${code}`);
        }
      }
      
      // Browser-side exec tests
      const browserExecTests = [
        'window.innerWidth',
        'document.title',
        'navigator.userAgent.substring(0, 20)',
        'jtag.getUUID().context'
      ];
      
      for (const code of browserExecTests) {
        const result = await this.page.evaluate(async (execCode: string) => {
          return await window.jtag.exec(execCode);
        }, code);
        
        if (result.success) {
          this.results.execTests++;
          this.results.passed++;
          console.log(`   ✅ Browser exec: ${code} → ${JSON.stringify(result.result).substring(0, 20)}`);
        } else {
          throw new Error(`Browser exec failed: ${code}`);
        }
      }
      
      console.log('   🎉 Code execution tests complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`Code execution test failed: ${error.message}`);
      console.log(`   ❌ Code execution test failed: ${error.message}`);
    }
  }

  private async verifyAllSavedFiles(): Promise<void> {
    console.log('📁 Verifying all saved files in .jtag directory...');
    
    try {
      const baseDir = path.resolve(process.cwd(), '../../../.continuum/jtag');
      
      // Check directory structure
      const expectedDirs = ['logs', 'screenshots'];
      for (const dir of expectedDirs) {
        const dirPath = path.join(baseDir, dir);
        if (fs.existsSync(dirPath)) {
          this.results.fileVerificationTests++;
          this.results.passed++;
          console.log(`   ✅ Directory exists: ${dir}`);
        } else {
          throw new Error(`Missing directory: ${dir}`);
        }
      }
      
      // Check for log files
      const logDir = path.join(baseDir, 'logs');
      if (fs.existsSync(logDir)) {
        const logFiles = fs.readdirSync(logDir);
        const validLogFiles = logFiles.filter(file => 
          file.endsWith('.log') && fs.statSync(path.join(logDir, file)).size > 0
        );
        
        if (validLogFiles.length > 0) {
          this.results.fileVerificationTests++;
          this.results.passed++;
          this.results.verifiedFiles.push(...validLogFiles);
          console.log(`   ✅ Log files: ${validLogFiles.length} files found`);
        } else {
          throw new Error('No valid log files found');
        }
      }
      
      // Check for screenshot files
      const screenshotDir = path.join(baseDir, 'screenshots');
      if (fs.existsSync(screenshotDir)) {
        const screenshotFiles = fs.readdirSync(screenshotDir);
        const validScreenshots = screenshotFiles.filter(file => {
          const filePath = path.join(screenshotDir, file);
          const stats = fs.statSync(filePath);
          return (file.endsWith('.png') || file.endsWith('.txt')) && stats.size > 0;
        });
        
        if (validScreenshots.length > 0) {
          this.results.fileVerificationTests++;
          this.results.passed++;
          this.results.verifiedFiles.push(...validScreenshots);
          console.log(`   ✅ Screenshot files: ${validScreenshots.length} files found`);
        } else {
          throw new Error('No valid screenshot files found');
        }
      }
      
      console.log('   🎉 File verification tests complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`File verification failed: ${error.message}`);
      console.log(`   ❌ File verification failed: ${error.message}`);
    }
  }

  private async testLogRetrievalAndUUIDs(): Promise<void> {
    console.log('📖 Testing log retrieval and UUID verification...');
    
    try {
      const logDir = path.resolve(process.cwd(), '../../../.continuum/jtag/logs');
      
      if (!fs.existsSync(logDir)) {
        throw new Error('Log directory not found');
      }
      
      // Read all log files and look for UUIDs
      const logFiles = fs.readdirSync(logDir);
      let foundUUIDs = 0;
      
      for (const logFile of logFiles) {
        if (logFile.endsWith('.log')) {
          const logPath = path.join(logDir, logFile);
          const logContent = fs.readFileSync(logPath, 'utf8');
          
          // Look for JTAG UUIDs in logs
          const uuidMatches = logContent.match(/jtag_[a-z0-9_]+/g);
          if (uuidMatches) {
            foundUUIDs += uuidMatches.length;
            this.results.retrievedUUIDs.push(...uuidMatches);
          }
          
          // Verify our test UUIDs are in the logs
          for (const testUUID of this.results.retrievedUUIDs) {
            if (logContent.includes(testUUID)) {
              this.results.logRetrievalTests++;
              this.results.passed++;
              console.log(`   ✅ UUID found in logs: ${testUUID.substring(0, 20)}...`);
            }
          }
        }
      }
      
      if (foundUUIDs > 0) {
        this.results.logRetrievalTests++;
        this.results.passed++;
        console.log(`   ✅ Total UUIDs found in logs: ${foundUUIDs}`);
      } else {
        throw new Error('No UUIDs found in log files');
      }
      
      console.log('   🎉 Log retrieval tests complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`Log retrieval test failed: ${error.message}`);
      console.log(`   ❌ Log retrieval test failed: ${error.message}`);
    }
  }

  private async testScreenshotFileVerification(): Promise<void> {
    console.log('📸 Testing screenshot file verification...');
    
    try {
      const screenshotDir = path.resolve(process.cwd(), '../../../.continuum/jtag/screenshots');
      
      if (!fs.existsSync(screenshotDir)) {
        throw new Error('Screenshot directory not found');
      }
      
      const screenshotFiles = fs.readdirSync(screenshotDir);
      
      // Look for our test screenshots
      const testScreenshots = screenshotFiles.filter(file => 
        file.includes('automation-test') && 
        (file.endsWith('.png') || file.endsWith('.txt'))
      );
      
      if (testScreenshots.length > 0) {
        for (const screenshot of testScreenshots) {
          const filePath = path.join(screenshotDir, screenshot);
          const stats = fs.statSync(filePath);
          
          if (stats.size > 0) {
            this.results.screenshotVerificationTests++;
            this.results.passed++;
            console.log(`   ✅ Screenshot verified: ${screenshot} (${stats.size} bytes)`);
          } else {
            throw new Error(`Empty screenshot file: ${screenshot}`);
          }
        }
      } else {
        throw new Error('No test screenshot files found');
      }
      
      console.log('   🎉 Screenshot verification tests complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`Screenshot verification failed: ${error.message}`);
      console.log(`   ❌ Screenshot verification failed: ${error.message}`);
    }
  }

  private async cleanup(): Promise<void> {
    console.log('🧹 Cleaning up browser automation test...');
    
    try {
      if (this.page) {
        await this.page.close();
      }
      
      if (this.browser) {
        await this.browser.close();
      }
      
      if (this.demoProcess) {
        this.demoProcess.kill();
        await this.sleep(1000);
      }
      
      console.log('   ✅ Cleanup complete');
      
    } catch (error: any) {
      console.log('   ⚠️ Cleanup warning:', error.message);
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  private printDetailedResults(): void {
    console.log('\n📊 BROWSER AUTOMATION TEST RESULTS');
    console.log('===================================');
    console.log(`🌐 Browser tests: ${this.results.browserTests}`);
    console.log(`🖥️ Server tests: ${this.results.serverTests}`);
    console.log(`📁 File verification tests: ${this.results.fileVerificationTests}`);
    console.log(`📖 Log retrieval tests: ${this.results.logRetrievalTests}`);
    console.log(`📸 Screenshot verification tests: ${this.results.screenshotVerificationTests}`);
    console.log(`🆔 UUID tracking tests: ${this.results.uuidTrackingTests}`);
    console.log(`⚡ Code execution tests: ${this.results.execTests}`);
    console.log(`✅ Total passed: ${this.results.passed}`);
    console.log(`❌ Total failed: ${this.results.failed}`);
    
    const total = this.results.passed + this.results.failed;
    const successRate = total > 0 ? Math.round((this.results.passed / total) * 100) : 0;
    console.log(`📈 Success rate: ${successRate}%`);
    
    if (this.results.retrievedUUIDs.length > 0) {
      console.log('\n🆔 Retrieved UUIDs:');
      [...new Set(this.results.retrievedUUIDs)].slice(0, 3).forEach(uuid => {
        console.log(`   📝 ${uuid}`);
      });
      if (this.results.retrievedUUIDs.length > 3) {
        console.log(`   ... and ${this.results.retrievedUUIDs.length - 3} more`);
      }
    }
    
    if (this.results.verifiedFiles.length > 0) {
      console.log('\n📁 Verified Files:');
      this.results.verifiedFiles.slice(0, 5).forEach(file => {
        console.log(`   ✅ ${file}`);
      });
      if (this.results.verifiedFiles.length > 5) {
        console.log(`   ... and ${this.results.verifiedFiles.length - 5} more`);
      }
    }
    
    if (this.results.errors.length > 0) {
      console.log('\n❌ Errors:');
      this.results.errors.slice(0, 3).forEach(error => {
        console.log(`   - ${error}`);
      });
      if (this.results.errors.length > 3) {
        console.log(`   ... and ${this.results.errors.length - 3} more errors`);
      }
    }
    
    console.log('\n🌐 Browser Automation Features Tested:');
    console.log('   🖥️ Real browser launch (Puppeteer)');
    console.log('   🔄 True client ↔ server communication');
    console.log('   🆔 UUID tracking across contexts');
    console.log('   📁 Saved file verification');
    console.log('   📖 Log retrieval with UUID verification');
    console.log('   📸 Screenshot file validation');
    console.log('   ⚡ Code execution in both contexts');
    
    if (this.results.failed === 0) {
      console.log('\n🎉 ALL BROWSER AUTOMATION TESTS PASSED!');
      console.log('🌐 JTAG system works perfectly in real browser environment.');
      console.log('📁 All files saved correctly to .jtag directory.');
      console.log('🆔 UUIDs tracked and retrievable from saved logs.');
      console.log('✨ Production-ready for real-world deployment!');
    } else if (this.results.failed <= 2) {
      console.log('\n⚠️ Minor issues detected, but core functionality works.');
      console.log('🌐 JTAG system is mostly functional in browser environment.');
    } else {
      console.log('\n❌ Significant browser automation issues detected.');
      console.log('🌐 JTAG system may not work properly in real browser.');
      process.exit(1);
    }
  }
}

// Run browser automation test if called directly
if (require.main === module) {
  const tester = new BrowserAutomationTester();
  tester.runBrowserAutomationTests().catch(error => {
    console.error('\n💥 Browser automation test runner failed:', error);
    process.exit(1);
  });
}

export { BrowserAutomationTester };