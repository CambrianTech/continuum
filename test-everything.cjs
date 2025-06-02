#!/usr/bin/env node
/**
 * COMPREHENSIVE TEST RUNNER - TEST THE WHOLE DAMN SYSTEM
 * 
 * This runs every single test every single time
 * NO EXCEPTIONS, NO SHORTCUTS
 */

const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

class ComprehensiveTestRunner {
  constructor() {
    this.testFiles = [];
    this.results = [];
    this.totalTests = 0;
    this.passedTests = 0;
    this.failedTests = 0;
  }

  async discoverTests() {
    console.log('🔍 Discovering all test files...');
    
    const testPatterns = [
      /test.*\.c?js$/,
      /.*\.test\.c?js$/,
      /.*test\.c?js$/
    ];
    
    const scanDirectory = (dir) => {
      if (!fs.existsSync(dir)) return;
      
      const entries = fs.readdirSync(dir);
      for (const entry of entries) {
        const fullPath = path.join(dir, entry);
        const stat = fs.statSync(fullPath);
        
        if (stat.isDirectory() && !entry.startsWith('.') && entry !== 'node_modules') {
          scanDirectory(fullPath);
        } else if (stat.isFile()) {
          const isTest = testPatterns.some(pattern => pattern.test(entry));
          if (isTest) {
            this.testFiles.push(fullPath);
            console.log(`📝 Found test: ${path.relative(process.cwd(), fullPath)}`);
          }
        }
      }
    };
    
    scanDirectory(process.cwd());
    console.log(`📊 Total test files found: ${this.testFiles.length}`);
  }

  async runSingleTest(testFile) {
    return new Promise((resolve) => {
      console.log(`\n🧪 Running: ${path.basename(testFile)}`);
      console.log('=' + '='.repeat(50));
      
      const startTime = Date.now();
      const child = spawn('node', [testFile], {
        stdio: 'pipe',
        cwd: process.cwd()
      });
      
      let stdout = '';
      let stderr = '';
      
      child.stdout.on('data', (data) => {
        const output = data.toString();
        stdout += output;
        process.stdout.write(output); // Show real-time output
      });
      
      child.stderr.on('data', (data) => {
        const output = data.toString();
        stderr += output;
        process.stderr.write(output); // Show real-time errors
      });
      
      child.on('close', (code) => {
        const duration = Date.now() - startTime;
        const result = {
          file: testFile,
          passed: code === 0,
          duration,
          stdout,
          stderr,
          exitCode: code
        };
        
        if (code === 0) {
          console.log(`✅ PASSED: ${path.basename(testFile)} (${duration}ms)`);
          this.passedTests++;
        } else {
          console.log(`❌ FAILED: ${path.basename(testFile)} (${duration}ms)`);
          console.log(`   Exit code: ${code}`);
          this.failedTests++;
        }
        
        this.results.push(result);
        resolve(result);
      });
      
      // Timeout after 60 seconds
      setTimeout(() => {
        child.kill('SIGTERM');
        console.log(`⏰ TIMEOUT: ${path.basename(testFile)}`);
        this.failedTests++;
        resolve({
          file: testFile,
          passed: false,
          duration: 60000,
          stdout: '',
          stderr: 'Test timed out after 60 seconds',
          exitCode: -1
        });
      }, 60000);
    });
  }

  async runAllTests() {
    console.log(`\n🚀 Running ${this.testFiles.length} test files...`);
    this.totalTests = this.testFiles.length;
    
    for (const testFile of this.testFiles) {
      await this.runSingleTest(testFile);
    }
  }

