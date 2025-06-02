/**
 * Self-Improvement Coordinator
 * 
 * Makes the continuum repository self-improving with proper feedback loops:
 * - Tracks what works and what doesn't across all operations
 * - Monitors costs, disk usage, memory consumption
 * - Creates feedback loops to avoid expensive operations
 * - Learns patterns and builds a strategy database
 * - Coordinates mundane tasks and system optimization
 * - Prevents runaway resource usage
 */

import * as fs from 'fs';
import * as path from 'path';
import { promisify } from 'util';
import { exec } from 'child_process';
import { ContinuumMemory, DatabaseAI } from '../memory/index.js';

const execAsync = promisify(exec);

export interface FeedbackLoop {
  id: string;
  operation: string;
  input: any;
  output: any;
  success: boolean;
  cost: number;
  timeMs: number;
  memoryUsageMB: number;
  diskUsageMB: number;
  timestamp: number;
  learnings: string[];
  improvements: string[];
}

export interface ResourceMonitor {
  diskUsageMB: number;
  memoryUsageMB: number;
  cpuUsagePercent: number;
  openFiles: number;
  networkRequests: number;
  costSpent: number;
  lastCheck: number;
  warnings: string[];
  limits: {
    maxDiskMB: number;
    maxMemoryMB: number;
    maxCostDaily: number;
    maxOpenFiles: number;
  };
}

export interface StrategyPattern {
  pattern: string;
  context: string;
  successCount: number;
  failureCount: number;
  totalCost: number;
  avgTime: number;
  reliability: number;
  lastUsed: number;
  improvements: string[];
  antiPatterns: string[];
}

export class SelfImprovementCoordinator {
  private projectRoot: string;
  private memory: ContinuumMemory;
  private feedbackLoops: FeedbackLoop[] = [];
  private strategyPatterns: Map<string, StrategyPattern> = new Map();
  private resourceMonitor: ResourceMonitor;
  private improvementHistory: string[] = [];
  private isMonitoring = false;

  constructor(projectRoot: string) {
    this.projectRoot = projectRoot;
    this.memory = new ContinuumMemory(projectRoot);
    
    this.resourceMonitor = {
      diskUsageMB: 0,
      memoryUsageMB: 0,
      cpuUsagePercent: 0,
      openFiles: 0,
      networkRequests: 0,
      costSpent: 0,
      lastCheck: Date.now(),
      warnings: [],
      limits: {
        maxDiskMB: 500, // 500MB limit for .continuum directory
        maxMemoryMB: 100, // 100MB memory limit
        maxCostDaily: 1.00, // $1 daily limit
        maxOpenFiles: 50
      }
    };

    this.initializeSelfImprovement();
  }

  private async initializeSelfImprovement(): Promise<void> {
    console.log('🚀 SELF-IMPROVEMENT COORDINATOR ACTIVATING');
    console.log('==========================================');
    console.log('📈 Creating feedback loops for continuous improvement');
    console.log('💰 Monitoring costs and resource usage');
    console.log('🧠 Learning from successes and failures');
    console.log('');

    // Load existing patterns and feedback
    await this.loadExistingPatterns();
    
    // Start resource monitoring
    this.startResourceMonitoring();
    
    console.log('✅ Self-improvement system ready');
    console.log(`📊 Loaded ${this.strategyPatterns.size} strategy patterns`);
    console.log(`🔄 ${this.feedbackLoops.length} feedback loops in history`);
  }

  private async loadExistingPatterns(): Promise<void> {
    try {
      const patternsPath = path.join(this.projectRoot, '.continuum', 'improvement-patterns.json');
      if (fs.existsSync(patternsPath)) {
        const data = JSON.parse(fs.readFileSync(patternsPath, 'utf-8'));
        data.forEach((pattern: any) => {
          this.strategyPatterns.set(pattern.pattern, pattern);
        });
      }

      const feedbackPath = path.join(this.projectRoot, '.continuum', 'feedback-loops.json');
      if (fs.existsSync(feedbackPath)) {
        this.feedbackLoops = JSON.parse(fs.readFileSync(feedbackPath, 'utf-8'));
      }
    } catch (error) {
      console.log('📝 Starting with fresh improvement tracking');
    }
  }

  // Core Feedback Loop System
  async recordOperation(operation: {
    name: string;
    input: any;
    startTime: number;
    startMemory: number;
    startDisk: number;
  }): Promise<string> {
    const operationId = `op_${Date.now()}_${Math.random().toString(36).substr(2, 5)}`;
    
    console.log(`📊 Recording operation: ${operation.name} [${operationId}]`);
    
    // Store operation start for tracking
    (this as any).activeOperations = (this as any).activeOperations || new Map();
    (this as any).activeOperations.set(operationId, operation);
    
    return operationId;
  }

