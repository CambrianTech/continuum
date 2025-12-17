#!/usr/bin/env tsx
/**
 * Automated Performance Monitoring System - Make Dev Always Easier & Better
 * 
 * Continuously monitors performance, auto-starts system, collects metrics in JSON,
 * provides instant feedback, and optimizes iteratively. No manual steps.
 */

import { performance } from 'perf_hooks';
import { spawn, ChildProcess } from 'child_process';
import * as fs from 'fs/promises';
import * as path from 'path';
import { globalProfiler } from '../shared/performance/PerformanceProfiler';

interface AutomatedMonitoringConfig {
  autoStartSystem: boolean;
  monitoringInterval: number;
  performanceLogPath: string;
  maxLogRetention: number;
  autoOptimization: boolean;
  realTimeAlerts: boolean;
}

interface PerformanceSnapshot {
  timestamp: number;
  memory: NodeJS.MemoryUsage;
  systemHealth: {
    httpLatency?: number;
    wsLatency?: number;
    queueDepth?: number;
    errorRate?: number;
  };
  testResults?: {
    successRate: number;
    averageLatency: number;
    throughput: number;
  };
}

interface DevExperienceMetrics {
  startupTime: number;
  buildTime: number;
  testTime: number;
  iterationTime: number;
  systemStabilityScore: number;
  developmentVelocity: number;
}

class AutomatedPerformanceMonitor {
  private config: AutomatedMonitoringConfig;
  private monitoring = false;
  private systemProcess?: ChildProcess;
  private performanceHistory: PerformanceSnapshot[] = [];
  private devMetrics: DevExperienceMetrics[] = [];
  private monitoringStartTime = 0;
  
  constructor(config: Partial<AutomatedMonitoringConfig> = {}) {
    this.config = {
      autoStartSystem: true,
      monitoringInterval: 5000, // 5 seconds
      performanceLogPath: '.continuum/performance/',
      maxLogRetention: 1000, // Keep 1000 snapshots
      autoOptimization: true,
      realTimeAlerts: true,
      ...config
    };
  }
  
  async startAutomatedMonitoring(): Promise<void> {
    console.log('🚀 AUTOMATED PERFORMANCE MONITORING');
    console.log('===================================');
    console.log('Making dev always easier and better with continuous optimization!');
    
    this.monitoringStartTime = performance.now();
    
    try {
      // Step 1: Auto-start system if enabled
      if (this.config.autoStartSystem) {
        await this.autoStartSystem();
      }
      
      // Step 2: Ensure performance logging directory exists
      await this.ensurePerformanceDirectory();
      
      // Step 3: Start continuous monitoring
      await this.startContinuousMonitoring();
      
      // Step 4: Run automated optimization cycles
      if (this.config.autoOptimization) {
        this.startAutoOptimization();
      }
      
    } catch (error: any) {
      console.error('❌ Automated monitoring failed:', error.message);
      await this.saveErrorReport(error);
    }
  }
  
  private async autoStartSystem(): Promise<void> {
    console.log('\n🔧 AUTO-STARTING SYSTEM');
    console.log('======================');
    
    const startTime = performance.now();
    
    // Check if system is already running
    if (await this.isSystemRunning()) {
      console.log('✅ System already running - continuing with monitoring');
      return;
    }
    
    console.log('🚀 Starting system automatically...');
    
    // Kill any existing processes
    await this.cleanupExistingProcesses();
    
    // Start system in background
    this.systemProcess = spawn('npm', ['start'], {
      cwd: process.cwd(),
      detached: true,
      stdio: ['ignore', 'pipe', 'pipe']
    });
    
    // Monitor startup
    let started = false;
    const maxWaitTime = 60000; // 60 seconds max
    const checkInterval = 2000; // Check every 2 seconds
    
    for (let waited = 0; waited < maxWaitTime; waited += checkInterval) {
      await this.sleep(checkInterval);
      
      if (await this.isSystemRunning()) {
        started = true;
        break;
      }
      
      console.log(`   ⏳ Waiting for system startup... ${waited/1000}s`);
    }
    
    const startupTime = performance.now() - startTime;
    
    if (started) {
      console.log(`✅ System started successfully in ${(startupTime/1000).toFixed(1)}s`);
      this.recordDevMetric('startupTime', startupTime);
    } else {
      throw new Error(`System failed to start within ${maxWaitTime/1000}s`);
    }
  }
  
  private async isSystemRunning(): Promise<boolean> {
    try {
      // Use dynamic port resolution
      const { getActivePorts } = require('../examples/server/ExampleConfigServer');
      const activePorts = await getActivePorts();
      const response = await fetch(`http://localhost:${activePorts.http_server}`, { 
        method: 'GET',
        signal: AbortSignal.timeout(3000)
      });
      return response.ok;
    } catch {
      return false;
    }
  }
  
