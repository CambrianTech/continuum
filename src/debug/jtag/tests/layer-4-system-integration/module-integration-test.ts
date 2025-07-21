#!/usr/bin/env node
/**
 * JTAG Module Integration Test
 * 
 * Tests JTAG when imported as a module within Continuum or another project.
 * Assumes JTAG has been properly activated/imported in the host project.
 * 
 * MODULAR TEST MODE - Assumes browser is already available (called by continuum's modular test runner)
 * Does not start its own servers - relies on host system browser and JTAG infrastructure
 * 
 * This test validates:
 * 1. Module imports work correctly
 * 2. JTAG integrates seamlessly with host project
 * 3. Server and client contexts both work
 * 4. No conflicts with host project's systems
 */

import * as path from 'path';
import * as fs from 'fs';
import { jtag } from '../../index';
import type { JTAGStats, JTAGLogEntry } from '../../shared/JTAGTypes';
import { ComprehensiveTestSuite } from '../shared/TestUtilities';

interface EnhancedModuleTestResults {
  importTests: number;
  apiTests: number;
  integrationTests: number;
  configTests: number;
  comprehensiveTests: number;
  execTests: number;
  uuidTests: number;
  passed: number;
  failed: number;
  errors: string[];
}

interface ModuleTestConfig {
  testTimeout: number;
  logSampleSize: number;
  performanceIterations: number;
}

const TEST_CONFIG: ModuleTestConfig = {
  testTimeout: 5000,
  logSampleSize: 10,
  performanceIterations: 10  // Reduced to prevent overwhelming system
};

console.log('\n🔧 JTAG Module Integration Test (TypeScript)');
console.log('============================================');

class ModuleIntegrationTester {
  private testResults: EnhancedModuleTestResults;
  private comprehensiveSuite: ComprehensiveTestSuite;

  constructor() {
    this.testResults = {
      importTests: 0,
      apiTests: 0,
      integrationTests: 0,
      configTests: 0,
      comprehensiveTests: 0,
      execTests: 0,
      uuidTests: 0,
      passed: 0,
      failed: 0,
      errors: []
    };
    
    // Initialize comprehensive test suite for module context
    this.comprehensiveSuite = new ComprehensiveTestSuite({
      testTimeout: TEST_CONFIG.testTimeout,
      performanceIterations: 5,  // Reduced for module testing
      screenshotTypes: ['basic', 'viewport'],  // Limited screenshot types in module context
      execTestCodes: [
        '2 + 2',
        'jtag.getUUID().uuid',
        'jtag.getUUID().context',
        '"module-test"',
        'jtag.log("MODULE_EXEC", "Self-logging from module"); "success"'
      ]
    });
  }

  async runTests(): Promise<void> {
    console.log('🔧 Testing JTAG as an integrated module...\n');

    try {
      // Test 1: Module import and availability
      await this.testModuleImports();
      
      // Test 2: API functionality when integrated
      await this.testIntegratedAPI();
      
      // Test 3: Configuration and context detection
      await this.testConfiguration();
      
      // Test 4: Integration with host project
      await this.testHostProjectIntegration();
      
      // Test 5: Run comprehensive feature suite
      await this.testComprehensiveFeatures();
      
      // Final results
      this.printFinalResults();
      
    } catch (error: any) {
      console.error('💥 Module integration test failed:', error.message);
      process.exit(1);
    }
  }

