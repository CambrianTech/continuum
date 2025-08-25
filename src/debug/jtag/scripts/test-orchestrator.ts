#!/usr/bin/env tsx
/**
 * Test Orchestrator - Replaces hacky bash timeout with proper signal handling
 * 
 * Key improvements:
 * - Proper CTRL+C signal forwarding to child processes
 * - Type-safe test phase definitions with optional error recovery
 * - Granular progress tracking instead of monolithic 300s timeout  
 * - Cleaner exit handling with resource cleanup
 */

import { spawn, ChildProcess } from 'child_process';
import { EventEmitter } from 'events';

interface TestPhase {
  readonly name: string;
  readonly command: string;
  readonly args?: string[];
  readonly optional?: boolean;
  readonly timeoutMs?: number; // Only for genuine external dependencies
}

interface TestResult {
  readonly phase: string;
  readonly success: boolean;
  readonly duration: number;
  readonly error?: string;
}

class TestOrchestrator extends EventEmitter {
  private currentProcess: ChildProcess | null = null;
  private isShuttingDown = false;
  private results: TestResult[] = [];

  constructor() {
    super();
    this.setupSignalHandling();
  }

  private setupSignalHandling(): void {
    // Proper signal forwarding - the key fix for CTRL+C issues
    const signals = ['SIGINT', 'SIGTERM', 'SIGHUP'] as const;
    
    signals.forEach((signal) => {
      process.on(signal, () => {
        console.log(`\n🛑 Received ${signal} - forwarding to current process and exiting gracefully`);
        this.shutdown(signal);
      });
    });

    // Prevent uncaught exceptions from hanging
    process.on('uncaughtException', (error) => {
      console.error('💥 Uncaught exception:', error.message);
      this.shutdown('SIGTERM');
    });

    process.on('unhandledRejection', (error) => {
      console.error('💥 Unhandled rejection:', error);
      this.shutdown('SIGTERM');
    });
  }

  private async shutdown(signal?: string): Promise<void> {
    if (this.isShuttingDown) return;
    this.isShuttingDown = true;

    console.log('🧹 Cleaning up test orchestrator...');

    try {
      // Forward signal to current child process with proper error handling
      if (this.currentProcess && !this.currentProcess.killed) {
        console.log('📤 Forwarding signal to child process...');
        
        try {
          this.currentProcess.kill(signal || 'SIGTERM');
          
          // Wait for child process to exit gracefully
          const exitPromise = new Promise<void>((resolve) => {
            this.currentProcess!.on('exit', () => resolve());
            this.currentProcess!.on('error', () => resolve()); // Handle error case too
          });
          
          // Give child process 2 seconds to clean up, then force kill
          const timeoutPromise = new Promise<void>((resolve) => {
            setTimeout(() => {
              if (this.currentProcess && !this.currentProcess.killed) {
                console.log('⚡ Force killing unresponsive child process');
                this.currentProcess.kill('SIGKILL');
              }
              resolve();
            }, 2000);
          });
          
          await Promise.race([exitPromise, timeoutPromise]);
          
        } catch (killError) {
          console.error('❌ Error killing child process:', killError instanceof Error ? killError.message : killError);
        }
      }

      // Clean up orchestrator state
      this.currentProcess = null;
      this.removeAllListeners();

    } catch (cleanupError) {
      console.error('❌ Error during cleanup:', cleanupError instanceof Error ? cleanupError.message : cleanupError);
    } finally {
      // Always print results and exit, even if cleanup failed
      this.printResultsSummary();
      
      const exitCode = signal === 'SIGINT' ? 130 : (this.results.some(r => !r.success) ? 1 : 0);
      process.exit(exitCode);
    }
  }

