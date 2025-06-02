#!/usr/bin/env node
/**
 * Test Script for Graceful Shutdown and Port Conflict Resolution
 */

const { spawn, exec } = require('child_process');
const fs = require('fs');
const path = require('path');
const net = require('net');

const CONTINUUM_SCRIPT = path.join(__dirname, 'continuum.cjs');
const TEST_DIR = path.join(__dirname, '.test-continuum');
const PID_FILE = path.join(TEST_DIR, 'continuum.pid');

// Ensure test directory exists
if (!fs.existsSync(TEST_DIR)) {
  fs.mkdirSync(TEST_DIR, { recursive: true });
}

class ContinuumTester {
  constructor() {
    this.processes = [];
    this.testResults = [];
  }

  async runTest(testName, testFunction) {
    console.log(`\n🧪 Running test: ${testName}`);
    console.log('='.repeat(50));
    
    try {
      await testFunction();
      console.log(`✅ ${testName} PASSED`);
      this.testResults.push({ name: testName, status: 'PASSED' });
    } catch (error) {
      console.error(`❌ ${testName} FAILED:`, error.message);
      this.testResults.push({ name: testName, status: 'FAILED', error: error.message });
    }
  }

  async cleanup() {
    console.log('\n🧹 Cleaning up test processes...');
    
    // Kill all spawned processes
    for (const proc of this.processes) {
      if (proc && !proc.killed) {
        proc.kill('SIGTERM');
      }
    }
    
    // Clean up test directory
    if (fs.existsSync(TEST_DIR)) {
      try {
        fs.rmSync(TEST_DIR, { recursive: true, force: true });
      } catch (error) {
        console.warn('Failed to clean test directory:', error.message);
      }
    }
    
    // Wait a bit for processes to clean up
    await new Promise(resolve => setTimeout(resolve, 1000));
  }

  async startContinuum(options = {}) {
    return new Promise((resolve, reject) => {
      const args = ['start'];
      if (options.stayAlive) args.push('--stay-alive');
      if (options.port) args.push('--port', options.port.toString());
      
      const proc = spawn('node', [CONTINUUM_SCRIPT, ...args], {
        stdio: 'pipe',
        cwd: TEST_DIR
      });
      
      this.processes.push(proc);
      
      let output = '';
      let resolved = false;
      
      proc.stdout.on('data', (data) => {
        output += data.toString();
        if (output.includes('Continuum ready:') && !resolved) {
          resolved = true;
          resolve(proc);
        }
      });
      
      proc.stderr.on('data', (data) => {
        console.log('STDERR:', data.toString());
      });
      
      proc.on('error', (error) => {
        if (!resolved) {
          resolved = true;
          reject(error);
        }
      });
      
      // Timeout after 10 seconds
      setTimeout(() => {
        if (!resolved) {
          resolved = true;
          reject(new Error('Continuum startup timeout'));
        }
      }, 10000);
    });
  }

  async testPortConflictResolution() {
    const testPort = 5558;
    
    // Start first instance on test port
    const proc1 = await this.startContinuum({ port: testPort });
    console.log(`✅ First instance started on port ${testPort}`);
    
    // Wait a bit
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Start second instance - should find alternative port
    const proc2 = await this.startContinuum({ port: testPort });
    console.log(`✅ Second instance started (should be on different port)`);
    
    // Verify both are running
    if (!proc1.killed && !proc2.killed) {
      console.log(`✅ Both instances running successfully`);
    } else {
      throw new Error('One or both instances failed to start');
    }
  }

  async testStayAliveMode() {
    const testPort = 5559;
    
    // Start first instance
    const proc1 = await this.startContinuum({ port: testPort });
    console.log(`✅ First instance started`);
    
    // Wait a bit
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Try to start second instance with stay-alive mode
    return new Promise((resolve, reject) => {
      const proc2 = spawn('node', [CONTINUUM_SCRIPT, 'start', '--stay-alive', '--port', testPort.toString()], {
        stdio: 'pipe',
        cwd: TEST_DIR
      });
      
      let output = '';
      
      proc2.stdout.on('data', (data) => {
        output += data.toString();
      });
      
      proc2.on('exit', (code) => {
        if (output.includes('Stay-alive mode enabled, keeping existing instance')) {
          console.log(`✅ Stay-alive mode worked correctly`);
          resolve();
        } else {
          reject(new Error('Stay-alive mode did not work as expected'));
        }
      });
      
      setTimeout(() => reject(new Error('Stay-alive test timeout')), 5000);
    });
  }

