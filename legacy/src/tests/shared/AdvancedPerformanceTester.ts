/**
 * Advanced Performance Testing Framework
 * 
 * Enhanced with microsecond precision timing, detailed logging, and structured
 * JSON outputs that personas, widgets, and APIs can easily consume.
 * 
 * Features:
 * - Microsecond timing precision for accurate performance measurement
 * - Structured insights accessible to AI personas and automated systems
 * - Real-time console summaries with detailed timing breakdowns
 * - Widget-friendly JSON outputs with categorized metrics
 * - API-consumable performance data with optimization recommendations
 */

import { performance } from 'perf_hooks';
import * as fs from 'fs';
import * as path from 'path';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

export interface MicrosecondTiming {
  readonly startTimestamp: number;
  readonly endTimestamp: number;
  readonly durationMicroseconds: number;
  readonly durationMilliseconds: number;
  readonly precisionLevel: 'microsecond' | 'millisecond' | 'nanosecond';
}

export interface DetailedOperationMetrics {
  readonly operationName: string;
  readonly operationId: UUID;
  readonly timing: MicrosecondTiming;
  readonly success: boolean;
  readonly bytesProcessed: number;
  readonly resourceUsage: {
    readonly memoryDeltaMB: number;
    readonly cpuUsagePercent: number;
  };
  readonly metadata: Record<string, unknown>;
  readonly layerBreakdown?: Record<string, MicrosecondTiming>;
}

export interface PersonaAccessibleInsights {
  readonly testExecutionId: UUID;
  readonly testName: string;
  readonly timestamp: string;
  readonly executionEnvironment: {
    readonly nodeVersion: string;
    readonly platform: string;
    readonly architecture: string;
    readonly workingDirectory: string;
  };
  readonly performanceProfile: {
    readonly totalOperations: number;
    readonly successRate: number;
    readonly averageLatencyMicroseconds: number;
    readonly throughputOpsPerSecond: number;
    readonly memoryEfficiencyScore: number; // 0-100
    readonly reliabilityScore: number; // 0-100
  };
  readonly operationBreakdown: readonly DetailedOperationMetrics[];
  readonly optimizationInsights: {
    readonly automaticRecommendations: readonly string[];
    readonly performanceBottlenecks: readonly string[];
    readonly architecturalStrengths: readonly string[];
    readonly scalabilityPredictions: readonly string[];
  };
  readonly widgetCompatibleSummary: {
    readonly status: 'excellent' | 'good' | 'needs-optimization' | 'critical';
    readonly keyMetrics: Record<string, number>;
    readonly visualizationData: {
      readonly latencyDistribution: readonly number[];
      readonly throughputOverTime: readonly number[];
      readonly resourceUsagePattern: readonly number[];
    };
  };
}

export class AdvancedPerformanceTester {
  private readonly testExecutionId: UUID;
  private readonly operations: DetailedOperationMetrics[] = [];
  private readonly timingStack: Array<{ name: string; start: number; metadata: Record<string, unknown> }> = [];
  private startTime = 0;
  private initialMemory = 0;
  private peakMemory = 0;
  private logEntries: string[] = [];

  constructor(
    private testName: string,
    private logDirectory: string,
    private enableMicrosecondTiming = true
  ) {
    this.testExecutionId = `test-${Date.now()}-${Math.random().toString(36).substr(2, 9)}` as UUID;
    this.ensureLogDirectory();
    this.logWithTiming('🎯 Advanced Performance Testing Framework Initialized');
  }

  private ensureLogDirectory(): void {
    const dirs = [
      path.join(this.logDirectory, 'advanced-scorecards'),
      path.join(this.logDirectory, 'persona-insights'),
      path.join(this.logDirectory, 'widget-data'),
      path.join(this.logDirectory, 'api-consumable'),
      path.join(this.logDirectory, 'timing-breakdowns')
    ];
    
    dirs.forEach(dir => fs.mkdirSync(dir, { recursive: true }));
  }