  private async cleanupExistingProcesses(): Promise<void> {
    try {
      // Kill any existing npm/node processes on our ports
      spawn('pkill', ['-f', 'npm.*start']);
      spawn('pkill', ['-f', 'node.*continuum']);
      await this.sleep(2000);
    } catch {
      // Cleanup best effort - don't fail if it doesn't work
    }
  }
  
  private async startContinuousMonitoring(): Promise<void> {
    console.log('\n📊 STARTING CONTINUOUS MONITORING');
    console.log('=================================');
    console.log(`Monitoring every ${this.config.monitoringInterval/1000}s`);
    
    this.monitoring = true;
    
    const monitor = async () => {
      if (!this.monitoring) return;
      
      try {
        // Collect performance snapshot
        const snapshot = await this.collectPerformanceSnapshot();
        this.performanceHistory.push(snapshot);
        
        // Trim history if too large
        if (this.performanceHistory.length > this.config.maxLogRetention) {
          this.performanceHistory = this.performanceHistory.slice(-this.config.maxLogRetention);
        }
        
        // Save to JSON
        await this.savePerformanceSnapshot(snapshot);
        
        // Real-time alerts
        if (this.config.realTimeAlerts) {
          this.checkPerformanceAlerts(snapshot);
        }
        
        // Print live status
        this.printLiveStatus(snapshot);
        
      } catch (error: any) {
        console.warn(`⚠️ Monitoring error: ${error.message}`);
      }
      
      // Schedule next monitoring cycle
      setTimeout(monitor, this.config.monitoringInterval);
    };
    
    monitor();
  }
  
  private async collectPerformanceSnapshot(): Promise<PerformanceSnapshot> {
    const snapshot: PerformanceSnapshot = {
      timestamp: Date.now(),
      memory: process.memoryUsage(),
      systemHealth: {}
    };
    
    // Test HTTP latency
    try {
      const httpStart = performance.now();
      // Use dynamic port resolution  
      const { getActivePorts } = require('../examples/server/ExampleConfigServer');
      const activePorts = await getActivePorts();
      const response = await fetch(`http://localhost:${activePorts.http_server}`, { 
        signal: AbortSignal.timeout(5000)
      });
      snapshot.systemHealth.httpLatency = performance.now() - httpStart;
    } catch {
      snapshot.systemHealth.httpLatency = -1; // Indicate failure
    }
    
    // Run quick performance test
    try {
      const testStart = performance.now();
      const quickTestResult = await this.runQuickPerformanceTest();
      snapshot.testResults = quickTestResult;
      snapshot.systemHealth.queueDepth = quickTestResult.averageLatency > 100 ? 1 : 0;
    } catch (error: any) {
      snapshot.systemHealth.errorRate = 1;
    }
    
    return snapshot;
  }
  
  private async runQuickPerformanceTest(): Promise<{ successRate: number; averageLatency: number; throughput: number }> {
    const testCount = 5;
    const results = { successful: 0, totalLatency: 0 };
    
    globalProfiler.startTimer('quick-performance-test');
    
    // Run quick tests
    for (let i = 0; i < testCount; i++) {
      const start = performance.now();
      
      try {
        // Simple HTTP ping test
        const { getActivePorts } = require('../examples/server/ExampleConfigServer');
        const activePorts = await getActivePorts();
        await fetch(`http://localhost:${activePorts.http_server}`, { 
          method: 'HEAD',
          signal: AbortSignal.timeout(2000)
        });
        
        results.successful++;
        results.totalLatency += performance.now() - start;
      } catch {
        results.totalLatency += performance.now() - start;
      }
    }
    
    const timing = globalProfiler.endTimer('quick-performance-test');
    
    return {
      successRate: (results.successful / testCount) * 100,
      averageLatency: results.totalLatency / testCount,
      throughput: results.successful / ((timing?.duration || 1) / 1000)
    };
  }
  