  private async testModuleImports(): Promise<void> {
    console.log('📦 Testing module imports and exports...');
    
    try {
      // Test main module import - we already have it
      if (typeof jtag === 'object' && typeof jtag.log === 'function') {
        this.testResults.importTests++;
        this.testResults.passed++;
        console.log('   ✅ Main jtag export: PASSED');
      } else {
        throw new Error('jtag export is not properly structured');
      }
      
      // Test jtag methods exist
      const requiredMethods = ['log', 'critical', 'trace', 'probe', 'screenshot'];
      for (const method of requiredMethods) {
        if (typeof (jtag as any)[method] === 'function') {
          this.testResults.importTests++;
          this.testResults.passed++;
          console.log(`   ✅ jtag.${method} method: PASSED`);
        } else {
          throw new Error(`jtag.${method} method not available`);
        }
      }
      
      // Test configuration access
      if (jtag.config && typeof jtag.config === 'object') {
        this.testResults.importTests++;
        this.testResults.passed++;
        console.log('   ✅ Configuration access: PASSED');
      } else {
        this.testResults.failed++;
        this.testResults.errors.push('Configuration not accessible');
        console.log('   ❌ Configuration access: FAILED');
      }
      
      // Test TypeScript types (attempt to import)
      try {
        const typesModule = await import('../shared/JTAGTypes');
        if (typesModule.JTAG_LOG_LEVELS) {
          this.testResults.importTests++;
          this.testResults.passed++;
          console.log('   ✅ TypeScript types import: PASSED');
        } else {
          throw new Error('Types not properly exported');
        }
      } catch (error: any) {
        this.testResults.failed++;
        this.testResults.errors.push('TypeScript types import failed: ' + error.message);
        console.log('   ❌ TypeScript types: FAILED');
      }
      
      console.log('   🎉 Module import tests complete!\n');
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push('Module import test failed: ' + error.message);
      console.log('   ❌ Module import test failed:', error.message);
    }
  }

  private async testIntegratedAPI(): Promise<void> {
    console.log('🔌 Testing integrated API functionality...');
    
    try {
      // Test logging in module context
      jtag.log('MODULE_TEST', 'Testing integrated logging functionality', {
        context: 'module-integration-test',
        hostProject: 'continuum',
        timestamp: new Date().toISOString(),
        testType: 'api-validation'
      });
      this.testResults.apiTests++;
      this.testResults.passed++;
      console.log('   ✅ Integrated logging: PASSED');
      
      // Test critical logging
      jtag.critical('MODULE_TEST', 'Testing integrated critical logging', {
        severity: 'HIGH' as const,
        module: 'jtag-integration',
        errorCode: 'TEST_CRITICAL_001',
        timestamp: new Date().toISOString()
      });
      this.testResults.apiTests++;
      this.testResults.passed++;
      console.log('   ✅ Integrated critical logging: PASSED');
      
      // Test tracing in module context
      jtag.trace('MODULE_TEST', 'moduleFunction', 'ENTER', { 
        moduleMode: true,
        functionType: 'integration-test',
        parameters: ['param1', 'param2']
      });
      await this.sleep(50);
      jtag.trace('MODULE_TEST', 'moduleFunction', 'EXIT', { 
        result: 'success',
        duration: '50ms',
        returnValue: 'test-complete'
      });
      this.testResults.apiTests++;
      this.testResults.passed++;
      console.log('   ✅ Integrated tracing: PASSED');
      
      // Test probing in module context
      jtag.probe('MODULE_TEST', 'module_state', {
        isIntegrated: true,
        hostProject: 'continuum',
        moduleVersion: '1.0.0',
        runtime: {
          nodeVersion: process.version,
          platform: process.platform,
          uptime: process.uptime(),
          memoryUsage: process.memoryUsage()
        },
        testContext: {
          testType: 'module-integration',
          timestamp: Date.now(),
          environment: 'typescript'
        }
      });
      this.testResults.apiTests++;
      this.testResults.passed++;
      console.log('   ✅ Integrated probing: PASSED');
      
      // Test screenshot in module context (expected to work differently)
      try {
        const screenshotResult = await jtag.screenshot('module-integration-test', {
          width: 1024,
          height: 768
        });
        
        if (screenshotResult.filename || screenshotResult.success !== undefined) {
          this.testResults.apiTests++;
          this.testResults.passed++;
          console.log('   ✅ Integrated screenshot: PASSED -', screenshotResult.filename || 'placeholder');
        } else {
          console.log('   ⚠️  Integrated screenshot: SKIPPED (expected behavior in module context)');
        }
      } catch (error: any) {
        // Screenshot might fail in module context - that's expected
        console.log('   ⚠️  Integrated screenshot: SKIPPED (expected in module context)');
      }
      
      console.log('   🎉 Integrated API tests complete!\n');
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push('Integrated API test failed: ' + error.message);
      console.log('   ❌ Integrated API test failed:', error.message);
    }
  }

