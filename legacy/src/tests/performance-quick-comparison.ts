#!/usr/bin/env tsx

/**
 * Quick Performance Comparison Test
 * 
 * Fast iteration cycle for testing AI optimizations - this is exactly 
 * what autonomous improvement with proper feedback looks like!
 */

import { performance } from 'perf_hooks';
import { JTAGRouter } from '../system/core/router/shared/JTAGRouter';
import { JTAGRouterOptimized } from '../system/core/router/shared/JTAGRouterOptimized';
import { JTAGContext } from '../system/types/JTAGTypes';
import * as crypto from 'crypto';

async function quickPerformanceComparison() {
  console.log('🔥 QUICK AI OPTIMIZATION VALIDATION');
  console.log('====================================');
  
  // Test configuration - smaller for fast iteration
  const testMessages = 50;
  const iterations = 3;
  
  const context: JTAGContext = {
    uuid: crypto.randomUUID() as any,
    environment: 'server', 
    timestamp: new Date().toISOString()
  };
  
  const config = {
    transport: {
      preferred: 'websocket' as any,
      role: 'server' as any,
      serverPort: 9998,
      enableP2P: false,
      fallback: 'http' as any
    },
    queue: {
      maxSize: 1000,
      enableDeduplication: false, // Disable for clean performance testing
      flushInterval: 50
    },
    enableLogging: false
  };
  
  console.log('\n📊 Testing BASELINE JTAGRouter...');
  const baselineRouter = new JTAGRouter(context, config);
  const baselineTime = await measureRouterPerformance(baselineRouter, testMessages, iterations);
  await baselineRouter.shutdown();
  
  console.log('\n🚀 Testing OPTIMIZED JTAGRouter...');
  const optimizedRouter = new JTAGRouterOptimized(context, config);
  const optimizedTime = await measureRouterPerformance(optimizedRouter, testMessages, iterations);
  
  // Get optimization stats
  const optimizationStats = optimizedRouter.getOptimizationStats();
  console.log(`📊 Optimization Stats:`);
  console.log(`   Batches processed: ${optimizationStats.totalBatches}`);
  console.log(`   Average batch size: ${optimizationStats.averageBatchSize.toFixed(1)}`);
  console.log(`   Success rate: ${(optimizationStats.successRate * 100).toFixed(1)}%`);
  
  await optimizedRouter.shutdown();
  
  // Calculate improvement
  const improvement = ((baselineTime - optimizedTime) / baselineTime) * 100;
  
  console.log('\n✨ RESULTS:');
  console.log(`   Baseline: ${baselineTime.toFixed(2)}ms`);
  console.log(`   Optimized: ${optimizedTime.toFixed(2)}ms`);
  console.log(`   Improvement: ${improvement >= 0 ? '+' : ''}${improvement.toFixed(1)}%`);
  console.log(`   Status: ${improvement > 10 ? '🎉 SUCCESS' : improvement > 0 ? '✅ IMPROVEMENT' : '⚠️  NEEDS TUNING'}`);
  
  return { baseline: baselineTime, optimized: optimizedTime, improvement };
}

async function measureRouterPerformance(router: any, messages: number, iterations: number): Promise<number> {
  // Register test subscriber
  const testSubscriber = {
    handleMessage: async () => ({ success: true, timestamp: new Date().toISOString() }),
    endpoint: 'test-daemon',
    uuid: crypto.randomUUID()
  };
  router.registerSubscriber('test-daemon', testSubscriber);
  
  const times: number[] = [];
  
  for (let i = 0; i < iterations; i++) {
    const start = performance.now();
    
    // Test simple message routing (avoid complex transport setup)  
    for (let j = 0; j < messages; j++) {
      const message = {
        messageType: 'request',
        origin: 'performance-test',
        endpoint: 'test-daemon',
        payload: { testData: `Message ${j}` },
        timestamp: new Date().toISOString(),
        correlationId: crypto.randomUUID()
      };
      
      try {
        // Test internal routing performance
        await (router as any).routeLocally(message);
      } catch (error) {
        // Ignore routing errors for performance testing
      }
    }
    
    const end = performance.now();
    times.push(end - start);
    
    console.log(`   Iteration ${i + 1}: ${(end - start).toFixed(2)}ms`);
  }
  
  // Return average time
  return times.reduce((sum, time) => sum + time, 0) / times.length;
}

// Run test
if (require.main === module) {
  quickPerformanceComparison().catch(console.error);
}

export { quickPerformanceComparison };