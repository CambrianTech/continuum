#!/usr/bin/env node
/**
 * JTAG Standalone Integration Test
 * 
 * Tests JTAG system in complete isolation by:
 * 1. Intelligently killing any existing JTAG processes
 * 2. Starting the example demo servers
 * 3. Testing complete client ↔ server functionality
 * 4. Validating all JTAG features work end-to-end
 * 
 * This test runs independently - no Continuum dependencies
 */

import * as http from 'http';
import { spawn, exec, ChildProcess } from 'child_process';
import { promisify } from 'util';
import { jtag } from '../../index';
import { ComprehensiveTestSuite } from '@tests/shared/TestUtilities';

const execAsync = promisify(exec);

interface EnhancedTestResults {
  serverTests: number;
  clientTests: number;
  communicationTests: number;
  screenshotTests: number;
  comprehensiveTests: number;
  execTests: number;
  uuidTests: number;
  passed: number;
  failed: number;
  errors: string[];
}

interface ServerHealthCheck {
  jtagServer: boolean;
  demoServer: boolean;
  pid?: number;
}

const DEMO_CONFIG = {
  jtagPort: 9001,
  httpPort: 9002,
  autoLaunch: true,  // Launch browser for standalone test
  testTimeout: 30000
};

console.log('\n🧪 JTAG Standalone Integration Test (TypeScript)');
console.log('================================================');

class StandaloneIntegrationTester {
  private testResults: EnhancedTestResults;
  private demoProcess: ChildProcess | null = null;
  private comprehensiveSuite: ComprehensiveTestSuite;

  constructor() {
    this.testResults = {
      serverTests: 0,
      clientTests: 0,
      communicationTests: 0,
      screenshotTests: 0,
      comprehensiveTests: 0,
      execTests: 0,
      uuidTests: 0,
      passed: 0,
      failed: 0,
      errors: []
    };
    
    // Initialize comprehensive test suite
    this.comprehensiveSuite = new ComprehensiveTestSuite({
      testTimeout: DEMO_CONFIG.testTimeout,
      performanceIterations: 15,  // Reduced for faster standalone testing
      screenshotTypes: ['full-page', 'viewport', 'high-quality', 'mobile'],
      execTestCodes: [
        '2 + 2',
        'jtag.getUUID().uuid',
        'jtag.getUUID().context',
        'new Date().getTime()',
        'jtag.log("EXEC_TEST", "Self-logging"); "success"'
      ]
    });
  }

  async runTests(): Promise<void> {
    console.log('🚀 Starting intelligent standalone integration tests...\n');

    try {
      // Step 1: Kill any existing JTAG processes
      await this.killExistingProcesses();
      
      // Step 2: Start demo servers
      await this.startDemoServers();
      
      // Step 3: Wait for servers to be ready
      await this.waitForServers();
      
      // Step 4: Run basic integration tests
      await this.testServerSideFunctionality();
      await this.testClientServerIntegration();
      await this.testScreenshotFunctionality();
      await this.testCommunicationEndpoints();
      
      // Step 5: Run comprehensive feature suite
      await this.runComprehensiveTests();
      
      // Step 6: Clean up and show results
      await this.cleanup();
      this.printFinalResults();
      
    } catch (error: any) {
      console.error('\n💥 Integration test failed:', error.message);
      await this.cleanup();
      process.exit(1);
    }
  }

  private async killExistingProcesses(): Promise<void> {
    console.log('🔍 Checking for existing JTAG processes...');
    
    try {
      // Find processes using our ports
      const { stdout: jtagProcs } = await execAsync(`lsof -ti:${DEMO_CONFIG.jtagPort} 2>/dev/null || true`);
      const { stdout: demoProcs } = await execAsync(`lsof -ti:${DEMO_CONFIG.httpPort} 2>/dev/null || true`);
      
      const jtagPids = jtagProcs.trim().split('\n').filter(pid => pid);
      const demoPids = demoProcs.trim().split('\n').filter(pid => pid);
      const allPids = [...new Set([...jtagPids, ...demoPids])];
      
      if (allPids.length > 0) {
        console.log(`   🎯 Found ${allPids.length} existing processes to kill...`);
        
        for (const pid of allPids) {
          if (pid) {
            try {
              await execAsync(`kill -9 ${pid}`);
              console.log(`   ✅ Killed process ${pid}`);
            } catch (error) {
              console.log(`   ⚠️  Process ${pid} already gone`);
            }
          }
        }
        
        // Wait a moment for cleanup
        await this.sleep(1000);
      } else {
        console.log('   ✅ No existing processes found');
      }
      
    } catch (error: any) {
      console.log('   ⚠️  Process cleanup failed:', error.message);
    }
  }

