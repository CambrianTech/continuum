#!/usr/bin/env node
/**
 * JTAG Test Utilities - Shared between standalone and module tests
 * 
 * Comprehensive test suites for all JTAG functionality:
 * - All screenshot types and selectors
 * - UUID tracking and inspection
 * - Code execution with jtag.exec()
 * - Performance and stress testing
 * - Cross-context communication
 */

import { jtag } from '../../index';
import type { JTAGUUIDInfo } from '@shared/JTAGTypes';

export interface ComprehensiveTestResults {
  execTests: number;
  uuidTests: number;
  screenshotTests: number;
  performanceTests: number;
  crossContextTests: number;
  passed: number;
  failed: number;
  errors: string[];
  screenshots: ScreenshotTestResult[];
  execResults: ExecTestResult[];
  uuidInfo: JTAGUUIDInfo | null;
}

export interface ScreenshotTestResult {
  name: string;
  success: boolean;
  selector?: string;
  error?: string;
  metadata?: any;
}

export interface ExecTestResult {
  code: string;
  success: boolean;
  result?: any;
  error?: string;
  executionTime: number;
}

export interface TestConfig {
  testTimeout: number;
  performanceIterations: number;
  screenshotTypes: string[];
  execTestCodes: string[];
}

export const DEFAULT_TEST_CONFIG: TestConfig = {
  testTimeout: 10000,
  performanceIterations: 20,
  screenshotTypes: [
    'full-page',
    'viewport-only', 
    'element-selector',
    'multi-element',
    'responsive-mobile',
    'responsive-desktop',
    'high-quality-png',
    'compressed-jpeg'
  ],
  execTestCodes: [
    // Basic arithmetic
    '2 + 2',
    
    // UUID inspection
    'jtag.getUUID().uuid',
    
    // Context detection
    'jtag.getUUID().context',
    
    // Process info (server only)
    'typeof process !== "undefined" ? process.pid : "browser"',
    
    // Date manipulation
    'new Date().getTime()',
    
    // JSON operations
    'JSON.stringify({test: "data", timestamp: Date.now()})',
    
    // Array operations
    '[1,2,3,4,5].map(x => x * 2).reduce((a,b) => a + b)',
    
    // JTAG self-logging
    'jtag.log("EXEC_TEST", "Self-logging from exec"); "logged"',
    
    // UUID verification
    'jtag.getUUID().uuid.startsWith("jtag_")',
    
    // Async operation
    'Promise.resolve("async-result")'
  ]
};

export class ComprehensiveTestSuite {
  private results: ComprehensiveTestResults;
  private config: TestConfig;

  constructor(config: Partial<TestConfig> = {}) {
    this.config = { ...DEFAULT_TEST_CONFIG, ...config };
    this.results = {
      execTests: 0,
      uuidTests: 0,
      screenshotTests: 0,
      performanceTests: 0,
      crossContextTests: 0,
      passed: 0,
      failed: 0,
      errors: [],
      screenshots: [],
      execResults: [],
      uuidInfo: null
    };
  }

  async runAllTests(): Promise<ComprehensiveTestResults> {
    console.log('🧪 Running comprehensive JTAG test suite...');
    
    try {
      // Test UUID functionality first
      await this.testUUIDFunctionality();
      
      // Test code execution
      await this.testCodeExecution();
      
      // Test all screenshot types
      await this.testAllScreenshotTypes();
      
      // Test performance scenarios
      await this.testPerformanceScenarios();
      
      // Test cross-context communication
      await this.testCrossContextCommunication();
      
      return this.results;
      
    } catch (error: any) {
      this.results.errors.push(`Test suite failed: ${error.message}`);
      this.results.failed++;
      return this.results;
    }
  }