  async completeOperation(operationId: string, result: {
    success: boolean;
    output: any;
    cost: number;
    learnings?: string[];
    errors?: string[];
  }): Promise<void> {
    const activeOps = (this as any).activeOperations || new Map();
    const operation = activeOps.get(operationId);
    
    if (!operation) {
      console.log(`⚠️  Operation ${operationId} not found in tracking`);
      return;
    }

    const endTime = Date.now();
    const currentResources = await this.getCurrentResourceUsage();
    
    const feedback: FeedbackLoop = {
      id: operationId,
      operation: operation.name,
      input: operation.input,
      output: result.output,
      success: result.success,
      cost: result.cost,
      timeMs: endTime - operation.startTime,
      memoryUsageMB: currentResources.memoryUsageMB - operation.startMemory,
      diskUsageMB: currentResources.diskUsageMB - operation.startDisk,
      timestamp: endTime,
      learnings: result.learnings || [],
      improvements: this.extractImprovements(result)
    };

    this.feedbackLoops.push(feedback);
    activeOps.delete(operationId);

    // Update strategy patterns
    await this.updateStrategyPatterns(feedback);
    
    // Check for resource warnings
    await this.checkResourceLimits(currentResources);
    
    // Save feedback
    await this.saveFeedbackData();
    
    console.log(`✅ Operation completed: ${operation.name}`);
    console.log(`   Success: ${result.success}`);
    console.log(`   Cost: $${result.cost.toFixed(4)}`);
    console.log(`   Time: ${feedback.timeMs}ms`);
    console.log(`   Memory: ${feedback.memoryUsageMB.toFixed(1)}MB`);
    
    if (feedback.learnings.length > 0) {
      console.log(`   💡 Learnings: ${feedback.learnings.join(', ')}`);
    }
  }

  private extractImprovements(result: any): string[] {
    const improvements = [];
    
    if (result.cost > 0.1) {
      improvements.push('High cost operation - consider free alternatives');
    }
    
    if (result.errors && result.errors.length > 0) {
      improvements.push('Error handling needs improvement');
      improvements.push('Add retry logic for failed operations');
    }
    
    if (!result.success) {
      improvements.push('Operation failed - add better error recovery');
    }
    
    return improvements;
  }

  private async updateStrategyPatterns(feedback: FeedbackLoop): Promise<void> {
    const patternKey = this.extractPatternKey(feedback);
    const existing = this.strategyPatterns.get(patternKey);
    
    if (existing) {
      // Update existing pattern
      existing.successCount += feedback.success ? 1 : 0;
      existing.failureCount += feedback.success ? 0 : 1;
      existing.totalCost += feedback.cost;
      existing.avgTime = (existing.avgTime + feedback.timeMs) / 2;
      existing.reliability = existing.successCount / (existing.successCount + existing.failureCount);
      existing.lastUsed = feedback.timestamp;
      existing.improvements.push(...feedback.improvements);
      
      // Extract anti-patterns from failures
      if (!feedback.success) {
        existing.antiPatterns.push(`Failed: ${feedback.operation} - ${JSON.stringify(feedback.input).substring(0, 50)}`);
      }
    } else {
      // Create new pattern
      const newPattern: StrategyPattern = {
        pattern: patternKey,
        context: feedback.operation,
        successCount: feedback.success ? 1 : 0,
        failureCount: feedback.success ? 0 : 1,
        totalCost: feedback.cost,
        avgTime: feedback.timeMs,
        reliability: feedback.success ? 1.0 : 0.0,
        lastUsed: feedback.timestamp,
        improvements: [...feedback.improvements],
        antiPatterns: feedback.success ? [] : [`Failed: ${feedback.operation}`]
      };
      
      this.strategyPatterns.set(patternKey, newPattern);
    }
  }

  private extractPatternKey(feedback: FeedbackLoop): string {
    // Create a pattern key based on operation type and characteristics
    let key = feedback.operation;
    
    if (feedback.cost === 0) key += '_free';
    else if (feedback.cost < 0.01) key += '_cheap';
    else key += '_expensive';
    
    if (feedback.timeMs < 1000) key += '_fast';
    else if (feedback.timeMs < 10000) key += '_medium';
    else key += '_slow';
    
    return key;
  }

  // Resource Monitoring
  private startResourceMonitoring(): void {
    if (this.isMonitoring) return;
    
    this.isMonitoring = true;
    console.log('📊 Starting resource monitoring...');
    
    // Check resources every 30 seconds
    setInterval(async () => {
      await this.monitorResources();
    }, 30000);
    
    // Initial check
    this.monitorResources();
  }

