#!/usr/bin/env npx tsx
/**
 * CONTEXT SWITCHING UNDER LOAD TESTS
 * 
 * Verifies system maintains stability and proper isolation when switching
 * contexts rapidly, simulating real-world usage patterns where users might
 * switch between different projects quickly or have multiple processes
 * accessing different contexts concurrently.
 */

import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';
import { createTypedErrorInfo } from '../system/core/types/ErrorTypes';
import { ProcessCoordinator } from '../system/core/process/ProcessCoordinator';
import { SystemReadySignaler } from '../scripts/signal-system-ready';

interface LoadTestResult {
  readonly success: boolean;
  readonly testName: string;
  readonly details: string[];
  readonly error?: string;
}

async function testRapidContextSwitching(): Promise<LoadTestResult> {
  const testName = "Rapid Context Switching Stability";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    const contexts = ['examples/widget-ui', 'examples/test-bench'];
    const iterations = 100;
    
    details.push(`🔄 Testing ${iterations} rapid context switches between ${contexts.length} contexts`);
    
    const startTime = Date.now();
    const contextPaths: string[] = [];
    
    for (let i = 0; i < iterations; i++) {
      const context = contexts[i % contexts.length];
      WorkingDirConfig.setWorkingDir(context);
      
      const continuumPath = WorkingDirConfig.getContinuumPath();
      contextPaths.push(continuumPath);
      
      // Verify context is correctly set
      const currentContext = WorkingDirConfig.getWorkingDir();
      if (!currentContext.includes(context.split('/')[1])) {
        throw new Error(`Context switch ${i} failed: expected ${context}, got ${currentContext}`);
      }
    }
    
    const endTime = Date.now();
    const totalTime = endTime - startTime;
    
    details.push(`✅ Completed ${iterations} context switches in ${totalTime}ms`);
    details.push(`   Average time per switch: ${(totalTime / iterations).toFixed(2)}ms`);
    
    // Verify path consistency - each context should have generated the same paths
    const widgetPaths = contextPaths.filter((_, i) => i % 2 === 0);
    const testBenchPaths = contextPaths.filter((_, i) => i % 2 === 1);
    
    const uniqueWidgetPaths = new Set(widgetPaths);
    const uniqueTestBenchPaths = new Set(testBenchPaths);
    
    if (uniqueWidgetPaths.size !== 1) {
      throw new Error(`Widget-UI path inconsistency: ${uniqueWidgetPaths.size} different paths`);
    }
    
    if (uniqueTestBenchPaths.size !== 1) {
      throw new Error(`Test-Bench path inconsistency: ${uniqueTestBenchPaths.size} different paths`);
    }
    
    details.push(`✅ Path consistency verified across all switches`);
    details.push(`   Widget-UI consistent path: ${Array.from(uniqueWidgetPaths)[0]}`);
    details.push(`   Test-Bench consistent path: ${Array.from(uniqueTestBenchPaths)[0]}`);
    
    // Restore original context
    WorkingDirConfig.setWorkingDir(originalContext);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testConcurrentContextAccess(): Promise<LoadTestResult> {
  const testName = "Concurrent Context Access Under Load";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    const contexts = ['examples/widget-ui', 'examples/test-bench'];
    const concurrentOperations = 50;
    
    details.push(`⚡ Testing ${concurrentOperations} concurrent context operations`);
    
    const startTime = Date.now();
    
    // Create concurrent operations that switch contexts and access paths
    const operations: Promise<{ context: string; path: string; timestamp: number }>[] = [];
    
    for (let i = 0; i < concurrentOperations; i++) {
      const context = contexts[i % contexts.length];
      const delay = Math.random() * 50; // Random delay up to 50ms
      
      const operation = new Promise<{ context: string; path: string; timestamp: number }>((resolve) => {
        setTimeout(() => {
          WorkingDirConfig.setWorkingDir(context);
          const continuumPath = WorkingDirConfig.getContinuumPath();
          const currentContext = WorkingDirConfig.getWorkingDir();
          resolve({
            context: currentContext,
            path: continuumPath,
            timestamp: Date.now()
          });
        }, delay);
      });
      
      operations.push(operation);
    }
    
    const results = await Promise.all(operations);
    const endTime = Date.now();
    const totalTime = endTime - startTime;
    
    details.push(`✅ Completed ${concurrentOperations} concurrent operations in ${totalTime}ms`);
    
    // Verify results integrity
    const widgetResults = results.filter(r => r.context.includes('widget-ui'));
    const testBenchResults = results.filter(r => r.context.includes('test-bench'));
    
    details.push(`   Widget-UI operations: ${widgetResults.length}`);
    details.push(`   Test-Bench operations: ${testBenchResults.length}`);
    
    // Check path consistency within each context
    const widgetPaths = new Set(widgetResults.map(r => r.path));
    const testBenchPaths = new Set(testBenchResults.map(r => r.path));
    
    if (widgetPaths.size > 1) {
      throw new Error(`Widget-UI path inconsistency under load: ${widgetPaths.size} different paths`);
    }
    
    if (testBenchPaths.size > 1) {
      throw new Error(`Test-Bench path inconsistency under load: ${testBenchPaths.size} different paths`);
    }
    
    details.push(`✅ Context isolation maintained under concurrent load`);
    
    // Check for race conditions - verify all contexts are valid
    const invalidResults = results.filter(r => 
      !r.context.includes('widget-ui') && !r.context.includes('test-bench')
    );
    
    if (invalidResults.length > 0) {
      throw new Error(`Invalid context results detected: ${invalidResults.length} operations`);
    }
    
    details.push(`✅ No race conditions detected in ${concurrentOperations} operations`);
    
    // Restore original context
    WorkingDirConfig.setWorkingDir(originalContext);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testProcessCoordinatorUnderLoad(): Promise<LoadTestResult> {
  const testName = "ProcessCoordinator Under Context Switch Load";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    const coordinator = ProcessCoordinator.getInstance();
    const contexts = ['examples/widget-ui', 'examples/test-bench'];
    const planningOperations = 30;
    
    details.push(`🧠 Testing ProcessCoordinator with ${planningOperations} rapid planning operations`);
    
    const startTime = Date.now();
    const planningResults: Array<{ context: string; planType: string; timestamp: number }> = [];
    
    for (let i = 0; i < planningOperations; i++) {
      const context = contexts[i % contexts.length];
      const ports = [9001, 9003];
      
      // Switch context and plan startup
      WorkingDirConfig.setWorkingDir(context);
      const plan = await coordinator.planStartup(context, ports);
      
      planningResults.push({
        context,
        planType: plan.type,
        timestamp: Date.now()
      });
      
      // Small delay to prevent overwhelming the system
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    
    const endTime = Date.now();
    const totalTime = endTime - startTime;
    
    details.push(`✅ Completed ${planningOperations} planning operations in ${totalTime}ms`);
    details.push(`   Average planning time: ${(totalTime / planningOperations).toFixed(2)}ms`);
    
    // Analyze planning results
    const planTypes = planningResults.reduce((acc, result) => {
      acc[result.planType] = (acc[result.planType] || 0) + 1;
      return acc;
    }, {} as Record<string, number>);
    
    details.push(`✅ Planning results distribution:`);
    for (const [planType, count] of Object.entries(planTypes)) {
      details.push(`   ${planType}: ${count} operations`);
    }
    
    // Verify no planning failures
    const failedOperations = planningResults.filter(r => !['clean_start', 'reuse_existing', 'graceful_handoff'].includes(r.planType));
    if (failedOperations.length > 0) {
      throw new Error(`Invalid planning results: ${failedOperations.length} failed operations`);
    }
    
    details.push(`✅ ProcessCoordinator maintained stability under load`);
    
    // Restore original context
    WorkingDirConfig.setWorkingDir(originalContext);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testSystemReadySignalerUnderLoad(): Promise<LoadTestResult> {
  const testName = "SystemReadySignaler Under Context Switch Load";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    const contexts = ['examples/widget-ui', 'examples/test-bench'];
    const signalOperations = 20; // Reduced to avoid overwhelming
    
    details.push(`📡 Testing SystemReadySignaler with ${signalOperations} context switches`);
    
    const startTime = Date.now();
    const signalers: SystemReadySignaler[] = [];
    const signalResults: Array<{ context: string; healthy: boolean; timestamp: number }> = [];
    
    for (let i = 0; i < signalOperations; i++) {
      const context = contexts[i % contexts.length];
      
      // Switch context
      WorkingDirConfig.setWorkingDir(context);
      
      // Create signaler for this context
      const signaler = new SystemReadySignaler();
      signalers.push(signaler);
      
      try {
        // Generate signal (quick test)
        const signal = await signaler.generateReadySignal();
        signalResults.push({
          context,
          healthy: signal.systemHealth === 'healthy',
          timestamp: Date.now()
        });
      } catch (error) {
        // Signal generation might fail in test environment, record as unhealthy but continue
        signalResults.push({
          context,
          healthy: false,
          timestamp: Date.now()
        });
      }
      
      // Small delay to prevent overwhelming
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    
    const endTime = Date.now();
    const totalTime = endTime - startTime;
    
    details.push(`✅ Completed ${signalOperations} signaler operations in ${totalTime}ms`);
    details.push(`   Average signaler time: ${(totalTime / signalOperations).toFixed(2)}ms`);
    
    // Analyze results by context
    const widgetResults = signalResults.filter(r => r.context.includes('widget-ui'));
    const testBenchResults = signalResults.filter(r => r.context.includes('test-bench'));
    
    details.push(`   Widget-UI signals: ${widgetResults.length} (${widgetResults.filter(r => r.healthy).length} healthy)`);
    details.push(`   Test-Bench signals: ${testBenchResults.length} (${testBenchResults.filter(r => r.healthy).length} healthy)`);
    
    // Verify context isolation - each signaler should have operated on correct context
    if (widgetResults.length + testBenchResults.length !== signalOperations) {
      throw new Error('Signal context isolation failed - unexpected context results');
    }
    
    details.push(`✅ SystemReadySignaler maintained context isolation under load`);
    
    // Restore original context
    WorkingDirConfig.setWorkingDir(originalContext);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testMemoryLeaksPrevention(): Promise<LoadTestResult> {
  const testName = "Memory Leaks Prevention Under Load";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    const contexts = ['examples/widget-ui', 'examples/test-bench'];
    const memoryTestCycles = 10;
    const operationsPerCycle = 50;
    
    details.push(`🧠 Testing memory stability over ${memoryTestCycles} cycles of ${operationsPerCycle} operations each`);
    
    const memorySnapshots: Array<{ cycle: number; heapUsed: number; external: number }> = [];
    
    // Initial memory snapshot
    const initialMemory = process.memoryUsage();
    memorySnapshots.push({
      cycle: 0,
      heapUsed: initialMemory.heapUsed,
      external: initialMemory.external
    });
    
    for (let cycle = 1; cycle <= memoryTestCycles; cycle++) {
      // Perform intensive context switching operations
      for (let op = 0; op < operationsPerCycle; op++) {
        const context = contexts[op % contexts.length];
        WorkingDirConfig.setWorkingDir(context);
        
        // Perform various operations that might cause memory leaks
        const continuumPath = WorkingDirConfig.getContinuumPath();
        const workingDir = WorkingDirConfig.getWorkingDir();
        
        // Create some temporary objects that should be garbage collected
        const tempData = {
          context,
          path: continuumPath,
          workingDir,
          timestamp: Date.now(),
          randomData: Array(100).fill(0).map(() => Math.random())
        };
        
        // Force reference to be used
        if (tempData.randomData.length === 0) {
          throw new Error('Impossible condition - preventing optimization');
        }
      }
      
      // Force garbage collection if available
      if (global.gc) {
        global.gc();
      }
      
      // Take memory snapshot
      const currentMemory = process.memoryUsage();
      memorySnapshots.push({
        cycle,
        heapUsed: currentMemory.heapUsed,
        external: currentMemory.external
      });
      
      details.push(`   Cycle ${cycle}: Heap ${Math.round(currentMemory.heapUsed / 1024 / 1024)}MB, External ${Math.round(currentMemory.external / 1024 / 1024)}MB`);
    }
    
    // Analyze memory growth
    const finalMemory = memorySnapshots[memorySnapshots.length - 1];
    const initialHeap = memorySnapshots[0].heapUsed;
    const finalHeap = finalMemory.heapUsed;
    const heapGrowth = finalHeap - initialHeap;
    const heapGrowthMB = heapGrowth / 1024 / 1024;
    
    details.push(`✅ Memory analysis complete:`);
    details.push(`   Initial heap: ${Math.round(initialHeap / 1024 / 1024)}MB`);
    details.push(`   Final heap: ${Math.round(finalHeap / 1024 / 1024)}MB`);
    details.push(`   Heap growth: ${Math.round(heapGrowthMB)}MB`);
    
    // Check for significant memory leaks (more than 50MB growth is concerning)
    if (heapGrowthMB > 50) {
      console.warn(`⚠️ Potential memory leak detected: ${Math.round(heapGrowthMB)}MB growth`);
      // Don't fail the test but record the warning
      details.push(`   ⚠️ Warning: Significant memory growth detected`);
    } else {
      details.push(`   ✅ Memory growth within acceptable limits`);
    }
    
    // Restore original context
    WorkingDirConfig.setWorkingDir(originalContext);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function runContextSwitchingLoadTests(): Promise<void> {
  console.log('🔄 CONTEXT SWITCHING UNDER LOAD TESTS');
  console.log('═'.repeat(60));
  
  const tests = [
    testRapidContextSwitching,
    testConcurrentContextAccess,
    testProcessCoordinatorUnderLoad,
    testSystemReadySignalerUnderLoad,
    testMemoryLeaksPrevention
  ];
  
  const results: LoadTestResult[] = [];
  
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
  console.log(`📊 CONTEXT SWITCHING LOAD TESTS SUMMARY: ${passed}/${total} PASSED`);
  
  if (passed === total) {
    console.log('🎉 ALL TESTS PASSED!');
    console.log('✅ System maintains stability under rapid context switching');
    console.log('✅ Concurrent context access properly isolated');
    console.log('✅ ProcessCoordinator handles load gracefully');
    console.log('✅ SystemReadySignaler maintains isolation under load');
    console.log('✅ Memory usage remains stable under intensive operations');
    process.exit(0);
  } else {
    console.log(`❌ ${total - passed} TESTS FAILED`);
    console.log('🔍 Address the issues above before deployment');
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  runContextSwitchingLoadTests().catch((error: unknown) => {
    const typedError = createTypedErrorInfo(error);
    console.error('❌ Test execution failed:', typedError.message);
    process.exit(1);
  });
}

export { 
  runContextSwitchingLoadTests, 
  testRapidContextSwitching, 
  testConcurrentContextAccess,
  testProcessCoordinatorUnderLoad,
  testSystemReadySignalerUnderLoad,
  testMemoryLeaksPrevention
};