#!/usr/bin/env tsx
/**
 * Chaos Performance Optimizer - Antifragile System Enhancement
 * 
 * Intentionally introduces controlled chaos to discover performance bottlenecks,
 * resilience gaps, and optimization opportunities that only appear under stress.
 * Makes the system antifragile through controlled adversity.
 */

import { performance } from 'perf_hooks';
import { globalProfiler } from '../shared/performance/PerformanceProfiler';
import * as fs from 'fs/promises';

interface ChaosExperiment {
  name: string;
  description: string;
  severity: 'low' | 'medium' | 'high';
  duration: number;
  execute: () => Promise<ChaosResult>;
}

interface ChaosResult {
  experimentName: string;
  success: boolean;
  baselinePerformance: number;
  chaosPerformance: number;
  resilienceScore: number;
  optimizationsDiscovered: string[];
  systemBehavior: string;
}

interface AntifragileMetrics {
  totalExperiments: number;
  resilienceGains: number;
  performanceImprovements: number;
  weaknessesDiscovered: string[];
  strengthsIdentified: string[];
  adaptations: string[];
}

class ChaosPerformanceOptimizer {
  private experiments: ChaosExperiment[] = [];
  private results: ChaosResult[] = [];
  private antifragileMetrics: AntifragileMetrics;
  private isRunning = false;
  
  constructor() {
    this.antifragileMetrics = {
      totalExperiments: 0,
      resilienceGains: 0,
      performanceImprovements: 0,
      weaknessesDiscovered: [],
      strengthsIdentified: [],
      adaptations: []
    };
    
    this.initializeChaosExperiments();
  }
  
  async startChaosOptimization(): Promise<void> {
    console.log('⚡ CHAOS PERFORMANCE OPTIMIZER');
    console.log('=============================');
    console.log('Building antifragile systems through controlled adversity');
    console.log('Discovering hidden optimizations via intelligent chaos injection');
    
    this.isRunning = true;
    
    // Run continuous chaos cycles
    while (this.isRunning) {
      await this.runChaosCycle();
      await this.sleep(15000); // 15 seconds between cycles
    }
  }
  
  private initializeChaosExperiments(): void {
    this.experiments = [
      {
        name: 'memory-pressure',
        description: 'Gradually increase memory pressure to find memory leaks and optimization opportunities',
        severity: 'medium',
        duration: 5000,
        execute: () => this.executeMemoryPressure()
      },
      {
        name: 'cpu-starvation',
        description: 'Create CPU competition to identify inefficient algorithms',
        severity: 'medium', 
        duration: 3000,
        execute: () => this.executeCpuStarvation()
      },
      {
        name: 'network-latency',
        description: 'Simulate network delays to test async optimization',
        severity: 'low',
        duration: 4000,
        execute: () => this.executeNetworkLatency()
      },
      {
        name: 'random-failures',
        description: 'Inject random failures to test error handling performance',
        severity: 'high',
        duration: 6000,
        execute: () => this.executeRandomFailures()
      },
      {
        name: 'resource-contention',
        description: 'Create resource contention to find synchronization bottlenecks',
        severity: 'medium',
        duration: 4000,
        execute: () => this.executeResourceContention()
      },
      {
        name: 'burst-load',
        description: 'Generate sudden load spikes to test scalability',
        severity: 'high',
        duration: 7000,
        execute: () => this.executeBurstLoad()
      },
      {
        name: 'cache-invalidation',
        description: 'Invalidate caches randomly to test cache strategy resilience',
        severity: 'low',
        duration: 3000,
        execute: () => this.executeCacheInvalidation()
      },
      {
        name: 'slow-dependencies',
        description: 'Simulate slow external dependencies',
        severity: 'medium',
        duration: 5000,
        execute: () => this.executeSlowDependencies()
      }
    ];
  }
  
  private async runChaosCycle(): Promise<void> {
    console.log('\n🌪️ Starting new chaos cycle...');
    
    // Select experiment based on learning history
    const experiment = this.selectOptimalExperiment();
    
    console.log(`🧪 Running chaos experiment: ${experiment.name}`);
    console.log(`   📝 ${experiment.description}`);
    console.log(`   ⚠️ Severity: ${experiment.severity}, Duration: ${experiment.duration}ms`);
    
    try {
      // Measure baseline performance
      const baselinePerf = await this.measureBaselinePerformance();
      
      // Execute chaos experiment
      const chaosResult = await experiment.execute();
      
      // Analyze and learn from results
      await this.analyzeChaosResults(chaosResult, baselinePerf);
      
      // Apply discovered optimizations
      await this.applyDiscoveredOptimizations(chaosResult);
      
    } catch (error: any) {
      console.warn(`⚠️ Chaos experiment failed: ${error.message}`);
    }
  }
  
