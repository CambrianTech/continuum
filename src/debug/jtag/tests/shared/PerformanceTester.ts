/**
 * Performance Testing Framework
 * 
 * Modular, well-typed performance testing utilities that integrate with
 * existing JTAG test infrastructure. Measures latency, throughput, memory,
 * and generates optimization suggestions.
 */

import { performance } from 'perf_hooks';
import * as fs from 'fs';
import * as path from 'path';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

export interface PerformanceMetrics {
  latency: LatencyMetrics;
  throughput: ThroughputMetrics;
  reliability: ReliabilityMetrics;
  resources: ResourceMetrics;
}

export interface LatencyMetrics {
  min: number;
  max: number;
  avg: number;
  p50: number;
  p95: number;
  p99: number;
  measurements: number[];
}

export interface ThroughputMetrics {
  operationsPerSecond: number;
  bytesPerSecond: number;
  peakOpsPerSecond: number;
}

export interface ReliabilityMetrics {
  totalOperations: number;
  successfulOperations: number;
  failedOperations: number;
  successRate: number;
  errorRate: number;
}

export interface ResourceMetrics {
  peakMemoryMB: number;
  avgMemoryMB: number;
  memoryGrowthMB: number;
  cpuUsagePercent: number;
}

export interface TestScorecard {
  testName: string;
  timestamp: string;
  duration: number;
  configuration: Record<string, unknown>;
  metrics: PerformanceMetrics;
  optimizations: OptimizationSuggestion[];
  overallScore: number;
  grade: 'A' | 'B' | 'C' | 'D' | 'F';
}

export interface OptimizationSuggestion {
  category: 'latency' | 'throughput' | 'memory' | 'reliability';
  severity: 'critical' | 'high' | 'medium' | 'low';
  issue: string;
  suggestion: string;
  expectedImprovement: string;
}

export class PerformanceTester {
  private measurements: number[] = [];
  private operations = 0;
  private successes = 0;
  private failures = 0;
  private startTime = 0;
  private initialMemory = 0;
  private peakMemory = 0;
  private memoryMeasurements: number[] = [];
  private bytesTransferred = 0;
  private logEntries: string[] = [];

  constructor(
    private testName: string,
    private logDirectory: string
  ) {
    this.ensureLogDirectory();
  }

  private ensureLogDirectory(): void {
    const dirs = [
      path.join(this.logDirectory, 'scorecards'),
      path.join(this.logDirectory, 'logs'),
      path.join(this.logDirectory, 'metrics')
    ];
    
    dirs.forEach(dir => fs.mkdirSync(dir, { recursive: true }));
  }

  /**
   * Start performance measurement session
   */
  start(): void {
    this.startTime = performance.now();
    this.initialMemory = this.getCurrentMemoryMB();
    this.log(`🎯 Starting performance test: ${this.testName}`);
  }

  /**
   * Measure latency of an async operation
   */
  async measureLatency<T>(
    operationName: string,
    operation: () => Promise<T>,
    expectedBytes = 0
  ): Promise<T> {
    const start = performance.now();
    this.trackMemory();

    try {
      const result = await operation();
      const latency = performance.now() - start;
      
      this.measurements.push(latency);
      this.operations++;
      this.successes++;
      this.bytesTransferred += expectedBytes;
      
      this.log(`✅ ${operationName}: ${latency.toFixed(2)}ms`);
      return result;
      
    } catch (error) {
      const latency = performance.now() - start;
      this.measurements.push(latency);
      this.operations++;
      this.failures++;
      
      this.log(`❌ ${operationName}: Failed in ${latency.toFixed(2)}ms - ${error}`);
      throw error;
    }
  }

  /**
   * Measure throughput of batch operations
   */
  async measureThroughput<T>(
    batchName: string,
    operations: Array<() => Promise<T>>,
    expectedBytesPerOp = 0
  ): Promise<T[]> {
    this.log(`⚡ Starting batch: ${batchName} (${operations.length} operations)`);
    const batchStart = performance.now();
    
    const results = await Promise.all(
      operations.map((op, i) => 
        this.measureLatency(`${batchName}-${i}`, op, expectedBytesPerOp)
      )
    );
    
    const batchTime = performance.now() - batchStart;
    const opsPerSecond = (operations.length / batchTime) * 1000;
    
    this.log(`📊 Batch completed: ${opsPerSecond.toFixed(1)} ops/sec`);
    return results;
  }

