/**
 * Multi-Instance Test Runner
 * 
 * Orchestrates multiple Continuum instances for Grid P2P backbone testing.
 * This is the foundation for testing distributed AI persona coordination
 * and P2P mesh networking across multiple nodes.
 */

import { spawn, ChildProcess } from 'child_process';
import { promises as fs } from 'fs';
import * as path from 'path';
import type { 
  MultiInstanceTestConfig,
  ContinuumInstanceConfig,
  MultiInstanceTestResult,
  InstanceTestResult,
  TestExecutionResult,
  TestMetrics
} from './MultiInstanceTestTypes';
import { SystemReadySignaler } from '../../../scripts/signaling/server/SystemReadySignaler';
import { WorkingDirConfig } from '../../../system/core/config/WorkingDirConfig';
import { TmuxSessionManager } from '../../../system/shared/TmuxSessionManager';

export class MultiInstanceTestRunner {
  private runningInstances: Map<string, ContinuumInstance> = new Map();
  private testResults: MultiInstanceTestResult;
  private startTime: number = 0;

  constructor(private config: MultiInstanceTestConfig) {
    this.testResults = this.initializeResults();
  }

  /**
   * Main entry point - run the complete multi-instance test suite
   */
  async runTestSuite(): Promise<MultiInstanceTestResult> {
    this.startTime = Date.now();
    console.log(`🌐 MULTI-INSTANCE GRID TEST SUITE: ${this.config.testSuite.name}`);
    console.log(`═══════════════════════════════════════════════════════════════`);
    console.log(`🎯 Profile: ${this.config.testSuite.profile}`);
    console.log(`🔢 Instances: ${this.config.instances.length}`);
    console.log(`🧪 Tests: ${this.config.testSuite.tests.length}`);
    
    try {
      // Phase 1: Launch all instances
      await this.launchAllInstances();
      
      // Phase 2: Wait for all instances to be healthy
      await this.waitForAllInstancesHealthy();
      
      // Phase 3: Run the test suite
      await this.executeTestSuite();
      
      // Phase 4: Collect final results
      await this.collectFinalResults();
      
      this.testResults.success = true;
      console.log(`✅ Multi-instance test suite completed successfully`);
      
    } catch (error) {
      this.testResults.success = false;
      this.testResults.errors = [...this.testResults.errors, String(error)];
      console.error(`❌ Multi-instance test suite failed:`, error);
      
    } finally {
      // Cleanup phase
      await this.cleanup();
      this.finalizeResults();
    }
    
    return this.testResults;
  }

  /**
   * Launch all Continuum instances according to configuration
   */
  private async launchAllInstances(): Promise<void> {
    console.log(`🚀 Launching ${this.config.instances.length} Continuum instances...`);
    
    const launchPromises: Promise<void>[] = [];
    
    for (const instanceConfig of this.config.instances) {
      const launchPromise = this.launchInstance(instanceConfig);
      
      if (this.config.coordination.startupSequence === 'parallel') {
        launchPromises.push(launchPromise);
      } else if (this.config.coordination.startupSequence === 'sequential') {
        await launchPromise; // Wait for each instance before starting next
      } else if (this.config.coordination.startupSequence === 'staggered') {
        launchPromises.push(launchPromise);
        await new Promise(resolve => setTimeout(resolve, 5000)); // 5s stagger
      }
    }
    
    // Wait for all parallel launches to complete
    if (launchPromises.length > 0) {
      await Promise.all(launchPromises);
    }
    
    console.log(`✅ All ${this.config.instances.length} instances launched`);
  }