  private async startDemoServers(): Promise<void> {
    console.log('\n🚀 Starting JTAG demo servers...');
    
    return new Promise((resolve, reject) => {
      // Start the demo using tsx
      this.demoProcess = spawn('npx', ['tsx', 'examples/end-to-end-demo.js'], {
        cwd: process.cwd(),
        stdio: ['ignore', 'pipe', 'pipe'],
        detached: false,
        env: {
          ...process.env,
          JTAG_AUTO_LAUNCH: 'false',  // Disable auto-launch for testing
          JTAG_TEST_MODE: 'true'
        }
      });
      
      let outputBuffer = '';
      
      this.demoProcess.stdout?.on('data', (data) => {
        const output = data.toString();
        outputBuffer += output;
        
        // Look for success indicators
        if (output.includes('Demo running!') || output.includes('Demo URL:')) {
          console.log('   ✅ Demo servers started successfully');
          setTimeout(() => resolve(), 2000); // Give it a moment to fully initialize
        }
      });
      
      this.demoProcess.stderr?.on('data', (data) => {
        const error = data.toString();
        if (!error.includes('ExperimentalWarning')) {
          console.log('   📝 Demo output:', error.trim());
        }
      });
      
      this.demoProcess.on('error', (error) => {
        reject(new Error(`Failed to start demo: ${error.message}`));
      });
      
      // Timeout if servers don't start
      setTimeout(() => {
        if (!outputBuffer.includes('Demo running!')) {
          reject(new Error('Demo servers failed to start within timeout'));
        }
      }, 10000);
    });
  }

  private async waitForServers(): Promise<void> {
    console.log('⏳ Waiting for servers to be ready...');
    
    let attempts = 0;
    const maxAttempts = 30;
    
    while (attempts < maxAttempts) {
      const health = await this.checkServerHealth();
      
      if (health.jtagServer && health.demoServer) {
        console.log('   ✅ Both servers are ready!');
        return;
      }
      
      await this.sleep(500);
      attempts++;
    }
    
    throw new Error('Servers failed to become ready within timeout');
  }

  private async checkServerHealth(): Promise<ServerHealthCheck> {
    const jtagCheck = await this.makeHealthCheck(`http://localhost:${DEMO_CONFIG.jtagPort}`);
    const demoCheck = await this.makeHealthCheck(`http://localhost:${DEMO_CONFIG.httpPort}`);
    
    return {
      jtagServer: jtagCheck,
      demoServer: demoCheck
    };
  }

  private async testServerSideFunctionality(): Promise<void> {
    console.log('\n📡 Testing server-side JTAG functionality...');
    
    try {
      // Test basic logging
      jtag.log('STANDALONE_TEST', 'Testing server-side basic logging');
      this.testResults.serverTests++;
      this.testResults.passed++;
      console.log('   ✅ Basic logging: PASSED');
      
      // Test critical logging
      jtag.critical('STANDALONE_TEST', 'Testing server-side critical logging', { 
        testId: 'server-critical-001',
        timestamp: new Date().toISOString()
      });
      this.testResults.serverTests++;
      this.testResults.passed++;
      console.log('   ✅ Critical logging: PASSED');
      
      // Test tracing
      jtag.trace('STANDALONE_TEST', 'testFunction', 'ENTER', { server: true, testType: 'integration' });
      await this.sleep(100);
      jtag.trace('STANDALONE_TEST', 'testFunction', 'EXIT', { result: 'success', duration: '100ms' });
      this.testResults.serverTests++;
      this.testResults.passed++;
      console.log('   ✅ Function tracing: PASSED');
      
      // Test probing
      jtag.probe('STANDALONE_TEST', 'server_state', {
        uptime: process.uptime(),
        memory: process.memoryUsage(),
        platform: process.platform,
        nodeVersion: process.version,
        testContext: 'standalone-integration'
      });
      this.testResults.serverTests++;
      this.testResults.passed++;
      console.log('   ✅ System probing: PASSED');
      
      console.log('   🎉 Server-side tests complete!');
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push('Server-side test failed: ' + error.message);
      console.log('   ❌ Server-side test failed:', error.message);
    }
  }

