#!/usr/bin/env tsx
/**
 * System Ready Signaler (Refactored)
 * 
 * Modular, event-driven system readiness detection using configurable milestones.
 * Provides 20ms C++ style responsiveness through file system watchers.
 */

import { promises as fs, watch as fsWatch, FSWatcher } from 'fs';
import * as path from 'path';
import { SystemReadySignal } from '../shared/SystemSignalingTypes';
import { getDefaultMilestoneConfig } from '../shared/MilestoneConfiguration';
import { ProgressCalculator } from '../shared/ProgressCalculator';
import { SystemMetricsCollector } from './SystemMetricsCollector';
import { WorkingDirConfig } from '../../../system/core/config/WorkingDirConfig';

export class SystemReadySignaler {
  private progressCalculator: ProgressCalculator;
  private metricsCollector: SystemMetricsCollector;
  
  // State tracking for intelligent logging (prevent spam)
  private hasSeenFirstSignal = false;
  private hasLoggedStaleFile = false;
  private hasLoggedNoFile = false;
  private hasLoggedNotReady = false;
  private hasLoggedError = false;
  private hasWaitedForBrowser = false;
  private lastNotReadyLog = 0;
  private lastProgressSignal?: SystemReadySignal;
  private lastErrorKey?: string;

  constructor() {
    const milestoneConfig = getDefaultMilestoneConfig();
    this.progressCalculator = new ProgressCalculator(milestoneConfig);
    this.metricsCollector = new SystemMetricsCollector();
  }

  private get signalDir(): string {
    // Use WorkingDirConfig for per-project isolation
    const continuumPath = WorkingDirConfig.getContinuumPath();
    return path.join(continuumPath, 'jtag', 'signals');
  }
  
  private get readyFile(): string {
    const instanceId = this.getInstanceIdentifier();
    return path.join(this.signalDir, `system-ready-${instanceId}.json`);
  }
  
  private get pidFile(): string {
    const instanceId = this.getInstanceIdentifier();
    return path.join(this.signalDir, `system-${instanceId}.pid`);
  }
  
  private getInstanceIdentifier(): string {
    // Use instance ID from multi-instance runner if available
    const instanceId = process.env.JTAG_INSTANCE_ID;
    if (instanceId) {
      return instanceId;
    }
    
    // Fall back to port-based identification for backward compatibility
    const websocketPort = process.env.JTAG_WEBSOCKET_PORT || 
                          process.env.JTAG_EXAMPLE_WEBSOCKET_PORT || '9000';
    return `port-${websocketPort}`;
  }

  async generateReadySignal(): Promise<SystemReadySignal> {
    try {
      // Ensure signal directory exists
      await fs.mkdir(this.signalDir, { recursive: true });

      // AUTOMATIC STALE SIGNAL CLEARING: Always clear before generating new signals
      await this.clearStaleSignals();

      // Gather system readiness metrics
      const signal = await this.metricsCollector.collectSystemMetrics();
      
      // Write signal file atomically
      await this.writeSignalFile(signal);
      
      // Write PID file for system tracking
      await this.writePidFile();
      
      // Only log meaningful state changes, not every check
      const readinessCheck = this.progressCalculator.isSystemReady(signal);
      const instanceId = this.getInstanceIdentifier();
      
      if (readinessCheck.fullyHealthy) {
        console.log(`✅ [${instanceId}] System healthy: ${signal.commandCount} commands ready`);
        console.log(`🌐 [${instanceId}] Active ports: ${signal.portsActive.join(', ')}`);
      } else if (readinessCheck.hasErrors && signal.errors.length > 0) {
        // Only log errors once to avoid spam
        const errorKey = signal.errors.join('|');
        if (!this.lastErrorKey || this.lastErrorKey !== errorKey) {
          console.log(`❌ [${instanceId}] System error: ${signal.errors[0]}`);
          this.lastErrorKey = errorKey;
        }
      }
      
      return signal;
      
    } catch (error: any) {
      console.error('❌ Failed to generate ready signal:', error.message);
      throw error;
    }
  }