  /**
   * Track memory usage throughout test
   */
  private trackMemory(): void {
    const currentMB = this.getCurrentMemoryMB();
    this.memoryMeasurements.push(currentMB);
    
    if (currentMB > this.peakMemory) {
      this.peakMemory = currentMB;
    }
  }

  private getCurrentMemoryMB(): number {
    const used = process.memoryUsage();
    return used.heapUsed / 1024 / 1024;
  }

  private log(message: string): void {
    const timestamp = new Date().toISOString();
    const entry = `[${timestamp}] ${message}`;
    console.log(entry);
    this.logEntries.push(entry);
  }

  /**
   * Generate comprehensive performance scorecard
   */
  generateScorecard(configuration: Record<string, unknown>): TestScorecard {
    const duration = performance.now() - this.startTime;
    const metrics = this.calculateMetrics(duration);
    const optimizations = this.generateOptimizations(metrics);
    const score = this.calculateOverallScore(metrics);
    const grade = this.scoreToGrade(score);

    const scorecard: TestScorecard = {
      testName: this.testName,
      timestamp: new Date().toISOString(),
      duration,
      configuration,
      metrics,
      optimizations,
      overallScore: score,
      grade
    };

    this.log(`🎊 Test completed: ${score}/100 (Grade: ${grade})`);
    return scorecard;
  }

  private calculateMetrics(duration: number): PerformanceMetrics {
    // Sort measurements for percentile calculations
    const sortedMeasurements = [...this.measurements].sort((a, b) => a - b);
    
    const latency: LatencyMetrics = {
      min: Math.min(...this.measurements),
      max: Math.max(...this.measurements),
      avg: this.measurements.reduce((sum, val) => sum + val, 0) / this.measurements.length || 0,
      p50: this.getPercentile(sortedMeasurements, 0.50),
      p95: this.getPercentile(sortedMeasurements, 0.95),
      p99: this.getPercentile(sortedMeasurements, 0.99),
      measurements: this.measurements
    };

    const throughput: ThroughputMetrics = {
      operationsPerSecond: (this.operations / duration) * 1000,
      bytesPerSecond: (this.bytesTransferred / duration) * 1000,
      peakOpsPerSecond: this.calculatePeakThroughput()
    };

    const reliability: ReliabilityMetrics = {
      totalOperations: this.operations,
      successfulOperations: this.successes,
      failedOperations: this.failures,
      successRate: (this.successes / this.operations) * 100 || 0,
      errorRate: (this.failures / this.operations) * 100 || 0
    };

    const resources: ResourceMetrics = {
      peakMemoryMB: this.peakMemory,
      avgMemoryMB: this.memoryMeasurements.reduce((sum, val) => sum + val, 0) / this.memoryMeasurements.length || 0,
      memoryGrowthMB: this.peakMemory - this.initialMemory,
      cpuUsagePercent: this.estimateCpuUsage(duration)
    };

    return { latency, throughput, reliability, resources };
  }

  private getPercentile(sortedArray: number[], percentile: number): number {
    if (sortedArray.length === 0) return 0;
    const index = Math.floor(sortedArray.length * percentile);
    return sortedArray[Math.min(index, sortedArray.length - 1)];
  }

  private calculatePeakThroughput(): number {
    // Calculate peak throughput in 1-second windows
    if (this.measurements.length < 10) return 0;
    
    // Simplified peak calculation - would be more sophisticated in production
    return this.operations * 1.2; // Estimate 20% above average
  }

  private estimateCpuUsage(duration: number): number {
    // Rough estimation based on operations and complexity
    const opsPerMs = this.operations / duration;
    return Math.min(100, opsPerMs * 10);
  }

