#!/usr/bin/env npx tsx
/**
 * Middle-Out Test 00: Test-Bench Integration
 * 
 * This test validates that the test-bench is running and ready for cross-environment testing.
 * It's the foundation for all other middle-out tests.
 * 
 * REQUIRES: npm start to be running (launches test-bench)
 */

import * as http from 'http';
import * as fs from 'fs';
import * as path from 'path';

class TestBenchValidator {
  private readonly TEST_BENCH_PORT = 9002;
  private readonly JTAG_PORT = 9001;
  private readonly LOG_DIR = '.continuum/jtag/logs';
  private readonly SCREENSHOT_DIR = '.continuum/jtag/screenshots';

  async validateTestBenchIntegration(): Promise<void> {
    console.log('🧪 Middle-Out Test 00: Test-Bench Integration');
    console.log('============================================');
    
    // Domain 1: Validate test-bench server is running
    await this.validateTestBenchServer();
    
    // Domain 2: Validate JTAG system is accessible
    await this.validateJTAGSystem();
    
    // Domain 3: Validate file system structure
    await this.validateFileSystemSetup();
    
    // Domain 4: Validate browser assets are built and served
    await this.validateBrowserAssets();
    
    console.log('✅ Test-bench integration validated - ready for cross-environment tests');
  }

  private async validateTestBenchServer(): Promise<void> {
    console.log('1. Validating test-bench server...');
    
    try {
      const response = await this.httpGet(`http://localhost:${this.TEST_BENCH_PORT}`);
      
      if (response.statusCode !== 200) {
        throw new Error(`Test-bench server returned ${response.statusCode}`);
      }
      
      console.log(`   ✅ Test-bench server running on port ${this.TEST_BENCH_PORT}`);
    } catch (error: any) {
      console.error(`   ❌ Test-bench server not accessible: ${error.message}`);
      console.error('   💡 Make sure "npm start" is running to launch test-bench');
      throw error;
    }
  }

  private async validateJTAGSystem(): Promise<void> {
    console.log('2. Validating JTAG system accessibility...');
    
    try {
      // Try to connect to JTAG WebSocket port
      const response = await this.httpGet(`http://localhost:${this.JTAG_PORT}`);
      
      // We expect this to fail with connection refused OR return some response
      // The key is that the port is open and something is listening
      console.log(`   ✅ JTAG system accessible on port ${this.JTAG_PORT}`);
    } catch (error: any) {
      if (error.code === 'ECONNREFUSED') {
        console.error(`   ❌ JTAG system not running on port ${this.JTAG_PORT}`);
        console.error('   💡 JTAG should be started automatically by test-bench');
        throw error;
      }
      // Other errors might be OK (like 404 for HTTP on WebSocket port)
      console.log(`   ✅ JTAG port ${this.JTAG_PORT} is accessible`);
    }
  }

  private async validateFileSystemSetup(): Promise<void> {
    console.log('3. Validating file system structure...');
    
    // Check log directory exists
    if (!fs.existsSync(this.LOG_DIR)) {
      throw new Error(`Log directory ${this.LOG_DIR} does not exist`);
    }
    console.log(`   ✅ Log directory exists: ${this.LOG_DIR}`);
    
    // Check screenshot directory exists  
    if (!fs.existsSync(this.SCREENSHOT_DIR)) {
      throw new Error(`Screenshot directory ${this.SCREENSHOT_DIR} does not exist`);
    }
    console.log(`   ✅ Screenshot directory exists: ${this.SCREENSHOT_DIR}`);
    
    // Check if directories are writable
    const testFile = path.join(this.LOG_DIR, 'test-write.tmp');
    try {
      fs.writeFileSync(testFile, 'test');
      fs.unlinkSync(testFile);
      console.log(`   ✅ Log directory is writable`);
    } catch (error) {
      throw new Error(`Log directory is not writable: ${error}`);
    }
  }

  private async validateBrowserAssets(): Promise<void> {
    console.log('4. Validating browser assets...');
    
    try {
      // Check if browser bundle is served by test-bench
      const response = await this.httpGet(`http://localhost:${this.TEST_BENCH_PORT}/index.js`);
      
      if (response.statusCode !== 200) {
        throw new Error(`Browser bundle not served: ${response.statusCode}`);
      }
      
      console.log(`   ✅ Browser bundle served by test-bench`);
      
      // Check if HTML page is served
      const htmlResponse = await this.httpGet(`http://localhost:${this.TEST_BENCH_PORT}/`);
      if (htmlResponse.statusCode !== 200) {
        throw new Error(`HTML page not served: ${htmlResponse.statusCode}`);
      }
      
      console.log(`   ✅ HTML page served by test-bench`);
      
    } catch (error: any) {
      console.error(`   ❌ Browser assets validation failed: ${error.message}`);
      throw error;
    }
  }

  private httpGet(url: string): Promise<{statusCode: number, data: string}> {
    return new Promise((resolve, reject) => {
      const request = http.get(url, (response) => {
        let data = '';
        response.on('data', (chunk) => data += chunk);
        response.on('end', () => {
          resolve({
            statusCode: response.statusCode || 0,
            data
          });
        });
      });
      
      request.on('error', reject);
      request.setTimeout(5000, () => {
        request.destroy();
        reject(new Error('Request timeout'));
      });
    });
  }
}

async function runTestBenchValidation(): Promise<void> {
  const validator = new TestBenchValidator();
  
  try {
    await validator.validateTestBenchIntegration();
    
    console.log('\n🎉 Test-bench integration validation PASSED');
    console.log('📋 Ready for cross-environment middle-out testing');
    console.log('');
    console.log('Next steps:');
    console.log('  • Run environment detection tests');
    console.log('  • Run cross-context routing tests');
    console.log('  • Run console transport tests');
    
    process.exit(0);
  } catch (error: any) {
    console.error('\n❌ Test-bench integration validation FAILED');
    console.error(`💥 Error: ${error.message}`);
    console.error('');
    console.error('Troubleshooting:');
    console.error('  1. Make sure "npm start" is running');
    console.error('  2. Check that test-bench is building successfully');
    console.error('  3. Verify ports 9001 and 9002 are available');
    console.error('  4. Ensure file system permissions are correct');
    
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runTestBenchValidation();
}

export { TestBenchValidator };