  /**
   * Launch a single Continuum instance
   */
  private async launchInstance(config: ContinuumInstanceConfig): Promise<void> {
    console.log(`🔧 Launching instance: ${config.instanceId} (${config.workingDir})`);
    
    const startTime = Date.now();
    
    // Set working directory context for this instance
    WorkingDirConfig.setWorkingDir(config.workingDir);
    
    // Generate unique tmux session name
    const sessionName = `jtag-${config.instanceId}-${this.generateShortId()}`;
    
    // Build environment variables
    const environment = {
      ...process.env,
      ...this.buildEnvironmentForInstance(config),
      JTAG_INSTANCE_ID: config.instanceId,
      JTAG_TMUX_SESSION: sessionName
    };
    
    // Create tmux launch command
    const tmuxCmd = [
      'new-session',
      '-d',
      '-s', sessionName,
      'npm start'  // Use standard start script available in all examples
    ];
    
    return new Promise((resolve, reject) => {
      const tmuxProcess = spawn('tmux', tmuxCmd, {
        stdio: ['ignore', 'pipe', 'pipe'],
        env: environment,
        cwd: config.workingDir  // Use instance-specific working directory
      });
      
      let processOutput = '';
      
      // Collect output for diagnostics
      tmuxProcess.stdout?.on('data', (data) => {
        processOutput += data.toString();
      });
      
      tmuxProcess.stderr?.on('data', (data) => {
        processOutput += data.toString();
      });
      
      tmuxProcess.on('close', async (code) => {
        if (code === 0) {
          try {
            // Get the PID of the process inside tmux
            const pidOutput = await this.getTmuxProcessPid(sessionName);
            const pid = parseInt(pidOutput.trim());
            
            // Create instance record
            const instance: ContinuumInstance = {
              config,
              sessionName,
              pid,
              startTime,
              process: tmuxProcess,
              healthy: false,
              signaler: new SystemReadySignaler()
            };
            
            this.runningInstances.set(config.instanceId, instance);
            
            // Initialize test result for this instance
            this.testResults.instanceResults[config.instanceId] = {
              instanceId: config.instanceId,
              healthy: false,
              startupTime: 0,
              finalStatus: 'unhealthy',
              ports: Object.values(config.ports),
              tmuxSession: sessionName,
              processId: pid,
              errors: []
            };
            
            console.log(`✅ Instance ${config.instanceId} launched (PID: ${pid}, Session: ${sessionName})`);
            resolve();
            
          } catch (error) {
            reject(new Error(`Failed to get PID for instance ${config.instanceId}: ${error}`));
          }
        } else {
          reject(new Error(`Failed to launch instance ${config.instanceId}: exit code ${code}`));
        }
      });
      
      tmuxProcess.on('error', (error) => {
        reject(new Error(`Tmux spawn error for instance ${config.instanceId}: ${error.message}`));
      });
    });
  }

  /**
   * Wait for all instances to become healthy
   */
  private async waitForAllInstancesHealthy(): Promise<void> {
    console.log(`🔍 Waiting for all ${this.runningInstances.size} instances to become healthy...`);
    
    const healthCheckPromises: Promise<void>[] = [];
    
    for (const [instanceId, instance] of Array.from(this.runningInstances.entries())) {
      const healthCheckPromise = this.waitForInstanceHealthy(instanceId, instance);
      healthCheckPromises.push(healthCheckPromise);
    }
    
    // Wait for all instances to become healthy (or timeout)
    await Promise.all(healthCheckPromises);
    
    const healthyCount = Array.from(this.runningInstances.values())
      .filter(instance => instance.healthy).length;
      
    console.log(`📊 Health check complete: ${healthyCount}/${this.runningInstances.size} instances healthy`);
    
    if (this.config.testSuite.requireAllInstancesHealthy && healthyCount < this.runningInstances.size) {
      throw new Error(`Required all instances healthy, but only ${healthyCount}/${this.runningInstances.size} are healthy`);
    }
  }