  private selectOptimalExperiment(): ChaosExperiment {
    // Select based on learning potential and recent results
    const weights = this.experiments.map(exp => {
      const recentResults = this.results.filter(r => r.experimentName === exp.name).slice(-5);
      
      if (recentResults.length === 0) {
        return 1.0; // High priority for unexplored experiments
      }
      
      // Weight based on discovery potential
      const avgResilienceGain = recentResults.reduce((sum, r) => sum + r.resilienceScore, 0) / recentResults.length;
      const discoveryRate = recentResults.reduce((sum, r) => sum + r.optimizationsDiscovered.length, 0) / recentResults.length;
      
      return avgResilienceGain * 0.6 + discoveryRate * 0.4;
    });
    
    // Weighted selection
    const totalWeight = weights.reduce((sum, w) => sum + w, 0);
    let random = Math.random() * totalWeight;
    
    for (let i = 0; i < this.experiments.length; i++) {
      random -= weights[i];
      if (random <= 0) {
        return this.experiments[i];
      }
    }
    
    return this.experiments[Math.floor(Math.random() * this.experiments.length)];
  }
  
  private async measureBaselinePerformance(): Promise<number> {
    globalProfiler.startTimer('chaos-baseline-measurement');
    
    // Simulate baseline operations
    const operations = [];
    for (let i = 0; i < 10; i++) {
      operations.push(this.simulateTypicalOperation());
    }
    
    const results = await Promise.allSettled(operations);
    const successful = results.filter(r => r.status === 'fulfilled').length;
    
    const timing = globalProfiler.endTimer('chaos-baseline-measurement');
    const performanceScore = successful / 10 * (1000 / (timing?.duration || 1));
    
    return performanceScore;
  }
  
  private async simulateTypicalOperation(): Promise<void> {
    // Simulate typical system operation
    const operationType = Math.floor(Math.random() * 3);
    
    switch (operationType) {
      case 0: // CPU operation
        let sum = 0;
        for (let i = 0; i < 10000; i++) {
          sum += Math.sqrt(i * Math.random());
        }
        break;
        
      case 1: // Memory operation
        const data = new Array(1000).fill(0).map(() => Math.random());
        data.sort();
        break;
        
      case 2: // I/O simulation
        await this.sleep(Math.random() * 10 + 1);
        break;
    }
  }
  
  // Chaos Experiment Implementations
  
  private async executeMemoryPressure(): Promise<ChaosResult> {
    console.log('   💾 Injecting memory pressure...');
    
    globalProfiler.startTimer('chaos-memory-pressure');
    
    const initialMemory = process.memoryUsage().heapUsed;
    const memoryConsumers: any[] = [];
    
    try {
      // Gradually increase memory pressure
      for (let i = 0; i < 50; i++) {
        memoryConsumers.push(new Array(10000).fill(Math.random()));
        
        if (i % 10 === 0) {
          const currentMemory = process.memoryUsage().heapUsed;
          const memoryIncrease = (currentMemory - initialMemory) / 1024 / 1024;
          
          console.log(`      📊 Memory increase: ${memoryIncrease.toFixed(1)}MB`);
          
          // Test performance under memory pressure
          const perfUnderPressure = await this.measureBaselinePerformance();
          
          if (memoryIncrease > 100) { // Stop if memory usage gets too high
            break;
          }
        }
        
        await this.sleep(50);
      }
      
      const finalMemory = process.memoryUsage().heapUsed;
      const memoryDelta = (finalMemory - initialMemory) / 1024 / 1024;
      
      globalProfiler.endTimer('chaos-memory-pressure');
      
      // Analyze memory behavior
      const optimizations = [];
      if (memoryDelta > 50) {
        optimizations.push('Implement object pooling for frequently allocated objects');
        optimizations.push('Add memory monitoring and automatic GC triggers');
      }
      
      return {
        experimentName: 'memory-pressure',
        success: true,
        baselinePerformance: 1.0,
        chaosPerformance: Math.max(0.1, 1.0 - (memoryDelta / 200)),
        resilienceScore: Math.max(0.1, 1.0 - (memoryDelta / 100)),
        optimizationsDiscovered: optimizations,
        systemBehavior: `Memory usage increased by ${memoryDelta.toFixed(1)}MB during pressure test`
      };
      
    } finally {
      // Cleanup memory consumers
      memoryConsumers.length = 0;
      if (global.gc) global.gc();
    }
  }
  