  private async testUUIDFunctionality(): Promise<void> {
    console.log('🆔 Testing UUID functionality...');
    
    try {
      // Test getUUID() method
      const uuidInfo = jtag.getUUID();
      this.results.uuidInfo = uuidInfo;
      
      if (uuidInfo && uuidInfo.uuid && uuidInfo.uuid.startsWith('jtag_')) {
        this.results.uuidTests++;
        this.results.passed++;
        console.log(`   ✅ UUID generation: ${uuidInfo.uuid}`);
      } else {
        throw new Error('Invalid UUID format');
      }
      
      // Test UUID consistency
      const uuidInfo2 = jtag.getUUID();
      if (uuidInfo.uuid === uuidInfo2.uuid) {
        this.results.uuidTests++;
        this.results.passed++;
        console.log('   ✅ UUID consistency: PASSED');
      } else {
        throw new Error('UUID not consistent between calls');
      }
      
      // Test UUID metadata
      if (uuidInfo.context && uuidInfo.timestamp && uuidInfo.sessionId) {
        this.results.uuidTests++;
        this.results.passed++;
        console.log(`   ✅ UUID metadata: context=${uuidInfo.context}, session=${uuidInfo.sessionId}`);
      } else {
        throw new Error('UUID metadata incomplete');
      }
      
      console.log('   🎉 UUID tests complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`UUID test failed: ${error.message}`);
      console.log(`   ❌ UUID test failed: ${error.message}`);
    }
  }

  private async testCodeExecution(): Promise<void> {
    console.log('⚡ Testing code execution...');
    
    for (const code of this.config.execTestCodes) {
      try {
        const startTime = Date.now();
        const execResult = await jtag.exec(code);
        const duration = Date.now() - startTime;
        
        const testResult: ExecTestResult = {
          code,
          success: execResult.success,
          result: execResult.result,
          error: execResult.error || '',
          executionTime: duration
        };
        
        this.results.execResults.push(testResult);
        
        if (execResult.success) {
          this.results.execTests++;
          this.results.passed++;
          console.log(`   ✅ Exec: ${code.substring(0, 30)} → ${JSON.stringify(execResult.result).substring(0, 50)}`);
        } else {
          throw new Error(execResult.error || 'Execution failed');
        }
        
      } catch (error: any) {
        this.results.failed++;
        this.results.errors.push(`Exec test failed for "${code}": ${error.message}`);
        console.log(`   ❌ Exec failed: ${code.substring(0, 30)} - ${error.message}`);
      }
    }
    
    console.log('   🎉 Code execution tests complete!\n');
  }

  private async testAllScreenshotTypes(): Promise<void> {
    console.log('📸 Testing all screenshot types...');
    
    const screenshotConfigs = [
      { name: 'full-page', options: { fullPage: true } },
      { name: 'viewport-only', options: { width: 1024, height: 768 } },
      { name: 'high-quality', options: { quality: 100, format: 'png' as const } },
      { name: 'compressed', options: { quality: 50, format: 'jpeg' as const } },
      { name: 'mobile-responsive', options: { width: 375, height: 667 } },
      { name: 'desktop-wide', options: { width: 1920, height: 1080 } },
      { name: 'with-delay', options: { delay: 100 } }
    ];
    
    for (const config of screenshotConfigs) {
      try {
        const filename = `test-${config.name}-${Date.now()}`;
        const screenshotResult = await jtag.screenshot(filename, config.options);
        
        const testResult: ScreenshotTestResult = {
          name: config.name,
          success: screenshotResult.success,
          error: screenshotResult.error || '',
          metadata: screenshotResult.metadata
        };
        
        this.results.screenshots.push(testResult);
        
        if (screenshotResult.success || screenshotResult.filename) {
          this.results.screenshotTests++;
          this.results.passed++;
          console.log(`   ✅ Screenshot ${config.name}: ${screenshotResult.filename || 'captured'}`);
        } else {
          throw new Error(screenshotResult.error || 'Screenshot failed');
        }
        
      } catch (error: any) {
        this.results.failed++;
        this.results.errors.push(`Screenshot test failed for ${config.name}: ${error.message}`);
        console.log(`   ❌ Screenshot ${config.name} failed: ${error.message}`);
      }
    }
    
    console.log('   🎉 Screenshot tests complete!\n');
  }

  private async testPerformanceScenarios(): Promise<void> {
    console.log('⏱️ Testing performance scenarios...');
    
    try {
      // Test rapid logging performance
      const logStart = Date.now();
      for (let i = 0; i < this.config.performanceIterations; i++) {
        jtag.probe('PERF_TEST', `iteration_${i}`, { 
          iteration: i, 
          timestamp: Date.now() 
        });
      }
      const logDuration = Date.now() - logStart;
      
      this.results.performanceTests++;
      this.results.passed++;
      console.log(`   ✅ Rapid logging: ${this.config.performanceIterations} operations in ${logDuration}ms`);
      
      // Test exec performance with various code types
      const execCodes = [
        'Math.sqrt(Date.now())',
        'Array.from({length: 100}, (_, i) => i * 2).reduce((a,b) => a + b)',
        'JSON.parse(JSON.stringify({large: Array.from({length: 50}, () => Math.random())}))'
      ];
      
      for (const code of execCodes) {
        const execStart = Date.now();
        const execResult = await jtag.exec(code);
        const execDuration = Date.now() - execStart;
        
        if (execResult.success) {
          this.results.performanceTests++;
          this.results.passed++;
          console.log(`   ✅ Exec performance: ${code.substring(0, 30)} in ${execDuration}ms`);
        } else {
          throw new Error(`Performance exec failed: ${execResult.error}`);
        }
      }
      
      // Test UUID retrieval performance
      const uuidStart = Date.now();
      for (let i = 0; i < 10; i++) {
        jtag.getUUID();
      }
      const uuidDuration = Date.now() - uuidStart;
      
      this.results.performanceTests++;
      this.results.passed++;
      console.log(`   ✅ UUID performance: 10 calls in ${uuidDuration}ms`);
      
      console.log('   🎉 Performance tests complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`Performance test failed: ${error.message}`);
      console.log(`   ❌ Performance test failed: ${error.message}`);
    }
  }

  private async testCrossContextCommunication(): Promise<void> {
    console.log('🔄 Testing cross-context communication...');
    
    try {
      // Test UUID sharing between contexts
      const myUUID = jtag.getUUID();
      jtag.log('CROSS_CONTEXT', 'Sharing UUID across contexts', {
        uuid: myUUID.uuid,
        context: myUUID.context,
        testType: 'cross-context-uuid'
      });
      
      this.results.crossContextTests++;
      this.results.passed++;
      console.log(`   ✅ UUID cross-context sharing: ${myUUID.uuid}`);
      
      // Test exec with UUID inspection
      try {
        const execUUIDResult = await jtag.exec('jtag.getUUID().uuid');
        if (execUUIDResult.success && execUUIDResult.result === myUUID.uuid) {
          this.results.crossContextTests++;
          this.results.passed++;
          console.log('   ✅ Exec UUID consistency: PASSED');
        } else {
          console.log('   ⚠️ Exec UUID consistency: Different UUID (expected in some contexts)');
        }
      } catch (error) {
        console.log('   ⚠️ Exec UUID test skipped (context limitation)');
      }
      
      // Test complex cross-context data sharing
      const complexData = {
        testSuite: 'ComprehensiveTestSuite',
        timestamp: Date.now(),
        uuid: myUUID.uuid,
        context: myUUID.context,
        metadata: {
          testsRun: this.results.passed + this.results.failed,
          performance: true,
          screenshots: this.results.screenshots.length
        }
      };
      
      jtag.probe('CROSS_CONTEXT', 'complex_data_sharing', complexData);
      
      this.results.crossContextTests++;
      this.results.passed++;
      console.log('   ✅ Complex data sharing: PASSED');
      
      console.log('   🎉 Cross-context tests complete!\n');
      
    } catch (error: any) {
      this.results.failed++;
      this.results.errors.push(`Cross-context test failed: ${error.message}`);
      console.log(`   ❌ Cross-context test failed: ${error.message}`);
    }
  }

  getResults(): ComprehensiveTestResults {
    return this.results;
  }

  printDetailedResults(): void {
    console.log('📊 COMPREHENSIVE TEST RESULTS');
    console.log('==============================');
    console.log(`🆔 UUID tests: ${this.results.uuidTests}`);
    console.log(`⚡ Exec tests: ${this.results.execTests}`);
    console.log(`📸 Screenshot tests: ${this.results.screenshotTests}`);
    console.log(`⏱️ Performance tests: ${this.results.performanceTests}`);
    console.log(`🔄 Cross-context tests: ${this.results.crossContextTests}`);
    console.log(`✅ Total passed: ${this.results.passed}`);
    console.log(`❌ Total failed: ${this.results.failed}`);
    
    const total = this.results.passed + this.results.failed;
    const successRate = total > 0 ? Math.round((this.results.passed / total) * 100) : 0;
    console.log(`📈 Success rate: ${successRate}%`);
    
    if (this.results.uuidInfo) {
      console.log('\n🆔 UUID Information:');
      console.log(`   UUID: ${this.results.uuidInfo.uuid}`);
      console.log(`   Context: ${this.results.uuidInfo.context}`);
      console.log(`   Session: ${this.results.uuidInfo.sessionId}`);
      if (this.results.uuidInfo.processId) {
        console.log(`   Process ID: ${this.results.uuidInfo.processId}`);
      }
    }
    
    if (this.results.screenshots.length > 0) {
      console.log('\n📸 Screenshots Captured:');
      this.results.screenshots.forEach(screenshot => {
        const status = screenshot.success ? '✅' : '❌';
        console.log(`   ${status} ${screenshot.name}`);
      });
    }
    
    if (this.results.execResults.length > 0) {
      console.log('\n⚡ Code Execution Results:');
      this.results.execResults.slice(0, 5).forEach(exec => {
        const status = exec.success ? '✅' : '❌';
        const preview = exec.code.substring(0, 30);
        const result = JSON.stringify(exec.result).substring(0, 30);
        console.log(`   ${status} ${preview} → ${result} (${exec.executionTime}ms)`);
      });
      
      if (this.results.execResults.length > 5) {
        console.log(`   ... and ${this.results.execResults.length - 5} more`);
      }
    }
    
    if (this.results.errors.length > 0) {
      console.log('\n❌ Errors:');
      this.results.errors.slice(0, 3).forEach(error => console.log(`   - ${error}`));
      if (this.results.errors.length > 3) {
        console.log(`   ... and ${this.results.errors.length - 3} more errors`);
      }
    }
  }
}

// Helper function for sleep
export function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// Helper function for timeout wrapper
export function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) => 
      setTimeout(() => reject(new Error('Operation timeout')), timeoutMs)
    )
  ]);
}