#!/usr/bin/env tsx
/**
 * Autonomous Test Suite - Implements Proven 45-Second Development Methodology
 * 
 * This script follows the successful debugging methodology from dev-process.md:
 * 1. Intelligent system detection and startup
 * 2. 45-second wait for TypeScript compilation
 * 3. Bootstrap verification before testing
 * 4. Systematic failure analysis following established checklist
 * 5. Full log analysis with diagnostic guidance
 * 
 * Usage:
 *   npm test              # Main entry point - autonomous testing
 *   npm run test:auto     # Same as above 
 *   npm run test:manual   # Skip automatic startup (system assumed running)
 */

import { spawn, exec } from 'child_process';
import { promises as fs } from 'fs';
import path from 'path';
import { SystemReadySignaler, type SystemReadySignal } from './signal-system-ready';

interface TestResult {
  name: string;
  success: boolean;
  duration: number;
  error?: string;
  diagnostics?: string[];
}

interface SystemStatus {
  isRunning: boolean;
  startupRequired: boolean;
  bootstrapComplete: boolean;
  commandCount?: number;
  logTimestamp?: Date;
}

class AutonomousTestSuite {
  private results: TestResult[] = [];
  private startTime = Date.now();
  private signaler = new SystemReadySignaler();

  async runComprehensiveTestSuite(): Promise<boolean> {
    console.log('🚀 JTAG Autonomous Test Suite - Following Proven 45-Second Methodology');
    console.log('=' .repeat(80));
    console.log('📚 Based on successful patterns from dev-process.md');
    console.log('');

    try {
      // Phase 1: System Detection and Startup (CORE METHODOLOGY)
      const systemStatus = await this.detectSystemStatus();
      await this.ensureSystemReady(systemStatus);
      
      // Phase 2: Bootstrap Verification (CRITICAL CHECKPOINT)
      const bootstrapOk = await this.verifyBootstrap();
      if (!bootstrapOk) {
        throw new Error('Bootstrap verification failed - system not ready for testing');
      }
      
      // Phase 3: Execute Test Suite in Proven Order
      await this.runTestPhases();
      
      // Phase 4: Comprehensive Result Analysis
      this.printComprehensiveResults();
      
      return this.allTestsPassed();
      
    } catch (error: any) {
      console.error('❌ Autonomous test suite failed:', error.message);
      await this.printDiagnosticGuidance(error);
      return false;
    }
  }

  /**
   * Phase 1: Intelligent System Detection
   * Implements the proven system detection methodology
   */
  private async detectSystemStatus(): Promise<SystemStatus> {
    console.log('🔍 Phase 1: Intelligent System Detection...');
    
    // Check tmux session (from proven methodology)
    const tmuxRunning = await this.checkTmuxSession();
    
    // Check port availability using dynamic configuration
    const { getActivePorts } = require('../examples/shared/ExampleConfig');
    const activePorts = await getActivePorts();
    const portsActive = await this.checkPorts([activePorts.websocket_server, activePorts.http_server]);
    
    // Check log freshness (critical timing check)
    const logStatus = await this.checkLogFreshness();
    
    const status: SystemStatus = {
      isRunning: tmuxRunning && portsActive,
      startupRequired: !tmuxRunning || !portsActive,
      bootstrapComplete: false,
      logTimestamp: logStatus.timestamp,
    };
    
    console.log(`📊 System Status: ${status.isRunning ? '✅ Running' : '❌ Needs Startup'}`);
    if (status.logTimestamp) {
      console.log(`📅 Last Activity: ${status.logTimestamp.toISOString()}`);
    }
    
    return status;
  }