  /**
   * Start performance measurement session with microsecond precision
   */
  start(): void {
    this.startTime = performance.now();
    this.initialMemory = this.getCurrentMemoryMB();
    this.logWithTiming(`🚀 Starting advanced performance test: ${this.testName}`);
    this.logWithTiming(`📊 Initial memory: ${this.initialMemory.toFixed(3)}MB`);
    this.logWithTiming(`🔬 Timing precision: ${this.enableMicrosecondTiming ? 'microsecond' : 'millisecond'}`);
  }

  /**
   * Measure operation with detailed timing breakdown and metadata
   */
  async measureOperationDetailed<T>(
    operationName: string,
    operation: () => Promise<T>,
    metadata: Record<string, unknown> = {},
    expectedBytes = 0
  ): Promise<T> {
    const operationId = `op-${Date.now()}-${Math.random().toString(36).substr(2, 6)}` as UUID;
    const memoryBefore = this.getCurrentMemoryMB();
    
    // Push to timing stack for nested operation tracking
    this.timingStack.push({ 
      name: operationName, 
      start: this.getPreciseTimestamp(), 
      metadata 
    });

    this.logWithTiming(`⚡ Starting: ${operationName} (${operationId})`);
    this.logWithTiming(`   Memory before: ${memoryBefore.toFixed(3)}MB`);
    if (Object.keys(metadata).length > 0) {
      this.logWithTiming(`   Metadata: ${JSON.stringify(metadata, null, 2)}`);
    }

    const startTimestamp = this.getPreciseTimestamp();
    let success = false;
    let result: T;

    try {
      result = await operation();
      success = true;
      
      const endTimestamp = this.getPreciseTimestamp();
      const memoryAfter = this.getCurrentMemoryMB();
      this.trackMemory();

      const timing = this.calculateMicrosecondTiming(startTimestamp, endTimestamp);
      const memoryDelta = memoryAfter - memoryBefore;

      const operationMetrics: DetailedOperationMetrics = {
        operationName,
        operationId,
        timing,
        success,
        bytesProcessed: expectedBytes,
        resourceUsage: {
          memoryDeltaMB: memoryDelta,
          cpuUsagePercent: this.estimateCpuUsage(timing.durationMilliseconds)
        },
        metadata,
        layerBreakdown: this.getLayerBreakdown()
      };

      this.operations.push(operationMetrics);

      // Detailed console logging
      this.logWithTiming(`✅ Completed: ${operationName}`);
      this.logWithTiming(`   Duration: ${timing.durationMicroseconds.toFixed(1)}μs (${timing.durationMilliseconds.toFixed(3)}ms)`);
      this.logWithTiming(`   Memory delta: ${memoryDelta > 0 ? '+' : ''}${memoryDelta.toFixed(3)}MB`);
      this.logWithTiming(`   Success: ${success}`);
      
      if (expectedBytes > 0) {
        const throughputMBps = (expectedBytes / 1024 / 1024) / (timing.durationMilliseconds / 1000);
        this.logWithTiming(`   Throughput: ${throughputMBps.toFixed(2)} MB/s`);
      }

      // Pop from timing stack
      this.timingStack.pop();
      
      return result;

    } catch (error) {
      const endTimestamp = this.getPreciseTimestamp();
      const timing = this.calculateMicrosecondTiming(startTimestamp, endTimestamp);
      
      const operationMetrics: DetailedOperationMetrics = {
        operationName,
        operationId,
        timing,
        success: false,
        bytesProcessed: 0,
        resourceUsage: {
          memoryDeltaMB: this.getCurrentMemoryMB() - memoryBefore,
          cpuUsagePercent: 0
        },
        metadata: { ...metadata, error: String(error) }
      };

      this.operations.push(operationMetrics);

      this.logWithTiming(`❌ Failed: ${operationName}`);
      this.logWithTiming(`   Duration: ${timing.durationMicroseconds.toFixed(1)}μs (${timing.durationMilliseconds.toFixed(3)}ms)`);
      this.logWithTiming(`   Error: ${error}`);

      this.timingStack.pop();
      throw error;
    }
  }