  private async runPhase(phase: TestPhase): Promise<TestResult> {
    const startTime = Date.now();
    
    return new Promise((resolve) => {
      console.log(`\n🚀 ${phase.name}`);
      console.log(`📋 Command: ${phase.command} ${phase.args?.join(' ') || ''}`);

      const child = spawn(phase.command, phase.args || [], {
        stdio: 'inherit', // Show output directly - no hiding
        cwd: process.cwd(),
        shell: true
      });

      this.currentProcess = child;

      const cleanup = (success: boolean, error?: string) => {
        try {
          const duration = Date.now() - startTime;
          this.currentProcess = null;
          
          const result: TestResult = {
            phase: phase.name,
            success,
            duration,
            error
          };
          
          this.results.push(result);
          
          if (success) {
            console.log(`✅ ${phase.name} completed in ${duration}ms`);
          } else {
            console.log(`❌ ${phase.name} failed in ${duration}ms${error ? `: ${error}` : ''}`);
          }
          
          resolve(result);
        } catch (cleanupError) {
          console.error(`💥 Error during phase cleanup: ${cleanupError instanceof Error ? cleanupError.message : cleanupError}`);
          resolve({
            phase: phase.name,
            success: false,
            duration: Date.now() - startTime,
            error: `Cleanup error: ${cleanupError instanceof Error ? cleanupError.message : cleanupError}`
          });
        }
      };

      child.on('exit', (code, signal) => {
        if (signal) {
          cleanup(false, `Terminated by signal: ${signal}`);
        } else {
          cleanup(code === 0, code !== 0 ? `Exit code: ${code}` : undefined);
        }
      });

      child.on('error', (error) => {
        cleanup(false, `Process error: ${error.message}`);
      });

      // Optional timeout for external dependencies (like browser operations)
      if (phase.timeoutMs) {
        setTimeout(() => {
          if (child && !child.killed) {
            console.log(`⏰ ${phase.name} timeout after ${phase.timeoutMs}ms - killing process`);
            child.kill('SIGTERM');
            cleanup(false, `Timeout after ${phase.timeoutMs}ms`);
          }
        }, phase.timeoutMs);
      }
    });
  }

  private printResultsSummary(): void {
    console.log('\n📊 TEST ORCHESTRATOR RESULTS:');
    console.log('================================');
    
    const passed = this.results.filter(r => r.success).length;
    const failed = this.results.filter(r => !r.success).length;
    const totalDuration = this.results.reduce((sum, r) => sum + r.duration, 0);
    
    this.results.forEach((result) => {
      const status = result.success ? '✅' : '❌';
      const duration = `${result.duration}ms`;
      console.log(`${status} ${result.phase.padEnd(30)} ${duration.padStart(8)} ${result.error || ''}`);
    });
    
    console.log('--------------------------------');
    console.log(`📋 Total: ${this.results.length} phases | ✅ Passed: ${passed} | ❌ Failed: ${failed}`);
    console.log(`⏱️ Total duration: ${totalDuration}ms (${(totalDuration/1000).toFixed(1)}s)`);
    
    if (failed === 0) {
      console.log('🎉 ALL PHASES PASSED!');
    } else {
      console.log(`💥 ${failed} phases failed - see errors above`);
    }
  }

  async runTests(): Promise<boolean> {
    console.log('🎯 JTAG TEST ORCHESTRATOR - Signal-Aware Testing');
    console.log('================================================');
    
    // Define test phases with proper typing and optional recovery
    const testPhases: TestPhase[] = [
      {
        name: 'System Ensure',
        command: 'npm',
        args: ['run', 'system:ensure'],
        optional: false
      },
      {
        name: 'Test Cleanup',
        command: './scripts/safe-test-cleanup.sh',
        optional: true // Cleanup can fail without breaking tests
      },
      {
        name: 'TypeScript Compilation Check',
        command: 'npm',
        args: ['run', 'test:compiler-check'],
        optional: false
      },
      {
        name: 'Global CLI Installation',
        command: 'npm', 
        args: ['run', 'test:global-cli'],
        optional: false
      },
      {
        name: 'Process Coordinator',
        command: 'npm',
        args: ['run', 'test:process-coordinator'], 
        optional: false
      },
      {
        name: 'Session Isolation',
        command: 'npm',
        args: ['run', 'test:session-isolation'],
        optional: false
      },
      {
        name: 'Load Testing',
        command: 'npm',
        args: ['run', 'test:load'],
        optional: false
      },
      {
        name: 'Start and Test Integration',
        command: 'npm',
        args: ['run', 'test:start-and-test'],
        optional: false,
        timeoutMs: 120000 // 2 minutes for integration tests that involve browser
      }
    ];

    // Run phases sequentially with proper error handling
    for (const phase of testPhases) {
      if (this.isShuttingDown) {
        console.log('🛑 Orchestrator shutdown requested - stopping test execution');
        break;
      }

      const result = await this.runPhase(phase);
      
      // Stop on failure unless phase is optional
      if (!result.success && !phase.optional) {
        console.error(`💥 Critical phase failed: ${phase.name}`);
        console.error('🛑 Stopping test execution due to critical failure');
        break;
      }
    }

    this.printResultsSummary();
    
    // Success if all non-optional phases passed
    const criticalFailures = this.results.filter(r => !r.success).length;
    return criticalFailures === 0;
  }
}

// Run orchestrator if called directly
async function main(): Promise<void> {
  const orchestrator = new TestOrchestrator();
  
  try {
    const success = await orchestrator.runTests();
    process.exit(success ? 0 : 1);
  } catch (error) {
    console.error('💥 Test orchestrator fatal error:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}