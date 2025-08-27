/**
 * Smart Port Cleanup - Registry-aware process management
 * 
 * Uses ProcessRegistry to identify JTAG processes before cleanup:
 * 1. Distinguishes between JTAG processes and other applications
 * 2. Preserves active JTAG systems during startup
 * 3. Provides surgical cleanup for specific processes
 * 4. Enables safe multi-instance P2P testing
 */

import { execSync } from 'child_process';
import { getProcessRegistry, ProcessRegistryEntry, ProcessType } from './ProcessRegistry';
import { getSystemConfig } from '../config/SystemConfiguration';

export interface CleanupOptions {
  readonly forceAll: boolean;           // Kill all processes, ignore registry
  readonly preserveActive: boolean;     // Preserve active JTAG processes
  readonly targetProcessId?: string;    // Only cleanup specific process
  readonly targetPorts?: number[];      // Only cleanup specific ports
}

export interface CleanupResult {
  readonly killedProcesses: readonly ProcessRegistryEntry[];
  readonly preservedProcesses: readonly ProcessRegistryEntry[];
  readonly cleanedPorts: readonly number[];
  readonly errors: readonly string[];
}

export class SmartPortCleanup {
  
  /**
   * Intelligent cleanup that respects JTAG process registry
   */
  static async cleanup(options: Partial<CleanupOptions> = {}): Promise<CleanupResult> {
    const opts: CleanupOptions = {
      forceAll: false,
      preserveActive: true,
      ...options
    };

    console.log('🧹 Smart Port Cleanup: Starting registry-aware cleanup...');
    
    try {
      const registry = getProcessRegistry();
      const activeProcesses = await registry.getActiveProcesses();
      const systemConfig = getSystemConfig();
      
      console.log(`🔍 Found ${activeProcesses.length} active JTAG processes in registry`);
      
      // Determine which processes to preserve vs kill
      const { toKill, toPreserve } = SmartPortCleanup.categorizeProcesses(
        activeProcesses, 
        opts
      );

      const result: {
        killedProcesses: ProcessRegistryEntry[];
        preservedProcesses: readonly ProcessRegistryEntry[];
        cleanedPorts: number[];
        errors: string[];
      } = {
        killedProcesses: [],
        preservedProcesses: toPreserve,
        cleanedPorts: [],
        errors: []
      };

      // Kill tmux sessions (safe to always clean)
      await SmartPortCleanup.cleanupTmuxSessions(result);

      // Handle JTAG processes
      for (const proc of toKill) {
        await SmartPortCleanup.killJTAGProcess(proc, result);
      }

      // Handle non-JTAG processes on target ports
      const targetPorts = opts.targetPorts || [
        systemConfig.getWebSocketPort(),
        systemConfig.getHTTPPort(),
        systemConfig.getUDPConfig().port,
        systemConfig.getUDPConfig().unicastPort
      ].filter(port => port > 0);

      if (opts.forceAll) {
        // Force kill all processes on target ports
        await SmartPortCleanup.forceKillPorts(targetPorts, result);
      } else {
        // Only kill non-JTAG processes
        await SmartPortCleanup.cleanupNonJTAGProcesses(
          targetPorts, 
          activeProcesses,
          result
        );
      }

      // Log results
      SmartPortCleanup.logCleanupResults(result);
      
      return result;

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('❌ Smart Port Cleanup failed:', errorMsg);
      
      const errorResult: CleanupResult = {
        killedProcesses: [],
        preservedProcesses: [],
        cleanedPorts: [],
        errors: [errorMsg]
      };
      return errorResult;
    }
  }

  /**
   * Categorize processes into kill vs preserve based on options
   */
  private static categorizeProcesses(
    processes: ProcessRegistryEntry[],
    options: CleanupOptions
  ): { toKill: ProcessRegistryEntry[]; toPreserve: ProcessRegistryEntry[] } {
    
    const toKill: ProcessRegistryEntry[] = [];
    const toPreserve: ProcessRegistryEntry[] = [];

    for (const proc of processes) {
      let shouldKill = false;

      // Force kill all if requested
      if (options.forceAll) {
        shouldKill = true;
      }
      // Target specific process
      else if (options.targetProcessId && proc.processId === options.targetProcessId) {
        shouldKill = true;
      }
      // Target specific ports
      else if (options.targetPorts && proc.ports.some(port => options.targetPorts!.includes(port))) {
        shouldKill = !options.preserveActive;
      }
      // Default: preserve active processes unless force mode
      else {
        shouldKill = !options.preserveActive;
      }

      if (shouldKill) {
        toKill.push(proc);
      } else {
        toPreserve.push(proc);
      }
    }

    return { toKill, toPreserve };
  }