  /**
   * Generate comprehensive persona-accessible insights
   */
  generatePersonaAccessibleInsights(configuration: Record<string, unknown>): PersonaAccessibleInsights {
    const duration = performance.now() - this.startTime;
    const successfulOps = this.operations.filter(op => op.success);
    const successRate = (successfulOps.length / this.operations.length) * 100 || 0;
    
    // Calculate advanced metrics
    const avgLatencyMicroseconds = successfulOps.reduce((sum, op) => sum + op.timing.durationMicroseconds, 0) / successfulOps.length || 0;
    const throughputOpsPerSecond = (successfulOps.length / duration) * 1000;
    
    // Memory efficiency score (0-100, higher is better)
    const totalMemoryUsed = this.peakMemory - this.initialMemory;
    const memoryEfficiency = Math.max(0, 100 - Math.min(100, totalMemoryUsed * 2)); // Penalty for high memory usage
    
    // Generate optimization insights
    const insights = this.generateAdvancedOptimizationInsights();
    
    // Widget-compatible status
    const status = this.determineOverallStatus(successRate, avgLatencyMicroseconds / 1000, memoryEfficiency);
    
    const personaInsights: PersonaAccessibleInsights = {
      testExecutionId: this.testExecutionId,
      testName: this.testName,
      timestamp: new Date().toISOString(),
      executionEnvironment: {
        nodeVersion: process.version,
        platform: process.platform,
        architecture: process.arch,
        workingDirectory: process.cwd()
      },
      performanceProfile: {
        totalOperations: this.operations.length,
        successRate,
        averageLatencyMicroseconds: avgLatencyMicroseconds,
        throughputOpsPerSecond,
        memoryEfficiencyScore: memoryEfficiency,
        reliabilityScore: Math.min(100, successRate + (memoryEfficiency * 0.2))
      },
      operationBreakdown: this.operations,
      optimizationInsights: insights,
      widgetCompatibleSummary: {
        status,
        keyMetrics: {
          operations: this.operations.length,
          successRate,
          avgLatencyMs: avgLatencyMicroseconds / 1000,
          throughputOpsPerSec: throughputOpsPerSecond,
          memoryUsageMB: totalMemoryUsed,
          reliabilityScore: Math.min(100, successRate + (memoryEfficiency * 0.2))
        },
        visualizationData: {
          latencyDistribution: this.getLatencyDistribution(),
          throughputOverTime: this.getThroughputOverTime(),
          resourceUsagePattern: this.getResourceUsagePattern()
        }
      }
    };

    this.logWithTiming('📊 Generated persona-accessible insights');
    this.logWithTiming(`   Total operations: ${personaInsights.performanceProfile.totalOperations}`);
    this.logWithTiming(`   Success rate: ${personaInsights.performanceProfile.successRate.toFixed(1)}%`);
    this.logWithTiming(`   Avg latency: ${(personaInsights.performanceProfile.averageLatencyMicroseconds / 1000).toFixed(3)}ms`);
    this.logWithTiming(`   Throughput: ${personaInsights.performanceProfile.throughputOpsPerSecond.toFixed(1)} ops/sec`);
    this.logWithTiming(`   Memory efficiency: ${personaInsights.performanceProfile.memoryEfficiencyScore}/100`);
    this.logWithTiming(`   Overall status: ${personaInsights.widgetCompatibleSummary.status}`);

    return personaInsights;
  }

