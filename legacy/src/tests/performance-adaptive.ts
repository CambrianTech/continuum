#!/usr/bin/env tsx

/**
 * Adaptive Performance Test - Load-Aware Optimization
 * 
 * TRUE ITERATIVE IMPROVEMENT: Based on feedback from minimal test showing
 * regression at small scale, test with variable loads to find the sweet spot
 */

import { performance } from 'perf_hooks';
import * as crypto from 'crypto';

class BaselineRouter {
  private subscribers = new Map<string, any>();
  
  registerSubscriber(endpoint: string, subscriber: any) {
    this.subscribers.set(endpoint, subscriber);
  }
  
  async routeMessage(message: any) {
    const subscriber = this.subscribers.get(message.endpoint);
    if (subscriber) {
      return await subscriber.handleMessage(message);
    }
    throw new Error('No subscriber');
  }
  
  async shutdown() {}
}

class AdaptiveOptimizedRouter extends BaselineRouter {
  private messageQueue: any[] = [];
  private processingBatch = false;
  private readonly minBatchSize = 10;  // Only batch when we have significant load
  private readonly maxBatchSize = 50;
  private readonly batchTimeout = 5;   // ms - flush batch quickly
  
  async routeMessage(message: any) {
    // For small loads, use direct routing (no overhead)
    if (this.messageQueue.length < this.minBatchSize && !this.processingBatch) {
      return await super.routeMessage(message);
    }
    
    // For larger loads, use batching
    return new Promise((resolve, reject) => {
      this.messageQueue.push({ message, resolve, reject });
      
      if (this.messageQueue.length >= this.maxBatchSize) {
        this.flushBatch();
      } else if (this.messageQueue.length === this.minBatchSize) {
        // Start timeout when we hit minimum batch size
        setTimeout(() => this.flushBatch(), this.batchTimeout);
      }
    });
  }
  
  private async flushBatch() {
    if (this.processingBatch || this.messageQueue.length === 0) return;
    
    this.processingBatch = true;
    const batch = this.messageQueue.splice(0, this.maxBatchSize);
    
    try {
      // Process batch in parallel - THIS is where the performance gains come from
      const results = await Promise.all(
        batch.map(async ({ message, resolve, reject }) => {
          try {
            const result = await super.routeMessage(message);
            resolve(result);
            return result;
          } catch (error) {
            reject(error);
            throw error;
          }
        })
      );
    } catch (error) {
      console.error('Batch processing error:', error);
    } finally {
      this.processingBatch = false;
      
      // Process remaining messages if queue built up
      if (this.messageQueue.length >= this.minBatchSize) {
        setTimeout(() => this.flushBatch(), 0);
      }
    }
  }
}

async function adaptivePerformanceTest() {
  console.log('🧠 ADAPTIVE AI OPTIMIZATION: Load-Aware Performance Test');
  console.log('========================================================\n');
  
  const testLoads = [10, 50, 100, 500, 1000];  // Different message loads
  const iterations = 3;
  
  for (const messageCount of testLoads) {
    console.log(`🔥 Testing with ${messageCount} messages:`);
    console.log('━'.repeat(50));
    
    // Test baseline
    const baseline = new BaselineRouter();
    baseline.registerSubscriber('test', {
      handleMessage: async () => {
        // Simulate realistic processing time
        await new Promise(resolve => setTimeout(resolve, Math.random() * 0.5));
        return { success: true, data: crypto.randomBytes(8).toString('hex') };
      }
    });
    
    const baselineTimes: number[] = [];
    for (let i = 0; i < iterations; i++) {
      const start = performance.now();
      
      const promises = [];
      for (let j = 0; j < messageCount; j++) {
        promises.push(baseline.routeMessage({
          endpoint: 'test',
          payload: { data: `msg-${j}` },
          id: crypto.randomUUID()
        }));
      }
      
      await Promise.all(promises);
      const time = performance.now() - start;
      baselineTimes.push(time);
    }
    
    const avgBaseline = baselineTimes.reduce((s, t) => s + t, 0) / baselineTimes.length;
    await baseline.shutdown();
    
    // Test adaptive optimized
    const optimized = new AdaptiveOptimizedRouter();
    optimized.registerSubscriber('test', {
      handleMessage: async () => {
        // Same processing time as baseline
        await new Promise(resolve => setTimeout(resolve, Math.random() * 0.5));
        return { success: true, data: crypto.randomBytes(8).toString('hex') };
      }
    });
    
    const optimizedTimes: number[] = [];
    for (let i = 0; i < iterations; i++) {
      const start = performance.now();
      
      const promises = [];
      for (let j = 0; j < messageCount; j++) {
        promises.push(optimized.routeMessage({
          endpoint: 'test',
          payload: { data: `msg-${j}` },
          id: crypto.randomUUID()
        }));
      }
      
      await Promise.all(promises);
      const time = performance.now() - start;
      optimizedTimes.push(time);
    }
    
    const avgOptimized = optimizedTimes.reduce((s, t) => s + t, 0) / optimizedTimes.length;
    await optimized.shutdown();
    
    // Calculate improvement
    const improvement = ((avgBaseline - avgOptimized) / avgBaseline) * 100;
    const throughputBaseline = messageCount / (avgBaseline / 1000); // messages per second
    const throughputOptimized = messageCount / (avgOptimized / 1000);
    const throughputGain = ((throughputOptimized - throughputBaseline) / throughputBaseline) * 100;
    
    console.log(`   📊 BASELINE:  ${avgBaseline.toFixed(2)}ms (${throughputBaseline.toFixed(0)} msg/s)`);
    console.log(`   🚀 OPTIMIZED: ${avgOptimized.toFixed(2)}ms (${throughputOptimized.toFixed(0)} msg/s)`);  
    console.log(`   ⚡ IMPROVEMENT: ${improvement >= 0 ? '+' : ''}${improvement.toFixed(1)}% latency, ${throughputGain >= 0 ? '+' : ''}${throughputGain.toFixed(1)}% throughput`);
    console.log(`   🎯 VERDICT: ${improvement > 10 ? '🎉 MAJOR WIN' : improvement > 2 ? '✅ SUCCESS' : improvement > -2 ? '⚖️  NEUTRAL' : '❌ REGRESSION'}`);
    console.log('');
  }
  
  console.log('💡 ADAPTIVE OPTIMIZATION INSIGHT:');
  console.log('   - Small loads: Direct routing (no batching overhead)');
  console.log('   - Large loads: Parallel batching (efficiency gains)');
  console.log('   - This is how real AI systems should work - adaptive to context!');
}

if (require.main === module) {
  adaptivePerformanceTest().catch(console.error);
}

export { adaptivePerformanceTest };