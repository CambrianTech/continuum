/**
 * UDP Transport Comprehensive Test
 * 
 * Tests the actual UDP transport using the universal test framework.
 * This validates both the transport and the test framework architecture.
 */

import { 
  TransportTestSuite, 
  TransportTestRunner, 
  TestResultFormatter,
  TestEnvironment,
  TestCategory 
} from './framework/TransportTestFramework';
import { UDPTransportFactory } from './factories/UDPTransportFactory';
import { BASIC_TRANSPORT_SCENARIOS } from './scenarios/BasicTransportScenarios';

/**
 * Run comprehensive UDP transport tests
 */
async function runUDPTransportComprehensiveTests(): Promise<void> {
  console.log('🚀 UDP TRANSPORT COMPREHENSIVE TESTS');
  console.log('=====================================');
  console.log('Testing the real UDP multicast transport using universal test framework');
  console.log();

  // Create factory and test runner
  const factory = new UDPTransportFactory();
  const runner = new TransportTestRunner(factory, TestEnvironment.SERVER);
  
  try {
    // Create test suite
    const testSuite = new TransportTestSuite('UDP Multicast Transport');
    testSuite.addScenarios(BASIC_TRANSPORT_SCENARIOS);
    
    console.log(`📋 Test Suite: ${testSuite.name}`);
    console.log(`🏭 Transport Factory: ${factory.name}`);
    console.log(`🌍 Test Environment: ${TestEnvironment.SERVER}`);
    console.log(`📂 Test Categories: ${testSuite.getCategories().join(', ')}`);
    console.log();
    
    // Run the full test suite
    const result = await testSuite.execute(runner);
    
    // Format and display results
    TestResultFormatter.formatSuiteResult(result);
    
    // Exit with appropriate code
    const exitCode = result.successRate === 100 ? 0 : 1;
    console.log(`\\n🏁 Test execution complete - exit code: ${exitCode}`);
    
    process.exit(exitCode);
    
  } catch (error: any) {
    console.error('\\n💥 Test suite execution failed:', error.message);
    console.error('Stack trace:', error.stack);
    process.exit(1);
  } finally {
    // Ensure cleanup happens
    try {
      await runner.cleanup();
      await factory.cleanupAll();
    } catch (cleanupError: any) {
      console.warn('⚠️ Cleanup error:', cleanupError.message);
    }
  }
}

// Run if executed directly
if (require.main === module) {
  runUDPTransportComprehensiveTests().catch(error => {
    console.error('❌ Test suite crashed:', error);
    process.exit(1);
  });
}

export { runUDPTransportComprehensiveTests };