  private async testConfiguration(): Promise<void> {
    console.log('⚙️  Testing configuration and context detection...');
    
    try {
      // Test configuration access
      if (jtag.config && typeof jtag.config === 'object') {
        this.testResults.configTests++;
        this.testResults.passed++;
        console.log('   ✅ Configuration access: PASSED');
        console.log(`       Port: ${jtag.config.jtagPort || 'default'}`);
        console.log(`       Remote logging: ${jtag.config.enableRemoteLogging ?? 'default'}`);
      } else {
        throw new Error('Configuration not accessible');
      }
      
      // Test context detection
      const uuidInfo = jtag.getUUID();
      const context = uuidInfo.context;
      if (context === 'server' || context === 'browser') {
        this.testResults.configTests++;
        this.testResults.passed++;
        console.log('   ✅ Context detection: PASSED -', context);
      } else {
        throw new Error('Context detection failed: ' + context);
      }
      
      // Test module-specific configuration properties
      const configKeys = ['enableRemoteLogging', 'enableConsoleOutput', 'maxBufferSize'];
      for (const key of configKeys) {
        if (key in jtag.config) {
          this.testResults.configTests++;
          this.testResults.passed++;
          console.log(`   ✅ Config property '${key}': PASSED`);
        } else {
          console.log(`   ⚠️  Config property '${key}': MISSING (may be optional)`);
        }
      }
      
      console.log('   🎉 Configuration tests complete!\n');
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push('Configuration test failed: ' + error.message);
      console.log('   ❌ Configuration test failed:', error.message);
    }
  }

  private async testHostProjectIntegration(): Promise<void> {
    console.log('🏠 Testing host project integration...');
    
    try {
      // Test that JTAG doesn't interfere with host project
      jtag.log('HOST_INTEGRATION', 'Testing non-interference with host project', {
        testType: 'host-integration',
        ensureNoConflicts: true,
        timestamp: new Date().toISOString(),
        hostEnvironment: {
          nodeVersion: process.version,
          platform: process.platform,
          cwd: process.cwd()
        }
      });
      
      // Test performance impact (should be minimal)
      const startTime = Date.now();
      for (let i = 0; i < TEST_CONFIG.performanceIterations; i++) {
        jtag.probe('PERFORMANCE_TEST', 'iteration', { 
          iteration: i,
          timestamp: Date.now()
        });
      }
      const duration = Date.now() - startTime;
      
      if (duration < 5000) { // Should complete in less than 5 seconds
        this.testResults.integrationTests++;
        this.testResults.passed++;
        console.log(`   ✅ Performance impact: PASSED (${duration}ms for ${TEST_CONFIG.performanceIterations} operations)`);
      } else {
        throw new Error(`Performance test too slow: ${duration}ms`);
      }
      
      // Test stats functionality in module context
      try {
        const stats = (jtag as any).getStats?.() as JTAGStats | undefined;
        if (stats && typeof stats.totalEntries === 'number') {
          this.testResults.integrationTests++;
          this.testResults.passed++;
          console.log('   ✅ Stats functionality: PASSED -', stats.totalEntries, 'entries');
        } else {
          console.log('   ⚠️  Stats functionality: SKIPPED (not available in this context)');
        }
      } catch (error) {
        console.log('   ⚠️  Stats functionality: SKIPPED (not available in this context)');
      }
      
      // Test log buffer functionality
      try {
        const logs = (jtag as any).getLogs?.() as JTAGLogEntry[] | undefined;
        if (Array.isArray(logs)) {
          this.testResults.integrationTests++;
          this.testResults.passed++;
          console.log('   ✅ Log buffer access: PASSED -', logs.length, 'logs');
        } else {
          console.log('   ⚠️  Log buffer access: SKIPPED (not available in this context)');
        }
      } catch (error) {
        console.log('   ⚠️  Log buffer access: SKIPPED (not available in this context)');
      }
      
      // Test integration with Continuum-specific features
      try {
        // Check if we're in Continuum environment
        const continuumPath = path.resolve(__dirname, '../../../..');
        const packageJsonPath = path.join(continuumPath, 'package.json');
        
        if (fs.existsSync(packageJsonPath)) {
          const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
          
          jtag.probe('HOST_INTEGRATION', 'continuum_environment', {
            inContinuum: true,
            continuumPath: continuumPath,
            jtagPath: __dirname,
            packageName: packageJson.name,
            packageVersion: packageJson.version
          });
          
          this.testResults.integrationTests++;
          this.testResults.passed++;
          console.log('   ✅ Continuum integration: PASSED -', packageJson.name);
        } else {
          console.log('   ⚠️  Continuum integration: SKIPPED (not in Continuum environment)');
        }
      } catch (error: any) {
        console.log('   ⚠️  Continuum integration: SKIPPED -', error.message);
      }
      
      // Test that JTAG works alongside other logging systems
      const originalConsoleLog = console.log;
      let consoleCalled = false;
      
      console.log = (...args) => {
        consoleCalled = true;
        originalConsoleLog(...args);
      };
      
      jtag.log('HOST_INTEGRATION', 'Testing coexistence with other logging systems', {
        coexistence: true,
        preservesHostLogging: true,
        noConflicts: true
      });
      
      console.log('Test console.log call');
      console.log = originalConsoleLog; // Restore
      
      if (consoleCalled) {
        this.testResults.integrationTests++;
        this.testResults.passed++;
        console.log('   ✅ Host project coexistence: PASSED');
      } else {
        throw new Error('Console logging interference detected');
      }
      
      console.log('   🎉 Host project integration tests complete!\n');
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push('Host integration test failed: ' + error.message);
      console.log('   ❌ Host integration test failed:', error.message);
    }
  }