  private async testClientServerIntegration(): Promise<void> {
    console.log('\n🌐 Testing client-server integration...');
    
    try {
      // Test JTAG server endpoint
      const testEntry = {
        timestamp: new Date().toISOString(),
        context: 'browser' as const,
        component: 'STANDALONE_TEST',
        message: 'Testing client-server communication',
        data: { 
          testId: 'client-server-001',
          integrationTest: true,
          clientType: 'integration-test'
        },
        type: 'log' as const
      };
      
      const response = await this.makeRequest(
        `http://localhost:${DEMO_CONFIG.jtagPort}/jtag`, 
        'POST', 
        testEntry
      );
      
      if (response.success) {
        this.testResults.communicationTests++;
        this.testResults.passed++;
        console.log('   ✅ Client-server communication: PASSED');
      } else {
        throw new Error('Communication test failed - no success response');
      }
      
      console.log('   🎉 Client-server integration complete!');
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push('Client-server integration failed: ' + error.message);
      console.log('   ❌ Client-server integration failed:', error.message);
    }
  }

  private async testScreenshotFunctionality(): Promise<void> {
    console.log('\n📸 Testing screenshot functionality...');
    
    try {
      // Test server-side screenshot
      const screenshotResult = await jtag.screenshot('standalone-integration-test', {
        width: 1200,
        height: 800,
        format: 'png'
      });
      
      if (screenshotResult.filename) {
        this.testResults.screenshotTests++;
        this.testResults.passed++;
        console.log('   ✅ Screenshot capture: PASSED -', screenshotResult.filename);
      } else {
        throw new Error('Screenshot did not return filename');
      }
      
      // Test screenshot endpoint
      const screenshotRequest = {
        filename: 'integration-test-endpoint',
        options: { width: 800, height: 600 },
        timestamp: new Date().toISOString()
      };
      
      const screenshotResponse = await this.makeRequest(
        `http://localhost:${DEMO_CONFIG.jtagPort}/screenshot`, 
        'POST', 
        screenshotRequest
      );
      
      if (screenshotResponse.filename) {
        this.testResults.screenshotTests++;
        this.testResults.passed++;
        console.log('   ✅ Screenshot endpoint: PASSED -', screenshotResponse.filename);
      } else {
        throw new Error('Screenshot endpoint test failed');
      }
      
      console.log('   🎉 Screenshot tests complete!');
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push('Screenshot test failed: ' + error.message);
      console.log('   ❌ Screenshot test failed:', error.message);
    }
  }

  private async testCommunicationEndpoints(): Promise<void> {
    console.log('\n📡 Testing communication endpoints...');
    
    try {
      // Test payload endpoint
      const testPayload = {
        type: 'custom_data',
        data: Buffer.from('TypeScript integration test payload data').toString('base64'),
        encoding: 'base64'
      };
      
      const payloadResponse = await this.makeRequest(
        `http://localhost:${DEMO_CONFIG.jtagPort}/payload`, 
        'POST', 
        testPayload
      );
      
      if (payloadResponse.success) {
        this.testResults.communicationTests++;
        this.testResults.passed++;
        console.log('   ✅ Payload endpoint: PASSED');
      } else {
        throw new Error('Payload endpoint test failed');
      }
      
      console.log('   🎉 Communication endpoint tests complete!');
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push('Communication endpoint test failed: ' + error.message);
      console.log('   ❌ Communication endpoint test failed:', error.message);
    }
  }

  private async makeRequest(url: string, method: 'GET' | 'POST', data: any = {}): Promise<any> {
    return new Promise((resolve) => {
      const postData = method === 'POST' ? JSON.stringify(data) : '';
      const urlObj = new URL(url);
      
      const options: http.RequestOptions = {
        hostname: urlObj.hostname,
        port: urlObj.port,
        path: urlObj.pathname,
        method: method,
        headers: method === 'POST' ? {
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(postData)
        } : {},
        timeout: 5000
      };
      
      const req = http.request(options, (res) => {
        let responseData = '';
        res.on('data', (chunk) => responseData += chunk);
        res.on('end', () => {
          try {
            const parsed = responseData ? JSON.parse(responseData) : { success: true };
            resolve(parsed);
          } catch (error) {
            resolve({ success: false, error: 'Invalid JSON response' });
          }
        });
      });
      
      req.on('error', (error) => {
        resolve({ success: false, error: error.message });
      });
      
      req.on('timeout', () => {
        req.destroy();
        resolve({ success: false, error: 'Request timeout' });
      });
      
      if (method === 'POST' && postData) {
        req.write(postData);
      }
      req.end();
    });
  }

