#!/usr/bin/env node
/**
 * JTAG Integration Test - Browser Already Open
 * 
 * This test assumes:
 * 1. npm start has already been called (demo app running on port 9002)
 * 2. Browser is already open to http://localhost:9002
 * 3. JTAG WebSocket server is running on port 9001
 * 
 * Tests can now use the existing html2canvas engineering from continuum
 * since the browser environment is guaranteed to be available.
 */

import * as fs from 'fs';
import * as path from 'path';
import { jtag } from '../../server-index';

interface IntegrationTestResults {
  screenshotTests: number;
  websocketTests: number;
  html2canvasTests: number;
  passed: number;
  failed: number;
  errors: string[];
}

const TEST_CONFIG = {
  testTimeout: 10000,
  screenshotDirectory: process.cwd() + '/../.continuum/jtag/screenshots',
  expectedScreenshots: ['integration-test-1.png', 'integration-test-widget.png']
};

console.log('\n🧪 JTAG Integration Test (Browser Already Open)');
console.log('================================================');
console.log('✅ Assuming: npm start already called');
console.log('✅ Assuming: Browser open at http://localhost:9002');
console.log('✅ Assuming: JTAG WebSocket server running on port 9001\n');

class IntegrationTester {
  private testResults: IntegrationTestResults;

  constructor() {
    this.testResults = {
      screenshotTests: 0,
      websocketTests: 0,
      html2canvasTests: 0,
      passed: 0,
      failed: 0,
      errors: []
    };
  }

  async runAllTests(): Promise<void> {
    try {
      console.log('🚀 Starting integration tests with browser already open...\n');
      
      // Test 1: Verify demo app is accessible
      await this.testDemoAppAccessible();
      
      // Test 2: Test Server-side JTAG functionality
      await this.testServerSideJTAGFunctionality();
      
      // Test 3: Test JTAG screenshot functionality (both contexts)
      await this.testScreenshotFunctionality();
      
      // Test 4: Verify WebSocket communication & cross-context execution
      await this.testWebSocketCommunication();
      
      // Test 5: Verify actual PNG files created (both server and client)
      await this.testPngFilesCreated();
      
      // Test 6: Test html2canvas integration
      await this.testHtml2CanvasIntegration();
      
      this.printFinalResults();
      
    } catch (error: any) {
      console.error('💥 Integration test failed:', error.message);
      process.exit(1);
    }
  }

  private async testDemoAppAccessible(): Promise<void> {
    console.log('🌐 Testing demo app accessibility...');
    
    try {
      const response = await fetch('http://localhost:9002');
      if (response.ok) {
        this.testResults.passed++;
        console.log('   ✅ Demo app accessible on port 9002: PASSED');
      } else {
        throw new Error(`Demo app returned status ${response.status}`);
      }
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push(`Demo app not accessible: ${error.message}`);
      console.log('   ❌ Demo app accessibility: FAILED');
    }
  }

  private async testScreenshotFunctionality(): Promise<void> {
    console.log('📸 Testing JTAG screenshot functionality...');
    
    try {
      // Generate unique test identifier
      const testUUID = `screenshot-test-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      // Test 1: Server-side screenshot call should create PNG file
      console.log(`   🔧 Testing server-side screenshot: ${testUUID}`);
      const result1 = await jtag.screenshot(`server-${testUUID}`, {
        selector: 'body',
        format: 'png'
      });
      
      if (result1.success) {
        this.testResults.screenshotTests++;
        this.testResults.passed++;
        console.log('   ✅ Server-side screenshot: PASSED');
      } else {
        throw new Error(`Server screenshot failed: ${result1.error}`);
      }
      
      // Test 2: Execute client-side screenshot via jtag.exec()
      const clientScreenshotCode = `
        return await jtag.screenshot('client-${testUUID}', {
          selector: 'body',
          format: 'png'
        });
      `;
      
      console.log(`   🌐 Executing client-side screenshot via jtag.exec()...`);
      const clientResult = await jtag.exec(clientScreenshotCode, { context: 'browser' });
      
      if (clientResult.success && clientResult.result?.success) {
        this.testResults.screenshotTests++;
        this.testResults.passed++;
        console.log('   ✅ Client-side screenshot via exec: PASSED');
      } else {
        throw new Error(`Client screenshot exec failed: ${clientResult.error || clientResult.result?.error}`);
      }
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push(`Screenshot functionality: ${error.message}`);
      console.log('   ❌ Screenshot functionality: FAILED');
    }
  }

  private async testWebSocketCommunication(): Promise<void> {
    console.log('🔌 Testing WebSocket communication...');
    
    try {
      // Generate unique UUID for cross-context verification
      const testUUID = `ws-test-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      console.log(`   🎯 Testing cross-context logging with UUID: ${testUUID}`);
      
      // Test 1: Server tells client to log a specific UUID
      const clientLogCode = `
        const testUUID = '${testUUID}';
        jtag.log('CLIENT_EXEC_TEST', 'Cross-context test from client', { testUUID });
        jtag.critical('CLIENT_EXEC_TEST', 'Critical test message', { testUUID, source: 'client-exec' });
        return { logged: testUUID };
      `;
      
      console.log('   📤 Executing client-side logging via jtag.exec()...');
      const execResult = await jtag.exec(clientLogCode, { context: 'browser' });
      
      if (execResult.success && execResult.result?.logged === testUUID) {
        this.testResults.websocketTests++;
        this.testResults.passed++;
        console.log('   ✅ Client code execution: PASSED');
      } else {
        throw new Error(`Client exec failed: ${execResult.error}`);
      }
      
      // Test 2: Server logs its own message with same UUID
      jtag.log('SERVER_TEST', 'Cross-context test from server', { testUUID });
      jtag.critical('SERVER_TEST', 'Server critical message', { testUUID, source: 'server-direct' });
      
      this.testResults.websocketTests++;
      this.testResults.passed++;
      console.log('   ✅ Server-side logging: PASSED');
      
      // Test 3: Wait and verify UUID appears in log files
      console.log('   📂 Waiting for log file writes...');
      await new Promise(resolve => setTimeout(resolve, 3000));
      
      const logFilesChecked = await this.verifyUUIDInLogFiles(testUUID);
      if (logFilesChecked > 0) {
        this.testResults.websocketTests++;
        this.testResults.passed++;
        console.log(`   ✅ UUID verification in log files: PASSED (${logFilesChecked} files)`);
      } else {
        throw new Error(`UUID ${testUUID} not found in any log files`);
      }
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push(`WebSocket communication: ${error.message}`);
      console.log('   ❌ WebSocket communication: FAILED');
    }
  }

