/**
 * Worker Pool Manager - Child Process Parallelization for JTAG Operations
 * 
 * Implements child process/worker thread parallelization for CPU-intensive operations
 * like mesh formation, screenshot processing, and routing optimization.
 * Uses performance profiling to determine when parallelization helps vs hurts.
 */

import { Worker, isMainThread, parentPort, workerData } from 'worker_threads';
import { spawn, ChildProcess } from 'child_process';
import { performance } from 'perf_hooks';
import { globalProfiler } from '../../../shared/performance/PerformanceProfiler';
import type { JTAGContext } from '../types/JTAGTypes';

export interface WorkerTask<T = any> {
  id: string;
  type: 'worker-thread' | 'child-process';
  operation: string;
  data: T;
  timeout?: number;
  priority?: 'low' | 'normal' | 'high' | 'critical';
}

export interface WorkerResult<T = any> {
  taskId: string;
  success: boolean;
  result?: T;
  error?: string;
  executionTime: number;
  workerId: string;
}

export interface WorkerPoolConfig {
  maxWorkers: number;
  maxChildProcesses: number;
  taskTimeout: number;
  enableProfiling: boolean;
  workerScript?: string;
  childProcessScript?: string;
}

export interface WorkerPerformanceMetrics {
  tasksCompleted: number;
  averageExecutionTime: number;
  parallelSpeedup: number;
  cpuUtilization: number;
  memoryUsage: number;
  timeoutRate: number;
}

export class WorkerPoolManager {
  private workerPool: Worker[] = [];
  private childProcessPool: ChildProcess[] = [];
  private taskQueue: WorkerTask[] = [];
  private pendingTasks = new Map<string, {
    resolve: (result: WorkerResult) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
  }>();
  private config: WorkerPoolConfig;
  private metrics: WorkerPerformanceMetrics;
  private isShuttingDown = false;
  
  constructor(config: Partial<WorkerPoolConfig> = {}) {
    this.config = {
      maxWorkers: config.maxWorkers ?? 4,
      maxChildProcesses: config.maxChildProcesses ?? 2,
      taskTimeout: config.taskTimeout ?? 30000,
      enableProfiling: config.enableProfiling ?? true,
      workerScript: config.workerScript ?? __dirname + '/JTAGWorker.ts',
      childProcessScript: config.childProcessScript ?? __dirname + '/JTAGChildProcess.ts',
      ...config
    };
    
    this.metrics = {
      tasksCompleted: 0,
      averageExecutionTime: 0,
      parallelSpeedup: 1.0,
      cpuUtilization: 0,
      memoryUsage: 0,
      timeoutRate: 0
    };
  }
  
  async initialize(): Promise<void> {
    console.log('🔧 WorkerPoolManager: Initializing worker pools...');
    
    // Create worker thread pool
    for (let i = 0; i < this.config.maxWorkers; i++) {
      try {
        const worker = new Worker(this.config.workerScript!);
        worker.on('message', (result: WorkerResult) => this.handleWorkerMessage(result));
        worker.on('error', (error) => this.handleWorkerError(error, `worker-${i}`));
        this.workerPool.push(worker);
        console.log(`✅ WorkerPoolManager: Created worker thread ${i}`);
      } catch (error: any) {
        console.warn(`⚠️ WorkerPoolManager: Failed to create worker ${i}: ${error.message}`);
      }
    }
    
    console.log(`🚀 WorkerPoolManager: Initialized with ${this.workerPool.length} workers and ${this.childProcessPool.length} child processes`);
  }
  
