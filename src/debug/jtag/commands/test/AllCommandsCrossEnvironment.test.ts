#!/usr/bin/env tsx
/**
 * All Commands Cross-Environment Test Suite
 * 
 * Ensures every JTAG command is tested across browser and server environments.
 * Follows middle-out Layer 4 (System Integration) testing methodology.
 */

import {
  testCommandCrossEnvironment,
  testMultipleCommandsCrossEnvironment,
  generateCrossEnvironmentReport,
  validateCrossEnvironmentResults,
  STANDARD_CROSS_ENVIRONMENT_CONFIGS,
  type EnvironmentTestConfig,
  type CrossEnvironmentTestResult
} from './utils/CrossEnvironmentTestUtils';

console.log('🧪 All Commands Cross-Environment Test Suite');

// Import dynamically generated command configurations
import { ALL_COMMAND_CONFIGS, COMMAND_METADATA } from './generated-cross-environment-configs';

/**
 * Run cross-environment tests for all commands
 */
async function runAllCommandsCrossEnvironmentTests(): Promise<CrossEnvironmentTestResult[]> {
  console.log(`🚀 Testing ${ALL_COMMAND_CONFIGS.length} dynamically discovered commands across environments`);
  console.log(`📊 Command Discovery Metadata:`);
  console.log(`   Generated at: ${COMMAND_METADATA.generatedAt}`);
  console.log(`   Categories: ${COMMAND_METADATA.categories.join(', ')}`);
  console.log(`   Core commands: ${COMMAND_METADATA.coreCommands.length}`);
  console.log(`   Browser commands: ${COMMAND_METADATA.browserCommands.length}`);
  console.log(`   Chat commands: ${COMMAND_METADATA.chatCommands.length}`);
  console.log(`   File commands: ${COMMAND_METADATA.fileCommands?.length || 0}`);
  console.log('');
  
  const results = await testMultipleCommandsCrossEnvironment(ALL_COMMAND_CONFIGS);
  
  return results;
}

/**
 * Analyze and report results
 */
function analyzeResults(results: CrossEnvironmentTestResult[]): void {
  console.log('\n📊 CROSS-ENVIRONMENT TEST ANALYSIS');
  console.log('=' .repeat(50));
  
  // Overall statistics
  const totalCommands = results.length;
  const fullySuccessful = results.filter(r => r.summary.passed === r.summary.totalTests).length;
  const partiallySuccessful = results.filter(r => r.summary.passed > 0 && r.summary.passed < r.summary.totalTests).length;
  const failed = results.filter(r => r.summary.passed === 0).length;
  
  console.log(`\n📈 Overall Statistics:`);
  console.log(`   Total Commands: ${totalCommands}`);
  console.log(`   Fully Successful: ${fullySuccessful} (${(fullySuccessful/totalCommands*100).toFixed(1)}%)`);
  console.log(`   Partially Successful: ${partiallySuccessful} (${(partiallySuccessful/totalCommands*100).toFixed(1)}%)`);
  console.log(`   Failed: ${failed} (${(failed/totalCommands*100).toFixed(1)}%)`);
  
  // Environment-specific analysis
  const browserSuccesses = results.filter(r => 
    r.environments.find(e => e.environment === 'browser')?.success
  ).length;
  const serverSuccesses = results.filter(r => 
    r.environments.find(e => e.environment === 'server')?.success  
  ).length;
  
  console.log(`\n🌍 Environment Compatibility:`);
  console.log(`   Browser Success Rate: ${browserSuccesses}/${totalCommands} (${(browserSuccesses/totalCommands*100).toFixed(1)}%)`);
  console.log(`   Server Success Rate: ${serverSuccesses}/${totalCommands} (${(serverSuccesses/totalCommands*100).toFixed(1)}%)`);
  
  // Performance analysis
  const avgTimes = results
    .filter(r => r.summary.passed > 0)
    .map(r => r.summary.avgExecutionTime);
  
  if (avgTimes.length > 0) {
    const overallAvgTime = avgTimes.reduce((sum, time) => sum + time, 0) / avgTimes.length;
    const maxTime = Math.max(...avgTimes);
    const minTime = Math.min(...avgTimes);
    
    console.log(`\n⚡ Performance Analysis:`);
    console.log(`   Overall Average Time: ${overallAvgTime.toFixed(1)}ms`);
    console.log(`   Fastest Command: ${minTime.toFixed(1)}ms`);
    console.log(`   Slowest Command: ${maxTime.toFixed(1)}ms`);
  }
  
  // Failed commands analysis
  const failedCommands = results.filter(r => r.summary.passed === 0);
  if (failedCommands.length > 0) {
    console.log(`\n❌ Failed Commands:`);
    failedCommands.forEach(result => {
      console.log(`   ${result.commandName}: ${result.environments.map(e => e.error).join(', ')}`);
    });
  }
  
  // Partially successful commands
  const partialCommands = results.filter(r => r.summary.passed > 0 && r.summary.passed < r.summary.totalTests);
  if (partialCommands.length > 0) {
    console.log(`\n⚠️  Partially Successful Commands:`);
    partialCommands.forEach(result => {
      const successfulEnvs = result.environments.filter(e => e.success).map(e => e.environment);
      const failedEnvs = result.environments.filter(e => !e.success).map(e => e.environment);
      console.log(`   ${result.commandName}: ✅ ${successfulEnvs.join(', ')} | ❌ ${failedEnvs.join(', ')}`);
    });
  }
}

/**
 * Main test execution
 */
async function main(): Promise<void> {
  try {
    console.log('🏗️ Starting comprehensive cross-environment command testing...\n');
    
    // Run all cross-environment tests
    const results = await runAllCommandsCrossEnvironmentTests();
    
    // Analyze results
    analyzeResults(results);
    
    // Validate against requirements
    const validation = validateCrossEnvironmentResults(results, {
      minimumSuccessRate: 0.7, // 70% minimum success rate
      maxAvgExecutionTime: 10000, // 10 second max average
      requiredEnvironments: ['browser', 'server']
    });
    
    console.log(`\n🔍 Validation Results:`);
    if (validation.passed) {
      console.log('✅ All validation requirements met!');
    } else {
      console.log('❌ Validation failures:');
      validation.violations.forEach(violation => {
        console.log(`   - ${violation}`);
      });
    }
    
    // Generate detailed report
    const report = generateCrossEnvironmentReport(results);
    console.log('\n📋 Detailed report generated (see below):\n');
    console.log(report);
    
    // Final summary
    const overallSuccess = results.filter(r => r.summary.passed === r.summary.totalTests).length;
    const totalCommands = results.length;
    
    console.log('\n🎯 FINAL RESULTS:');
    console.log(`   Commands tested: ${totalCommands}`);
    console.log(`   Cross-environment success: ${overallSuccess}/${totalCommands} (${(overallSuccess/totalCommands*100).toFixed(1)}%)`);
    
    if (overallSuccess === totalCommands) {
      console.log('\n🎉 ALL COMMANDS SUCCESSFULLY TESTED ACROSS ENVIRONMENTS!');
      console.log('✅ Location transparency confirmed for entire JTAG command suite');
    } else {
      console.log(`\n⚠️  ${totalCommands - overallSuccess} commands need attention for full cross-environment compatibility`);
    }
    
  } catch (error) {
    console.error('\n❌ Cross-environment testing failed:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  main();
} else {
  module.exports = { 
    runAllCommandsCrossEnvironmentTests,
    ALL_COMMAND_CONFIGS,
    analyzeResults
  };
}