  private checkPerformanceAlerts(snapshot: PerformanceSnapshot): void {
    const alerts: string[] = [];
    
    // Memory alerts
    const memoryMB = snapshot.memory.heapUsed / 1024 / 1024;
    if (memoryMB > 200) {
      alerts.push(`High memory usage: ${memoryMB.toFixed(1)}MB`);
    }
    
    // Latency alerts
    if (snapshot.systemHealth.httpLatency && snapshot.systemHealth.httpLatency > 500) {
      alerts.push(`High HTTP latency: ${snapshot.systemHealth.httpLatency.toFixed(1)}ms`);
    }
    
    // Success rate alerts
    if (snapshot.testResults && snapshot.testResults.successRate < 80) {
      alerts.push(`Low success rate: ${snapshot.testResults.successRate.toFixed(1)}%`);
    }
    
    // Print alerts
    if (alerts.length > 0) {
      console.log(`\n⚠️ PERFORMANCE ALERTS:`);
      alerts.forEach(alert => console.log(`   🚨 ${alert}`));
    }
  }
  
  private printLiveStatus(snapshot: PerformanceSnapshot): void {
    const memoryMB = (snapshot.memory.heapUsed / 1024 / 1024).toFixed(1);
    const httpLatency = snapshot.systemHealth.httpLatency ? 
      `${snapshot.systemHealth.httpLatency.toFixed(1)}ms` : 'N/A';
    const successRate = snapshot.testResults ? 
      `${snapshot.testResults.successRate.toFixed(1)}%` : 'N/A';
    
    // Clear previous line and print status
    process.stdout.write('\r\x1b[K'); // Clear line
    process.stdout.write(`📊 Live: Memory=${memoryMB}MB, HTTP=${httpLatency}, Success=${successRate} | ${new Date().toLocaleTimeString()}`);
  }
  
  private async startAutoOptimization(): Promise<void> {
    console.log('\n🎯 AUTO-OPTIMIZATION ENABLED');
    console.log('============================');
    
    // Run optimization every 30 seconds
    setInterval(async () => {
      if (this.performanceHistory.length < 5) return;
      
      try {
        await this.runOptimizationCycle();
      } catch (error: any) {
        console.warn(`⚠️ Auto-optimization error: ${error.message}`);
      }
    }, 30000);
  }
  
  private async runOptimizationCycle(): Promise<void> {
    const recentSnapshots = this.performanceHistory.slice(-10);
    
    // Analyze trends
    const memoryTrend = this.calculateTrend(recentSnapshots, s => s.memory.heapUsed);
    const latencyTrend = this.calculateTrend(recentSnapshots, s => s.systemHealth.httpLatency || 0);
    
    const optimizations: string[] = [];
    
    // Memory optimization
    if (memoryTrend > 0 && recentSnapshots[recentSnapshots.length - 1].memory.heapUsed > 200 * 1024 * 1024) {
      optimizations.push('Memory usage increasing - triggering GC');
      if (global.gc) {
        global.gc();
      }
    }
    
    // Latency optimization
    if (latencyTrend > 10) {
      optimizations.push('Latency increasing - system may need restart');
    }
    
    if (optimizations.length > 0) {
      console.log(`\n🎯 AUTO-OPTIMIZATIONS APPLIED:`);
      optimizations.forEach(opt => console.log(`   ✅ ${opt}`));
    }
  }
  
  private calculateTrend(snapshots: PerformanceSnapshot[], getValue: (s: PerformanceSnapshot) => number): number {
    if (snapshots.length < 2) return 0;
    
    const values = snapshots.map(getValue);
    const first = values[0];
    const last = values[values.length - 1];
    
    return last - first;
  }
  
  private recordDevMetric(type: string, value: number): void {
    // Record development velocity metrics
    const current = Date.now();
    this.devMetrics.push({
      startupTime: type === 'startupTime' ? value : 0,
      buildTime: type === 'buildTime' ? value : 0,
      testTime: type === 'testTime' ? value : 0,
      iterationTime: current - this.monitoringStartTime,
      systemStabilityScore: this.calculateStabilityScore(),
      developmentVelocity: this.calculateDevelopmentVelocity()
    });
  }
  
  private calculateStabilityScore(): number {
    if (this.performanceHistory.length < 5) return 1.0;
    
    const recent = this.performanceHistory.slice(-10);
    const successRates = recent.map(s => s.testResults?.successRate || 0);
    const avgSuccessRate = successRates.reduce((sum, rate) => sum + rate, 0) / successRates.length;
    
    return avgSuccessRate / 100;
  }
  
  private calculateDevelopmentVelocity(): number {
    // Simple metric: iterations per hour
    const runtime = (Date.now() - this.monitoringStartTime) / 1000 / 3600; // hours
    return this.performanceHistory.length / Math.max(runtime, 0.01);
  }
  
  private async ensurePerformanceDirectory(): Promise<void> {
    const dir = path.resolve(this.config.performanceLogPath);
    await fs.mkdir(dir, { recursive: true });
  }
  
