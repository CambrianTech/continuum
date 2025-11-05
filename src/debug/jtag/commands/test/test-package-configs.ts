#!/usr/bin/env tsx
/**
 * Test Package.json-based Configuration Discovery
 * 
 * Demonstrates loading EnvironmentTestConfig from command package.json files
 * instead of using generated configuration files.
 */

import { join } from 'path';
import { discoverCommandConfigs, getAllEnvironmentTestConfigs } from './utils/PackageConfigLoader';
import { testCommandCrossEnvironment } from './utils/CrossEnvironmentTestUtils';

async function main() {
  console.log('🧪 Testing Package.json-based Configuration Discovery');
  console.log('='.repeat(60));
  
  const commandsPath = join(__dirname, '..');
  
  try {
    // Discover all command configurations
    const commands = await discoverCommandConfigs(commandsPath);
    
    console.log(`\n📊 Summary:`);
    console.log(`   Total commands with configs: ${commands.length}`);
    
    // Group by category
    const byCategory = commands.reduce((acc, cmd) => {
      acc[cmd.category] = (acc[cmd.category] || 0) + 1;
      return acc;
    }, {} as Record<string, number>);
    
    console.log(`   By category:`);
    for (const [category, count] of Object.entries(byCategory)) {
      console.log(`     ${category}: ${count} commands`);
    }
    
    // Show detailed configs
    console.log(`\n📋 Command Configurations:`);
    for (const command of commands) {
      console.log(`\n🔧 ${command.name} (${command.category})`);
      console.log(`   Path: ${command.path}`);
      console.log(`   Environments: ${command.environmentTestConfig.environments.join(', ')}`);
      console.log(`   Performance threshold: ${command.environmentTestConfig.performanceThresholdMs}ms`);
      
      if (command.packageConfig.environmentTestConfig?.testScenarios) {
        console.log(`   Test scenarios: ${command.packageConfig.environmentTestConfig.testScenarios.length}`);
        for (const scenario of command.packageConfig.environmentTestConfig.testScenarios) {
          console.log(`     - ${scenario.name}: ${scenario.description}`);
        }
      }
    }
    
    // Test a specific command if available
    if (commands.length > 0) {
      console.log(`\n🧪 Testing first command: ${commands[0].name}`);
      
      // Run cross-environment test on the first command
      try {
        const testResult = await testCommandCrossEnvironment(commands[0].environmentTestConfig);
        
        console.log(`\n📊 Cross-Environment Test Results:`);
        console.log(`   Command: ${testResult.commandName}`);
        console.log(`   Tests: ${testResult.summary.passed}/${testResult.summary.totalTests} passed`);
        
        for (const envResult of testResult.environments) {
          const status = envResult.success ? '✅' : '❌';
          console.log(`   ${status} ${envResult.environment}: ${envResult.executionTime}ms`);
          if (envResult.error) {
            console.log(`      Error: ${envResult.error}`);
          }
        }
      } catch (error) {
        console.log(`❌ Cross-environment test failed: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
    
    console.log(`\n✅ Package.json configuration discovery successful!`);
    
  } catch (error) {
    console.error('❌ Configuration discovery failed:', error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  main();
}