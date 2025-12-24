#!/usr/bin/env npx tsx
/**
 * AI Compiler Error Detection Test
 * Tests that our AI-optimized system clearly detects and reports TypeScript compiler errors
 * Following middle-out testing philosophy from dev-process.md
 */

import { execSync } from 'child_process';
import { existsSync } from 'fs';

console.log('🧪 AI COMPILER ERROR DETECTION TEST');
console.log('=' .repeat(50));

async function testCompilerErrorDetection() {
  console.log('📋 Testing TypeScript compiler error detection...');
  
  // Test 1: Check TypeScript compilation catches our intentional errors
  console.log('\n🎯 TEST 1: TypeScript Compiler Detection');
  try {
    execSync('npx tsc --noEmit --project .', { encoding: 'utf8', stdio: 'pipe' });
    console.log('❌ FAILED: TypeScript should have detected our intentional errors');
    return false;
  } catch (error: any) {
    const output = error.stdout + error.stderr;
    
    const hasBrowserError = output.includes('InvalidBrowserType');
    const hasServerError = output.includes('UnknownType');
    
    console.log(`✅ Browser error detected: ${hasBrowserError ? 'YES' : 'NO'}`);
    console.log(`✅ Server error detected: ${hasServerError ? 'YES' : 'NO'}`);
    
    if (hasBrowserError || hasServerError) {
      console.log('🎉 SUCCESS: TypeScript compiler correctly detected intentional errors');
      console.log('📊 Sample error output:');
      console.log(output.substring(0, 300) + '...');
      return true;
    } else {
      console.log('❌ FAILED: TypeScript compiler did not detect our intentional errors');
      return false;
    }
  }
}

async function testAgentDashboardErrorReporting() {
  console.log('\n🎯 TEST 2: Agent Dashboard Error Reporting');
  
  try {
    // Run agent dashboard quick check
    const result = execSync('npm run agent:quick 2>/dev/null', { encoding: 'utf8' });
    
    // Should report system as unhealthy due to compilation errors preventing startup
    const isUnhealthy = result.includes('❌ NOT READY') || result.includes('unhealthy');
    const hasGuidance = result.includes('FIRST STEP') || result.includes('system:start');
    
    console.log(`✅ System reported as unhealthy: ${isUnhealthy ? 'YES' : 'NO'}`);
    console.log(`✅ Provides AI guidance: ${hasGuidance ? 'YES' : 'NO'}`);
    
    if (isUnhealthy && hasGuidance) {
      console.log('🎉 SUCCESS: Agent dashboard correctly reports compilation issues');
      return true;
    } else {
      console.log('⚠️ PARTIAL: Agent dashboard may not clearly show compilation errors');
      console.log('📊 Dashboard output sample:');
      console.log(result.substring(0, 400));
      return false;
    }
    
  } catch (error: any) {
    console.log('⚠️ WARNING: Could not run agent dashboard - may be affected by compilation errors');
    console.log(`Error: ${error.message.substring(0, 200)}`);
    return false;
  }
}

async function testAIErrorWorkflow() {
  console.log('\n🎯 TEST 3: AI Error Recovery Workflow');
  
  // Test if system signal reflects compilation problems
  const signalFile = '.continuum/jtag/signals/system-ready.json';
  
  if (existsSync(signalFile)) {
    try {
      const signalContent = execSync(`cat ${signalFile}`, { encoding: 'utf8' });
      const signalData = JSON.parse(signalContent);
      
      const isUnhealthy = signalData.systemHealth === 'unhealthy' || signalData.systemHealth === 'error';
      const hasErrors = signalData.errors && signalData.errors.length > 0;
      const compilationFailed = signalData.compilationStatus === 'failed';
      
      console.log(`✅ Signal shows unhealthy: ${isUnhealthy ? 'YES' : 'NO'}`);
      console.log(`✅ Signal shows errors: ${hasErrors ? 'YES' : 'NO'}`);
      console.log(`✅ Compilation status failed: ${compilationFailed ? 'YES' : 'NO'}`);
      console.log(`📊 System health: ${signalData.systemHealth}`);
      
      if (isUnhealthy || hasErrors || compilationFailed) {
        console.log('🎉 SUCCESS: System signals correctly reflect compilation issues');
        return true;
      } else {
        console.log('⚠️ PARTIAL: System signals may not clearly reflect compilation errors');
        return false;
      }
      
    } catch (error) {
      console.log('⚠️ WARNING: Could not parse system signal file');
      return false;
    }
  } else {
    console.log('⚠️ WARNING: No system signal file found - system may not be running');
    return false;
  }
}

async function showAICommands() {
  console.log('\n🤖 AI-FRIENDLY ERROR DETECTION COMMANDS');
  console.log('-' .repeat(40));
  
  const aiCommands = [
    {
      name: 'Check TypeScript compilation',
      command: 'npx tsc --noEmit --project .',
      purpose: 'Detect all TypeScript errors before runtime'
    },
    {
      name: 'Quick system health',
      command: 'npm run agent:quick',
      purpose: 'Get instant AI-optimized system status'
    },
    {
      name: 'Full AI dashboard',
      command: 'npm run agent',
      purpose: 'Complete development control room for AIs'
    },
    {
      name: 'Check system signal',
      command: 'cat .continuum/jtag/signals/system-ready.json',
      purpose: 'See system health in machine-readable format'
    },
    {
      name: 'Check startup logs',
      command: 'tail -20 .continuum/jtag/logs/system/npm-start.log',
      purpose: 'See why system startup may be failing'
    }
  ];
  
  aiCommands.forEach((cmd, i) => {
    console.log(`${i + 1}. ${cmd.name}`);
    console.log(`   Command: ${cmd.command}`);
    console.log(`   Purpose: ${cmd.purpose}`);
    console.log('');
  });
}

// Run the tests
async function main() {
  const results = [
    await testCompilerErrorDetection(),
    await testAgentDashboardErrorReporting(), 
    await testAIErrorWorkflow()
  ];
  
  await showAICommands();
  
  const passedTests = results.filter(Boolean).length;
  const totalTests = results.length;
  
  console.log('\n📊 TEST RESULTS');
  console.log('=' .repeat(50));
  console.log(`Passed: ${passedTests}/${totalTests} tests`);
  
  if (passedTests === totalTests) {
    console.log('🎉 ALL TESTS PASSED: AI compiler error detection system is working!');
  } else if (passedTests > 0) {
    console.log('⚠️ PARTIAL SUCCESS: Some error detection features working');
  } else {
    console.log('❌ TESTS FAILED: AI error detection needs improvement');
  }
  
  console.log('\n💡 NEXT STEPS FOR AI AGENTS:');
  console.log('1. Use "npx tsc --noEmit" to detect TypeScript errors');
  console.log('2. Use "npm run agent:quick" for instant health checks');
  console.log('3. Fix detected errors before running system');
  console.log('4. Verify fixes with "npm start" or "./jtag screenshot"');
}

main().catch(console.error);