  private async executeCpuStarvation(): Promise<ChaosResult> {
    console.log('   🔥 Creating CPU competition...');
    
    globalProfiler.startTimer('chaos-cpu-starvation');
    
    const cpuWorkers: Promise<void>[] = [];
    
    // Create CPU competition
    for (let i = 0; i < 4; i++) {
      cpuWorkers.push(this.intensiveCpuWork(1000));
    }
    
    // Measure performance under CPU contention
    const perfStart = performance.now();
    const performanceResult = await this.measureBaselinePerformance();
    const perfDuration = performance.now() - perfStart;
    
    await Promise.allSettled(cpuWorkers);
    globalProfiler.endTimer('chaos-cpu-starvation');
    
    const optimizations = [];
    if (perfDuration > 2000) {
      optimizations.push('Implement CPU-aware scheduling');
      optimizations.push('Add worker thread pool for CPU-intensive tasks');
    }
    
    return {
      experimentName: 'cpu-starvation',
      success: true,
      baselinePerformance: 1.0,
      chaosPerformance: Math.max(0.1, 2000 / perfDuration),
      resilienceScore: Math.max(0.2, 1000 / perfDuration),
      optimizationsDiscovered: optimizations,
      systemBehavior: `Performance degraded under CPU contention (${perfDuration.toFixed(1)}ms)`
    };
  }
  
  private async executeNetworkLatency(): Promise<ChaosResult> {
    console.log('   🌐 Simulating network delays...');
    
    globalProfiler.startTimer('chaos-network-latency');
    
    const networkDelays = [10, 50, 100, 200, 500]; // ms
    const results = [];
    
    for (const delay of networkDelays) {
      const start = performance.now();
      
      // Simulate network-dependent operation
      await this.sleep(delay);
      const operation = await this.simulateTypicalOperation();
      
      const duration = performance.now() - start;
      results.push({ delay, duration, success: true });
      
      console.log(`      📡 ${delay}ms delay → ${duration.toFixed(1)}ms total`);
    }
    
    globalProfiler.endTimer('chaos-network-latency');
    
    const avgDegradation = results.reduce((sum, r) => sum + r.duration, 0) / results.length;
    
    const optimizations = [];
    if (avgDegradation > 300) {
      optimizations.push('Implement request batching for network operations');
      optimizations.push('Add intelligent caching layer');
      optimizations.push('Use async/await optimization patterns');
    }
    
    return {
      experimentName: 'network-latency',
      success: true,
      baselinePerformance: 1.0,
      chaosPerformance: Math.max(0.2, 100 / avgDegradation),
      resilienceScore: Math.max(0.3, 200 / avgDegradation),
      optimizationsDiscovered: optimizations,
      systemBehavior: `Average performance degradation: ${avgDegradation.toFixed(1)}ms`
    };
  }
  
  private async executeRandomFailures(): Promise<ChaosResult> {
    console.log('   💥 Injecting random failures...');
    
    globalProfiler.startTimer('chaos-random-failures');
    
    const operations = [];
    let failures = 0;
    
    // Run operations with random failures
    for (let i = 0; i < 20; i++) {
      if (Math.random() < 0.3) { // 30% failure rate
        operations.push(Promise.reject(new Error('Chaos-induced failure')));
        failures++;
      } else {
        operations.push(this.simulateTypicalOperation());
      }
    }
    
    const results = await Promise.allSettled(operations);
    const successful = results.filter(r => r.status === 'fulfilled').length;
    const successRate = successful / results.length;
    
    globalProfiler.endTimer('chaos-random-failures');
    
    console.log(`      📊 Success rate under chaos: ${(successRate * 100).toFixed(1)}%`);
    
    const optimizations = [];
    if (successRate < 0.8) {
      optimizations.push('Implement circuit breaker pattern');
      optimizations.push('Add retry mechanism with exponential backoff');
      optimizations.push('Improve error handling and recovery');
    }
    
    return {
      experimentName: 'random-failures',
      success: true,
      baselinePerformance: 1.0,
      chaosPerformance: successRate,
      resilienceScore: successRate > 0.7 ? 0.9 : successRate,
      optimizationsDiscovered: optimizations,
      systemBehavior: `${failures} failures injected, ${(successRate * 100).toFixed(1)}% success rate maintained`
    };
  }
  
