#!/usr/bin/env tsx
/**
 * Test Runner with Proper CTRL+C Signal Handling
 * 
 * Fixes the npm test CTRL+C issue by implementing proper signal forwarding
 * to all child processes in the test chain.
 * 
 * Also fixes working directory issues - ensures tests run with correct
 * working directory configuration to find existing JTAG systems.
 */

import { spawn, ChildProcess } from 'child_process';
import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';

interface TestPhase {
  name: string;
  command: string;
  args: string[];
  timeoutSeconds: number;
  required: boolean;
}

class TestRunner {
  private childProcesses: ChildProcess[] = [];
  private interrupted = false;

  constructor() {
    // Setup proper CTRL+C handling
    process.on('SIGINT', () => this.handleInterrupt());
    process.on('SIGTERM', () => this.handleInterrupt());
  }

  private async handleInterrupt(): Promise<void> {
    if (this.interrupted) return;
    this.interrupted = true;

    console.log('\n🛑 Test execution interrupted - cleaning up...');
    
    // Kill all child processes
    for (const child of this.childProcesses) {
      if (child && !child.killed) {
        console.log(`🔄 Terminating process ${child.pid}...`);
        child.kill('SIGINT'); // Forward SIGINT to child
        
        // Force kill after 2 seconds if graceful termination fails
        setTimeout(() => {
          if (!child.killed) {
            console.log(`💀 Force killing process ${child.pid}`);
            child.kill('SIGKILL');
          }
        }, 2000);
      }
    }

    // Give processes time to cleanup
    setTimeout(() => {
      console.log('✅ Cleanup completed');
      process.exit(130); // Standard exit code for SIGINT
    }, 500);
  }

  private async executePhase(phase: TestPhase): Promise<boolean> {
    if (this.interrupted) return false;

    return new Promise((resolve) => {
      console.log(`🔄 Running: ${phase.name}...`);
      
      // For system management commands, always run from main JTAG directory
      const isSystemCommand = phase.name.includes('System') || phase.name.includes('Smart Build');
      
      // Set environment to override example detection for npm test
      const testEnv = isSystemCommand ? {
        ...process.env,
        JTAG_ACTIVE_EXAMPLE: 'test-bench'  // Force test-bench example for npm test
      } : process.env;
      
      const child = spawn(phase.command, phase.args, {
        stdio: 'inherit', // Show output in real-time
        shell: true,
        env: testEnv
      });

      this.childProcesses.push(child);
      
      // Timeout handler
      const timeout = setTimeout(() => {
        if (!child.killed && !this.interrupted) {
          console.error(`❌ ERROR: ${phase.name} TIMED OUT after ${phase.timeoutSeconds}s`);
          console.warn(`⚠️  WARNING: Killing process due to timeout`);
          child.kill('SIGTERM');
        }
      }, phase.timeoutSeconds * 1000);

      child.on('exit', (code, signal) => {
        clearTimeout(timeout);
        this.childProcesses = this.childProcesses.filter(p => p !== child);
        
        if (this.interrupted) {
          resolve(false);
          return;
        }

        const success = code === 0;
        if (success) {
          console.log(`✅ ${phase.name} completed`);
        } else {
          console.log(`❌ ${phase.name} failed (exit code: ${code})`);
        }
        resolve(success);
      });

      child.on('error', (err) => {
        clearTimeout(timeout);
        this.childProcesses = this.childProcesses.filter(p => p !== child);
        console.log(`❌ ${phase.name} error: ${err.message}`);
        resolve(false);
      });
    });
  }

  async runTests(): Promise<boolean> {
    console.log('🧪 JTAG Test Runner - Starting System and Running Tests');
    console.log('📋 This will launch browser and run all 48 tests\n');

    // SIMPLE APPROACH: Just start system and run the integration tests
    // This should launch browser and complete all tests like before
    const phases: TestPhase[] = [
      {
        name: 'System Stop (cleanup)',
        command: 'npm',
        args: ['run', 'system:stop'],
        timeoutSeconds: 15,
        required: false  // Don't fail if already stopped
      },
      {
        name: 'Start System (with browser)',
        command: 'npx',
        args: ['tsx', 'scripts/test-system-start.ts'],  // Custom script that launches system+browser and exits
        timeoutSeconds: 90,
        required: true
      },
      {
        name: 'Wait for System Ready',
        command: 'npm',
        args: ['run', 'signal:wait'],  // Use signal-based waiting instead of arbitrary sleep
        timeoutSeconds: 60,
        required: true
      },
      {
        name: 'Run All Tests',
        command: 'npm',
        args: ['run', 'test:start-and-test'],  // This runs the 48 tests
        timeoutSeconds: 120,
        required: true
      }
    ];

    for (const phase of phases) {
      const success = await this.executePhase(phase);
      
      if (!success) {
        if (phase.required) {
          console.log(`\n💥 Required phase '${phase.name}' failed - aborting`);
          return false;
        } else {
          console.log(`\n⚠️  Optional phase '${phase.name}' failed - continuing`);
        }
      }
      
      if (this.interrupted) {
        console.log('\n🛑 Test execution interrupted');
        return false;
      }
    }

    console.log('\n🎉 All tests completed successfully!');
    return true;
  }
}