  printSummary() {
    console.log('\n' + '='.repeat(60));
    console.log('📊 COMPREHENSIVE TEST RESULTS');
    console.log('='.repeat(60));
    console.log(`📁 Total test files: ${this.totalTests}`);
    console.log(`✅ Passed: ${this.passedTests}`);
    console.log(`❌ Failed: ${this.failedTests}`);
    console.log(`📈 Success Rate: ${((this.passedTests / this.totalTests) * 100).toFixed(1)}%`);
    
    const totalDuration = this.results.reduce((sum, r) => sum + r.duration, 0);
    console.log(`⏱️  Total Duration: ${(totalDuration / 1000).toFixed(1)} seconds`);
    
    if (this.failedTests > 0) {
      console.log('\n❌ FAILED TESTS:');
      this.results.filter(r => !r.passed).forEach(r => {
        console.log(`   - ${path.basename(r.file)} (exit code: ${r.exitCode})`);
        if (r.stderr) {
          console.log(`     Error: ${r.stderr.split('\n')[0]}`);
        }
      });
    }
    
    console.log('\n🎯 TEST BREAKDOWN BY FILE:');
    this.results.forEach(r => {
      const status = r.passed ? '✅' : '❌';
      const duration = `${r.duration}ms`;
      console.log(`   ${status} ${path.basename(r.file).padEnd(30)} ${duration}`);
    });
    
    if (this.failedTests === 0) {
      console.log('\n🎉 ALL TESTS PASSED! SYSTEM IS SOLID!');
      return true;
    } else {
      console.log('\n⚠️  SOME TESTS FAILED! FIX THEM BEFORE PROCEEDING!');
      return false;
    }
  }

  async systemHealthCheck() {
    console.log('\n🔧 SYSTEM HEALTH CHECK');
    console.log('='.repeat(30));
    
    // Check dependencies
    const deps = ['@anthropic-ai/sdk', 'openai', 'ws', 'node-fetch'];
    for (const dep of deps) {
      try {
        require(dep);
        console.log(`✅ Dependency: ${dep}`);
      } catch (error) {
        console.log(`❌ Missing dependency: ${dep}`);
        return false;
      }
    }
    
    // Check API keys
    require('dotenv').config();
    const apiKeys = [
      { name: 'ANTHROPIC_API_KEY', key: process.env.ANTHROPIC_API_KEY },
      { name: 'OPENAI_API_KEY', key: process.env.OPENAI_API_KEY }
    ];
    
    for (const { name, key } of apiKeys) {
      if (key && key.length > 10) {
        console.log(`✅ API Key: ${name} (${key.substring(0, 15)}...)`);
      } else {
        console.log(`❌ Missing API Key: ${name}`);
        return false;
      }
    }
    
    // Check core files
    const coreFiles = [
      'continuum.cjs',
      'package.json',
      '.env'
    ];
    
    for (const file of coreFiles) {
      if (fs.existsSync(file)) {
        console.log(`✅ Core file: ${file}`);
      } else {
        console.log(`❌ Missing core file: ${file}`);
        return false;
      }
    }
    
    console.log('✅ System health check passed');
    return true;
  }
}

async function runComprehensiveTests() {
  const runner = new ComprehensiveTestRunner();
  
  console.log('🌟 COMPREHENSIVE TEST RUNNER');
  console.log('============================');
  console.log('Testing the WHOLE DAMN SYSTEM every time!');
  console.log('No shortcuts, no exceptions, no mercy!\n');
  
  // System health check first
  const healthOk = await runner.systemHealthCheck();
  if (!healthOk) {
    console.log('\n❌ System health check failed! Fix dependencies first.');
    process.exit(1);
  }
  
  // Discover and run all tests
  await runner.discoverTests();
  
  if (runner.testFiles.length === 0) {
    console.log('\n⚠️  No test files found! Create some tests!');
    process.exit(1);
  }
  
  await runner.runAllTests();
  const allPassed = runner.printSummary();
  
  if (allPassed) {
    console.log('\n🚀 READY FOR REFACTORING! All tests pass!');
    process.exit(0);
  } else {
    console.log('\n🛑 NOT READY! Fix failing tests first!');
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runComprehensiveTests().catch(error => {
    console.error('\n💥 Test runner crashed:', error.message);
    console.error(error.stack);
    process.exit(1);
  });
}

module.exports = ComprehensiveTestRunner;