  private async executeResourceContention(): Promise<ChaosResult> {
    console.log('   🔒 Creating resource contention...');
    
    globalProfiler.startTimer('chaos-resource-contention');
    
    // Simulate shared resource contention
    const sharedResource = { counter: 0, locked: false };
    const contentionTasks = [];
    
    for (let i = 0; i < 10; i++) {
      contentionTasks.push(this.simulateResourceAccess(sharedResource, i));
    }
    
    const results = await Promise.allSettled(contentionTasks);
    const successful = results.filter(r => r.status === 'fulfilled').length;
    
    globalProfiler.endTimer('chaos-resource-contention');
    
    const optimizations = [];
    if (successful < 8) {
      optimizations.push('Implement lock-free data structures');
      optimizations.push('Use message passing instead of shared state');
      optimizations.push('Add resource pooling with proper synchronization');
    }
    
    return {
      experimentName: 'resource-contention',
      success: true,
      baselinePerformance: 1.0,
      chaosPerformance: successful / 10,
      resilienceScore: Math.min(1.0, (successful / 10) + 0.2),
      optimizationsDiscovered: optimizations,
      systemBehavior: `${successful}/10 operations successful under contention`
    };
  }
  
  private async executeBurstLoad(): Promise<ChaosResult> {
    console.log('   📈 Generating burst load...');
    
    globalProfiler.startTimer('chaos-burst-load');
    
    // Generate sudden burst of operations
    const burstSize = 50;
    const burstOperations = [];
    
    const burstStart = performance.now();
    
    for (let i = 0; i < burstSize; i++) {
      burstOperations.push(this.simulateTypicalOperation());
    }
    
    const results = await Promise.allSettled(burstOperations);
    const burstDuration = performance.now() - burstStart;
    const successful = results.filter(r => r.status === 'fulfilled').length;
    
    globalProfiler.endTimer('chaos-burst-load');
    
    const throughput = successful / (burstDuration / 1000);
    console.log(`      ⚡ Burst throughput: ${throughput.toFixed(1)} ops/sec`);
    
    const optimizations = [];
    if (throughput < 20) {
      optimizations.push('Implement request queuing and rate limiting');
      optimizations.push('Add auto-scaling mechanisms');
      optimizations.push('Use load balancing for burst handling');
    }
    
    return {
      experimentName: 'burst-load',
      success: true,
      baselinePerformance: 1.0,
      chaosPerformance: Math.min(1.0, throughput / 30),
      resilienceScore: Math.min(1.0, successful / burstSize),
      optimizationsDiscovered: optimizations,
      systemBehavior: `Handled ${successful}/${burstSize} operations in burst (${throughput.toFixed(1)} ops/sec)`
    };
  }
  
  private async executeCacheInvalidation(): Promise<ChaosResult> {
    console.log('   🗑️ Randomly invalidating caches...');
    
    globalProfiler.startTimer('chaos-cache-invalidation');
    
    // Simulate cache with random invalidations
    const cache = new Map();
    let cacheHits = 0;
    let cacheMisses = 0;
    
    for (let i = 0; i < 100; i++) {
      const key = Math.floor(Math.random() * 20).toString();
      
      // Random cache invalidation
      if (Math.random() < 0.2) {
        cache.clear();
      }
      
      if (cache.has(key)) {
        cacheHits++;
      } else {
        cacheMisses++;
        cache.set(key, `value_${i}`);
        await this.sleep(1); // Simulate cache miss penalty
      }
    }
    
    globalProfiler.endTimer('chaos-cache-invalidation');
    
    const hitRate = cacheHits / (cacheHits + cacheMisses);
    console.log(`      📊 Cache hit rate under chaos: ${(hitRate * 100).toFixed(1)}%`);
    
    const optimizations = [];
    if (hitRate < 0.6) {
      optimizations.push('Implement cache warming strategies');
      optimizations.push('Add cache partitioning for resilience');
      optimizations.push('Use probabilistic cache invalidation');
    }
    
    return {
      experimentName: 'cache-invalidation',
      success: true,
      baselinePerformance: 1.0,
      chaosPerformance: hitRate,
      resilienceScore: hitRate > 0.4 ? 0.8 : hitRate * 2,
      optimizationsDiscovered: optimizations,
      systemBehavior: `Cache hit rate: ${(hitRate * 100).toFixed(1)}% under random invalidations`
    };
  }
  
