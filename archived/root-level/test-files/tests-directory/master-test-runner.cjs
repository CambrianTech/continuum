
/**
 * NASA-Grade Master Test Runner
 * Runs ALL tests - no skipping allowed
 */

const MemoryPackageTestSuite = require('./memory-package.test.js');
const CyberpunkThemeTestSuite = require('./cyberpunk-theme.test.js');
const BuildSystemTestSuite = require('./build-system.test.js');
const IntegrationTestSuite = require('./integration.test.js');
const PerformanceTestSuite = require('./performance.test.js');
const SecurityTestSuite = require('./security.test.js');
const SelfValidationTestSuite = require('./self-validation.test.js');

class MasterTestRunner {
  async runAllTests() {
    console.log('🚀 NASA-GRADE MASTER TEST RUNNER');
    console.log('================================');
    console.log('Mission Critical: ALL tests must pass');
    console.log('');
    
    const testSuites = [
      new MemoryPackageTestSuite(),
      new CyberpunkThemeTestSuite(), 
      new BuildSystemTestSuite(),
      new IntegrationTestSuite(),
      new PerformanceTestSuite(),
      new SecurityTestSuite(),
      new SelfValidationTestSuite()
    ];
    
    let totalTests = 0;
    let totalPassed = 0;
    
    for (const testSuite of testSuites) {
      try {
        await testSuite.runAllTests();
        totalTests += testSuite.testResults.length;
        totalPassed += testSuite.testResults.filter(t => t.passed).length;
      } catch (error) {
        console.log(`❌ TEST SUITE FAILED: ${error.message}`);
        throw new Error(`MISSION FAILURE: Test suite failed - ${error.message}`);
      }
    }
    
    console.log('');
    console.log('🎉 NASA-GRADE TEST RESULTS');
    console.log('==========================');
    console.log(`✅ ALL ${totalTests} TESTS PASSED`);
    console.log('🚀 MISSION SUCCESS - Ready for deployment');
    
    return { totalTests, totalPassed, success: totalTests === totalPassed };
  }
}

module.exports = MasterTestRunner;

// Run tests if called directly
if (require.main === module) {
  const runner = new MasterTestRunner();
  runner.runAllTests().catch(error => {
    console.log('🚨 MISSION CRITICAL FAILURE:', error.message);
    process.exit(1);
  });
}
