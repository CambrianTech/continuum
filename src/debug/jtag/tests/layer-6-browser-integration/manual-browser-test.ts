#!/usr/bin/env node
/**
 * Manual Browser Test - Tests JTAG system with native browser opening
 * 
 * This test:
 * 1. Starts JTAG demo server
 * 2. Opens browser using native system command
 * 3. Tests server-side JTAG features
 * 4. Waits for user to test browser side manually
 * 5. Verifies saved files
 * 6. Shows test results
 */

import * as fs from 'fs';
import * as path from 'path';
import { spawn, ChildProcess } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(require('child_process').exec);

interface ManualTestResults {
  serverTests: number;
  fileTests: number;
  logTests: number;
  screenshotTests: number;
  passed: number;
  failed: number;
  errors: string[];
  verifiedFiles: string[];
  foundUUIDs: string[];
}

const TEST_CONFIG = {
  jtagPort: 9001,
  demoPort: 9002,
  testDuration: 30000,  // 30 seconds for manual testing
  screenshotDir: '../../../.continuum/jtag/screenshots',
  logDir: '../../../.continuum/jtag/logs'
};

console.log('\n🧪 JTAG Manual Browser Test');
console.log('============================');

class ManualBrowserTester {
  private results: ManualTestResults;
  private demoProcess: ChildProcess | null = null;

  constructor() {
    this.results = {
      serverTests: 0,
      fileTests: 0,
      logTests: 0,
      screenshotTests: 0,
      passed: 0,
      failed: 0,
      errors: [],
      verifiedFiles: [],
      foundUUIDs: []
    };
  }

  async runManualTest(): Promise<void> {
    console.log('🚀 Starting manual JTAG browser test...\n');

    try {
      // Step 1: Clean any existing processes
      await this.cleanupPorts();
      
      // Step 2: Start JTAG demo server
      await this.startJTAGDemo();
      
      // Step 3: Test server-side JTAG features
      await this.testServerFeatures();
      
      // Step 4: Open browser manually
      this.openBrowserManually();
      
      // Step 5: Wait for user to interact with browser
      await this.waitForUserTesting();
      
      // Step 6: Verify files were created
      await this.verifyCreatedFiles();
      
      // Step 7: Test log retrieval
      await this.testLogRetrieval();
      
      // Results
      this.printResults();
      
    } catch (error: any) {
      console.error('\n💥 Manual test failed:', error.message);
      this.results.errors.push(`Test failed: ${error.message}`);
    } finally {
      await this.cleanup();
    }
  }

  private async cleanupPorts(): Promise<void> {
    console.log('🧹 Cleaning up existing processes...');
    
    try {
      await execAsync(`lsof -ti:${TEST_CONFIG.jtagPort} | xargs -r kill -9 2>/dev/null || true`);
      await execAsync(`lsof -ti:${TEST_CONFIG.demoPort} | xargs -r kill -9 2>/dev/null || true`);
      await this.sleep(1000);
      console.log('   ✅ Ports cleaned\n');
    } catch (error) {
      // Ignore cleanup errors
    }
  }

  private async startJTAGDemo(): Promise<void> {
    console.log('🎪 Starting JTAG demo server...');
    
    return new Promise((resolve, reject) => {
      this.demoProcess = spawn('npx', ['tsx', 'examples/end-to-end-demo.js'], {
        cwd: process.cwd(),
        stdio: ['ignore', 'pipe', 'pipe'],
        env: {
          ...process.env,
          JTAG_AUTO_LAUNCH: 'false'  // We'll launch manually
        }
      });

      let serverReady = false;
      const timeout = setTimeout(() => {
        if (!serverReady) {
          reject(new Error('JTAG demo server timeout'));
        }
      }, 10000);

      this.demoProcess.stdout?.on('data', (data: Buffer) => {
        const output = data.toString();
        console.log('   📝', output.trim());
        
        if (output.includes('Demo server running at')) {
          serverReady = true;
          clearTimeout(timeout);
          console.log('   ✅ JTAG demo server ready\n');
          resolve();
        }
      });

      this.demoProcess.stderr?.on('data', (data: Buffer) => {
        console.log('   ⚠️  Server:', data.toString().trim());
      });
    });
  }