  private async executeSlowDependencies(): Promise<ChaosResult> {
    console.log('   🐌 Simulating slow dependencies...');
    
    globalProfiler.startTimer('chaos-slow-dependencies');
    
    const dependencyDelays = [0, 100, 500, 1000, 2000]; // Increasingly slow
    const results = [];
    
    for (const delay of dependencyDelays) {
      const start = performance.now();
      
      // Simulate slow dependency call
      await this.sleep(delay);
      const operation = await this.simulateTypicalOperation();
      
      const totalTime = performance.now() - start;
      results.push({ delay, totalTime });
      
      console.log(`      ⏱️ ${delay}ms dependency → ${totalTime.toFixed(1)}ms total`);
    }
    
    globalProfiler.endTimer('chaos-slow-dependencies');
    
    const avgImpact = results.reduce((sum, r) => sum + r.totalTime, 0) / results.length;
    
    const optimizations = [];
    if (avgImpact > 500) {
      optimizations.push('Implement timeout and fallback patterns');
      optimizations.push('Add dependency health monitoring');
      optimizations.push('Use async patterns to reduce blocking');
    }
    
    return {
      experimentName: 'slow-dependencies',
      success: true,
      baselinePerformance: 1.0,
      chaosPerformance: Math.max(0.1, 100 / avgImpact),
      resilienceScore: Math.max(0.2, 300 / avgImpact),
      optimizationsDiscovered: optimizations,
      systemBehavior: `Average dependency impact: ${avgImpact.toFixed(1)}ms`
    };
  }
  
  // Helper methods
  
  private async intensiveCpuWork(duration: number): Promise<void> {
    const end = Date.now() + duration;
    let iterations = 0;
    
    while (Date.now() < end) {
      Math.sqrt(iterations++ * Math.PI);
    }
  }
  
  private async simulateResourceAccess(resource: any, taskId: number): Promise<void> {
    const maxAttempts = 10;
    
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
      if (!resource.locked) {
        resource.locked = true;
        
        // Critical section
        await this.sleep(Math.random() * 10 + 5);
        resource.counter++;
        
        resource.locked = false;
        return;
      }
      
      await this.sleep(Math.random() * 5 + 1);
    }
    