  private async monitorResources(): Promise<void> {
    try {
      this.resourceMonitor = await this.getCurrentResourceUsage();
      this.resourceMonitor.lastCheck = Date.now();
      
      // Check for warnings
      await this.checkResourceLimits(this.resourceMonitor);
      
      // Auto-cleanup if needed
      if (this.resourceMonitor.diskUsageMB > this.resourceMonitor.limits.maxDiskMB * 0.8) {
        await this.performAutoCleanup();
      }
      
    } catch (error) {
      console.log(`⚠️  Resource monitoring error: ${error.message}`);
    }
  }

  private async getCurrentResourceUsage(): Promise<ResourceMonitor> {
    const current = { ...this.resourceMonitor };
    
    try {
      // Check disk usage of .continuum directory
      const continuum Dir = path.join(this.projectRoot, '.continuum');
      if (fs.existsSync(continuumDir)) {
        const { stdout } = await execAsync(`du -sm "${continuumDir}"`);
        current.diskUsageMB = parseInt(stdout.split('\t')[0]) || 0;
      }
      
      // Check memory usage (simplified - would use proper memory tracking in production)
      const memInfo = process.memoryUsage();
      current.memoryUsageMB = memInfo.heapUsed / 1024 / 1024;
      
      // Count open files in project
      try {
        const { stdout: openFiles } = await execAsync(`find "${this.projectRoot}" -type f | wc -l`);
        current.openFiles = parseInt(openFiles.trim()) || 0;
      } catch {
        current.openFiles = 0;
      }
      
    } catch (error) {
      console.log(`⚠️  Error getting resource usage: ${error.message}`);
    }
    
    return current;
  }

  private async checkResourceLimits(resources: ResourceMonitor): Promise<void> {
    resources.warnings = [];
    
    if (resources.diskUsageMB > resources.limits.maxDiskMB) {
      resources.warnings.push(`Disk usage (${resources.diskUsageMB}MB) exceeds limit (${resources.limits.maxDiskMB}MB)`);
      await this.performAutoCleanup();
    }
    
    if (resources.memoryUsageMB > resources.limits.maxMemoryMB) {
      resources.warnings.push(`Memory usage (${resources.memoryUsageMB.toFixed(1)}MB) exceeds limit (${resources.limits.maxMemoryMB}MB)`);
    }
    
    if (resources.costSpent > resources.limits.maxCostDaily) {
      resources.warnings.push(`Daily cost ($${resources.costSpent.toFixed(2)}) exceeds limit ($${resources.limits.maxCostDaily})`);
    }
    
    if (resources.warnings.length > 0) {
      console.log('⚠️  RESOURCE WARNINGS:');
      resources.warnings.forEach(warning => console.log(`   - ${warning}`));
    }
  }

  private async performAutoCleanup(): Promise<void> {
    console.log('🧹 Performing auto-cleanup to reduce resource usage...');
    
    try {
      // Clean old feedback loops (keep only last 1000)
      if (this.feedbackLoops.length > 1000) {
        this.feedbackLoops = this.feedbackLoops.slice(-1000);
        console.log('   🗑️  Cleaned old feedback loops');
      }
      
      // Clean old strategy patterns (remove unused ones)
      const cutoffTime = Date.now() - (30 * 24 * 60 * 60 * 1000); // 30 days
      let cleaned = 0;
      for (const [key, pattern] of this.strategyPatterns.entries()) {
        if (pattern.lastUsed < cutoffTime && pattern.reliability < 0.3) {
          this.strategyPatterns.delete(key);
          cleaned++;
        }
      }
      if (cleaned > 0) {
        console.log(`   🗑️  Cleaned ${cleaned} unreliable strategy patterns`);
      }
      
      // Clean temporary files
      const tempDir = path.join(this.projectRoot, '.continuum', 'temp');
      if (fs.existsSync(tempDir)) {
        await execAsync(`rm -rf "${tempDir}"/*`);
        console.log('   🗑️  Cleaned temporary files');
      }
      
      // Save cleaned data
      await this.saveFeedbackData();
      
      console.log('✅ Auto-cleanup completed');
      
    } catch (error) {
      console.log(`❌ Auto-cleanup failed: ${error.message}`);
    }
  }

