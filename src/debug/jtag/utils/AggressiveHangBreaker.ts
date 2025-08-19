#!/usr/bin/env tsx
/**
 * Aggressive Hang Breaker - FORCES process exit when tests hang
 * 
 * This tool guarantees no test will hang for more than 30 seconds.
 * It immediately breaks hanging processes with full diagnostic output.
 */

export class AggressiveHangBreaker {
  private startTime = Date.now();
  private forceTimer: NodeJS.Timeout;
  private warningTimer: NodeJS.Timeout;
  
  constructor(private testName: string, private maxHangTimeMs = 30000) {
    console.log(`⚡ HANG BREAKER: Armed for ${testName} (${maxHangTimeMs/1000}s limit)`);
    this.armBreaker();
  }

  private armBreaker(): void {
    // Warning at 15 seconds
    this.warningTimer = setTimeout(() => {
      const elapsed = Date.now() - this.startTime;
      console.log(`\n⚠️  HANG WARNING [${Math.round(elapsed/1000)}s]: ${this.testName}`);
      console.log('🔍 Checking for hanging operations...');
      this.showEmergencyDiagnostics();
    }, Math.max(15000, this.maxHangTimeMs / 2));

    // FORCE EXIT at timeout
    this.forceTimer = setTimeout(() => {
      const elapsed = Date.now() - this.startTime;
      
      console.log(`\n💥💥💥 HANG BREAKER ACTIVATED 💥💥💥`);
      console.log(`🔥 FORCE TERMINATING: ${this.testName}`);
      console.log(`⏱️  Hung for: ${Math.round(elapsed/1000)} seconds`);
      console.log(`🚫 Maximum hang time exceeded: ${this.maxHangTimeMs/1000}s`);
      
      this.showEmergencyDiagnostics();
      
      console.log(`\n🛑 TERMINATING PROCESS NOW - NO MORE SILENT HANGS`);
      process.exit(1);
      
    }, this.maxHangTimeMs);
  }

  private showEmergencyDiagnostics(): void {
    try {
      const { execSync } = require('child_process');
      
      console.log('\n🚨 EMERGENCY HANG DIAGNOSTICS:');
      console.log('================================');
      
      // System health check
      try {
        const health = execSync('npm run signal:check 2>/dev/null || echo "Health check failed"', {
          encoding: 'utf8',
          timeout: 3000
        });
        console.log(`🏥 System Health: ${health.includes('ready') ? 'READY' : 'DEGRADED/FAILED'}`);
      } catch {
        console.log('🏥 System Health: CHECK FAILED');
      }
      
      // WebSocket status
      try {
        const ports = execSync('lsof -iTCP -sTCP:LISTEN | grep ":900[0-9]" | wc -l', {
          encoding: 'utf8',
          timeout: 2000
        });
        console.log(`🔌 WebSocket Ports: ${ports.trim()} active`);
      } catch {
        console.log('🔌 WebSocket Ports: CHECK FAILED');
      }
      
      // Active processes
      try {
        const processes = execSync('ps aux | grep -E "(screenshot|tsx)" | grep -v grep | wc -l', {
          encoding: 'utf8', 
          timeout: 2000
        });
        console.log(`🔄 Screenshot Processes: ${processes.trim()} running`);
      } catch {
        console.log('🔄 Screenshot Processes: CHECK FAILED');
      }
      
      console.log('\n💡 IMMEDIATE ACTION REQUIRED:');
      console.log('• This test is confirmed hanging');
      console.log('• Check system logs: npm run logs:current');
      console.log('• Restart system: npm run system:restart');
      console.log('• Apply hang detection to prevent future hangs');
      
    } catch (error) {
      console.log('❌ Diagnostics failed:', error.message);
    }
  }

  disarm(): void {
    if (this.warningTimer) clearTimeout(this.warningTimer);
    if (this.forceTimer) clearTimeout(this.forceTimer);
    
    const elapsed = Date.now() - this.startTime;
    console.log(`✅ HANG BREAKER: Disarmed for ${this.testName} (completed in ${Math.round(elapsed/1000)}s)`);
  }
}

/**
 * Wraps any function with aggressive hang breaking
 */
export async function withHangBreaker<T>(
  testName: string, 
  fn: () => Promise<T>, 
  maxHangTimeMs = 30000
): Promise<T> {
  const breaker = new AggressiveHangBreaker(testName, maxHangTimeMs);
  
  try {
    const result = await fn();
    breaker.disarm();
    return result;
  } catch (error) {
    breaker.disarm();
    throw error;
  }
}