  /**
   * Save comprehensive results in multiple formats for different consumers
   */
  async saveAdvancedResults(insights: PersonaAccessibleInsights): Promise<void> {
    const timestamp = insights.timestamp.substring(0, 10);
    const testNameSlug = this.testName.toLowerCase().replace(/\s+/g, '-');

    // 1. Persona-accessible insights (detailed JSON for AI consumption)
    const personaPath = path.join(
      this.logDirectory,
      'persona-insights',
      `${timestamp}_${testNameSlug}_persona-insights.json`
    );
    fs.writeFileSync(personaPath, JSON.stringify(insights, null, 2));

    // 2. Widget-compatible data (simplified for UI widgets)
    const widgetPath = path.join(
      this.logDirectory,
      'widget-data',
      `${timestamp}_${testNameSlug}_widget-data.json`
    );
    fs.writeFileSync(widgetPath, JSON.stringify(insights.widgetCompatibleSummary, null, 2));

    // 3. API-consumable format (REST API friendly)
    const apiData = {
      testId: insights.testExecutionId,
      status: insights.widgetCompatibleSummary.status,
      metrics: insights.performanceProfile,
      recommendations: insights.optimizationInsights.automaticRecommendations,
      timestamp: insights.timestamp
    };
    const apiPath = path.join(
      this.logDirectory,
      'api-consumable',
      `${timestamp}_${testNameSlug}_api-data.json`
    );
    fs.writeFileSync(apiPath, JSON.stringify(apiData, null, 2));

    // 4. Detailed timing breakdown (for performance analysis)
    const timingBreakdown = {
      testExecutionId: insights.testExecutionId,
      operations: insights.operationBreakdown.map(op => ({
        name: op.operationName,
        durationMicroseconds: op.timing.durationMicroseconds,
        durationMilliseconds: op.timing.durationMilliseconds,
        success: op.success,
        memoryDelta: op.resourceUsage.memoryDeltaMB,
        layerBreakdown: op.layerBreakdown
      }))
    };
    const timingPath = path.join(
      this.logDirectory,
      'timing-breakdowns',
      `${timestamp}_${testNameSlug}_timing-breakdown.json`
    );
    fs.writeFileSync(timingPath, JSON.stringify(timingBreakdown, null, 2));

    // 5. Human-readable logs
    const logPath = path.join(
      this.logDirectory,
      'advanced-scorecards',
      `${timestamp}_${testNameSlug}_detailed-log.log`
    );
    fs.writeFileSync(logPath, this.logEntries.join('\n'));

    this.logWithTiming('💾 Advanced results saved:');
    this.logWithTiming(`   📊 Persona insights: ${personaPath}`);
    this.logWithTiming(`   🎨 Widget data: ${widgetPath}`);
    this.logWithTiming(`   🔌 API data: ${apiPath}`);
    this.logWithTiming(`   ⏱️  Timing breakdown: ${timingPath}`);
    this.logWithTiming(`   📝 Detailed logs: ${logPath}`);
  }

  private getPreciseTimestamp(): number {
    if (this.enableMicrosecondTiming && performance.timeOrigin) {
      return performance.timeOrigin + performance.now();
    }
    return performance.now();
  }

  private calculateMicrosecondTiming(start: number, end: number): MicrosecondTiming {
    const durationMs = end - start;
    const durationMicroseconds = durationMs * 1000; // Convert ms to μs
    
    return {
      startTimestamp: start,
      endTimestamp: end,
      durationMicroseconds,
      durationMilliseconds: durationMs,
      precisionLevel: this.enableMicrosecondTiming ? 'microsecond' : 'millisecond'
    };
  }

  private getLayerBreakdown(): Record<string, MicrosecondTiming> | undefined {
    if (this.timingStack.length <= 1) return undefined;
    
    const breakdown: Record<string, MicrosecondTiming> = {};
    
    // Create breakdown for nested operations
    for (let i = 0; i < this.timingStack.length - 1; i++) {
      const layer = this.timingStack[i];
      const now = this.getPreciseTimestamp();
      breakdown[layer.name] = this.calculateMicrosecondTiming(layer.start, now);
    }
    
    return breakdown;
  }