  // Strategy Query System
  async queryStrategy(operation: string, context: any = {}): Promise<{
    recommended: StrategyPattern | null;
    alternatives: StrategyPattern[];
    warnings: string[];
    costsEstimate: number;
  }> {
    console.log(`🔍 Querying strategy for operation: ${operation}`);
    
    // Find relevant patterns
    const relevantPatterns = Array.from(this.strategyPatterns.values())
      .filter(pattern => 
        pattern.context.toLowerCase().includes(operation.toLowerCase()) ||
        operation.toLowerCase().includes(pattern.context.toLowerCase())
      )
      .sort((a, b) => b.reliability - a.reliability);
    
    const warnings = [];
    
    // Find best pattern
    let recommended = relevantPatterns[0] || null;
    
    if (recommended) {
      // Check for cost warnings
      if (recommended.totalCost / Math.max(recommended.successCount, 1) > 0.05) {
        warnings.push('Recommended strategy has high cost - consider alternatives');
      }
      
      // Check for reliability warnings
      if (recommended.reliability < 0.7) {
        warnings.push('Recommended strategy has low reliability - proceed with caution');
      }
      
      // Check anti-patterns
      if (recommended.antiPatterns.length > 0) {
        warnings.push(`Known failure patterns: ${recommended.antiPatterns.slice(0, 2).join(', ')}`);
      }
    } else {
      warnings.push('No existing strategy found for this operation');
      warnings.push('Consider starting with a free, low-risk approach');
    }
    
    const alternatives = relevantPatterns.slice(1, 4); // Top 3 alternatives
    const costEstimate = recommended ? 
      recommended.totalCost / Math.max(recommended.successCount, 1) : 0.01;
    
    console.log(`📊 Strategy query results:`);
    console.log(`   Recommended: ${recommended?.pattern || 'None'}`);
    console.log(`   Reliability: ${recommended?.reliability ? (recommended.reliability * 100).toFixed(1) + '%' : 'N/A'}`);
    console.log(`   Cost estimate: $${costEstimate.toFixed(4)}`);
    console.log(`   Alternatives: ${alternatives.length}`);
    console.log(`   Warnings: ${warnings.length}`);
    
    return {
      recommended,
      alternatives,
      warnings,
      costsEstimate: costEstimate
    };
  }

  async learnFromFailure(operation: string, failure: {
    error: string;
    cost: number;
    context: any;
  }): Promise<void> {
    console.log(`🚨 Learning from failure: ${operation}`);
    
    // Record failure pattern
    const antiPattern = `${operation}_failure: ${failure.error}`;
    const existingPattern = this.strategyPatterns.get(operation);
    
    if (existingPattern) {
      existingPattern.antiPatterns.push(antiPattern);
      existingPattern.failureCount++;
      existingPattern.reliability = existingPattern.successCount / 
        (existingPattern.successCount + existingPattern.failureCount);
    } else {
      // Create new pattern focused on avoiding this failure
      this.strategyPatterns.set(operation, {
        pattern: operation,
        context: operation,
        successCount: 0,
        failureCount: 1,
        totalCost: failure.cost,
        avgTime: 0,
        reliability: 0.0,
        lastUsed: Date.now(),
        improvements: [`Avoid: ${failure.error}`],
        antiPatterns: [antiPattern]
      });
    }
    
    // Store in memory for other AIs to learn from
    await this.memory.storeStrategy({
      id: `failure_${Date.now()}`,
      projectType: 'self-improvement',
      strategy: {
        taskDelegation: {},
        costOptimization: ['Avoid expensive operations that fail'],
        successfulPatterns: [],
        failurePatterns: [antiPattern]
      },
      performance: {
        totalCost: failure.cost,
        successRate: 0.0,
        completionTime: 0,
        userSatisfaction: 0.0
      },
      timestamp: Date.now(),
      sessionId: `failure_learning_${Date.now()}`,
      aiAgentsUsed: ['self-improvement-coordinator'],
      tags: ['failure', 'learning', operation]
    });
    
    console.log(`💡 Failure pattern recorded: ${antiPattern}`);
    await this.saveFeedbackData();
  }