  /**
   * Phase 2: System Startup with Intelligent Signal-Based Waiting
   * Replaces fixed 45-second timing with smart readiness detection
   */
  private async ensureSystemReady(status: SystemStatus): Promise<void> {
    if (!status.startupRequired) {
      console.log('✅ Phase 2: System already running, verifying readiness with signals...');
      
      // Even if system appears to be running, check the signal for health status
      const signalResult = await this.waitForSystemReadySignal(30000); // Quick check for existing system
      if (!signalResult) {
        console.log('⚠️ System running but no ready signal - forcing restart for reliability');
        status.startupRequired = true;
      } else {
        console.log(`✅ System confirmed ready: ${signalResult.systemHealth} (${signalResult.commandCount} commands)`);
        return;
      }
    }
    
    console.log('🚀 Phase 2: System Startup Required - Using Intelligent Signal-Based Detection...');
    console.log('📈 Improvement: No more fixed 45-second waits! System tells us when ready.');
    
    // Step 1: Clean shutdown and clear old signals
    console.log('🛑 Stopping any existing system and clearing signals...');
    await this.executeCommand('npm run system:stop');
    
    // Step 2: Start system (proven command)
    console.log('🚀 Starting system with npm run system:start...');
    const startProcess = this.executeCommandAsync('npm run system:start');
    
    // Step 3: Intelligent wait for ready signal (replaces 45-second sleep)
    console.log('⏳ Waiting for system ready signal (intelligent detection)...');
    console.log('🧠 System will signal when TypeScript compilation and bootstrap complete');
    
    const readySignal = await this.waitForSystemReadySignal(90000); // Max 90 seconds
    
    if (!readySignal) {
      throw new Error('System startup timeout - check logs with: npm run signal:logs');
    }
    
    if (readySignal.systemHealth === 'error') {
      console.error('❌ System started but has errors:');
      if (readySignal.nodeErrors?.length) {
        readySignal.nodeErrors.forEach(error => console.error(`   - ${error}`));
      }
      throw new Error(`System startup failed - health: ${readySignal.systemHealth}. Check logs: npm run signal:logs`);
    }
    
    if (readySignal.systemHealth === 'degraded') {
      console.warn('⚠️ System started but degraded:');
      if (readySignal.errors?.length) {
        readySignal.errors.forEach(error => console.warn(`   - ${error}`));
      }
      console.log('🤔 Continuing with degraded system - tests may have issues');
    }
    
    console.log(`✅ Phase 2: System ready! Health: ${readySignal.systemHealth}`);
    console.log(`📊 Bootstrap: ${readySignal.bootstrapComplete}, Commands: ${readySignal.commandCount}, Ports: ${readySignal.portsActive.join(',')}`);
    console.log(`⏱️ Total startup time: Smart detection (no unnecessary waiting!)`);
  }

  /**
   * Phase 3: Bootstrap Verification (CRITICAL CHECKPOINT)
   * Implements the "Bootstrap complete! Discovered X commands" verification
   */
  private async verifyBootstrap(): Promise<boolean> {
    console.log('🔍 Phase 3: Bootstrap Verification (CRITICAL CHECKPOINT)...');
    
    try {
      // Check the specific pattern from dev-process.md
      const logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      
      const command = `tail -10 ${logPath} | grep "Bootstrap complete"`;
      const result = await this.executeCommand(command);
      
      if (result.includes('Bootstrap complete')) {
        const commandMatch = result.match(/Discovered (\d+) commands/);
        const commandCount = commandMatch ? parseInt(commandMatch[1]) : 0;
        
        console.log(`✅ Bootstrap Verified: Discovered ${commandCount} commands`);
        
        if (commandCount < 15) {
          console.warn(`⚠️ Warning: Only ${commandCount} commands discovered (expected 15+)`);
          console.log('💡 System may still be initializing - continuing with caution');
        }
        
        return true;
      } else {
        console.error('❌ Bootstrap verification failed');
        console.log('💡 System not ready for testing - check startup logs');
        
        // Provide diagnostic guidance
        await this.printBootstrapDiagnostics();
        return false;
      }
      
    } catch (error: any) {
      console.error('❌ Bootstrap check failed:', error.message);
      await this.printBootstrapDiagnostics();
      return false;
    }
  }

  /**
   * Phase 4: Execute Test Phases in Proven Order
   * Follows the successful testing hierarchy from existing scripts
   */
  private async runTestPhases(): Promise<void> {
    console.log('🧪 Phase 4: Execute Test Suite in Proven Order...');
    console.log('');
    
    // Phase 4A: Transport Layer Tests (Foundation)
    await this.recordTest('Transport Layer Tests', async () => {
      await this.executeTestScript('tests/integration/transport/comprehensive-transport-test.ts');
    });
    
    // Phase 4B: Cross-Context Communication Tests  
    await this.recordTest('Cross-Context Communication Tests', async () => {
      await this.executeTestScript('tests/integration/transport/browser-server-commands.test.ts');
    });
    
    // Phase 4C: Router Integration Tests (Complex)
    await this.recordTest('Router Integration Tests', async () => {
      await this.executeTestScript('tests/integration/comprehensive-routing-validation.test.ts');
    });
    
    // Phase 4D: End-to-End Integration (Most Complex)
    await this.recordTest('End-to-End Integration Tests', async () => {
      await this.executeTestScript('tests/integration/server-client-integration.test.ts');
    });
  }