  private generateAdvancedOptimizationInsights(): PersonaAccessibleInsights['optimizationInsights'] {
    const recommendations: string[] = [];
    const bottlenecks: string[] = [];
    const strengths: string[] = [];
    const scalabilityPredictions: string[] = [];

    const avgLatencyMs = this.operations.reduce((sum, op) => sum + op.timing.durationMilliseconds, 0) / this.operations.length;
    const successRate = (this.operations.filter(op => op.success).length / this.operations.length) * 100;
    const totalMemoryUsed = this.peakMemory - this.initialMemory;

    // Recommendations based on detailed analysis
    if (avgLatencyMs > 100) {
      recommendations.push(`High average latency detected (${avgLatencyMs.toFixed(2)}ms) - consider operation parallelization`);
      bottlenecks.push('Operation serialization causing latency buildup');
    }

    if (successRate < 95) {
      recommendations.push(`Success rate below 95% (${successRate.toFixed(1)}%) - strengthen error handling and retry mechanisms`);
      bottlenecks.push('Reliability issues affecting overall system performance');
    }

    if (totalMemoryUsed > 50) {
      recommendations.push(`Memory usage growing (${totalMemoryUsed.toFixed(2)}MB) - investigate potential leaks`);
      bottlenecks.push('Memory management inefficiencies');
    } else if (totalMemoryUsed < 5) {
      strengths.push('Excellent memory efficiency - minimal resource consumption');
    }

    if (successRate >= 95) {
      strengths.push('High reliability - excellent error handling');
    }

    if (avgLatencyMs < 50) {
      strengths.push('Low latency operations - efficient processing pipeline');
    }

    // Scalability predictions based on current performance
    const throughput = this.operations.length / ((performance.now() - this.startTime) / 1000);
    if (throughput > 100) {
      scalabilityPredictions.push('High throughput indicates good horizontal scaling potential');
    }

    if (totalMemoryUsed < 10 && avgLatencyMs < 100) {
      scalabilityPredictions.push('Low resource usage suggests system can handle 5-10x current load');
    }

    if (strengths.length === 0) {
      strengths.push('System shows stable performance characteristics');
    }

    if (recommendations.length === 0) {
      recommendations.push('System performance is optimal - no immediate optimizations needed');
    }

    return {
      automaticRecommendations: recommendations,
      performanceBottlenecks: bottlenecks,
      architecturalStrengths: strengths,
      scalabilityPredictions: scalabilityPredictions
    };
  }

  private determineOverallStatus(
    successRate: number, 
    avgLatencyMs: number, 
    memoryEfficiency: number
  ): PersonaAccessibleInsights['widgetCompatibleSummary']['status'] {
    if (successRate >= 98 && avgLatencyMs < 50 && memoryEfficiency > 80) return 'excellent';
    if (successRate >= 90 && avgLatencyMs < 150 && memoryEfficiency > 60) return 'good';
    if (successRate >= 70 && avgLatencyMs < 500) return 'needs-optimization';
    return 'critical';
  }

  private getLatencyDistribution(): number[] {
    const latencies = this.operations.map(op => op.timing.durationMilliseconds);
    latencies.sort((a, b) => a - b);
    
    // Return percentiles for visualization
    if (latencies.length === 0) return [0];
    
    const percentiles = [0.1, 0.25, 0.5, 0.75, 0.9, 0.95, 0.99];
    return percentiles.map(p => {
      const index = Math.floor(latencies.length * p);
      return latencies[Math.min(index, latencies.length - 1)];
    });
  }

  private getThroughputOverTime(): number[] {
    // Simplified throughput over time (5 time buckets)
    const bucketCount = 5;
    const buckets = new Array(bucketCount).fill(0);
    const totalDuration = performance.now() - this.startTime;
    const bucketSize = totalDuration / bucketCount;

    this.operations.forEach(op => {
      const opTime = op.timing.startTimestamp - this.startTime;
      const bucketIndex = Math.floor(Math.min(opTime / bucketSize, bucketCount - 1));
      buckets[bucketIndex]++;
    });

    return buckets.map(count => (count / (bucketSize / 1000))); // ops per second per bucket
  }

  private getResourceUsagePattern(): number[] {
    // Return memory usage pattern over time
    return this.operations.map(op => op.resourceUsage.memoryDeltaMB);
  }

  private getCurrentMemoryMB(): number {
    const used = process.memoryUsage();
    return used.heapUsed / 1024 / 1024;
  }

  private trackMemory(): void {
    const currentMB = this.getCurrentMemoryMB();
    if (currentMB > this.peakMemory) {
      this.peakMemory = currentMB;
    }
  }

  private estimateCpuUsage(durationMs: number): number {
    // Rough CPU usage estimation based on operation complexity
    return Math.min(100, durationMs * 0.1);
  }

  private logWithTiming(message: string): void {
    const timestamp = new Date().toISOString();
    const elapsed = this.startTime ? (performance.now() - this.startTime).toFixed(3) : '0.000';
    const entry = `[${timestamp}] (+${elapsed}ms) ${message}`;
    console.log(entry);
    this.logEntries.push(entry);
  }
}