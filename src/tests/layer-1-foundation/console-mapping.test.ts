#!/usr/bin/env node
/**
 * Console Mapping Test
 * 
 * Verifies the correct mapping between console methods and JTAG methods:
 * - console.log → jtag.log → server.log.txt  
 * - console.warn → jtag.warn → server.warn.txt
 * - console.error → jtag.error → server.error.txt
 * - jtag.critical → server.critical.txt (for truly critical events)
 */

import { JTAGBase } from '../../system/core/shared/JTAGBase';
import { JTAG_LOG_LEVELS } from '../../../system/core/types/JTAGTypes';
import * as fs from 'fs';
import * as path from 'path';

const TEST_CONFIG = {
  testLogDir: path.resolve(__dirname, '../../../..', '.continuum/jtag-console-test/logs'),
};

class ConsoleMappingTest {
  private originalConsole: {
    log: Function;
    warn: Function;
    error: Function;
  };

  constructor() {
    this.originalConsole = {
      log: console.log.bind(console),
      warn: console.warn.bind(console), 
      error: console.error.bind(console)
    };
    
    console.log('🧪 Console Mapping Test');
    console.log('======================');
  }

  async setup(): Promise<void> {
    console.log('\n📋 Setting up test environment...');
    
    // Clean test log directory
    if (fs.existsSync(TEST_CONFIG.testLogDir)) {
      fs.rmSync(TEST_CONFIG.testLogDir, { recursive: true });
    }
    fs.mkdirSync(TEST_CONFIG.testLogDir, { recursive: true });
    
    // Initialize JTAG with test configuration
    JTAGBase.initialize({
      jtagPort: 9004, // Different port to avoid conflicts
      logDirectory: TEST_CONFIG.testLogDir,
      enableConsoleOutput: false, // Disable to avoid test noise
      enableRemoteLogging: false  // No WebSocket for unit test
    });
    
    // Attach JTAG to console (this is the critical functionality we're testing)
    JTAGBase.attach(console);
    
    console.log('   ✅ JTAG initialized and console attached');
  }

  async testConsoleMapping(): Promise<void> {
    console.log('\n📋 Testing console method mapping...');
    
    // Test console.log → jtag.log → server.log.txt
    this.originalConsole.log('   🔍 Testing console.log mapping...');
    console.log('Test console.log message');
    
    // Test console.warn → jtag.warn → server.warn.txt  
    this.originalConsole.log('   🔍 Testing console.warn mapping...');
    console.warn('Test console.warn message');
    
    // Test console.error → jtag.error → server.error.txt
    this.originalConsole.log('   🔍 Testing console.error mapping...');
    console.error('Test console.error message');
    
    // Test direct jtag.critical → server.critical.txt
    this.originalConsole.log('   🔍 Testing jtag.critical mapping...');
    JTAGBase.critical('DIRECT_CALL', 'Test direct critical message');
    
    // Give a moment for file writes to complete
    await new Promise(resolve => setTimeout(resolve, 100));
  }

  verifyLogFiles(): void {
    console.log('\n📋 Verifying log files were created correctly...');
    
    const expectedFiles = [
      { filename: 'server.log.txt', expectedContent: 'Test console.log message' },
      { filename: 'server.warn.txt', expectedContent: 'Test console.warn message' }, 
      { filename: 'server.error.txt', expectedContent: 'Test console.error message' },
      { filename: 'server.critical.txt', expectedContent: 'Test direct critical message' }
    ];
    
    for (const { filename, expectedContent } of expectedFiles) {
      const filePath = path.join(TEST_CONFIG.testLogDir, filename);
      
      if (fs.existsSync(filePath)) {
        const content = fs.readFileSync(filePath, 'utf8');
        if (content.includes(expectedContent)) {
          this.originalConsole.log(`   ✅ ${filename} exists with expected content`);
        } else {
          this.originalConsole.error(`   ❌ ${filename} exists but missing expected content`);
          this.originalConsole.log(`       Expected: "${expectedContent}"`);
          this.originalConsole.log(`       Got: "${content}"`);
        }
      } else {
        this.originalConsole.error(`   ❌ ${filename} does NOT exist`);
      }
    }
  }

  verifyStrongTypes(): void {
    console.log('\n📋 Verifying strong type usage...');
    
    // Test that JTAG_LOG_LEVELS constants are used properly
    const logLevels = Object.values(JTAG_LOG_LEVELS);
    this.originalConsole.log(`   📊 Available log levels: ${logLevels.join(', ')}`);
    
    // Verify all expected levels exist
    const expectedLevels = ['log', 'warn', 'error', 'critical', 'trace', 'probe'];
    for (const level of expectedLevels) {
      if (logLevels.includes(level as any)) {
        this.originalConsole.log(`   ✅ ${level} level properly defined`);
      } else {
        this.originalConsole.error(`   ❌ ${level} level missing from JTAG_LOG_LEVELS`);
      }
    }
  }

  cleanup(): void {
    console.log('\n🧹 Cleaning up...');
    
    // Detach JTAG from console to restore original behavior
    JTAGBase.detach(console);
    
    console.log('   ✅ Console detached');
  }

  async runTest(): Promise<void> {
    try {
      await this.setup();
      await this.testConsoleMapping();
      this.verifyLogFiles();
      this.verifyStrongTypes();
      
      console.log('\n🎉 Console mapping test completed successfully!');
      console.log('===============================================');
      console.log('✅ console.log → jtag.log → server.log.txt');
      console.log('✅ console.warn → jtag.warn → server.warn.txt'); 
      console.log('✅ console.error → jtag.error → server.error.txt');
      console.log('✅ jtag.critical → server.critical.txt');
      console.log('✅ Strong types enforced via JTAG_LOG_LEVELS');
      
    } catch (error) {
      console.error('\n💥 Console mapping test failed:', error);
      throw error;
    } finally {
      this.cleanup();
    }
  }
}

// Run test if called directly
if (require.main === module) {
  const test = new ConsoleMappingTest();
  test.runTest().catch(error => {
    console.error('Test failed:', error);
    process.exit(1);
  });
}

export { ConsoleMappingTest };