  // Event-driven system readiness checking with 20ms C++ style responsiveness
  async checkSystemReady(timeoutMs = 10000): Promise<SystemReadySignal | null> {
    console.log(`🔍 Checking system readiness with event-driven watching (timeout: ${timeoutMs}ms)...`);
    this.showStartupProgress();
    
    return new Promise((resolve) => {
      let watcher: FSWatcher | null = null;
      let isResolved = false;
      let timeoutHandle: NodeJS.Timeout | null = null;
      
      const resolveOnce = (signal: SystemReadySignal | null) => {
        if (isResolved) return;
        isResolved = true;
        if (watcher) {
          try { watcher.close(); } catch {}
        }
        if (timeoutHandle) {
          clearTimeout(timeoutHandle);
        }
        resolve(signal);
      };
      
      // Set up timeout
      timeoutHandle = setTimeout(() => {
        console.log(`⏰ Timeout after ${timeoutMs}ms - system readiness check timed out`);
        resolveOnce(null);
      }, timeoutMs);
      
      // Check existing signal first
      const checkExistingSignal = async (): Promise<boolean> => {
        try {
          const stats = await fs.stat(this.readyFile);
          const ageMs = Date.now() - stats.mtimeMs;
          
          // File age check: automatically clear stale signals (older than 2 minutes)
          if (ageMs > 120000) {
            if (!this.hasLoggedStaleFile) {
              console.log(`🧹 Signal file is stale (${Math.round(ageMs / 1000)}s old), automatically clearing...`);
              this.hasLoggedStaleFile = true;
              
              // AUTOMATIC STALE SIGNAL CLEARING: Clear stale file immediately
              try {
                await this.clearSignals();
                console.log(`✅ Stale signal cleared automatically`);
              } catch (clearError) {
                console.log(`⚠️ Could not clear stale signal: ${clearError}`);
              }
            }
            return false;
          }
          
          // Reset stale flag when we get a fresh file
          this.hasLoggedStaleFile = false;
          
          // Read and parse signal (most expensive operation)
          const signalData = await fs.readFile(this.readyFile, 'utf-8');
          let signalRaw = JSON.parse(signalData);
          
          // Migrate old signal format to new required fields
          let signal: SystemReadySignal = this.migrateSignalFormat(signalRaw);
          
          // Check if this signal was generated by a different process
          if (signal.generatorPid !== process.pid) {
            // This is a signal from another process - mark ourselves as a consumer
            signal = await this.markAsConsumed(signal);
          }
          
          // Update progress display
          this.updateStartupProgress(signal);
          
          // Use modular readiness checking
          const readinessCheck = this.progressCalculator.isSystemReady(signal);
          
          if (readinessCheck.fullyHealthy) {
            // System is fully healthy - resolve immediately and clear signal
            console.log('✅ System fully healthy - resolving immediately');
            const progress = this.progressCalculator.calculateProgress(signal);
            console.log(`📊 Status: health=${signal.systemHealth}, commands=${signal.commandCount}, ports=${signal.portsActive?.length || 0}, browser=${signal.browserReady}`);
            
            // Note: Signal will be cleared by the generator process after all consumers finish
            
            resolveOnce(signal);
            return true;
          } else if (readinessCheck.functionallyReady) {
            // System is functionally ready but degraded - wait a moment for browser to catch up
            if (!this.hasWaitedForBrowser) {
              console.log('⏳ System functionally ready but degraded - waiting 2s for browser warmup...');
              this.hasWaitedForBrowser = true;
              setTimeout(async () => {
                if (isResolved) return;
                // Re-check after browser warmup
                await checkExistingSignal();
              }, 2000);
              return false;
            } else {
              // We've waited, accept degraded as ready
              console.log('✅ System ready (degraded but functional) - resolving after browser warmup');
              const progress = this.progressCalculator.calculateProgress(signal);
              console.log(`📊 Status: health=${signal.systemHealth}, commands=${signal.commandCount}, ports=${signal.portsActive?.length || 0}, browser=${signal.browserReady}`);
              
              // Note: Signal will be cleared by the generator process after all consumers finish
              
              resolveOnce(signal);
              return true;
            }
          } else if (readinessCheck.hasErrors) {
            // Only log errors once
            if (!this.hasLoggedError) {
              console.log(`❌ System in error state: ${signal.errors?.join(', ')}`);
              console.log('🔍 Check logs: npm run signal:logs');
              console.log('🔧 Try restart: npm run system:restart');
              this.hasLoggedError = true;
            }
            resolveOnce(signal);
            return true;
          } else {
            // Only log detailed status occasionally (not on every fast check)
            if (!this.hasLoggedNotReady || Date.now() - this.lastNotReadyLog > 5000) {
              console.log(`⚠️ System not fully ready: ${signal.systemHealth}`);
              const progress = this.progressCalculator.calculateProgress(signal);
              console.log(`   Progress: ${progress.completed}/${progress.total} systems ready`);
              if (signal.autonomousGuidance?.length) {
                console.log('💡 Guidance:', signal.autonomousGuidance[0]);
              }
              this.hasLoggedNotReady = true;
              this.lastNotReadyLog = Date.now();
            }
            // Fast path: return false silently for frequent checks
          }
          
        } catch (error) {
          // Signal file doesn't exist yet or parsing failed
          if (!this.hasLoggedNoFile) {
            console.log(`⚠️ No valid signal file yet, setting up file watcher...`);
            this.hasLoggedNoFile = true;
          }
        }
        return false;
      };
      
      // First check existing signal
      checkExistingSignal().then(found => {
        if (found || isResolved) return;
        
        // Set up file system watcher for the signal directory
        const signalDir = path.dirname(this.readyFile);
        const signalFileName = path.basename(this.readyFile);
        
        // Ensure signal directory exists before watching
        fs.mkdir(signalDir, { recursive: true }).then(() => {
          try {
            watcher = fsWatch(signalDir, { recursive: false }, (eventType, filename) => {
              if (filename === signalFileName && (eventType === 'change' || eventType === 'rename')) {
                // Event-driven with intelligent delays
                // Startup phase: Allow 1s for file write completion
                // Update phase: Respond in 20ms (C++ style)
                const isStartup = !this.hasSeenFirstSignal;
                const delay = isStartup ? 1000 : 20; // 1s grace period for startup, 20ms for updates
                
                setTimeout(async () => {
                  if (isResolved) return;
                  
                  this.hasSeenFirstSignal = true;
                  await checkExistingSignal();
                }, delay);
              }
            });
          } catch (watchError: any) {
            console.log(`⚠️ Failed to set up file watcher, falling back to polling: ${watchError.message}`);
            // Fallback: single poll check
            setTimeout(() => checkExistingSignal(), 1000);
          }
        }).catch(() => {
          // Directory creation failed - just try checking once
          setTimeout(() => checkExistingSignal(), 1000);
        });
      });
    });
  }

