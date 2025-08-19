#!/usr/bin/env tsx
/**
 * Diagnostics Logger for Build Detection and Auto-Spawn Systems
 * 
 * Provides comprehensive error reporting, timeout protection, and debugging
 * visibility to prevent hanging and enable rapid problem diagnosis.
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';

export interface DiagnosticContext {
  operation: string;
  startTime: number;
  details: Record<string, any>;
  errors: string[];
  warnings: string[];
  timeoutMs?: number;
}

export interface SystemSnapshot {
  timestamp: string;
  processInfo: {
    pid: number;
    uptime: number;
    memory: NodeJS.MemoryUsage;
    platform: string;
    nodeVersion: string;
  };
  filesystem: {
    cwd: string;
    sourceFiles: number;
    distFiles: number;
    tempFiles: number;
  };
  network: {
    activePorts: number[];
    processes: string[];
  };
  logs: {
    recentErrors: string[];
    systemHealth: string;
  };
}

export class DiagnosticsLogger {
  private logDir = '.continuum/jtag/diagnostics';
  private contexts: Map<string, DiagnosticContext> = new Map();

  constructor() {
    // Ensure diagnostics directory exists
    fs.mkdirSync(this.logDir, { recursive: true });
  }

  /**
   * Start tracking an operation with timeout protection
   */
  startOperation(operationId: string, operation: string, timeoutMs = 60000): DiagnosticContext {
    const context: DiagnosticContext = {
      operation,
      startTime: Date.now(),
      details: {},
      errors: [],
      warnings: [],
      timeoutMs
    };
    
    this.contexts.set(operationId, context);
    
    console.log(`🔍 DIAGNOSTICS: Starting ${operation} (id: ${operationId}, timeout: ${timeoutMs}ms)`);
    
    // Set up timeout protection
    if (timeoutMs > 0) {
      setTimeout(() => {
        if (this.contexts.has(operationId)) {
          const ctx = this.contexts.get(operationId)!;
          const elapsed = Date.now() - ctx.startTime;
          this.addError(operationId, `Operation timeout after ${elapsed}ms (limit: ${timeoutMs}ms)`);
          this.emergencyDump(operationId, 'TIMEOUT');
        }
      }, timeoutMs);
    }
    
    return context;
  }

  /**
   * Add contextual information to an operation
   */
  addDetail(operationId: string, key: string, value: any): void {
    const context = this.contexts.get(operationId);
    if (context) {
      context.details[key] = value;
    }
  }

  /**
   * Add an error to the diagnostic context
   */
  addError(operationId: string, error: string): void {
    const context = this.contexts.get(operationId);
    if (context) {
      context.errors.push(`[${new Date().toISOString()}] ${error}`);
      console.error(`🚨 DIAGNOSTICS ERROR [${operationId}]: ${error}`);
    }
  }

  /**
   * Add a warning to the diagnostic context
   */
  addWarning(operationId: string, warning: string): void {
    const context = this.contexts.get(operationId);
    if (context) {
      context.warnings.push(`[${new Date().toISOString()}] ${warning}`);
      console.warn(`⚠️ DIAGNOSTICS WARNING [${operationId}]: ${warning}`);
    }
  }

  /**
   * Complete an operation successfully
   */
  completeOperation(operationId: string): void {
    const context = this.contexts.get(operationId);
    if (context) {
      const elapsed = Date.now() - context.startTime;
      console.log(`✅ DIAGNOSTICS: Completed ${context.operation} in ${elapsed}ms`);
      
      if (context.errors.length > 0 || context.warnings.length > 0) {
        this.saveDiagnosticReport(operationId, 'COMPLETED_WITH_ISSUES');
      }
      
      this.contexts.delete(operationId);
    }
  }

  /**
   * Fail an operation with detailed error reporting
   */
  failOperation(operationId: string, reason: string): void {
    const context = this.contexts.get(operationId);
    if (context) {
      const elapsed = Date.now() - context.startTime;
      this.addError(operationId, `Operation failed after ${elapsed}ms: ${reason}`);
      console.error(`❌ DIAGNOSTICS: Failed ${context.operation} - ${reason}`);
      
      this.emergencyDump(operationId, 'FAILED');
      this.contexts.delete(operationId);
    }
  }

  /**
   * Create comprehensive system snapshot for debugging
   */
  async createSystemSnapshot(): Promise<SystemSnapshot> {
    const snapshot: SystemSnapshot = {
      timestamp: new Date().toISOString(),
      processInfo: {
        pid: process.pid,
        uptime: process.uptime() * 1000,
        memory: process.memoryUsage(),
        platform: process.platform,
        nodeVersion: process.version
      },
      filesystem: {
        cwd: process.cwd(),
        sourceFiles: this.countFiles('**/*.{ts,tsx}'),
        distFiles: this.countFiles('dist/**/*'),
        tempFiles: this.countFiles('.continuum/**/*')
      },
      network: {
        activePorts: await this.getActivePorts(),
        processes: await this.getActiveProcesses()
      },
      logs: {
        recentErrors: await this.getRecentErrors(),
        systemHealth: await this.getSystemHealth()
      }
    };
    
    return snapshot;
  }

  /**
   * Emergency diagnostic dump when things go wrong
   */
  private async emergencyDump(operationId: string, reason: string): Promise<void> {
    try {
      console.log(`🚨 EMERGENCY DIAGNOSTIC DUMP: ${reason} for operation ${operationId}`);
      
      const context = this.contexts.get(operationId);
      const snapshot = await this.createSystemSnapshot();
      
      const report = {
        reason,
        operationId,
        context,
        snapshot,
        emergencyInfo: {
          activeOperations: Array.from(this.contexts.keys()),
          systemLoad: this.getSafeLoadAverage(),
          freeMemory: this.getSafeFreeMemory(),
          totalMemory: this.getSafeTotalMemory()
        }
      };
      
      const reportPath = path.join(this.logDir, `emergency-${operationId}-${Date.now()}.json`);
      fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
      
      console.error(`🔍 Emergency diagnostic report saved: ${reportPath}`);
      console.error('🛠️  Diagnostic commands:');
      console.error(`   cat "${reportPath}" | jq .context.errors`);
      console.error(`   cat "${reportPath}" | jq .snapshot.logs.recentErrors`);
      console.error(`   cat "${reportPath}" | jq .snapshot.network.activePorts`);
      
    } catch (dumpError) {
      console.error('💥 Emergency dump failed:', dumpError.message);
      console.error('🔍 Basic debug info:');
      const safeContext = this.contexts.get(operationId);
      console.error(`   Operation: ${safeContext?.operation || 'unknown'}`);
      console.error(`   Errors: ${safeContext?.errors.length || 0}`);
      console.error(`   PID: ${process.pid}`);
      console.error(`   Memory: ${Math.round(process.memoryUsage().heapUsed / 1024 / 1024)}MB`);
    }
  }

  /**
   * Save diagnostic report for completed operations
   */
  private saveDiagnosticReport(operationId: string, status: string): void {
    try {
      const context = this.contexts.get(operationId);
      if (!context) return;
      
      const report = {
        operationId,
        status,
        operation: context.operation,
        duration: Date.now() - context.startTime,
        details: context.details,
        errors: context.errors,
        warnings: context.warnings
      };
      
      const reportPath = path.join(this.logDir, `${operationId}-${Date.now()}.json`);
      fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
      
      if (context.errors.length > 0) {
        console.log(`🔍 Diagnostic report with errors: ${reportPath}`);
      }
    } catch (error) {
      console.warn('⚠️ Failed to save diagnostic report:', error.message);
    }
  }

  // Helper methods for system information gathering

  private countFiles(pattern: string): number {
    try {
      const { globSync } = require('glob');
      return globSync(pattern).length;
    } catch {
      return -1;
    }
  }

  private async getActivePorts(): Promise<number[]> {
    try {
      const output = execSync('lsof -iTCP -sTCP:LISTEN -n -P 2>/dev/null | grep :900 || echo ""', { encoding: 'utf8' });
      const ports: number[] = [];
      const matches = output.match(/:(\d{4})/g);
      if (matches) {
        for (const match of matches) {
          const port = parseInt(match.substring(1));
          if (port >= 9000 && port <= 9010) {
            ports.push(port);
          }
        }
      }
      return [...new Set(ports)].sort();
    } catch {
      return [];
    }
  }

  private async getActiveProcesses(): Promise<string[]> {
    try {
      const output = execSync('ps aux | grep -E "(jtag|continuum|tsx)" | grep -v grep | head -5', { encoding: 'utf8' });
      return output.split('\n').filter(line => line.trim()).map(line => {
        const parts = line.trim().split(/\s+/);
        return `${parts[1]} ${parts.slice(10).join(' ')}`;
      });
    } catch {
      return [];
    }
  }

  private async getRecentErrors(): Promise<string[]> {
    const logPaths = [
      '.continuum/jtag/system/logs/npm-start.log',
      'examples/test-bench/.continuum/jtag/currentUser/logs/server.log'
    ];
    
    const errors: string[] = [];
    
    for (const logPath of logPaths) {
      try {
        if (fs.existsSync(logPath)) {
          const output = execSync(`tail -50 "${logPath}" | grep -i error | tail -3`, { encoding: 'utf8' });
          errors.push(...output.split('\n').filter(line => line.trim()));
        }
      } catch {
        // Continue with other logs
      }
    }
    
    return errors.slice(-5); // Last 5 errors
  }

  private async getSystemHealth(): Promise<string> {
    try {
      const signalFile = '.continuum/jtag/signals/system-ready.json';
      if (fs.existsSync(signalFile)) {
        const signal = JSON.parse(fs.readFileSync(signalFile, 'utf8'));
        return signal.systemHealth || 'unknown';
      }
    } catch {}
    return 'unavailable';
  }

  /**
   * Get diagnostic summary for current state
   */
  getActiveDiagnostics(): string[] {
    const active: string[] = [];
    
    for (const [operationId, context] of this.contexts.entries()) {
      const elapsed = Date.now() - context.startTime;
      const status = elapsed > (context.timeoutMs || 60000) ? 'TIMEOUT' : 'ACTIVE';
      active.push(`${operationId}: ${context.operation} (${elapsed}ms, ${status})`);
    }
    
    return active;
  }

  // Safe helper methods for cross-platform compatibility
  
  private getSafeLoadAverage(): number[] {
    try {
      return process.loadavg ? process.loadavg() : [0, 0, 0];
    } catch {
      return [0, 0, 0];
    }
  }
  
  private getSafeFreeMemory(): number {
    try {
      const os = require('os');
      return os.freemem ? os.freemem() : 0;
    } catch {
      return 0;
    }
  }
  
  private getSafeTotalMemory(): number {
    try {
      const os = require('os');
      return os.totalmem ? os.totalmem() : 0;
    } catch {
      return 0;
    }
  }

  /**
   * Clean up old diagnostic files
   */
  cleanup(maxAgeMs = 24 * 60 * 60 * 1000): void {
    try {
      const files = fs.readdirSync(this.logDir);
      const cutoff = Date.now() - maxAgeMs;
      
      for (const file of files) {
        const filePath = path.join(this.logDir, file);
        const stats = fs.statSync(filePath);
        
        if (stats.mtime.getTime() < cutoff) {
          fs.unlinkSync(filePath);
        }
      }
    } catch (error) {
      console.warn('⚠️ Diagnostics cleanup failed:', error.message);
    }
  }
}

// Global diagnostics instance
export const diagnostics = new DiagnosticsLogger();