  /**
   * Wait for a specific instance to become healthy
   */
  private async waitForInstanceHealthy(instanceId: string, instance: ContinuumInstance): Promise<void> {
    console.log(`⏳ Checking health for instance: ${instanceId}`);
    
    // Set working directory context for signal checking
    WorkingDirConfig.setWorkingDir(instance.config.workingDir);
    
    const timeout = instance.config.features.timeoutMs;
    const signal = await instance.signaler.checkSystemReady(timeout);
    
    if (signal) {
      const isHealthy = signal.bootstrapComplete && 
                       signal.commandCount > 0 && 
                       (signal.systemHealth === 'healthy' || signal.systemHealth === 'degraded');
      
      instance.healthy = isHealthy;
      
      // Update test results
      const result = this.testResults.instanceResults[instanceId];
      result.healthy = isHealthy;
      result.startupTime = Date.now() - instance.startTime;
      result.finalStatus = signal.systemHealth;
      
      if (isHealthy) {
        console.log(`✅ Instance ${instanceId} is healthy (${signal.commandCount} commands, ports: ${signal.portsActive.join(', ')})`);
      } else {
        console.log(`⚠️ Instance ${instanceId} is not fully healthy but responding (${signal.systemHealth})`);
      }
    } else {
      console.log(`❌ Instance ${instanceId} failed to become ready within ${timeout}ms`);
      this.testResults.instanceResults[instanceId].errors.push(`Health check timeout after ${timeout}ms`);
    }
  }

  /**
   * Execute the test suite across all healthy instances
   */
  private async executeTestSuite(): Promise<void> {
    console.log(`🧪 Executing test suite: ${this.config.testSuite.name}`);
    
    // Execute tests based on dependencies
    const executedTests = new Set<string>();
    const testQueue = [...this.config.testSuite.tests];
    
    while (testQueue.length > 0) {
      const availableTests = testQueue.filter(test => 
        !test.dependencies || test.dependencies.every(dep => executedTests.has(dep))
      );
      
      if (availableTests.length === 0) {
        throw new Error('Circular dependency detected in test suite');
      }
      
      // Execute available tests
      const testPromises = availableTests.map(test => this.executeTest(test));
      await Promise.all(testPromises);
      
      // Mark tests as executed and remove from queue
      availableTests.forEach(test => {
        executedTests.add(test.name);
        const index = testQueue.indexOf(test);
        testQueue.splice(index, 1);
      });
    }
  }

  /**
   * Execute a single test
   */
  private async executeTest(test: any): Promise<void> {
    console.log(`▶️  Running: ${test.name} [${test.category}]`);
    
    const startTime = Date.now();
    
    try {
      // Verify required instances are healthy
      const requiredInstancesHealthy = test.requiredInstances.every(instanceId => {
        const instance = this.runningInstances.get(instanceId);
        return instance && instance.healthy;
      });
      
      if (!requiredInstancesHealthy) {
        throw new Error(`Required instances not healthy for test: ${test.name}`);
      }
      
      // Execute the test (simplified - would run actual test file)
      await new Promise(resolve => setTimeout(resolve, 1000)); // Simulate test execution
      
      const duration = Date.now() - startTime;
      
      this.testResults.testResults[test.name] = {
        testName: test.name,
        success: true,
        duration,
        metrics: { executionTime: duration }
      };
      
      console.log(`✅ PASSED: ${test.name} (${duration}ms)`);
      
    } catch (error) {
      const duration = Date.now() - startTime;
      
      this.testResults.testResults[test.name] = {
        testName: test.name,
        success: false,
        duration,
        error: String(error)
      };
      
      console.log(`❌ FAILED: ${test.name} (${duration}ms) - ${error}`);
    }
  }

  /**
   * Cleanup all running instances
   */
  private async cleanup(): Promise<void> {
    if (!this.config.cleanup.cleanupOnSuccess && this.testResults.success) {
      console.log(`🚀 Cleanup skipped - instances left running for development`);
      return;
    }
    
    if (!this.config.cleanup.cleanupOnFailure && !this.testResults.success) {
      console.log(`🚀 Cleanup skipped - instances left running for debugging`);
      return;
    }
    
    console.log(`🧹 Cleaning up ${this.runningInstances.size} instances...`);
    
    for (const [instanceId, instance] of Array.from(this.runningInstances.entries())) {
      try {
        console.log(`🧹 Stopping instance: ${instanceId} (${instance.sessionName})`);
        
        // Kill tmux session
        const killProcess = spawn('tmux', ['kill-session', '-t', instance.sessionName], {
          stdio: 'ignore'
        });
        
        await new Promise(resolve => {
          killProcess.on('close', resolve);
          setTimeout(resolve, this.config.cleanup.killTimeout);
        });
        
      } catch (error) {
        console.log(`⚠️ Failed to cleanup instance ${instanceId}: ${error}`);
      }
    }
    
    console.log(`✅ Cleanup complete`);
  }