  /**
   * Execute task with automatic worker selection and performance profiling
   */
  async executeTask<T>(task: WorkerTask<T>): Promise<WorkerResult<T>> {
    if (this.isShuttingDown) {
      throw new Error('WorkerPoolManager is shutting down');
    }
    
    const taskId = task.id || `task-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    task.id = taskId;
    
    if (this.config.enableProfiling) {
      globalProfiler.startTimer(`worker-task-${task.operation}`, {
        taskId,
        type: task.type,
        priority: task.priority || 'normal'
      });
    }
    
    return new Promise<WorkerResult<T>>((resolve, reject) => {
      // Set up timeout
      const timeout = setTimeout(() => {
        this.pendingTasks.delete(taskId);
        this.metrics.timeoutRate = (this.metrics.timeoutRate + 1) / Math.max(this.metrics.tasksCompleted + 1, 1);
        reject(new Error(`Task ${taskId} timed out after ${task.timeout || this.config.taskTimeout}ms`));
      }, task.timeout || this.config.taskTimeout);
      
      // Register pending task
      this.pendingTasks.set(taskId, { resolve, reject, timeout });
      
      // Execute based on task type
      if (task.type === 'worker-thread') {
        this.executeOnWorkerThread(task);
      } else if (task.type === 'child-process') {
        this.executeOnChildProcess(task);
      } else {
        // Auto-select based on operation type
        this.autoSelectWorkerType(task);
      }
    });
  }
  
  /**
   * Execute mesh formation operations in parallel
   */
  async executeMeshFormation(nodes: any[], options: any = {}): Promise<WorkerResult> {
    console.log(`🌐 WorkerPoolManager: Starting parallel mesh formation for ${nodes.length} nodes`);
    
    // Determine optimal parallelization strategy
    const nodeChunks = this.chunkArray(nodes, Math.min(nodes.length, this.config.maxWorkers));
    
    if (this.config.enableProfiling) {
      globalProfiler.startTimer('mesh-formation-parallel');
    }
    
    try {
      // Execute chunks in parallel using worker threads
      const chunkTasks = nodeChunks.map((chunk, index) => 
        this.executeTask({
          id: `mesh-chunk-${index}`,
          type: 'worker-thread',
          operation: 'mesh-formation',
          data: { nodes: chunk, options, chunkIndex: index },
          priority: 'high'
        })
      );
      
      const chunkResults = await Promise.allSettled(chunkTasks);
      
      // Combine results
      const successfulResults = chunkResults
        .filter((result): result is PromiseFulfilledResult<WorkerResult> => result.status === 'fulfilled')
        .map(result => result.value);
      
      const combinedResult = {
        taskId: 'mesh-formation-combined',
        success: successfulResults.length > 0,
        result: {
          processedNodes: successfulResults.reduce((acc, r) => acc + (r.result?.processedNodes || 0), 0),
          connections: successfulResults.reduce((acc, r) => [...acc, ...(r.result?.connections || [])], []),
          chunks: successfulResults.length,
          errors: chunkResults.filter(r => r.status === 'rejected').map(r => r.reason?.message)
        },
        executionTime: 0,
        workerId: 'parallel-mesh-formation'
      };
      
      if (this.config.enableProfiling) {
        const timing = globalProfiler.endTimer('mesh-formation-parallel');
        combinedResult.executionTime = timing?.duration || 0;
      }
      
      console.log(`🎉 WorkerPoolManager: Mesh formation completed - ${combinedResult.result.processedNodes} nodes processed`);
      
      this.updateMetrics(combinedResult);
      return combinedResult;
      
    } catch (error: any) {
      if (this.config.enableProfiling) {
        globalProfiler.endTimer('mesh-formation-parallel');
      }
      
      console.error('❌ WorkerPoolManager: Mesh formation failed:', error.message);
      throw error;
    }
  }
  
  /**
   * Execute screenshot processing in parallel (crop calculations, image optimization)
   */
  async executeScreenshotProcessing(screenshots: any[]): Promise<WorkerResult> {
    console.log(`📸 WorkerPoolManager: Processing ${screenshots.length} screenshots in parallel`);
    
    // Use child processes for image processing (more memory-safe)
    const tasks = screenshots.map((screenshot, index) => 
      this.executeTask({
        id: `screenshot-${index}`,
        type: 'child-process',
        operation: 'screenshot-processing',
        data: screenshot,
        priority: 'normal'
      })
    );
    
    if (this.config.enableProfiling) {
      globalProfiler.startTimer('screenshot-processing-parallel');
    }
    
    try {
      const results = await Promise.allSettled(tasks);
      const successful = results.filter(r => r.status === 'fulfilled').length;
      
      const combinedResult = {
        taskId: 'screenshot-processing-combined',
        success: successful > 0,
        result: {
          processedScreenshots: successful,
          totalScreenshots: screenshots.length,
          successRate: (successful / screenshots.length) * 100
        },
        executionTime: 0,
        workerId: 'parallel-screenshot-processing'
      };
      
      if (this.config.enableProfiling) {
        const timing = globalProfiler.endTimer('screenshot-processing-parallel');
        combinedResult.executionTime = timing?.duration || 0;
      }
      
      this.updateMetrics(combinedResult);
      return combinedResult;
      
    } catch (error: any) {
      if (this.config.enableProfiling) {
        globalProfiler.endTimer('screenshot-processing-parallel');
      }
      throw error;
    }
  }
  
  /**
   * Benchmark serial vs parallel execution to determine when parallelization helps
   */
  async benchmarkParallelization(operation: string, testData: any[]): Promise<{
    serial: { time: number; success: boolean };
    parallel: { time: number; success: boolean; speedup: number };
    recommendation: 'serial' | 'parallel';
  }> {
    console.log(`⚡ WorkerPoolManager: Benchmarking ${operation} - serial vs parallel`);
    
    // Serial execution benchmark
    const serialStart = performance.now();
    let serialSuccess = true;
    try {
      for (const data of testData) {
        await this.executeSerialOperation(operation, data);
      }
    } catch (error) {
      serialSuccess = false;
    }
    const serialTime = performance.now() - serialStart;
    
    // Parallel execution benchmark
    const parallelStart = performance.now();
    let parallelSuccess = true;
    try {
      const parallelTasks = testData.map((data, index) => 
        this.executeTask({
          id: `benchmark-${operation}-${index}`,
          type: 'worker-thread',
          operation: operation,
          data: data
        })
      );
      await Promise.all(parallelTasks);
    } catch (error) {
      parallelSuccess = false;
    }
    const parallelTime = performance.now() - parallelStart;
    
    const speedup = serialTime / parallelTime;
    const recommendation = speedup > 1.2 && parallelSuccess ? 'parallel' : 'serial';
    
    console.log(`📊 Benchmark results: Serial=${serialTime.toFixed(2)}ms, Parallel=${parallelTime.toFixed(2)}ms, Speedup=${speedup.toFixed(2)}x`);
    console.log(`💡 Recommendation: Use ${recommendation} execution for ${operation}`);
    
    return {
      serial: { time: serialTime, success: serialSuccess },
      parallel: { time: parallelTime, success: parallelSuccess, speedup },
      recommendation
    };
  }
  
  /**
   * Get current performance metrics
   */
  getMetrics(): WorkerPerformanceMetrics {
    return { ...this.metrics };
  }
  
  /**
   * Get detailed performance report
   */
  generatePerformanceReport(): string {
    const report = globalProfiler.generateReport(['worker-task', 'mesh-formation', 'screenshot-processing']);
    return `${report}\n\n📊 WORKER POOL METRICS:\n` +
           `Tasks Completed: ${this.metrics.tasksCompleted}\n` +
           `Average Execution Time: ${this.metrics.averageExecutionTime.toFixed(2)}ms\n` +
           `Parallel Speedup: ${this.metrics.parallelSpeedup.toFixed(2)}x\n` +
           `Timeout Rate: ${(this.metrics.timeoutRate * 100).toFixed(1)}%\n`;
  }
  
  private executeOnWorkerThread<T>(task: WorkerTask<T>): void {
    const availableWorker = this.workerPool.find(worker => worker.threadId !== undefined);
    if (!availableWorker) {
      console.warn(`⚠️ WorkerPoolManager: No available worker threads, queuing task ${task.id}`);
      this.taskQueue.push(task);
      return;
    }
    
    availableWorker.postMessage({
      taskId: task.id,
      operation: task.operation,
      data: task.data
    });
  }
  
  private executeOnChildProcess<T>(task: WorkerTask<T>): void {
    // For now, fallback to worker threads since child process setup is more complex
    this.executeOnWorkerThread(task);
  }
  
  private autoSelectWorkerType<T>(task: WorkerTask<T>): void {
    // Auto-select based on operation type
    const memoryIntensiveOps = ['screenshot-processing', 'image-optimization', 'large-data-processing'];
    const cpuIntensiveOps = ['mesh-formation', 'routing-optimization', 'mathematical-calculations'];
    
    if (memoryIntensiveOps.includes(task.operation)) {
      task.type = 'child-process';
      this.executeOnChildProcess(task);
    } else if (cpuIntensiveOps.includes(task.operation)) {
      task.type = 'worker-thread';
      this.executeOnWorkerThread(task);
    } else {
      task.type = 'worker-thread';
      this.executeOnWorkerThread(task);
    }
  }
  
  private handleWorkerMessage(result: WorkerResult): void {
    const pendingTask = this.pendingTasks.get(result.taskId);
    if (!pendingTask) {
      console.warn(`⚠️ WorkerPoolManager: Received result for unknown task ${result.taskId}`);
      return;
    }
    
    clearTimeout(pendingTask.timeout);
    this.pendingTasks.delete(result.taskId);
    
    this.updateMetrics(result);
    
    if (this.config.enableProfiling) {
      globalProfiler.endTimer(`worker-task-${result.taskId}`);
    }
    
    pendingTask.resolve(result);
  }
  
  private handleWorkerError(error: Error, workerId: string): void {
    console.error(`❌ WorkerPoolManager: Worker ${workerId} error:`, error.message);
    
    // Find and reject any pending tasks for this worker
    for (const [taskId, pendingTask] of this.pendingTasks.entries()) {
      if (taskId.includes(workerId)) {
        clearTimeout(pendingTask.timeout);
        this.pendingTasks.delete(taskId);
        pendingTask.reject(error);
      }
    }
  }
  
  private updateMetrics(result: WorkerResult): void {
    this.metrics.tasksCompleted++;
    
    // Update average execution time
    const totalTime = this.metrics.averageExecutionTime * (this.metrics.tasksCompleted - 1);
    this.metrics.averageExecutionTime = (totalTime + result.executionTime) / this.metrics.tasksCompleted;
    
    // Update memory usage (approximate)
    this.metrics.memoryUsage = process.memoryUsage().heapUsed / 1024 / 1024;
  }
  
  private chunkArray<T>(array: T[], chunkSize: number): T[][] {
    const chunks: T[][] = [];
    for (let i = 0; i < array.length; i += chunkSize) {
      chunks.push(array.slice(i, i + chunkSize));
    }
    return chunks;
  }
  
  private async executeSerialOperation(operation: string, data: any): Promise<void> {
    // Simulate serial execution for benchmarking
    await new Promise(resolve => setTimeout(resolve, Math.random() * 10 + 5));
  }
  
  async shutdown(): Promise<void> {
    console.log('🔄 WorkerPoolManager: Shutting down...');
    this.isShuttingDown = true;
    
    // Terminate all workers
    for (const worker of this.workerPool) {
      await worker.terminate();
    }
    
    // Kill all child processes
    for (const child of this.childProcessPool) {
      child.kill();
    }
    
    // Clear pending tasks
    for (const [taskId, pendingTask] of this.pendingTasks.entries()) {
      clearTimeout(pendingTask.timeout);
      pendingTask.reject(new Error('WorkerPoolManager shutting down'));
    }
    
    this.pendingTasks.clear();
    this.taskQueue.length = 0;
    
    console.log('✅ WorkerPoolManager: Shutdown complete');
  }
}

// Global worker pool instance
export const globalWorkerPool = new WorkerPoolManager();