  /**
   * Kill a specific JTAG process and update registry
   */
  private static async killJTAGProcess(
    proc: ProcessRegistryEntry, 
    result: {
      killedProcesses: ProcessRegistryEntry[];
      preservedProcesses: readonly ProcessRegistryEntry[];
      cleanedPorts: number[];
      errors: string[];
    }
  ): Promise<void> {
    try {
      console.log(`🎯 Killing JTAG process ${proc.processId} (${proc.description}) PID ${proc.pid}`);
      
      // Kill the process
      process.kill(proc.pid, 'SIGTERM');
      
      // Give it time to cleanup gracefully
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Force kill if still running
      try {
        process.kill(proc.pid, 0); // Check if still exists
        process.kill(proc.pid, 'SIGKILL');
        console.log(`💀 Force killed JTAG process ${proc.processId}`);
      } catch {
        // Process already exited
      }
      
      result.killedProcesses.push(proc);
      result.cleanedPorts.push(...proc.ports);
      
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      result.errors.push(`Failed to kill JTAG process ${proc.processId}: ${errorMsg}`);
    }
  }

  /**
   * Clean up non-JTAG processes on target ports
   */
  private static async cleanupNonJTAGProcesses(
    targetPorts: number[],
    jtagProcesses: ProcessRegistryEntry[],
    result: CleanupResult
  ): Promise<void> {
    
    const jtagPorts = new Set<number>();
    jtagProcesses.forEach(proc => proc.ports.forEach(port => jtagPorts.add(port)));

    for (const port of targetPorts) {
      if (jtagPorts.has(port)) {
        console.log(`⚠️  Preserving port ${port} (owned by JTAG process)`);
        continue;
      }

      try {
        execSync(`lsof -ti:${port} | xargs kill -9 2>/dev/null`, { stdio: 'ignore' });
        console.log(`✅ Cleaned non-JTAG processes on port ${port}`);
        result.cleanedPorts.push(port);
      } catch {
        console.log(`ℹ️  No non-JTAG processes found on port ${port}`);
      }
    }
  }

  /**
   * Force kill all processes on target ports
   */
  private static async forceKillPorts(
    ports: number[], 
    result: CleanupResult
  ): Promise<void> {
    for (const port of ports) {
      try {
        execSync(`lsof -ti:${port} | xargs kill -9 2>/dev/null`, { stdio: 'ignore' });
        console.log(`💀 Force killed all processes on port ${port}`);
        result.cleanedPorts.push(port);
      } catch {
        console.log(`ℹ️  No processes found on port ${port}`);
      }
    }
  }

  /**
   * Clean up tmux sessions (always safe)
   */
  private static async cleanupTmuxSessions(result: CleanupResult): Promise<void> {
    try {
      execSync('tmux kill-session -t jtag-test 2>/dev/null', { stdio: 'ignore' });
      console.log('✅ Tmux session jtag-test killed');
    } catch {
      console.log('ℹ️  No tmux session jtag-test found');
    }

    // Kill npm start processes (usually safe)
    try {
      execSync(`pkill -f 'npm.*start' 2>/dev/null`, { stdio: 'ignore' });
      console.log('✅ npm start processes killed');
    } catch {
      console.log('ℹ️  No npm start processes found');
    }
  }

  /**
   * Log cleanup results
   */
  private static logCleanupResults(result: CleanupResult): void {
    console.log(`🎉 Smart cleanup complete:`);
    console.log(`  • Killed processes: ${result.killedProcesses.length}`);
    console.log(`  • Preserved processes: ${result.preservedProcesses.length}`);
    console.log(`  • Cleaned ports: [${result.cleanedPorts.join(', ')}]`);
    
    if (result.errors.length > 0) {
      console.log(`  • Errors: ${result.errors.length}`);
      result.errors.forEach(error => console.log(`    ❌ ${error}`));
    }

    if (result.preservedProcesses.length > 0) {
      console.log(`📋 Active JTAG processes preserved:`);
      result.preservedProcesses.forEach(proc => {
        console.log(`  • ${proc.processId}: ${proc.description} (ports: ${proc.ports.join(', ')})`);
      });
    }
  }

  /**
   * Get all processes using JTAG ports
   */
  static async getPortOwners(): Promise<ReadonlyArray<{ readonly port: number; readonly owner: ProcessRegistryEntry | null }>> {
    const registry = getProcessRegistry();
    const activeProcesses = await registry.getActiveProcesses();
    const systemConfig = getSystemConfig();
    
    const targetPorts = [
      systemConfig.getWebSocketPort(),
      systemConfig.getHTTPPort(),
      systemConfig.getUDPConfig().port,
      systemConfig.getUDPConfig().unicastPort
    ].filter(port => port > 0);

    return targetPorts.map(port => ({
      port,
      owner: activeProcesses.find(proc => proc.ports.includes(port)) || null
    }));
  }
}