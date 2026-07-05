#!/usr/bin/env tsx
/**
 * Immediate Hang Killer - ACTUALLY kills hanging processes
 * 
 * The previous hang breaker was too gentle - this one FORCES termination
 */

import { spawn } from 'child_process';

export class ImmediateHangKiller {
  private killTimer: NodeJS.Timeout;
  private warningTimer: NodeJS.Timeout;
  private startTime = Date.now();
  
  constructor(private testName: string, private timeoutMs = 20000) {
    console.log(`💀 HANG KILLER: Armed for ${testName} - WILL FORCE KILL after ${timeoutMs/1000}s`);
    this.armKiller();
  }

  private armKiller(): void {
    // Warning at 10 seconds
    this.warningTimer = setTimeout(() => {
      console.log(`\n⚠️  HANG WARNING: ${this.testName} running for 10 seconds`);
      console.log('💀 WILL FORCE KILL in 10 more seconds if not completed');
    }, 10000);

    // IMMEDIATE KILL at timeout
    this.killTimer = setTimeout(() => {
      console.log(`\n💥💥💥 HANG KILLER ACTIVATED 💥💥💥`);
      console.log(`🔥 FORCE KILLING: ${this.testName} after ${this.timeoutMs/1000}s`);
      
      // Kill this process and all child processes
      this.forceKillEverything();
      
    }, this.timeoutMs);
  }

  private forceKillEverything(): void {
    console.log('🔥 Killing current process and all children...');
    
    try {
      // Kill all screenshot/tsx processes
      spawn('pkill', ['-f', 'screenshot'], { stdio: 'inherit' });
      spawn('pkill', ['-f', 'tsx'], { stdio: 'inherit' });
      
      // Wait a moment then force exit
      setTimeout(() => {
        console.log('💀 FORCE EXIT NOW - NO MORE HANGS');
        process.exit(1);
      }, 1000);
      
    } catch (error) {
      // If spawn fails, just force exit
      console.log('💀 EMERGENCY FORCE EXIT');
      process.exit(1);
    }
  }

  disarm(): void {
    if (this.warningTimer) clearTimeout(this.warningTimer);
    if (this.killTimer) clearTimeout(this.killTimer);
    
    const elapsed = Date.now() - this.startTime;
    console.log(`✅ HANG KILLER: Disarmed for ${this.testName} (${elapsed}ms)`);
  }
}

/**
 * Wraps any function with immediate hang killing
 */
export async function withImmediateKill<T>(
  testName: string, 
  fn: () => Promise<T>, 
  timeoutMs = 20000
): Promise<T> {
  const killer = new ImmediateHangKiller(testName, timeoutMs);
  
  try {
    const result = await fn();
    killer.disarm();
    return result;
  } catch (error) {
    killer.disarm();
    throw error;
  }
}