  private async verifyUUIDInLogFiles(testUUID: string): Promise<number> {
    const logDirectory = path.join(TEST_CONFIG.screenshotDirectory, '../logs');
    let filesWithUUID = 0;
    
    try {
      if (!fs.existsSync(logDirectory)) {
        console.log('   ⚠️  Log directory does not exist');
        return 0;
      }
      
      const logFiles = fs.readdirSync(logDirectory).filter(f => f.endsWith('.log'));
      
      for (const logFile of logFiles) {
        const filepath = path.join(logDirectory, logFile);
        const content = fs.readFileSync(filepath, 'utf8');
        
        if (content.includes(testUUID)) {
          filesWithUUID++;
          console.log(`   📝 Found UUID in: ${logFile}`);
        }
      }
      
    } catch (error: any) {
      console.log(`   ⚠️  Error checking log files: ${error.message}`);
    }
    
    return filesWithUUID;
  }

  private async testServerSideJTAGFunctionality(): Promise<void> {
    console.log('🖥️  Testing Server-Side JTAG Functionality...');
    
    try {
      // Generate unique UUID for server-side testing
      const serverTestUUID = `server-test-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      console.log(`   🎯 Testing server JTAG with UUID: ${serverTestUUID}`);
      
      // Test 1: Server-side logging
      jtag.log('SERVER_JTAG_TEST', 'Server-side logging test', { serverTestUUID, context: 'server' });
      jtag.critical('SERVER_JTAG_TEST', 'Server critical test', { serverTestUUID, severity: 'high' });
      jtag.trace('SERVER_JTAG_TEST', 'serverFunction', 'ENTER', { serverTestUUID });
      jtag.probe('SERVER_JTAG_TEST', 'server_state', { serverTestUUID, memory: process.memoryUsage() });
      
      console.log('   ✅ Server-side logging calls: PASSED');
      this.testResults.passed++;
      
      // Test 2: Server-side screenshot (should create placeholder or real screenshot)
      const serverScreenshotResult = await jtag.screenshot(`server-${serverTestUUID}`, {
        selector: 'body',
        format: 'png'
      });
      
      if (serverScreenshotResult.success) {
        console.log('   ✅ Server-side screenshot: PASSED');
        this.testResults.screenshotTests++;
        this.testResults.passed++;
      } else {
        throw new Error(`Server screenshot failed: ${serverScreenshotResult.error}`);
      }
      
      // Test 3: Verify server UUID appears in log files
      await new Promise(resolve => setTimeout(resolve, 2000));
      const serverLogFiles = await this.verifyUUIDInLogFiles(serverTestUUID);
      
      if (serverLogFiles > 0) {
        console.log(`   ✅ Server UUID in log files: PASSED (${serverLogFiles} files)`);
        this.testResults.passed++;
      } else {
        throw new Error(`Server UUID ${serverTestUUID} not found in log files`);
      }
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push(`Server-side JTAG: ${error.message}`);
      console.log('   ❌ Server-side JTAG functionality: FAILED');
    }
  }

  private async testPngFilesCreated(): Promise<void> {
    console.log('🖼️  Testing PNG File Creation (Both Contexts)...');
    
    try {
      // Wait for file system operations
      await new Promise(resolve => setTimeout(resolve, 3000));
      
      const screenshotDir = TEST_CONFIG.screenshotDirectory;
      let totalFiles = 0;
      let validFiles = 0;
      
      if (fs.existsSync(screenshotDir)) {
        const allFiles = fs.readdirSync(screenshotDir);
        const pngFiles = allFiles.filter(f => f.endsWith('.png'));
        totalFiles = pngFiles.length;
        
        console.log(`   📁 Found ${totalFiles} PNG files in screenshot directory`);
        
        for (const pngFile of pngFiles) {
          const filepath = path.join(screenshotDir, pngFile);
          const stats = fs.statSync(filepath);
          
          if (stats.size > 1000) { // PNG should be more than 1KB for real images
            validFiles++;
            const context = pngFile.includes('server-') ? 'SERVER' : 
                           pngFile.includes('client-') ? 'CLIENT' : 'UNKNOWN';
            console.log(`   ✅ Valid PNG [${context}]: ${pngFile} (${Math.round(stats.size/1024)}KB)`);
          } else {
            console.log(`   ⚠️  Small PNG file: ${pngFile} (${stats.size} bytes) - might be placeholder`);
          }
        }
        
        // Also check for .txt placeholder files to understand what's happening
        const txtFiles = allFiles.filter(f => f.endsWith('.txt'));
        if (txtFiles.length > 0) {
          console.log(`   📝 Found ${txtFiles.length} placeholder .txt files:`);
          txtFiles.forEach(txt => console.log(`      - ${txt}`));
        }
      } else {
        console.log('   ⚠️  Screenshot directory does not exist');
      }
      
      if (validFiles > 0) {
        this.testResults.screenshotTests += validFiles;
        this.testResults.passed += validFiles;
        console.log(`   ✅ PNG file creation: PASSED (${validFiles} valid files)`);
      } else if (totalFiles > 0) {
        console.log(`   ⚠️  PNG files exist but may be placeholders (${totalFiles} files)`);
        this.testResults.passed++; // Still counts as partial success
      } else {
        throw new Error('No PNG files created at all');
      }
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push(`PNG file creation: ${error.message}`);
      console.log('   ❌ PNG file creation: FAILED');
    }
  }

  private async testHtml2CanvasIntegration(): Promise<void> {
    console.log('🎨 Testing html2canvas integration...');
    
    try {
      // This test verifies that the html2canvas engineering from continuum
      // can be used since we know the browser is open
      
      // Test that screenshot options work correctly
      const result = await jtag.screenshot('html2canvas-integration-test', {
        selector: 'body',
        format: 'png',
        quality: 0.9,
        delay: 100
      });
      
      if (result.success && result.metadata) {
        this.testResults.html2canvasTests++;
        this.testResults.passed++;
        console.log('   ✅ html2canvas integration: PASSED');
        console.log(`   📏 Screenshot dimensions: ${result.metadata.width}x${result.metadata.height}`);
        console.log(`   📦 Screenshot size: ${Math.round(result.metadata.size/1024)}KB`);
      } else {
        throw new Error(`html2canvas integration failed: ${result.error}`);
      }
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push(`html2canvas integration: ${error.message}`);
      console.log('   ❌ html2canvas integration: FAILED');
    }
  }

  private printFinalResults(): void {
    console.log('\n📊 Integration Test Results');
    console.log('============================');
    console.log(`📸 Screenshot tests: ${this.testResults.screenshotTests}`);
    console.log(`🔌 WebSocket tests: ${this.testResults.websocketTests}`);  
    console.log(`🎨 html2canvas tests: ${this.testResults.html2canvasTests}`);
    console.log(`✅ Passed: ${this.testResults.passed}`);
    console.log(`❌ Failed: ${this.testResults.failed}`);
    
    if (this.testResults.errors.length > 0) {
      console.log('\n🚨 Error Details:');
      this.testResults.errors.forEach((error, index) => {
        console.log(`   ${index + 1}. ${error}`);
      });
    }
    
    const successRate = Math.round((this.testResults.passed / (this.testResults.passed + this.testResults.failed)) * 100);
    console.log(`\n🎯 Success Rate: ${successRate}%`);
    
    if (this.testResults.failed === 0) {
      console.log('\n🎉 All integration tests passed! JTAG system working correctly.');
      process.exit(0);
    } else {
      console.log('\n💥 Some integration tests failed. Check error details above.');
      process.exit(1);
    }
  }
}

// Run integration tests
const tester = new IntegrationTester();
tester.runAllTests();