  private generateOptimizations(metrics: PerformanceMetrics): OptimizationSuggestion[] {
    const suggestions: OptimizationSuggestion[] = [];

    // Latency optimizations
    if (metrics.latency.avg > 100) {
      suggestions.push({
        category: 'latency',
        severity: metrics.latency.avg > 500 ? 'critical' : 'high',
        issue: `High average latency: ${metrics.latency.avg.toFixed(2)}ms`,
        suggestion: 'Consider implementing operation batching or connection pooling',
        expectedImprovement: '30-50% latency reduction'
      });
    }

    if (metrics.latency.p99 > metrics.latency.avg * 5) {
      suggestions.push({
        category: 'latency',
        severity: 'medium',
        issue: 'High P99 latency indicates tail latency spikes',
        suggestion: 'Investigate timeout handling and resource contention',
        expectedImprovement: 'More consistent response times'
      });
    }

    // Throughput optimizations  
    if (metrics.throughput.operationsPerSecond < 100) {
      suggestions.push({
        category: 'throughput',
        severity: 'medium',
        issue: `Low throughput: ${metrics.throughput.operationsPerSecond.toFixed(1)} ops/sec`,
        suggestion: 'Implement parallel processing or optimize critical path',
        expectedImprovement: '2-5x throughput increase'
      });
    }

    // Memory optimizations
    if (metrics.resources.memoryGrowthMB > 100) {
      suggestions.push({
        category: 'memory',
        severity: 'high',
        issue: `High memory growth: ${metrics.resources.memoryGrowthMB.toFixed(1)}MB`,
        suggestion: 'Check for memory leaks and optimize object lifecycle',
        expectedImprovement: 'Stable memory usage'
      });
    }

    // Reliability optimizations
    if (metrics.reliability.successRate < 99) {
      suggestions.push({
        category: 'reliability',
        severity: 'critical',
        issue: `Success rate below 99%: ${metrics.reliability.successRate.toFixed(1)}%`,
        suggestion: 'Strengthen error handling and retry mechanisms',
        expectedImprovement: '>99% success rate'
      });
    }

    return suggestions;
  }

  private calculateOverallScore(metrics: PerformanceMetrics): number {
    // Weighted scoring system
    const latencyScore = Math.max(0, 100 - (metrics.latency.avg - 50)); // Penalty if avg > 50ms
    const throughputScore = Math.min(100, metrics.throughput.operationsPerSecond / 10); // 1000 ops/sec = 100%
    const reliabilityScore = metrics.reliability.successRate;
    const memoryScore = Math.max(0, 100 - metrics.resources.memoryGrowthMB); // Penalty for memory growth

    // Weighted average: reliability is most important
    return Math.round(
      (reliabilityScore * 0.4) + 
      (latencyScore * 0.3) + 
      (throughputScore * 0.2) + 
      (memoryScore * 0.1)
    );
  }

  private scoreToGrade(score: number): 'A' | 'B' | 'C' | 'D' | 'F' {
    if (score >= 90) return 'A';
    if (score >= 80) return 'B';
    if (score >= 70) return 'C';
    if (score >= 60) return 'D';
    return 'F';
  }

  /**
   * Save scorecard and logs to disk
   */
  async saveResults(scorecard: TestScorecard): Promise<void> {
    const timestamp = scorecard.timestamp.substring(0, 10);
    
    // Save scorecard
    const scorecardPath = path.join(
      this.logDirectory, 
      'scorecards', 
      `${timestamp}_${this.testName.toLowerCase().replace(/\s+/g, '-')}.json`
    );
    fs.writeFileSync(scorecardPath, JSON.stringify(scorecard, null, 2));

    // Save detailed logs
    const logPath = path.join(
      this.logDirectory,
      'logs',
      `${timestamp}_${this.testName.toLowerCase().replace(/\s+/g, '-')}.log`
    );
    fs.writeFileSync(logPath, this.logEntries.join('\n'));

    // Save raw metrics
    const metricsPath = path.join(
      this.logDirectory,
      'metrics',
      `${timestamp}_${this.testName.toLowerCase().replace(/\s+/g, '-')}-metrics.json`
    );
    fs.writeFileSync(metricsPath, JSON.stringify({
      measurements: this.measurements,
      memoryMeasurements: this.memoryMeasurements,
      operations: this.operations,
      successes: this.successes,
      failures: this.failures
    }, null, 2));

    this.log(`💾 Results saved to: ${scorecardPath}`);
  }
}