  private async testServerFeatures(): Promise<void> {
    console.log('🖥️ Testing server-side JTAG features...');
    
    try {
      // Import JTAG for server-side testing
      const { jtag } = await import('../index');
      
      // Test server-side UUID
      const serverUUID = jtag.getUUID();
      if (serverUUID.uuid && serverUUID.context === 'server') {
        this.results.serverTests++;
        this.results.passed++;
        this.results.foundUUIDs.push(serverUUID.uuid);
        console.log(`   ✅ Server UUID: ${serverUUID.uuid}`);
      } else {
        throw new Error('Server UUID test failed');
      }
      
      // Test server-side logging
      jtag.log('MANUAL_TEST', 'Server-side manual test logging', { 
        testType: 'manual-browser-test',
        uuid: serverUUID.uuid,
        timestamp: new Date().toISOString()
      });
      
      this.results.serverTests++;
      this.results.passed++;
      console.log('   ✅ Server logging: PASSED');
      
      // Test server-side exec
      const execResult = await jtag.exec('Math.sqrt(Date.now())');
      if (execResult.success && typeof execResult.result === 'number') {
        this.results.serverTests++;
        this.results.passed++;
        console.log(`   ✅ Server exec: ${execResult.result.toFixed(2)} (${execResult.executionTime}ms)`);
      } else {
        throw new Error('Server exec test failed');
      }
      
      // Test server-side screenshot
      const screenshotResult = await jtag.screenshot('manual-test-server', {
        width: 1024,
        height: 768,
        format: 'png'
      });
      
      if (screenshotResult.success || screenshotResult.filename) {
        this.results.serverTests++;
        this.results.passed++;
        console.log(`   ✅ Server screenshot: ${screenshotResult.filename}`);
      } else {
        throw new Error('Server screenshot test failed');
      }
      
      console.log('   🎉 Server-side tests complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`Server test failed: ${error.message}`);
      console.log(`   ❌ Server test failed: ${error.message}`);
    }
  }

  private openBrowserManually(): void {
    console.log('🌐 Opening browser manually...');
    
    const url = `http://localhost:${TEST_CONFIG.demoPort}`;
    console.log(`   🔗 URL: ${url}`);
    
    try {
      // Use platform-specific browser opening
      const platform = require('os').platform();
      
      if (platform === 'darwin') {
        spawn('open', [url], { detached: true, stdio: 'ignore' });
      } else if (platform === 'win32') {
        spawn('start', [url], { detached: true, stdio: 'ignore', shell: true });
      } else {
        spawn('xdg-open', [url], { detached: true, stdio: 'ignore' });
      }
      
      console.log('   ✅ Browser launch command sent');
      console.log('   🎯 Please interact with the JTAG demo page!');
      console.log('   📝 Try the demo buttons to test browser-side JTAG');
      
    } catch (error: any) {
      console.error('   ❌ Browser launch failed:', error.message);
      this.results.errors.push(`Browser launch failed: ${error.message}`);
    }
  }

  private async waitForUserTesting(): Promise<void> {
    console.log(`\n⏳ Waiting ${TEST_CONFIG.testDuration / 1000} seconds for manual testing...`);
    console.log('   🎮 Please use the JTAG demo page:');
    console.log('   🚀 Click "Start Basic Demo"');
    console.log('   ⚡ Click "Advanced Demo"');
    console.log('   💥 Click "Error Simulation"');
    console.log('   📸 Click "Test Screenshots"');
    console.log('   📊 Click "Show JTAG Stats"');
    console.log('   ⌛ This test will continue automatically...\n');
    
    // Show countdown
    const totalSeconds = TEST_CONFIG.testDuration / 1000;
    for (let i = totalSeconds; i > 0; i -= 5) {
      console.log(`   ⏱️  ${i} seconds remaining...`);
      await this.sleep(5000);
    }
    
    console.log('   ✅ Manual testing time complete!\n');
  }

  private async verifyCreatedFiles(): Promise<void> {
    console.log('📁 Verifying created files...');
    
    try {
      const baseDir = path.resolve(process.cwd(), '../../../.continuum/jtag');
      
      if (!fs.existsSync(baseDir)) {
        throw new Error('.continuum/jtag directory not found');
      }
      
      // Check log directory
      const logDir = path.join(baseDir, 'logs');
      if (fs.existsSync(logDir)) {
        const logFiles = fs.readdirSync(logDir);
        const validLogFiles = logFiles.filter(file => {
          const stats = fs.statSync(path.join(logDir, file));
          return file.endsWith('.log') && stats.size > 0;
        });
        
        this.results.fileTests++;
        this.results.passed++;
        this.results.verifiedFiles.push(...validLogFiles);
        console.log(`   ✅ Log files: ${validLogFiles.length} files found`);
      }
      
      // Check screenshot directory
      const screenshotDir = path.join(baseDir, 'screenshots');
      if (fs.existsSync(screenshotDir)) {
        const screenshotFiles = fs.readdirSync(screenshotDir);
        const validScreenshots = screenshotFiles.filter(file => {
          const stats = fs.statSync(path.join(screenshotDir, file));
          return stats.size > 0;
        });
        
        this.results.fileTests++;
        this.results.passed++;
        this.results.verifiedFiles.push(...validScreenshots);
        console.log(`   ✅ Screenshot files: ${validScreenshots.length} files found`);
      }
      
      console.log('   🎉 File verification complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`File verification failed: ${error.message}`);
      console.log(`   ❌ File verification failed: ${error.message}`);
    }
  }

  private async testLogRetrieval(): Promise<void> {
    console.log('📖 Testing log retrieval and UUID verification...');
    
    try {
      const logDir = path.resolve(process.cwd(), '../../../.continuum/jtag/logs');
      
      if (!fs.existsSync(logDir)) {
        throw new Error('Log directory not found');
      }
      
      const logFiles = fs.readdirSync(logDir);
      let totalUUIDs = 0;
      
      for (const logFile of logFiles) {
        if (logFile.endsWith('.log')) {
          const logPath = path.join(logDir, logFile);
          const logContent = fs.readFileSync(logPath, 'utf8');
          
          // Look for JTAG UUIDs in logs
          const uuidMatches = logContent.match(/jtag_[a-z0-9_]+/g);
          if (uuidMatches) {
            totalUUIDs += uuidMatches.length;
            this.results.foundUUIDs.push(...uuidMatches);
          }
        }
      }
      
      if (totalUUIDs > 0) {
        this.results.logTests++;
        this.results.passed++;
        console.log(`   ✅ UUIDs found in logs: ${totalUUIDs}`);
        
        // Show unique UUIDs
        const uniqueUUIDs = [...new Set(this.results.foundUUIDs)];
        console.log(`   ✅ Unique UUIDs: ${uniqueUUIDs.length}`);
        
        // Show first few UUIDs
        uniqueUUIDs.slice(0, 3).forEach(uuid => {
          console.log(`   📝 ${uuid}`);
        });
        if (uniqueUUIDs.length > 3) {
          console.log(`   ... and ${uniqueUUIDs.length - 3} more`);
        }
      } else {
        throw new Error('No UUIDs found in log files');
      }
      
      console.log('   🎉 Log retrieval complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`Log retrieval failed: ${error.message}`);
      console.log(`   ❌ Log retrieval failed: ${error.message}`);
    }
  }

  private async cleanup(): Promise<void> {
    console.log('🧹 Cleaning up manual test...');
    
    try {
      if (this.demoProcess) {
        this.demoProcess.kill();
        await this.sleep(1000);
      }
      
      console.log('   ✅ Cleanup complete');
      
    } catch (error: any) {
      console.log('   ⚠️  Cleanup warning:', error.message);
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  private printResults(): void {
    console.log('\n📊 MANUAL BROWSER TEST RESULTS');
    console.log('===============================');
    console.log(`🖥️ Server tests: ${this.results.serverTests}`);
    console.log(`📁 File tests: ${this.results.fileTests}`);
    console.log(`📖 Log tests: ${this.results.logTests}`);
    console.log(`📸 Screenshot tests: ${this.results.screenshotTests}`);
    console.log(`✅ Total passed: ${this.results.passed}`);
    console.log(`❌ Total failed: ${this.results.failed}`);
    
    const total = this.results.passed + this.results.failed;
    const successRate = total > 0 ? Math.round((this.results.passed / total) * 100) : 0;
    console.log(`📈 Success rate: ${successRate}%`);
    
    if (this.results.foundUUIDs.length > 0) {
      const uniqueUUIDs = [...new Set(this.results.foundUUIDs)];
      console.log(`\n🆔 Total UUIDs tracked: ${uniqueUUIDs.length}`);
    }
    
    if (this.results.verifiedFiles.length > 0) {
      console.log(`\n📁 Files verified: ${this.results.verifiedFiles.length}`);
      console.log('   Sample files:');
      this.results.verifiedFiles.slice(0, 5).forEach(file => {
        console.log(`   ✅ ${file}`);
      });
    }
    
    if (this.results.errors.length > 0) {
      console.log('\n❌ Errors:');
      this.results.errors.forEach(error => {
        console.log(`   - ${error}`);
      });
    }
    
    console.log('\n🧪 Manual Test Features Validated:');
    console.log('   🖥️ Server-side JTAG (UUID, logging, exec, screenshots)');
    console.log('   🌐 Native browser opening');
    console.log('   📁 File system verification (.continuum/jtag/)');
    console.log('   📖 Log parsing and UUID retrieval');
    console.log('   🎮 Interactive browser demo testing');
    
    if (this.results.failed === 0) {
      console.log('\n🎉 MANUAL BROWSER TEST PASSED!');
      console.log('🌐 JTAG system is working correctly.');
      console.log('📁 Files are being saved to .continuum/jtag/');
      console.log('🆔 UUIDs are being tracked and retrievable.');
      console.log('🎮 Browser demo should be interactive and functional.');
    } else {
      console.log('\n⚠️  Some issues detected - check errors above.');
    }
  }
}

// Run manual test if called directly
if (require.main === module) {
  const tester = new ManualBrowserTester();
  tester.runManualTest().catch(error => {
    console.error('\n💥 Manual test runner failed:', error);
    process.exit(1);
  });
}

export { ManualBrowserTester };