  // System Improvement Recommendations
  async generateImprovementRecommendations(): Promise<{
    highPriority: string[];
    mediumPriority: string[];
    lowPriority: string[];
    resourceOptimizations: string[];
    costOptimizations: string[];
  }> {
    console.log('🎯 Generating improvement recommendations...');
    
    const highPriority = [];
    const mediumPriority = [];
    const lowPriority = [];
    const resourceOptimizations = [];
    const costOptimizations = [];
    
    // Analyze failure patterns
    const highFailureOps = Array.from(this.strategyPatterns.values())
      .filter(p => p.reliability < 0.5 && p.failureCount > 2);
    
    highFailureOps.forEach(op => {
      highPriority.push(`Fix high-failure operation: ${op.pattern} (${(op.reliability * 100).toFixed(1)}% success)`);
    });
    
    // Analyze cost patterns
    const expensiveOps = Array.from(this.strategyPatterns.values())
      .filter(p => (p.totalCost / Math.max(p.successCount, 1)) > 0.1);
    
    expensiveOps.forEach(op => {
      const avgCost = p.totalCost / Math.max(op.successCount, 1);
      costOptimizations.push(`Optimize expensive operation: ${op.pattern} ($${avgCost.toFixed(3)} avg)`);
    });
    
    // Resource usage recommendations
    if (this.resourceMonitor.diskUsageMB > this.resourceMonitor.limits.maxDiskMB * 0.7) {
      resourceOptimizations.push('Disk usage approaching limit - implement more aggressive cleanup');
    }
    
    if (this.feedbackLoops.length > 5000) {
      resourceOptimizations.push('Large feedback loop history - implement data compression');
    }
    
    // Performance recommendations
    const slowOps = Array.from(this.strategyPatterns.values())
      .filter(p => p.avgTime > 10000); // > 10 seconds
    
    slowOps.forEach(op => {
      mediumPriority.push(`Optimize slow operation: ${op.pattern} (${(op.avgTime/1000).toFixed(1)}s avg)`);
    });
    
    // Underutilized patterns
    const underutilized = Array.from(this.strategyPatterns.values())
      .filter(p => p.reliability > 0.8 && p.successCount < 5);
    
    underutilized.forEach(op => {
      lowPriority.push(`Promote reliable operation: ${op.pattern} (${(op.reliability * 100).toFixed(1)}% success, underused)`);
    });
    
    console.log(`📊 Generated recommendations:`);
    console.log(`   High priority: ${highPriority.length}`);
    console.log(`   Medium priority: ${mediumPriority.length}`);
    console.log(`   Low priority: ${lowPriority.length}`);
    console.log(`   Resource optimizations: ${resourceOptimizations.length}`);
    console.log(`   Cost optimizations: ${costOptimizations.length}`);
    
    return {
      highPriority,
      mediumPriority,
      lowPriority,
      resourceOptimizations,
      costOptimizations
    };
  }

  // Data Persistence
  private async saveFeedbackData(): Promise<void> {
    try {
      const continuumDir = path.join(this.projectRoot, '.continuum');
      if (!fs.existsSync(continuumDir)) {
        fs.mkdirSync(continuumDir, { recursive: true });
      }
      
      // Save strategy patterns
      const patternsPath = path.join(continuumDir, 'improvement-patterns.json');
      const patterns = Array.from(this.strategyPatterns.values());
      fs.writeFileSync(patternsPath, JSON.stringify(patterns, null, 2));
      
      // Save recent feedback loops (last 1000)
      const feedbackPath = path.join(continuumDir, 'feedback-loops.json');
      const recentFeedback = this.feedbackLoops.slice(-1000);
      fs.writeFileSync(feedbackPath, JSON.stringify(recentFeedback, null, 2));
      
      // Save resource monitor state
      const resourcePath = path.join(continuumDir, 'resource-monitor.json');
      fs.writeFileSync(resourcePath, JSON.stringify(this.resourceMonitor, null, 2));
      
    } catch (error) {
      console.log(`⚠️  Error saving feedback data: ${error.message}`);
    }
  }

  // Public Interface
  async getSystemHealth(): Promise<{
    status: 'healthy' | 'warning' | 'critical';
    resources: ResourceMonitor;
    patterns: number;
    feedbackLoops: number;
    recommendations: string[];
  }> {
    const recommendations = await this.generateImprovementRecommendations();
    const allRecommendations = [
      ...recommendations.highPriority,
      ...recommendations.resourceOptimizations
    ];
    
    let status: 'healthy' | 'warning' | 'critical' = 'healthy';
    
    if (this.resourceMonitor.warnings.length > 0) {
      status = 'warning';
    }
    
    if (recommendations.highPriority.length > 0 || 
        this.resourceMonitor.diskUsageMB > this.resourceMonitor.limits.maxDiskMB ||
        this.resourceMonitor.costSpent > this.resourceMonitor.limits.maxCostDaily) {
      status = 'critical';
    }
    
    return {
      status,
      resources: this.resourceMonitor,
      patterns: this.strategyPatterns.size,
      feedbackLoops: this.feedbackLoops.length,
      recommendations: allRecommendations.slice(0, 5) // Top 5 recommendations
    };
  }
}

// Export singleton for use across the system
export const selfImprovementCoordinator = new SelfImprovementCoordinator(process.cwd());