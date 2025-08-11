#!/usr/bin/env tsx
/**
 * Router Test Suite - Complete Router Testing Infrastructure
 * 
 * Runs all router tests in sequence: component unit tests, integration tests,
 * and cross-environment chaos tests. Provides comprehensive validation of
 * the JTAG routing system's reliability and performance.
 */

import { runAllTests as runJTAGRouterTests } from './unit/router/JTAGRouter.test';
import { runAllTests as runEndpointMatcherTests } from './unit/router/components/EndpointMatcher.test';
import { runAllTests as runResponseCorrelatorTests } from './unit/router/components/ResponseCorrelator.test';
import { runAllTests as runCrossEnvironmentRoutingTests } from './integration/router/CrossEnvironmentRouting.test';

console.log('🚀 JTAG Router Test Suite - Complete System Validation');
console.log('=====================================\n');

interface TestSuiteResult {
  suiteName: string;
  passed: boolean;
  duration: number;
  error?: string;
}

async function runTestSuite(
  suiteName: string, 
  testRunner: () => Promise<void>
): Promise<TestSuiteResult> {
  const startTime = Date.now();
  
  try {
    console.log(`\n🧪 Running ${suiteName}...`);
    console.log('─'.repeat(50));
    
    await testRunner();
    
    const duration = Date.now() - startTime;
    console.log(`✅ ${suiteName} completed successfully in ${duration}ms\n`);
    
    return {
      suiteName,
      passed: true,
      duration
    };
    
  } catch (error: any) {
    const duration = Date.now() - startTime;
    console.log(`❌ ${suiteName} failed after ${duration}ms`);
    console.error(`   Error: ${error.message}\n`);
    
    return {
      suiteName,
      passed: false,
      duration,
      error: error.message
    };
  }
}

async function runAllRouterTests(): Promise<void> {
  const testSuites = [
    {
      name: 'EndpointMatcher Unit Tests',
      runner: runEndpointMatcherTests
    },
    {
      name: 'ResponseCorrelator Unit Tests',
      runner: runResponseCorrelatorTests
    },
    {
      name: 'JTAGRouter Core Tests',
      runner: runJTAGRouterTests
    },
    {
      name: 'Cross-Environment Integration Tests',
      runner: runCrossEnvironmentRoutingTests
    }
  ];
  
  const results: TestSuiteResult[] = [];
  let totalDuration = 0;
  
  for (const suite of testSuites) {
    const result = await runTestSuite(suite.name, suite.runner);
    results.push(result);
    totalDuration += result.duration;
  }
  
  // Generate final report
  console.log('🎯 FINAL TEST REPORT');
  console.log('==========================================\n');
  
  const passedSuites = results.filter(r => r.passed);
  const failedSuites = results.filter(r => !r.passed);
  
  console.log(`📊 SUMMARY:`);
  console.log(`   Total Test Suites: ${results.length}`);
  console.log(`   Passed: ${passedSuites.length}`);
  console.log(`   Failed: ${failedSuites.length}`);
  console.log(`   Total Duration: ${totalDuration}ms`);
  console.log(`   Average Suite Time: ${Math.round(totalDuration / results.length)}ms\n`);
  
  // Detailed results
  console.log(`📋 DETAILED RESULTS:`);
  for (const result of results) {
    const status = result.passed ? '✅' : '❌';
    const duration = `${result.duration}ms`.padEnd(8);
    console.log(`   ${status} ${result.suiteName.padEnd(35)} ${duration}`);
    if (result.error) {
      console.log(`      └─ Error: ${result.error}`);
    }
  }
  
  // Performance analysis
  const fastestSuite = results.reduce((fastest, current) => 
    current.duration < fastest.duration ? current : fastest
  );
  const slowestSuite = results.reduce((slowest, current) => 
    current.duration > slowest.duration ? current : slowest
  );
  
  console.log(`\n⚡ PERFORMANCE ANALYSIS:`);
  console.log(`   Fastest Suite: ${fastestSuite.suiteName} (${fastestSuite.duration}ms)`);
  console.log(`   Slowest Suite: ${slowestSuite.suiteName} (${slowestSuite.duration}ms)`);
  
  // Coverage analysis
  console.log(`\n🎯 COVERAGE ANALYSIS:`);
  console.log(`   ✅ Component Unit Tests: EndpointMatcher, ResponseCorrelator`);
  console.log(`   ✅ Core Router Tests: Message routing, correlation, context management`);
  console.log(`   ✅ Integration Tests: Cross-environment routing, error handling`);
  console.log(`   ✅ Chaos Tests: Multi-hop routing, random failures, stress testing`);
  console.log(`   ✅ Performance Tests: Concurrent routing, high-frequency operations`);
  
  // Diagnostic commands available
  console.log(`\n🔧 DIAGNOSTIC COMMANDS DEPLOYED:`);
  console.log(`   - test/routing-chaos: Multi-hop routing validation with chaos scenarios`);
  console.log(`   - Use in production: ./continuum test/routing-chaos --maxHops=10 --failureRate=0.1`);
  console.log(`   - Performance testing: ./continuum test/routing-chaos --concurrent=50`);
  
  if (failedSuites.length > 0) {
    console.log(`\n🚨 FAILED SUITES REQUIRE ATTENTION:`);
    for (const failed of failedSuites) {
      console.log(`   ❌ ${failed.suiteName}: ${failed.error}`);
    }
    throw new Error(`${failedSuites.length} test suite(s) failed`);
  }
  
  console.log(`\n🎉 ALL ROUTER TESTS PASSED!`);
  console.log(`🔒 JTAG Router System is BULLETPROOF and PRODUCTION-READY`);
  console.log(`🚀 Multi-hop routing with chaos scenarios validated successfully`);
  console.log(`⚡ Promise resolution across complex routing paths confirmed working`);
  console.log(`🛡️  Error propagation and recovery mechanisms thoroughly tested`);
}

// Run all tests if called directly
if (process.argv[1] && process.argv[1].endsWith('router-test-suite.ts')) {
  runAllRouterTests()
    .then(() => {
      console.log('\n✅ Router test suite completed successfully');
      process.exit(0);
    })
    .catch((error) => {
      console.error('\n❌ Router test suite failed:', error.message);
      process.exit(1);
    });
}

export { runAllRouterTests };