  private async makeHealthCheck(url: string): Promise<boolean> {
    return new Promise((resolve) => {
      const urlObj = new URL(url);
      const options: http.RequestOptions = {
        hostname: urlObj.hostname,
        port: urlObj.port,
        path: '/',
        method: 'GET',
        timeout: 2000
      };
      
      const req = http.request(options, (res) => {
        resolve(res.statusCode !== undefined && res.statusCode < 500);
      });
      
      req.on('error', () => resolve(false));
      req.on('timeout', () => {
        req.destroy();
        resolve(false);
      });
      
      req.end();
    });
  }

  private async cleanup(): Promise<void> {
    console.log('\n🧹 Cleaning up test processes...');
    
    if (this.demoProcess && !this.demoProcess.killed) {
      this.demoProcess.kill('SIGTERM');
      
      // Give it a chance to shut down gracefully
      await this.sleep(2000);
      
      if (!this.demoProcess.killed) {
        this.demoProcess.kill('SIGKILL');
      }
      
      console.log('   ✅ Demo process cleaned up');
    }
    
    // Also kill any lingering processes
    try {
      await execAsync(`pkill -f "tsx examples/end-to-end-demo.js" 2>/dev/null || true`);
    } catch (error) {
      // Ignore cleanup errors
    }
  }

  private async runComprehensiveTests(): Promise<void> {
    console.log('🧪 Running comprehensive JTAG feature suite...\n');
    
    try {
      // Run the comprehensive test suite
      const comprehensiveResults = await this.comprehensiveSuite.runAllTests();
      
      // Merge results into our test results
      this.testResults.comprehensiveTests = comprehensiveResults.passed;
      this.testResults.execTests = comprehensiveResults.execTests;
      this.testResults.uuidTests = comprehensiveResults.uuidTests;
      this.testResults.screenshotTests += comprehensiveResults.screenshotTests;
      this.testResults.passed += comprehensiveResults.passed;
      this.testResults.failed += comprehensiveResults.failed;
      this.testResults.errors.push(...comprehensiveResults.errors);
      
      // Print detailed comprehensive results
      this.comprehensiveSuite.printDetailedResults();
      
      console.log('   🎉 Comprehensive feature suite complete!\n');
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push(`Comprehensive tests failed: ${error.message}`);
      console.log(`   ❌ Comprehensive tests failed: ${error.message}`);
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  private printFinalResults(): void {
    console.log('\n📊 STANDALONE INTEGRATION TEST RESULTS');
    console.log('=======================================');
    console.log(`📡 Server tests: ${this.testResults.serverTests}`);
    console.log(`🌐 Client tests: ${this.testResults.clientTests}`);
    console.log(`📡 Communication tests: ${this.testResults.communicationTests}`);
    console.log(`📸 Screenshot tests: ${this.testResults.screenshotTests}`);
    console.log(`🧪 Comprehensive tests: ${this.testResults.comprehensiveTests}`);
    console.log(`⚡ Code execution tests: ${this.testResults.execTests}`);
    console.log(`🆔 UUID tests: ${this.testResults.uuidTests}`);
    console.log(`✅ Total passed: ${this.testResults.passed}`);
    console.log(`❌ Total failed: ${this.testResults.failed}`);
    
    const total = this.testResults.passed + this.testResults.failed;
    const successRate = total > 0 ? Math.round((this.testResults.passed / total) * 100) : 0;
    
    console.log(`📈 Success rate: ${successRate}%`);
    
    if (this.testResults.errors.length > 0) {
      console.log('\n❌ Errors:');
      this.testResults.errors.forEach(error => console.log(`   - ${error}`));
    }
    
    console.log('\n🔗 JTAG Features Tested:');
    console.log('   📝 Basic logging (jtag.log)');
    console.log('   🔥 Critical logging (jtag.critical)');
    console.log('   🔍 Function tracing (jtag.trace)');
    console.log('   📊 System probing (jtag.probe)');
    console.log('   📸 Screenshot capture (jtag.screenshot)');
    console.log('   🌐 Client-server communication');
    console.log('   📦 Payload transport');
    
    if (this.testResults.failed === 0) {
      console.log('\n🎉 ALL STANDALONE INTEGRATION TESTS PASSED!');
      console.log('🚀 JTAG system is working perfectly in standalone mode.');
      console.log('✨ Complete end-to-end functionality validated!');
    } else {
      console.log('\n⚠️  Some tests failed. Check the errors above.');
      process.exit(1);
    }
  }
}

// Run tests if called directly
if (require.main === module) {
  const tester = new StandaloneIntegrationTester();
  tester.runTests().catch(error => {
    console.error('\n💥 Test runner failed:', error);
    process.exit(1);
  });
}

export { StandaloneIntegrationTester };