  private async writeSignalFile(signal: SystemReadySignal): Promise<void> {
    const tempFile = `${this.readyFile}.tmp`;
    await fs.writeFile(tempFile, JSON.stringify(signal, null, 2));
    await fs.rename(tempFile, this.readyFile);
  }

  private async writePidFile(): Promise<void> {
    await fs.writeFile(this.pidFile, process.pid.toString());
  }

  // Mark a signal as consumed by this process to prevent reuse confusion
  private async markAsConsumed(signal: SystemReadySignal): Promise<SystemReadySignal> {
    try {
      const consumerPids = signal.consumerPids;
      
      // Don't mark if we're already listed as a consumer
      if (consumerPids.includes(process.pid)) {
        return signal;
      }
      
      // Add ourselves to the consumer list
      const updatedSignal: SystemReadySignal = {
        ...signal,
        consumerPids: [...consumerPids, process.pid]
      };
      
      // Write back the updated signal with consumer tracking
      await this.writeSignalFile(updatedSignal);
      
      return updatedSignal;
    } catch (error) {
      // If we can't mark as consumed, just return original signal
      // This prevents blocking on file system issues
      return signal;
    }
  }

  // Clear signal after successful consumption to prevent stale reuse
  async clearSignalAfterConsumption(): Promise<void> {
    try {
      // Wait a brief moment to allow other consumers to mark themselves
      await new Promise(resolve => setTimeout(resolve, 100));
      
      // Read current signal to check consumers
      const signalData = await fs.readFile(this.readyFile, 'utf-8');
      const signalRaw = JSON.parse(signalData);
      const signal: SystemReadySignal = this.migrateSignalFormat(signalRaw);
      
      // Only clear if we're the generator (owns the signal)
      if (signal.generatorPid === process.pid) {
        console.log(`🧹 Clearing consumed signal (${signal.consumerPids.length} consumers marked)`);
        await this.clearSignals();
      }
    } catch (error) {
      // Silent fail - clearing signals is nice-to-have, not critical
    }
  }