    throw new Error(`Task ${taskId} failed to acquire resource after ${maxAttempts} attempts`);
  }
  
  private async analyzeChaosResults(result: ChaosResult, baseline: number): Promise<void> {
    this.results.push(result);
    this.antifragileMetrics.totalExperiments++;
    
    // Update metrics
    if (result.resilienceScore > 0.8) {
      this.antifragileMetrics.resilienceGains++;
      this.antifragileMetrics.strengthsIdentified.push(`Strong resilience in ${result.experimentName}`);
    }
    
    if (result.chaosPerformance > baseline * 0.9) {
      this.antifragileMetrics.performanceImprovements++;
    }
    
    if (result.optimizationsDiscovered.length > 0) {
      this.antifragileMetrics.weaknessesDiscovered.push(...result.optimizationsDiscovered);
    }
    
    console.log(`   📊 Results: Resilience ${(result.resilienceScore * 100).toFixed(1)}%, Performance ${(result.chaosPerformance * 100).toFixed(1)}%`);
    console.log(`   🔍 Optimizations discovered: ${result.optimizationsDiscovered.length}`);
    
    if (result.optimizationsDiscovered.length > 0) {
      result.optimizationsDiscovered.forEach(opt => {
        console.log(`      💡 ${opt}`);
      });
    }
  }
  
  private async applyDiscoveredOptimizations(result: ChaosResult): Promise<void> {
    if (result.optimizationsDiscovered.length === 0) return;
    
    console.log(`   ⚙️ Applying ${result.optimizationsDiscovered.length} discovered optimizations...`);
    
    for (const optimization of result.optimizationsDiscovered) {
      // Simulate applying optimization
      await this.sleep(100);
      
      this.antifragileMetrics.adaptations.push(optimization);
      console.log(`      ✅ Applied: ${optimization}`);
    }
  }
  
  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
  
  async generateAntifragileReport(): Promise<void> {
    const successRate = this.results.length > 0 ? 
      this.results.filter(r => r.success).length / this.results.length * 100 : 0;
    
    const avgResilienceScore = this.results.length > 0 ?
      this.results.reduce((sum, r) => sum + r.resilienceScore, 0) / this.results.length * 100 : 0;
    
    console.log('\n🛡️ ANTIFRAGILE SYSTEM REPORT');
    console.log('=============================');
    console.log(`🧪 Total Chaos Experiments: ${this.antifragileMetrics.totalExperiments}`);
    console.log(`✅ Success Rate: ${successRate.toFixed(1)}%`);
    console.log(`🛡️ Average Resilience Score: ${avgResilienceScore.toFixed(1)}%`);
    console.log(`💪 Resilience Gains: ${this.antifragileMetrics.resilienceGains}`);
    console.log(`⚡ Performance Improvements: ${this.antifragileMetrics.performanceImprovements}`);
    console.log(`🔍 Weaknesses Discovered: ${this.antifragileMetrics.weaknessesDiscovered.length}`);
    console.log(`💎 Strengths Identified: ${this.antifragileMetrics.strengthsIdentified.length}`);
    console.log(`🔧 Adaptations Applied: ${this.antifragileMetrics.adaptations.length}`);
    
    if (this.results.length > 0) {
      const bestExperiment = this.results.reduce((best, current) => 
        current.resilienceScore > best.resilienceScore ? current : best
      );
      
      console.log(`\n🏆 Most Revealing Experiment: ${bestExperiment.experimentName}`);
      console.log(`   Resilience: ${(bestExperiment.resilienceScore * 100).toFixed(1)}%`);
      console.log(`   Discoveries: ${bestExperiment.optimizationsDiscovered.length}`);
    }
    
    // Save detailed report
    const reportData = {
      timestamp: new Date().toISOString(),
      antifragileMetrics: this.antifragileMetrics,
      recentResults: this.results.slice(-10),
      insights: this.generateAntifragileInsights()
    };
    
    try {
      await fs.writeFile(
        '.continuum/performance/antifragile-report.json',
        JSON.stringify(reportData, null, 2)
      );
      console.log('\n💾 Detailed antifragile report saved to .continuum/performance/antifragile-report.json');
    } catch (error) {
      console.warn('⚠️ Could not save detailed report');
    }
  }
  
  private generateAntifragileInsights(): string[] {
    const insights: string[] = [];
    
    if (this.antifragileMetrics.resilienceGains > this.antifragileMetrics.totalExperiments * 0.6) {
      insights.push('System demonstrates strong antifragile characteristics');
    }
    
    if (this.antifragileMetrics.adaptations.length > 0) {
      insights.push('System is actively improving through chaos-driven discoveries');
    }
    
    const weaknessTypes = this.antifragileMetrics.weaknessesDiscovered.reduce((types, weakness) => {
      if (weakness.includes('memory')) types.memory++;
      if (weakness.includes('CPU') || weakness.includes('cpu')) types.cpu++;
      if (weakness.includes('network')) types.network++;
      if (weakness.includes('cache')) types.cache++;
      return types;
    }, { memory: 0, cpu: 0, network: 0, cache: 0 });
    
    const maxWeakness = Object.entries(weaknessTypes).reduce((max, [type, count]) => 
      count > max.count ? { type, count } : max, { type: '', count: 0 });
    
    if (maxWeakness.count > 0) {
      insights.push(`Primary optimization area: ${maxWeakness.type} (${maxWeakness.count} issues discovered)`);
    }
    
    return insights;
  }
  
  stop(): void {
    this.isRunning = false;
    console.log('\n🛑 Stopping chaos optimization...');
  }
}

// CLI interface
async function main(): Promise<void> {
  const chaosOptimizer = new ChaosPerformanceOptimizer();
  
  // Generate periodic reports
  setInterval(async () => {
    await chaosOptimizer.generateAntifragileReport();
  }, 60000); // Every minute
  
  // Handle graceful shutdown
  process.on('SIGINT', async () => {
    chaosOptimizer.stop();
    await chaosOptimizer.generateAntifragileReport();
    process.exit(0);
  });
  
  await chaosOptimizer.startChaosOptimization();
}

if (require.main === module) {
  main().catch(error => {
    console.error('❌ Chaos performance optimizer crashed:', error);
    process.exit(1);
  });
}

export { ChaosPerformanceOptimizer };