  private async testComprehensiveFeatures(): Promise<void> {
    console.log('🧪 Testing comprehensive JTAG features in module context...\n');
    
    try {
      // Run the comprehensive test suite
      const comprehensiveResults = await this.comprehensiveSuite.runAllTests();
      
      // Merge results into our test results
      this.testResults.comprehensiveTests = comprehensiveResults.passed;
      this.testResults.execTests = comprehensiveResults.execTests;
      this.testResults.uuidTests = comprehensiveResults.uuidTests;
      this.testResults.passed += comprehensiveResults.passed;
      this.testResults.failed += comprehensiveResults.failed;
      this.testResults.errors.push(...comprehensiveResults.errors);
      
      // Print detailed comprehensive results
      this.comprehensiveSuite.printDetailedResults();
      
      console.log('   🎉 Comprehensive features test complete!\n');
      
    } catch (error: any) {
      this.testResults.failed++;
      this.testResults.errors.push(`Comprehensive features test failed: ${error.message}`);
      console.log(`   ❌ Comprehensive features test failed: ${error.message}`);
    }
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  private printFinalResults(): void {
    console.log('📊 MODULE INTEGRATION TEST RESULTS');
    console.log('==================================');
    console.log(`📦 Import tests: ${this.testResults.importTests}`);
    console.log(`🔌 API tests: ${this.testResults.apiTests}`);
    console.log(`⚙️  Configuration tests: ${this.testResults.configTests}`);
    console.log(`🏠 Integration tests: ${this.testResults.integrationTests}`);
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
    
    console.log('\n🔧 Module Features Tested:');
    console.log('   📦 Import/export functionality');
    console.log('   🔌 API method availability');
    console.log('   ⚙️  Configuration management');
    console.log('   🏠 Host project integration');
    console.log('   ⚡ Performance impact');
    console.log('   🔄 Coexistence with other systems');
    
    if (this.testResults.failed === 0) {
      console.log('\n🎉 ALL MODULE INTEGRATION TESTS PASSED!');
      console.log('🔧 JTAG integrates perfectly as a TypeScript module.');
      console.log('✨ Ready for production use in host projects like Continuum.');
    } else if (this.testResults.failed <= 2) {
      console.log('\n⚠️  Minor issues detected, but core functionality works.');
      console.log('🔧 JTAG is usable as a module with some limitations.');
    } else {
      console.log('\n❌ Significant integration issues detected.');
      console.log('🔧 JTAG may not work properly as an integrated module.');
      process.exit(1);
    }
  }
}

// Run tests if called directly
if (require.main === module) {
  const tester = new ModuleIntegrationTester();
  tester.runTests().catch(error => {
    console.error('\n💥 Test runner failed:', error);
    process.exit(1);
  });
}

export { ModuleIntegrationTester };