  async clearSignals(): Promise<void> {
    try {
      await fs.unlink(this.readyFile);
    } catch {}
    try {
      await fs.unlink(this.pidFile);
    } catch {}
  }

  private updateStartupProgress(signal: SystemReadySignal): void {
    // Only update if something changed
    if (this.lastProgressSignal && 
        this.lastProgressSignal.systemHealth === signal.systemHealth &&
        this.lastProgressSignal.bootstrapComplete === signal.bootstrapComplete &&
        this.lastProgressSignal.commandCount === signal.commandCount &&
        this.lastProgressSignal.browserReady === signal.browserReady) {
      return;
    }
    
    this.lastProgressSignal = signal;
    
    const displayText = this.progressCalculator.formatProgressDisplay(signal, {
      showRequired: true,
      showDetailed: true
    });
    
    console.log(`\r${displayText}`);
    
    if (signal.autonomousGuidance && signal.autonomousGuidance.length > 0) {
      console.log(`   💡 ${signal.autonomousGuidance[0]}`);
    }
  }
  
  private showStartupProgress(): void {
    console.log('────────────────────────────────────────');
    console.log('🎯 JTAG System Readiness Check');
    console.log('📊 Event-driven detection with configurable milestones');
    console.log('⚡ 20ms C++ style responsiveness');
    console.log('────────────────────────────────────────');
  }

  // Migration function to handle backward compatibility with old signal format
  private migrateSignalFormat(signalRaw: any): SystemReadySignal {
    // If the signal already has the required fields, return as-is
    if (signalRaw.generatorPid !== undefined && signalRaw.consumerPids !== undefined) {
      return signalRaw as SystemReadySignal;
    }
    
    // Migrate old format by adding missing required fields
    return {
      ...signalRaw,
      generatorPid: signalRaw.generatorPid || process.pid, // Use current PID if missing
      consumerPids: signalRaw.consumerPids || []           // Empty array if missing
    } as SystemReadySignal;
  }

  // Clear stale signal files for this specific instance
  private async clearStaleSignals(): Promise<void> {
    try {
      const instanceId = this.getInstanceIdentifier();
      
      // Check if current signals are stale before clearing
      let isStale = false;
      try {
        const stats = await fs.stat(this.readyFile);
        const ageMs = Date.now() - stats.mtimeMs;
        isStale = ageMs > 120000; // Older than 2 minutes
      } catch {
        // File doesn't exist - not stale
      }

      // Remove instance-specific signal files if they exist
      await fs.unlink(this.readyFile).catch(() => {});
      await fs.unlink(this.pidFile).catch(() => {});
      
      if (isStale) {
        console.log(`🧹 [${instanceId}] Cleared stale signals (auto-cleanup)`);
      } else {
        console.log(`🧹 [${instanceId}] Cleared signals for fresh generation`);
      }
    } catch (error) {
      // Non-fatal - just log and continue
      console.log(`⚠️ Could not clear stale signals: ${error}`);
    }
  }
}