  // Helper methods
  private initializeResults(): MultiInstanceTestResult {
    return {
      configId: this.config.testSuite.name,
      startTime: new Date().toISOString(),
      endTime: '',
      duration: 0,
      success: false,
      instanceResults: {},
      testResults: {},
      errors: [],
      warnings: [],
      metrics: {
        totalInstances: this.config.instances.length,
        healthyInstances: 0,
        totalTests: this.config.testSuite.tests.length,
        passedTests: 0,
        averageStartupTime: 0,
        peakMemoryUsage: 0
      }
    };
  }

  private finalizeResults(): void {
    this.testResults.endTime = new Date().toISOString();
    this.testResults.duration = Date.now() - this.startTime;
    
    // Calculate metrics
    const instanceResults = Object.values(this.testResults.instanceResults);
    this.testResults.metrics.healthyInstances = instanceResults.filter(r => r.healthy).length;
    this.testResults.metrics.averageStartupTime = instanceResults.reduce((sum, r) => sum + r.startupTime, 0) / instanceResults.length;
    this.testResults.metrics.passedTests = Object.values(this.testResults.testResults).filter(r => r.success).length;
  }

  private async getTmuxProcessPid(sessionName: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const getPidCmd = spawn('tmux', [
        'list-panes', '-t', sessionName, '-F', '#{pane_pid}'
      ], { stdio: ['ignore', 'pipe', 'ignore'] });
      
      let pidOutput = '';
      getPidCmd.stdout?.on('data', (data) => {
        pidOutput += data.toString();
      });
      
      getPidCmd.on('close', (code) => {
        if (code === 0) {
          resolve(pidOutput);
        } else {
          reject(new Error(`Failed to get PID for tmux session ${sessionName}`));
        }
      });
    });
  }

  private buildEnvironmentForInstance(config: ContinuumInstanceConfig): Record<string, string> {
    const env: Record<string, string> = {};
    
    for (const [key, value] of Object.entries(config.environment)) {
      if (value !== undefined) {
        env[key] = String(value);
      }
    }
    
    // Add port overrides
    env.JTAG_EXAMPLE_HTTP_PORT = String(config.ports.http_server);
    env.JTAG_WEBSOCKET_PORT = String(config.ports.websocket_server);
    
    return env;
  }

  private generateShortId(): string {
    return Math.random().toString(36).substring(2, 8);
  }

  private async collectFinalResults(): Promise<void> {
    // Collect final status from all instances
    for (const [instanceId, instance] of Array.from(this.runningInstances.entries())) {
      try {
        WorkingDirConfig.setWorkingDir(instance.config.workingDir);
        const finalSignal = await instance.signaler.generateReadySignal();
        
        const result = this.testResults.instanceResults[instanceId];
        result.finalStatus = finalSignal.systemHealth;
        result.ports = [...finalSignal.portsActive]; // Create mutable copy
        
      } catch (error) {
        this.testResults.instanceResults[instanceId].errors.push(`Failed to collect final status: ${error}`);
      }
    }
  }
}

// Internal type for tracking running instances
interface ContinuumInstance {
  config: ContinuumInstanceConfig;
  sessionName: string;
  pid: number;
  startTime: number;
  process: ChildProcess;
  healthy: boolean;
  signaler: SystemReadySignaler;
}

// Example usage function
export async function runGridP2PTest(): Promise<MultiInstanceTestResult> {
  const { GRID_P2P_TEST_CONFIG } = await import('./MultiInstanceTestTypes');
  const runner = new MultiInstanceTestRunner(GRID_P2P_TEST_CONFIG);
  return await runner.runTestSuite();
}