// Auto-detect correct working directory for tests
async function detectWorkingDirectory(): Promise<void> {
  const fs = await import('fs').then(m => m.promises);
  const path = await import('path');
  
  // Check if we're already in the right working directory (has healthy active system)
  try {
    const currentSignalDir = path.join(WorkingDirConfig.getContinuumPath(), 'jtag', 'signals');
    const files = await fs.readdir(currentSignalDir);
    const signalFiles = files.filter(file => file.startsWith('system-ready-') && file.endsWith('.json'));
    
    for (const signalFile of signalFiles) {
      try {
        const signalPath = path.join(currentSignalDir, signalFile);
        const signalData = await fs.readFile(signalPath, 'utf-8');
        const signal = JSON.parse(signalData);
        
        // Check if this is a healthy system with commands
        if (signal.systemHealth === 'healthy' && signal.commandCount > 0) {
          console.log(`✅ Found healthy system in current directory: ${signal.commandCount} commands, health: ${signal.systemHealth}`);
          return;
        }
      } catch {
        // Invalid signal file, continue checking
      }
    }
  } catch {
    // No signals in current directory
  }
  
  // Check widget-ui example directory
  try {
    const widgetUIDir = path.resolve('examples/widget-ui');
    WorkingDirConfig.setWorkingDir(widgetUIDir);
    
    const signalDir = path.join(WorkingDirConfig.getContinuumPath(), 'jtag', 'signals');
    const files = await fs.readdir(signalDir);
    const signalFiles = files.filter(file => file.startsWith('system-ready-') && file.endsWith('.json'));
    
    for (const signalFile of signalFiles) {
      try {
        const signalPath = path.join(signalDir, signalFile);
        const signalData = await fs.readFile(signalPath, 'utf-8');
        const signal = JSON.parse(signalData);
        
        // Check if this is a healthy system with commands
        if (signal.systemHealth === 'healthy' && signal.commandCount > 0) {
          console.log(`✅ Found healthy system in widget-ui: ${signal.commandCount} commands, health: ${signal.systemHealth}`);
          console.log(`📂 Using working directory: ${widgetUIDir}`);
          return;
        }
      } catch {
        // Invalid signal file, continue checking
      }
    }
  } catch {
    // No signals in widget-ui
  }
  
  // Check test-bench example directory  
  try {
    const testBenchDir = path.resolve('examples/test-bench');
    WorkingDirConfig.setWorkingDir(testBenchDir);
    
    const signalDir = path.join(WorkingDirConfig.getContinuumPath(), 'jtag', 'signals');
    const files = await fs.readdir(signalDir);
    const signalFiles = files.filter(file => file.startsWith('system-ready-') && file.endsWith('.json'));
    
    for (const signalFile of signalFiles) {
      try {
        const signalPath = path.join(signalDir, signalFile);
        const signalData = await fs.readFile(signalPath, 'utf-8');
        const signal = JSON.parse(signalData);
        
        // Check if this is a healthy system with commands
        if (signal.systemHealth === 'healthy' && signal.commandCount > 0) {
          console.log(`✅ Found healthy system in test-bench: ${signal.commandCount} commands, health: ${signal.systemHealth}`);
          console.log(`📂 Using working directory: ${testBenchDir}`);
          return;
        }
      } catch {
        // Invalid signal file, continue checking
      }
    }
  } catch {
    // No signals in test-bench
  }
  
  // Default to widget-ui for testing (most common case)
  const defaultDir = path.resolve('examples/widget-ui');
  WorkingDirConfig.setWorkingDir(defaultDir);
  console.log(`⚠️  No active system detected - defaulting to widget-ui: ${defaultDir}`);
  console.log('💡 To start a system: cd examples/widget-ui && npm run system:start');
}

// Main execution
async function main() {
  // Keep current working directory (main JTAG directory) for system commands
  console.log(`📂 Working directory: ${process.cwd()}`);
  
  const runner = new TestRunner();
  
  try {
    const success = await runner.runTests();
    process.exit(success ? 0 : 1);
  } catch (error: any) {
    console.error('\n💥 Test runner failed:', error.message);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}