/**
 * Performance Profiler - Iterative Optimization Framework
 * 
 * The satisfaction of watching methods get faster with each iteration!
 * Provides microsecond-precision timing and comparison tools.
 */

export interface TimingResult {
  name: string;
  startTime: number;
  endTime: number;
  duration: number;
  memory?: {
    before: number;
    after: number;
    delta: number;
  };
  metadata?: Record<string, any>;
}

export interface PerformanceMetrics {
  totalDuration: number;
  averageDuration: number;
  minDuration: number;
  maxDuration: number;
  samples: number;
  throughputPerSecond?: number;
  memoryEfficiency?: number;
}

export class PerformanceProfiler {
  private timings: Map<string, TimingResult[]> = new Map();
  private activeTimers: Map<string, { startTime: number; startMemory: number }> = new Map();
  
  /**
   * Start timing a method/operation
   */
  startTimer(name: string, metadata?: Record<string, any>): void {
    const startTime = performance.now();
    const startMemory = this.getMemoryUsage();
    
    this.activeTimers.set(name, { startTime, startMemory });
    
    // Store metadata for later
    if (metadata) {
      const existing = this.timings.get(name) || [];
      existing.push({
        name,
        startTime,
        endTime: 0,
        duration: 0,
        metadata
      } as TimingResult);
      this.timings.set(name, existing);
    }
  }

  /**
   * End timing and record result
   */
  endTimer(name: string, additionalMetadata?: Record<string, any>): TimingResult | null {
    const active = this.activeTimers.get(name);
    if (!active) {
      console.warn(`⚠️ PerformanceProfiler: No active timer found for '${name}'`);
      return null;
    }

    const endTime = performance.now();
    const endMemory = this.getMemoryUsage();
    const duration = endTime - active.startTime;
    
    const result: TimingResult = {
      name,
      startTime: active.startTime,
      endTime,
      duration,
      memory: {
        before: active.startMemory,
        after: endMemory,
        delta: endMemory - active.startMemory
      },
      metadata: additionalMetadata
    };

    // Store timing result
    const existing = this.timings.get(name) || [];
    existing.push(result);
    this.timings.set(name, existing);
    
    this.activeTimers.delete(name);
    return result;
  }

  /**
   * Time a synchronous function
   */
  timeSync<T>(name: string, fn: () => T, metadata?: Record<string, any>): { result: T; timing: TimingResult } {
    this.startTimer(name, metadata);
    const result = fn();
    const timing = this.endTimer(name)!;
    return { result, timing };
  }

  /**
   * Time an async function
   */
  async timeAsync<T>(name: string, fn: () => Promise<T>, metadata?: Record<string, any>): Promise<{ result: T; timing: TimingResult }> {
    this.startTimer(name, metadata);
    const result = await fn();
    const timing = this.endTimer(name)!;
    return { result, timing };
  }

  /**
   * Get performance metrics for a specific operation
   */
  getMetrics(name: string): PerformanceMetrics | null {
    const timings = this.timings.get(name);
    if (!timings || timings.length === 0) return null;

    const durations = timings.map(t => t.duration);
    const totalDuration = durations.reduce((sum, d) => sum + d, 0);
    
    return {
      totalDuration,
      averageDuration: totalDuration / durations.length,
      minDuration: Math.min(...durations),
      maxDuration: Math.max(...durations),
      samples: durations.length,
      throughputPerSecond: durations.length / (totalDuration / 1000),
      memoryEfficiency: this.calculateMemoryEfficiency(timings)
    };
  }

  /**
   * Compare performance between iterations
   */
  compareIterations(name: string, baselineCount: number = 5): {
    baseline: PerformanceMetrics;
    current: PerformanceMetrics;
    improvement: {
      speedup: number;
      percentImprovement: number;
      memoryImprovement: number;
    };
  } | null {
    const timings = this.timings.get(name);
    if (!timings || timings.length < baselineCount * 2) return null;

    const baseline = timings.slice(0, baselineCount);
    const current = timings.slice(-baselineCount);

    const baselineAvg = baseline.reduce((sum, t) => sum + t.duration, 0) / baseline.length;
    const currentAvg = current.reduce((sum, t) => sum + t.duration, 0) / current.length;
    
    const baselineMemory = baseline.reduce((sum, t) => sum + (t.memory?.delta || 0), 0) / baseline.length;
    const currentMemory = current.reduce((sum, t) => sum + (t.memory?.delta || 0), 0) / current.length;

    const speedup = baselineAvg / currentAvg;
    const percentImprovement = ((baselineAvg - currentAvg) / baselineAvg) * 100;
    const memoryImprovement = ((baselineMemory - currentMemory) / Math.abs(baselineMemory)) * 100;

    return {
      baseline: this.calculateMetricsForTimings(baseline),
      current: this.calculateMetricsForTimings(current),
      improvement: {
        speedup,
        percentImprovement,
        memoryImprovement
      }
    };
  }