  async testGracefulShutdown() {
    const testPort = 5560;
    
    // Start instance
    const proc = await this.startContinuum({ port: testPort });
    console.log(`✅ Instance started`);
    
    // Wait a bit
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Send graceful shutdown signal
    return new Promise((resolve, reject) => {
      proc.kill('SIGTERM');
      
      proc.on('exit', (code) => {
        if (code === 0) {
          console.log(`✅ Graceful shutdown successful`);
          resolve();
        } else {
          reject(new Error(`Process exited with code ${code}`));
        }
      });
      
      setTimeout(() => reject(new Error('Shutdown timeout')), 5000);
    });
  }

  async testCommandLineShutdown() {
    const testPort = 5561;
    
    // Start instance
    const proc = await this.startContinuum({ port: testPort });
    console.log(`✅ Instance started`);
    
    // Wait a bit
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Use command line to shutdown
    return new Promise((resolve, reject) => {
      exec(`cd ${TEST_DIR} && node ${CONTINUUM_SCRIPT} stop`, (error, stdout, stderr) => {
        if (error) {
          reject(error);
          return;
        }
        
        if (stdout.includes('shut down successfully')) {
          console.log(`✅ Command line shutdown worked`);
          resolve();
        } else {
          reject(new Error('Command line shutdown failed'));
        }
      });
      
      setTimeout(() => reject(new Error('Command line shutdown timeout')), 5000);
    });
  }

  async testStatusCommand() {
    const testPort = 5562;
    
    // Test status when no instance running
    await new Promise((resolve, reject) => {
      exec(`cd ${TEST_DIR} && node ${CONTINUUM_SCRIPT} status`, (error, stdout, stderr) => {
        if (stdout.includes('No Continuum instance running')) {
          console.log(`✅ Status command works when no instance running`);
          resolve();
        } else {
          reject(new Error('Status command failed when no instance running'));
        }
      });
    });
    
    // Start instance
    const proc = await this.startContinuum({ port: testPort });
    console.log(`✅ Instance started for status test`);
    
    // Wait a bit
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Test status when instance is running
    return new Promise((resolve, reject) => {
      exec(`cd ${TEST_DIR} && node ${CONTINUUM_SCRIPT} status`, (error, stdout, stderr) => {
        if (stdout.includes('Continuum is running')) {
          console.log(`✅ Status command works when instance is running`);
          resolve();
        } else {
          reject(new Error('Status command failed when instance is running'));
        }
      });
      
      setTimeout(() => reject(new Error('Status command timeout')), 5000);
    });
  }

  printResults() {
    console.log('\n📊 TEST RESULTS');
    console.log('='.repeat(50));
    
    let passed = 0;
    let failed = 0;
    
    for (const result of this.testResults) {
      if (result.status === 'PASSED') {
        console.log(`✅ ${result.name}`);
        passed++;
      } else {
        console.log(`❌ ${result.name}: ${result.error}`);
        failed++;
      }
    }
    
    console.log('\n📈 SUMMARY');
    console.log(`Total tests: ${passed + failed}`);
    console.log(`Passed: ${passed}`);
    console.log(`Failed: ${failed}`);
    
    if (failed === 0) {
      console.log('\n🎉 All tests passed!');
    } else {
      console.log('\n⚠️  Some tests failed');
      process.exit(1);
    }
  }

  async runAllTests() {
    console.log('🚀 Starting Continuum Graceful Shutdown Tests');
    
    try {
      await this.runTest('Port Conflict Resolution', () => this.testPortConflictResolution());
      await this.cleanup();
      
      await this.runTest('Stay-Alive Mode', () => this.testStayAliveMode());
      await this.cleanup();
      
      await this.runTest('Graceful Shutdown (SIGTERM)', () => this.testGracefulShutdown());
      await this.cleanup();
      
      await this.runTest('Command Line Shutdown', () => this.testCommandLineShutdown());
      await this.cleanup();
      
      await this.runTest('Status Command', () => this.testStatusCommand());
      await this.cleanup();
      
    } finally {
      await this.cleanup();
      this.printResults();
    }
  }
}

// Run tests if this script is executed directly
if (require.main === module) {
  const tester = new ContinuumTester();
  tester.runAllTests().catch(error => {
    console.error('Test runner failed:', error);
    process.exit(1);
  });
}

module.exports = ContinuumTester;