  private async savePerformanceSnapshot(snapshot: PerformanceSnapshot): Promise<void> {
    try {
      const filename = `performance-${new Date().toISOString().slice(0, 10)}.jsonl`;
      const filepath = path.join(this.config.performanceLogPath, filename);
      const line = JSON.stringify(snapshot) + '\n';
      
      await fs.appendFile(filepath, line);
    } catch (error: any) {
      console.warn(`Failed to save performance snapshot: ${error.message}`);
    }
  }
  
  private async saveErrorReport(error: Error): Promise<void> {
    try {
      const report = {
        timestamp: Date.now(),
        error: error.message,
        stack: error.stack,
        performanceHistory: this.performanceHistory.slice(-5),
        devMetrics: this.devMetrics.slice(-5)
      };
      
      const filename = `error-report-${Date.now()}.json`;
      const filepath = path.join(this.config.performanceLogPath, filename);
      
      await fs.writeFile(filepath, JSON.stringify(report, null, 2));
      console.log(`💾 Error report saved: ${filepath}`);
    } catch {
      // Best effort error reporting
    }
  }
  
  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
  
  async stop(): Promise<void> {
    console.log('\n🔄 Stopping automated monitoring...');
    
    this.monitoring = false;
    
    if (this.systemProcess) {
      this.systemProcess.kill();
    }
    
    // Generate final report
    await this.generateFinalReport();
    
    console.log('✅ Automated monitoring stopped');
  }
  
  private async generateFinalReport(): Promise<void> {
    const runtime = (Date.now() - this.monitoringStartTime) / 1000;
    
    const report = {
      totalRuntime: runtime,
      snapshotsCollected: this.performanceHistory.length,
      averageMemoryUsage: this.calculateAverageMemory(),
      systemStabilityScore: this.calculateStabilityScore(),
      developmentVelocity: this.calculateDevelopmentVelocity(),
      performanceInsights: this.generatePerformanceInsights()
    };
    
    console.log('\n📊 FINAL PERFORMANCE REPORT:');
    console.log('============================');
    console.log(`⏱️  Runtime: ${(runtime/60).toFixed(1)} minutes`);
    console.log(`📊 Snapshots: ${report.snapshotsCollected}`);
    console.log(`💾 Avg Memory: ${report.averageMemoryUsage.toFixed(1)}MB`);
    console.log(`🎯 Stability: ${(report.systemStabilityScore * 100).toFixed(1)}%`);
    console.log(`🚀 Dev Velocity: ${report.developmentVelocity.toFixed(1)} iterations/hour`);
    
    // Save detailed report
    const filename = `final-report-${Date.now()}.json`;
    const filepath = path.join(this.config.performanceLogPath, filename);
    await fs.writeFile(filepath, JSON.stringify(report, null, 2));
    
    console.log(`💾 Detailed report saved: ${filepath}`);
  }
  
  private calculateAverageMemory(): number {
    if (this.performanceHistory.length === 0) return 0;
    
    const totalMemory = this.performanceHistory.reduce((sum, snapshot) => 
      sum + (snapshot.memory.heapUsed / 1024 / 1024), 0);
    
    return totalMemory / this.performanceHistory.length;
  }
  
  private generatePerformanceInsights(): string[] {
    const insights: string[] = [];
    
    if (this.performanceHistory.length > 0) {
      const latest = this.performanceHistory[this.performanceHistory.length - 1];
      
      if (latest.testResults && latest.testResults.successRate > 95) {
        insights.push('Excellent system reliability');
      }
      
      if (latest.systemHealth.httpLatency && latest.systemHealth.httpLatency < 50) {
        insights.push('Very responsive HTTP performance');
      }
      
      const memoryMB = latest.memory.heapUsed / 1024 / 1024;
      if (memoryMB < 100) {
        insights.push('Efficient memory usage');
      }
    }
    
    if (this.calculateStabilityScore() > 0.9) {
      insights.push('High system stability throughout monitoring period');
    }
    
    return insights;
  }
}

// CLI interface
async function main(): Promise<void> {
  const monitor = new AutomatedPerformanceMonitor({
    autoStartSystem: process.argv.includes('--auto-start'),
    realTimeAlerts: !process.argv.includes('--no-alerts'),
    autoOptimization: !process.argv.includes('--no-optimization')
  });
  
  // Handle graceful shutdown
  process.on('SIGINT', async () => {
    console.log('\n👋 Shutting down gracefully...');
    await monitor.stop();
    process.exit(0);
  });
  
  await monitor.startAutomatedMonitoring();
}

if (require.main === module) {
  main().catch(error => {
    console.error('❌ Automated monitoring crashed:', error);
    process.exit(1);
  });
}

export { AutomatedPerformanceMonitor };