  /**
   * Intelligent System Ready Detection
   * Replaces fixed sleep timers with signal-based waiting
   */
  private async waitForSystemReadySignal(timeoutMs: number): Promise<SystemReadySignal | null> {
    console.log(`🔍 Waiting for system ready signal (max ${timeoutMs/1000}s)...`);
    return await this.signaler.checkSystemReady(timeoutMs);
  }

  /**
   * Helper Methods for System Operations
   */
  private async checkTmuxSession(): Promise<boolean> {
    try {
      const result = await this.executeCommand('tmux has-session -t jtag-test 2>/dev/null');
      return result.length === 0; // tmux has-session returns empty on success
    } catch {
      return false;
    }
  }

  private async checkPorts(ports: number[]): Promise<boolean> {
    try {
      for (const port of ports) {
        const result = await this.executeCommand(`lsof -ti:${port}`);
        if (result.trim().length === 0) {
          return false; // Port not in use
        }
      }
      return true; // All ports active
    } catch {
      return false;
    }
  }

  private async checkLogFreshness(): Promise<{ timestamp?: Date }> {
    try {
      const logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      const stats = await fs.stat(logPath);
      return { timestamp: stats.mtime };
    } catch {
      return {};
    }
  }

  private async countdownTimer(seconds: number): Promise<void> {
    for (let i = seconds; i > 0; i--) {
      if (i % 15 === 0 || i <= 10) {
        console.log(`⏳ ${i} seconds remaining...`);
      }
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
  }

  private async executeCommand(command: string): Promise<string> {
    return new Promise((resolve, reject) => {
      exec(command, (error, stdout, stderr) => {
        if (error) {
          reject(new Error(`Command failed: ${command}\nError: ${error.message}\nStderr: ${stderr}`));
        } else {
          resolve(stdout);
        }
      });
    });
  }

  private executeCommandAsync(command: string) {
    return spawn('bash', ['-c', command], { 
      stdio: 'ignore', // Don't pipe output to avoid blocking
      detached: true 
    });
  }

  private async executeTestScript(scriptPath: string): Promise<void> {
    const command = `npx tsx ${scriptPath}`;
    const result = await this.executeCommand(command);
    
    // Check for success indicators in output
    if (result.includes('❌') || result.includes('FAILED')) {
      throw new Error(`Test script failed: ${scriptPath}\n${result}`);
    }
  }

  private async recordTest(name: string, testFn: () => Promise<void>): Promise<void> {
    const startTime = Date.now();
    
    try {
      console.log(`\n🧪 Running: ${name}`);
      await testFn();
      const duration = Date.now() - startTime;
      
      this.results.push({
        name,
        success: true,
        duration
      });
      
      console.log(`✅ ${name} - Passed (${duration}ms)`);
    } catch (error: any) {
      const duration = Date.now() - startTime;
      
      // Collect diagnostics for failed tests
      const diagnostics = await this.collectDiagnostics(error);
      
      this.results.push({
        name,
        success: false,
        duration,
        error: error.message,
        diagnostics
      });
      
      console.log(`❌ ${name} - Failed (${duration}ms): ${error.message}`);
    }
  }

  private async collectDiagnostics(error: any): Promise<string[]> {
    const diagnostics: string[] = [];
    
    try {
      // Check system logs for recent errors
      const systemLogCheck = await this.executeCommand(
        'tail -50 examples/test-bench/.continuum/jtag/sessions/system/*/logs/server-console-log.log 2>/dev/null | grep -i error | tail -5'
      );
      if (systemLogCheck.trim()) {
        diagnostics.push(`Recent system errors: ${systemLogCheck}`);
      }
      
      // Check if system is still responsive using dynamic ports
      const { getActivePorts } = require('../examples/shared/ExampleConfig');
      const activePorts = await getActivePorts();
      const portCheck = await this.checkPorts([activePorts.websocket_server, activePorts.http_server]);
      if (!portCheck) {
        diagnostics.push('System ports not responding - system may have crashed');
      }
      
    } catch {
      diagnostics.push('Unable to collect system diagnostics');
    }
    
    return diagnostics;
  }

  private async printBootstrapDiagnostics(): Promise<void> {
    console.log('\n🔍 BOOTSTRAP DIAGNOSTICS:');
    
    try {
      // Check if logs exist
      const logPath = 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log';
      const logExists = await fs.access(logPath).then(() => true).catch(() => false);
      
      if (!logExists) {
        console.log('❌ Browser log file not found - system likely not started');
        console.log('💡 Run: npm run system:start');
        return;
      }
      
      // Check log content
      const recentLogs = await this.executeCommand(`tail -20 ${logPath}`);
      console.log('📋 Recent browser logs:');
      console.log(recentLogs);
      
      // Check startup logs
      console.log('\n📋 Checking system startup logs...');
      const startupCheck = await this.executeCommand('tail -20 .continuum/jtag/system/logs/npm-start.log 2>/dev/null || echo "No startup logs found"');
      console.log(startupCheck);
      
    } catch (error: any) {
      console.log('❌ Unable to collect bootstrap diagnostics:', error.message);
    }
  }

  private async printDiagnosticGuidance(error: any): Promise<void> {
    console.log('\n🔍 DIAGNOSTIC GUIDANCE (from proven dev-process.md methodology):');
    console.log('');
    
    console.log('📋 Follow this systematic debugging checklist:');
    console.log('1. ☐ Check compilation: npx tsc --project tsconfig.json --noEmit');
    console.log('2. ☐ Verify system startup: npm run system:start && sleep 45');
    console.log('3. ☐ Check bootstrap: tail -10 examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log | grep "Bootstrap complete"');
    console.log('4. ☐ Analyze system logs: tail -50 examples/test-bench/.continuum/jtag/sessions/system/*/logs/server-console-log.log');
    console.log('');
    
    console.log('🎯 Common failure points (from proven experience):');
    console.log('- Command registration failure (Step 6D in dev-process.md)');
    console.log('- TypeScript compilation errors preventing startup');
    console.log('- System started but bootstrap incomplete');
    console.log('- WebSocket response routing missing (known infrastructure bug)');
    console.log('');
    
    console.log('📚 For complete debugging methodology, see: dev-process.md');
  }

  private printComprehensiveResults(): void {
    console.log('\n' + '=' .repeat(80));
    console.log('📊 AUTONOMOUS TEST SUITE RESULTS');
    console.log('=' .repeat(80));

    const passed = this.results.filter(r => r.success).length;
    const failed = this.results.filter(r => !r.success).length;
    const totalDuration = Date.now() - this.startTime;

    console.log(`✅ Passed: ${passed}`);
    console.log(`❌ Failed: ${failed}`);
    console.log(`⏱️  Total Duration: ${totalDuration}ms`);
    console.log(`📈 Success Rate: ${(passed / (this.results.length || 1) * 100).toFixed(1)}%`);

    if (failed > 0) {
      console.log('\n❌ Failed Tests with Diagnostics:');
      this.results.filter(r => !r.success).forEach(result => {
        console.log(`\n   📋 ${result.name}:`);
        console.log(`      Error: ${result.error}`);
        if (result.diagnostics?.length) {
          console.log(`      Diagnostics:`);
          result.diagnostics.forEach(diagnostic => {
            console.log(`        - ${diagnostic}`);
          });
        }
      });
    }

    if (passed > 0) {
      console.log('\n✅ Performance Metrics:');
      this.results.filter(r => r.success).forEach(result => {
        console.log(`   ${result.name}: ${result.duration}ms`);
      });
    }

    console.log('\n🎯 System Health Analysis:');
    const avgTestTime = this.results.reduce((sum, r) => sum + r.duration, 0) / (this.results.length || 1);
    console.log(`   Average Test Duration: ${avgTestTime.toFixed(1)}ms`);
    console.log(`   Total Suite Runtime: ${totalDuration}ms (includes 45-second startup)`);
    
    if (failed === 0) {
      console.log('\n🏆 ALL TESTS PASSED! JTAG system is functioning correctly.');
    } else {
      console.log('\n💥 Some tests failed. Review diagnostics above and check dev-process.md for debugging guidance.');
    }
  }

  private allTestsPassed(): boolean {
    return this.results.every(result => result.success);
  }
}

// Main execution
async function main() {
  const suite = new AutonomousTestSuite();
  const success = await suite.runComprehensiveTestSuite();
  
  if (success) {
    console.log('\n🎉 AUTONOMOUS TEST SUITE COMPLETED SUCCESSFULLY!');
    process.exit(0);
  } else {
    console.log('\n💥 AUTONOMOUS TEST SUITE FAILED - CHECK DIAGNOSTICS ABOVE');
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  main().catch(console.error);
}

export { AutonomousTestSuite };