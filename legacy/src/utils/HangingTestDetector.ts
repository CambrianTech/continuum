#!/usr/bin/env tsx
/**
 * Hanging Test Detector - Makes Errors Immediately Apparent
 * 
 * When tests hang, this reveals exactly what's happening:
 * - Shows pending operations with live timeouts
 * - Displays WebSocket connection state
 * - Captures screenshot command routing in real-time
 * - Provides immediate visibility into failures
 */

import * as fs from 'fs';
import { execSync } from 'child_process';

export class HangingTestDetector {
  private startTime = Date.now();
  private checkInterval: NodeJS.Timeout | null = null;
  private operations: Map<string, any> = new Map();

  constructor(private testName: string) {
    console.log(`🔍 HANG DETECTOR: Starting monitoring for ${testName}`);
    this.startMonitoring();
  }

  private startMonitoring(): void {
    this.checkInterval = setInterval(() => {
      const elapsed = Date.now() - this.startTime;
      console.log(`\n⏱️ HANG CHECK [${Math.round(elapsed/1000)}s]: ${this.testName}`);
      
      this.checkSystemHealth();
      this.checkActiveOperations();
      this.checkWebSocketState();
      this.showRecentLogs();
      
      if (elapsed > 30000) { // 30 second warning
        console.log(`🚨 WARNING: Test running for ${Math.round(elapsed/1000)}s - potential hang detected!`);
        this.emergencyDiagnostics();
      }
      
      if (elapsed > 60000) { // 60 second FORCE BREAK
        console.log(`💥 FORCE BREAK: Test hung for ${Math.round(elapsed/1000)}s - terminating NOW!`);
        console.log('🔥 HANG DETECTOR: Forcing process exit to break infinite hang');
        this.emergencyDiagnostics();
        process.exit(1); // FORCE EXIT - no more hanging
      }
    }, 5000); // Check every 5 seconds
  }

  trackOperation(operationId: string, description: string): void {
    const operation = {
      id: operationId,
      description,
      startTime: Date.now(),
      status: 'pending'
    };
    
    this.operations.set(operationId, operation);
    console.log(`📋 TRACKING: ${operationId} - ${description}`);
  }

  completeOperation(operationId: string): void {
    const operation = this.operations.get(operationId);
    if (operation) {
      operation.status = 'completed';
      operation.endTime = Date.now();
      console.log(`✅ COMPLETED: ${operationId} (${operation.endTime - operation.startTime}ms)`);
    }
  }

  private checkSystemHealth(): void {
    try {
      // Check JTAG system status
      const statusCheck = execSync('npm run signal:check', { 
        encoding: 'utf8', 
        timeout: 3000,
        stdio: 'pipe'
      });
      
      if (statusCheck.includes('✅ System is ready')) {
        console.log('✅ System health: READY');
      } else if (statusCheck.includes('degraded')) {
        console.log('⚠️ System health: DEGRADED');
      } else {
        console.log('❌ System health: UNHEALTHY');
      }
    } catch (error) {
      console.log(`❌ System health: ERROR (${error.message.substring(0, 50)})`);
    }
  }

  private checkActiveOperations(): void {
    let pendingOps = 0;
    const now = Date.now();
    
    for (const [id, op] of this.operations.entries()) {
      if (op.status === 'pending') {
        const elapsed = now - op.startTime;
        console.log(`   ⏳ ${id}: ${op.description} (${Math.round(elapsed/1000)}s)`);
        pendingOps++;
        
        if (elapsed > 15000) {
          console.log(`   🚨 HANGING: ${id} stuck for ${Math.round(elapsed/1000)}s!`);
        }
      }
    }
    
    if (pendingOps === 0) {
      console.log('   ✅ No pending operations');
    }
  }

  private checkWebSocketState(): void {
    try {
      // Check for active WebSocket connections on JTAG ports
      const wsCheck = execSync('lsof -iTCP -sTCP:LISTEN -n -P 2>/dev/null | grep ":900[0-9]" || echo "No JTAG ports"', { 
        encoding: 'utf8',
        timeout: 2000
      });
      
      if (wsCheck.includes('No JTAG ports')) {
        console.log('❌ WebSocket: No JTAG ports listening');
      } else {
        const portCount = (wsCheck.match(/:\d{4}/g) || []).length;
        console.log(`✅ WebSocket: ${portCount} JTAG ports active`);
      }
    } catch (error) {
      console.log(`❌ WebSocket check failed: ${error.message}`);
    }
  }

  private showRecentLogs(): void {
    try {
      const logPaths = [
        'examples/test-bench/.continuum/jtag/currentUser/logs/server.log',
        'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log'
      ];
      
      for (const logPath of logPaths) {
        if (fs.existsSync(logPath)) {
          const recentLogs = execSync(`tail -2 "${logPath}" 2>/dev/null || echo "No logs"`, {
            encoding: 'utf8',
            timeout: 1000
          });
          
          if (recentLogs && recentLogs.trim() !== 'No logs') {
            console.log(`   📄 Recent: ${recentLogs.trim().substring(0, 100)}...`);
          }
        }
      }
    } catch (error) {
      // Silent fail on log reading
    }
  }

  private emergencyDiagnostics(): void {
    console.log('\n🚨 EMERGENCY DIAGNOSTICS - TEST APPEARS HUNG:');
    
    try {
      // Show detailed process information
      const jtagProcesses = execSync('ps aux | grep -E "(jtag|tsx|screenshot)" | grep -v grep', {
        encoding: 'utf8',
        timeout: 3000
      });
      
      console.log('🔍 Active JTAG processes:');
      jtagProcesses.split('\n').slice(0, 5).forEach(line => {
        if (line.trim()) {
          console.log(`   ${line.substring(0, 120)}`);
        }
      });
    } catch (error) {
      console.log(`❌ Process check failed: ${error.message}`);
    }
    
    // Show hanging operations in detail
    console.log('\n🎯 DETAILED HANG ANALYSIS:');
    for (const [id, op] of this.operations.entries()) {
      if (op.status === 'pending') {
        const elapsed = Date.now() - op.startTime;
        console.log(`   🔴 HUNG: ${id}`);
        console.log(`      Description: ${op.description}`);
        console.log(`      Hanging for: ${Math.round(elapsed/1000)} seconds`);
      }
    }
  }

  stop(): void {
    if (this.checkInterval) {
      clearInterval(this.checkInterval);
      this.checkInterval = null;
    }
    
    const elapsed = Date.now() - this.startTime;
    console.log(`\n🏁 HANG DETECTOR: Stopped monitoring ${this.testName} (ran for ${Math.round(elapsed/1000)}s)`);
    
    // Final summary
    let completed = 0, pending = 0;
    for (const [_, op] of this.operations.entries()) {
      if (op.status === 'completed') completed++;
      else pending++;
    }
    
    console.log(`📊 Final Status: ${completed} completed, ${pending} pending`);
    
    if (pending > 0) {
      console.log('⚠️ WARNING: Test ended with pending operations - likely hung');
    }
  }
}

/**
 * Easy wrapper for any test function
 */
export function withHangDetection<T>(testName: string, testFunction: () => Promise<T>): Promise<T> {
  const detector = new HangingTestDetector(testName);
  
  return testFunction().finally(() => {
    detector.stop();
  });
}