  /**
   * Generate performance report
   */
  generateReport(operations?: string[]): string {
    const targetOps = operations || Array.from(this.timings.keys());
    let report = '🚀 PERFORMANCE OPTIMIZATION REPORT\n';
    report += '==================================\n\n';

    for (const operation of targetOps) {
      const metrics = this.getMetrics(operation);
      if (!metrics) continue;

      report += `📊 ${operation}:\n`;
      report += `   Average: ${metrics.averageDuration.toFixed(2)}ms\n`;
      report += `   Min: ${metrics.minDuration.toFixed(2)}ms\n`;
      report += `   Max: ${metrics.maxDuration.toFixed(2)}ms\n`;
      report += `   Throughput: ${metrics.throughputPerSecond?.toFixed(1)} ops/sec\n`;
      report += `   Samples: ${metrics.samples}\n`;

      // Show improvement if we have enough data
      const comparison = this.compareIterations(operation);
      if (comparison) {
        const improvement = comparison.improvement;
        const indicator = improvement.percentImprovement > 0 ? '🚀' : '🐌';
        report += `   ${indicator} Improvement: ${improvement.percentImprovement.toFixed(1)}% (${improvement.speedup.toFixed(2)}x speedup)\n`;
        
        if (Math.abs(improvement.memoryImprovement) > 5) {
          const memIndicator = improvement.memoryImprovement > 0 ? '💾' : '🗑️';
          report += `   ${memIndicator} Memory: ${improvement.memoryImprovement.toFixed(1)}% ${improvement.memoryImprovement > 0 ? 'reduced' : 'increased'}\n`;
        }
      }
      report += '\n';
    }

    return report;
  }

  /**
   * Clear all timing data
   */
  clear(): void {
    this.timings.clear();
    this.activeTimers.clear();
  }

  /**
   * Get memory usage in MB
   */
  private getMemoryUsage(): number {
    if (typeof process !== 'undefined' && process.memoryUsage) {
      return process.memoryUsage().heapUsed / 1024 / 1024;
    }
    return 0;
  }

  /**
   * Calculate memory efficiency score
   */
  private calculateMemoryEfficiency(timings: TimingResult[]): number {
    const memoryDeltas = timings
      .map(t => t.memory?.delta || 0)
      .filter(delta => delta !== 0);
    
    if (memoryDeltas.length === 0) return 100;
    
    const avgDelta = memoryDeltas.reduce((sum, delta) => sum + Math.abs(delta), 0) / memoryDeltas.length;
    return Math.max(0, 100 - avgDelta); // Lower memory usage = higher efficiency
  }

  /**
   * Calculate metrics for a subset of timings
   */
  private calculateMetricsForTimings(timings: TimingResult[]): PerformanceMetrics {
    const durations = timings.map(t => t.duration);
    const totalDuration = durations.reduce((sum, d) => sum + d, 0);
    
    return {
      totalDuration,
      averageDuration: totalDuration / durations.length,
      minDuration: Math.min(...durations),
      maxDuration: Math.max(...durations),
      samples: durations.length,
      throughputPerSecond: durations.length / (totalDuration / 1000),
      memoryEfficiency: this.calculateMemoryEfficiency(timings)
    };
  }

  /**
   * Create a decorator for automatic method timing
   */
  static createTimingDecorator(profiler: PerformanceProfiler) {
    return function timeMethod(target: any, propertyName: string, descriptor: PropertyDescriptor) {
      const method = descriptor.value;
      
      descriptor.value = function (...args: any[]) {
        const methodName = `${target.constructor.name}.${propertyName}`;
        
        if (method.constructor.name === 'AsyncFunction') {
          return profiler.timeAsync(methodName, () => method.apply(this, args));
        } else {
          return profiler.timeSync(methodName, () => method.apply(this, args));
        }
      };
    };
  }
}

// Global profiler instance for easy access
export const globalProfiler = new PerformanceProfiler();