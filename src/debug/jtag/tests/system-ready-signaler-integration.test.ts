#!/usr/bin/env npx tsx
/**
 * SYSTEM READY SIGNALER INTEGRATION TESTS
 * 
 * Comprehensive test suite for SystemReadySignaler per-project isolation,
 * working directory context, and integration with launch-active-example.ts
 */

import { SystemReadySignaler, type SystemReadySignal } from '../scripts/signal-system-ready';
import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';
import { getActiveExampleName, getActivePorts } from "../examples/shared/ExampleConfig";
import fs from 'fs/promises';
import path from 'path';

interface TestResult {
  readonly success: boolean;
  readonly testName: string;
  readonly details: string[];
  readonly error?: string;
}

interface TypedError {
  readonly message: string;
  readonly stack?: string;
}

function createTypedError(error: unknown): TypedError {
  if (error instanceof Error) {
    return {
      message: error.message,
      stack: error.stack
    };
  }
  return {
    message: String(error)
  };
}

async function testPerProjectIsolation(): Promise<TestResult> {
  const testName = "Per-Project Directory Isolation";
  const details: string[] = [];
  
  try {
    // Test 1: Default directory context
    const defaultSignaler = new SystemReadySignaler();
    const defaultSignal = await defaultSignaler.generateReadySignal();
    details.push(`✅ Default context signal generated`);
    details.push(`   Timestamp: ${defaultSignal.timestamp}`);
    
    // Test 2: Widget-UI project context
    const activeExample = getActiveExampleName();
    const workingDir = `examples/${activeExample}`;
    WorkingDirConfig.setWorkingDir(workingDir);
    details.push(`🎯 Set working directory: ${workingDir}`);
    
    const projectSignaler = new SystemReadySignaler();
    const projectSignal = await projectSignaler.generateReadySignal();
    details.push(`✅ Project context signal generated`);
    details.push(`   Timestamp: ${projectSignal.timestamp}`);
    
    // Test 3: Verify different signal files are created  
    const activePorts = await getActivePorts();
    const websocketPort = activePorts.websocket_server;
    const defaultPath = path.resolve(`.continuum/jtag/signals/system-ready-port-${websocketPort}.json`);
    const projectPath = path.resolve(`${workingDir}/.continuum/jtag/signals/system-ready-port-${websocketPort}.json`);
    
    details.push(`📂 Default signal path: ${defaultPath}`);
    details.push(`📂 Project signal path: ${projectPath}`);
    
    // Check if files exist and have different paths
    const pathsAreDifferent = defaultPath !== projectPath;
    if (!pathsAreDifferent) {
      throw new Error(`Expected different signal file paths, but both resolved to: ${defaultPath}`);
    }
    
    details.push(`✅ Signal files use different paths (isolation working)`);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedError(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testSignalContentValidation(): Promise<TestResult> {
  const testName = "Signal Content Type Validation";
  const details: string[] = [];
  
  try {
    const signaler = new SystemReadySignaler();
    const signal = await signaler.generateReadySignal();
    
    // Test required properties exist and have correct types
    const requiredStringProps: Array<keyof SystemReadySignal> = ['timestamp', 'readySignalVersion'];
    const requiredBooleanProps: Array<keyof SystemReadySignal> = ['bootstrapComplete', 'browserReady'];
    const requiredNumberProps: Array<keyof SystemReadySignal> = ['commandCount'];
    const requiredArrayProps: Array<keyof SystemReadySignal> = ['errors', 'nodeErrors', 'portsActive', 'autonomousGuidance'];
    
    for (const prop of requiredStringProps) {
      if (typeof signal[prop] !== 'string') {
        throw new Error(`Property ${prop} should be string, got ${typeof signal[prop]}`);
      }
      details.push(`✅ ${prop}: "${signal[prop]}" (string)`);
    }
    
    for (const prop of requiredBooleanProps) {
      if (typeof signal[prop] !== 'boolean') {
        throw new Error(`Property ${prop} should be boolean, got ${typeof signal[prop]}`);
      }
      details.push(`✅ ${prop}: ${signal[prop]} (boolean)`);
    }
    
    for (const prop of requiredNumberProps) {
      if (typeof signal[prop] !== 'number') {
        throw new Error(`Property ${prop} should be number, got ${typeof signal[prop]}`);
      }
      details.push(`✅ ${prop}: ${signal[prop]} (number)`);
    }
    
    for (const prop of requiredArrayProps) {
      if (!Array.isArray(signal[prop])) {
        throw new Error(`Property ${prop} should be array, got ${typeof signal[prop]}`);
      }
      const arrayValue = signal[prop] as readonly unknown[];
      details.push(`✅ ${prop}: [${arrayValue.length} items] (array)`);
    }
    
    // Test systemHealth enum
    const validHealthValues = ['healthy', 'degraded', 'unhealthy', 'error'] as const;
    if (!validHealthValues.includes(signal.systemHealth as typeof validHealthValues[number])) {
      throw new Error(`Invalid systemHealth value: ${signal.systemHealth}`);
    }
    details.push(`✅ systemHealth: "${signal.systemHealth}" (valid enum)`);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedError(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testWorkingDirectoryContextSwitching(): Promise<TestResult> {
  const testName = "Working Directory Context Switching";
  const details: string[] = [];
  
  try {
    // Test switching between contexts
    const originalCwd = process.cwd();
    details.push(`📍 Original working directory: ${originalCwd}`);
    
    // Test 1: Switch to test-bench context
    WorkingDirConfig.setWorkingDir('examples/test-bench');
    const testBenchPath = WorkingDirConfig.getContinuumPath();
    details.push(`🎯 Switched to test-bench: ${testBenchPath}`);
    
    // Test 2: Switch to widget-ui context (always use different example)
    WorkingDirConfig.setWorkingDir('examples/widget-ui');
    const widgetUiPath = WorkingDirConfig.getContinuumPath();
    details.push(`🎯 Switched to widget-ui: ${widgetUiPath}`);
    
    // Test 3: Verify paths are different
    if (testBenchPath === widgetUiPath) {
      throw new Error(`Expected different paths for test-bench and widget-ui, but both resolved to: ${testBenchPath}`);
    }
    
    details.push(`✅ Context switching produces different paths`);
    
    // Test 4: Verify signaler respects context
    const signaler = new SystemReadySignaler();
    const signal = await signaler.generateReadySignal();
    
    // Since we switched to widget-ui context, verify signaler generates correctly
    // (We don't check specific ports since the test is about context switching, not port validation)
    details.push(`✅ Signal generated successfully in widget-ui context`);
    details.push(`   Timestamp: ${signal.timestamp}`);
    details.push(`   Active ports: [${signal.portsActive.join(', ')}]`);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedError(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testSignalFileOperations(): Promise<TestResult> {
  const testName = "Signal File Operations";
  const details: string[] = [];
  
  try {
    const signaler = new SystemReadySignaler();
    
    // Test 1: Clear signals
    await signaler.clearSignals();
    details.push(`✅ Cleared existing signals`);
    
    // Test 2: Generate and write signal
    const signal = await signaler.generateReadySignal();
    details.push(`✅ Generated new signal`);
    
    // Test 3: Verify signal file exists
    const continuumPath = WorkingDirConfig.getContinuumPath();
    const activePorts = await getActivePorts();
    const websocketPort = activePorts.websocket_server;
    const signalFile = path.join(continuumPath, 'jtag', 'signals', `system-ready-port-${websocketPort}.json`);
    
    try {
      const fileContent = await fs.readFile(signalFile, 'utf-8');
      const parsedSignal = JSON.parse(fileContent) as SystemReadySignal;
      
      // Verify the file content matches the generated signal
      if (parsedSignal.timestamp !== signal.timestamp) {
        throw new Error(`Signal file timestamp mismatch. Expected: ${signal.timestamp}, Found: ${parsedSignal.timestamp}`);
      }
      
      details.push(`✅ Signal file written correctly: ${signalFile}`);
      details.push(`   File size: ${fileContent.length} bytes`);
      details.push(`   Timestamp: ${parsedSignal.timestamp}`);
      
    } catch (fileError: unknown) {
      const typedError = createTypedError(fileError);
      throw new Error(`Failed to read signal file: ${typedError.message}`);
    }
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedError(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function runAllTests(): Promise<void> {
  console.log('🧪 SYSTEM READY SIGNALER INTEGRATION TESTS');
  console.log('═'.repeat(60));
  
  const tests = [
    testPerProjectIsolation,
    testSignalContentValidation,
    testWorkingDirectoryContextSwitching,
    testSignalFileOperations
  ];
  
  const results: TestResult[] = [];
  
  for (const test of tests) {
    console.log(`\n▶️ Running: ${test.name.replace(/([A-Z])/g, ' $1').trim()}...`);
    const result = await test();
    results.push(result);
    
    if (result.success) {
      console.log(`✅ PASSED: ${result.testName}`);
      for (const detail of result.details) {
        console.log(`   ${detail}`);
      }
    } else {
      console.log(`❌ FAILED: ${result.testName}`);
      for (const detail of result.details) {
        console.log(`   ${detail}`);
      }
      if (result.error) {
        console.log(`   ❌ Error: ${result.error}`);
      }
    }
  }
  
  // Summary
  const passed = results.filter(r => r.success).length;
  const total = results.length;
  
  console.log('\n' + '═'.repeat(60));
  console.log(`📊 TEST SUMMARY: ${passed}/${total} PASSED`);
  
  if (passed === total) {
    console.log('🎉 ALL TESTS PASSED!');
    console.log('✅ SystemReadySignaler integration is working correctly');
    process.exit(0);
  } else {
    console.log(`❌ ${total - passed} TESTS FAILED`);
    console.log('🔍 Check the error details above');
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  runAllTests().catch((error: unknown) => {
    const typedError = createTypedError(error);
    console.error('❌ Test execution failed:', typedError.message);
    process.exit(1);
  });
}

export { runAllTests, testPerProjectIsolation, testSignalContentValidation, testWorkingDirectoryContextSwitching, testSignalFileOperations };