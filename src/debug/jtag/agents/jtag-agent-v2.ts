#!/usr/bin/env npx tsx
/**
 * JTAG Agent v2 - Modular Real-time Monitoring Dashboard
 * 
 * Features:
 * - Real-time browser connection monitoring
 * - Modular status detection
 * - Live updates every 2 seconds
 * - Clean separation of concerns
 */

import { execSync, spawn, ChildProcess } from 'child_process';
import { existsSync, statSync } from 'fs';

// ============================================================================
// STATUS DETECTION MODULES
// ============================================================================

class SystemStatus {
  static isPortListening(port: number): { listening: boolean, pid?: string, processes?: string[] } {
    try {
      const result = execSync(`lsof -ti:${port}`, { encoding: 'utf8' });
      const pids = result.trim().split('\n').filter(p => p.length > 0);
      if (pids.length > 0) {
        // Get process info
        try {
          const processes = pids.map(pid => {
            const info = execSync(`ps -p ${pid} -o comm=`, { encoding: 'utf8' }).trim();
            return `${info}(${pid})`;
          });
          return { listening: true, pid: pids[0], processes };
        } catch {
          return { listening: true, pid: pids[0] };
        }
      }
      return { listening: false };
    } catch {
      return { listening: false };
    }
  }

  static isHttpResponding(port: number): boolean {
    try {
      execSync(`curl -s --connect-timeout 2 http://localhost:${port} >/dev/null`, { encoding: 'utf8' });
      return true;
    } catch {
      return false;
    }
  }

  static isTmuxRunning(): { running: boolean, sessions?: string[] } {
    try {
      const result = execSync('tmux list-sessions 2>/dev/null', { encoding: 'utf8' });
      const sessions = result.trim().split('\n').filter(s => s.length > 0);
      const jtagSessions = sessions.filter(s => s.match(/jtag-test|jtag-debug/));
      return { running: jtagSessions.length > 0, sessions: jtagSessions };
    } catch {
      return { running: false };
    }
  }

  static hasBrowserConnections(): { connected: boolean, connections?: number } {
    try {
      const result = execSync('lsof -i :9001 2>/dev/null | grep ESTABLISHED', { encoding: 'utf8' });
      const connections = result.trim().split('\n').filter(c => c.length > 0);
      return { connected: connections.length > 0, connections: connections.length };
    } catch {
      return { connected: false, connections: 0 };
    }
  }

  static getJTAGProcesses(): string[] {
    try {
      const result = execSync('ps aux | grep -E "(minimal-server|tsx.*test-bench|jtag)" | grep -v grep', { encoding: 'utf8' });
      return result.trim().split('\n').filter(p => p.length > 0);
    } catch {
      return [];
    }
  }

  static getStartupProgress(): { 
    isStarting: boolean, 
    stage?: string, 
    logTail?: string[], 
    tmuxActive?: boolean,
    buildProcesses?: string[]
  } {
    try {
      // Check if tmux session exists (even if ports aren't up yet)
      let tmuxActive = false;
      try {
        execSync('tmux list-sessions | grep -E "(jtag-test|jtag-debug)"', { encoding: 'utf8' });
        tmuxActive = true;
      } catch {}

      // Check for npm/build processes
      const buildProcesses: string[] = [];
      try {
        const result = execSync('ps aux | grep -E "(npm.*start|npm.*build|tsc|esbuild)" | grep -v grep', { encoding: 'utf8' });
        buildProcesses.push(...result.trim().split('\n').filter(p => p.length > 0));
      } catch {}

      // Check startup log for recent activity
      const logPath = '.continuum/jtag/system/logs/npm-start.log';
      let logTail: string[] = [];
      let stage = 'unknown';
      
      if (existsSync(logPath)) {
        try {
          const result = execSync(`tail -10 "${logPath}"`, { encoding: 'utf8' });
          logTail = result.trim().split('\n').filter(l => l.length > 0);
          
          // Determine stage from log content
          const logContent = logTail.join(' ');
          if (logContent.includes('system:stop')) stage = 'stopping';
          else if (logContent.includes('clean:all')) stage = 'cleaning';
          else if (logContent.includes('build')) stage = 'building';
          else if (logContent.includes('version:bump')) stage = 'versioning';
          else if (logContent.includes('npm pack')) stage = 'packaging';
          else if (logContent.includes('npm install')) stage = 'installing';
          else if (logContent.includes('test-bench')) stage = 'starting-server';
          else if (logContent.includes('Listening on')) stage = 'server-ready';
        } catch {}
      }

      const isStarting = tmuxActive || buildProcesses.length > 0 || (logTail.length > 0 && !logTail.join(' ').includes('error'));
      
      return { 
        isStarting, 
        stage, 
        logTail: logTail.slice(-3), // Last 3 lines
        tmuxActive,
        buildProcesses: buildProcesses.slice(0, 2) // First 2 processes
      };
    } catch {
      return { isStarting: false };
    }
  }

  static getLogInfo(path: string) {
    if (!existsSync(path)) {
      return { exists: false, size: 0, ageMinutes: -1 };
    }
    
    const stats = statSync(path);
    const ageMinutes = Math.round((Date.now() - stats.mtime.getTime()) / (1000 * 60));
    const sizeKB = Math.round(stats.size / 1024);
    
    return { exists: true, size: sizeKB, ageMinutes };
  }

  static getFullStatus() {
    const tmux = this.isTmuxRunning();
    const port9001 = this.isPortListening(9001);
    const port9002 = this.isPortListening(9002);
    const http9002 = this.isHttpResponding(9002);
    const browser = this.hasBrowserConnections();
    const processes = this.getJTAGProcesses();
    const startup = this.getStartupProgress();
    const online = port9001.listening && port9002.listening && http9002;

    return { 
      tmux, 
      port9001, 
      port9002, 
      http9002,
      browser, 
      processes,
      startup,
      online 
    };
  }
}

// ============================================================================
// DISPLAY MODULES  
// ============================================================================

class StatusDisplay {
  static showStatus(status: ReturnType<typeof SystemStatus.getFullStatus>) {
    console.clear();
    console.log('🤖 JTAG Real-time Monitoring Dashboard');
    console.log(`⏰ ${new Date().toLocaleTimeString()}\n`);
    
    console.log('🔌 LIVE SYSTEM STATUS');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    
    // Tmux status with details
    if (status.tmux.running) {
      console.log(`   🪟 Tmux Session: ✅ Running (${status.tmux.sessions?.length || 0} sessions)`);
      if (status.tmux.sessions?.length) {
        status.tmux.sessions.slice(0, 2).forEach(session => {
          console.log(`      • ${session.split(':')[0]}`);
        });
      }
    } else {
      console.log(`   🪟 Tmux Session: ❌ Not running`);
    }
    
    // WebSocket port with process info
    if (status.port9001.listening) {
      const processInfo = status.port9001.processes ? ` (${status.port9001.processes.join(', ')})` : '';
      console.log(`   🔌 WebSocket (9001): ✅ Listening${processInfo}`);
      
      // Browser connections
      if (status.browser.connected) {
        console.log(`   🦊 Browser Connected: ✅ ${status.browser.connections} active connection(s)`);
      } else {
        console.log(`   🦊 Browser Connected: ❌ No WebSocket connections`);
      }
    } else {
      console.log(`   🔌 WebSocket (9001): ❌ Not listening`);
    }
    
    // HTTP port with process info and response test
    if (status.port9002.listening) {
      const processInfo = status.port9002.processes ? ` (${status.port9002.processes.join(', ')})` : '';
      const responding = status.http9002 ? '✅ Responding' : '⚠️ Not responding';
      console.log(`   🌐 HTTP (9002): ✅ Listening${processInfo}`);
      console.log(`   📡 HTTP Response: ${responding}`);
    } else {
      console.log(`   🌐 HTTP (9002): ❌ Not listening`);
    }
    
    // Overall status with startup awareness
    if (status.online) {
      console.log(`   📊 JTAG System: ✅ FULLY ONLINE`);
    } else if (status.startup.isStarting) {
      console.log(`   📊 JTAG System: 🔄 STARTING (${status.startup.stage || 'initializing'})`);
    } else {
      console.log(`   📊 JTAG System: ❌ OFFLINE`);
    }
    
    // Startup progress section
    if (status.startup.isStarting && !status.online) {
      console.log('\n🚀 STARTUP PROGRESS');
      console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
      
      const stageEmojis = {
        'stopping': '🛑 Stopping previous instance',
        'cleaning': '🧹 Cleaning build artifacts', 
        'building': '🔨 Building TypeScript',
        'versioning': '📋 Bumping version',
        'packaging': '📦 Creating package',
        'installing': '📥 Installing dependencies',
        'starting-server': '🚀 Starting server',
        'server-ready': '✅ Server ready'
      };
      
      const stageText = stageEmojis[status.startup.stage as keyof typeof stageEmojis] || `🔄 ${status.startup.stage}`;
      console.log(`   Current: ${stageText}`);
      
      if (status.startup.tmuxActive) {
        console.log(`   📺 Tmux session active - system is starting up`);
      }
      
      if (status.startup.buildProcesses && status.startup.buildProcesses.length > 0) {
        console.log(`   🔧 Active build processes: ${status.startup.buildProcesses.length}`);
      }
      
      if (status.startup.logTail && status.startup.logTail.length > 0) {
        console.log(`   📝 Recent activity:`);
        status.startup.logTail.forEach(line => {
          const shortLine = line.length > 50 ? line.substring(0, 47) + '...' : line;
          console.log(`      ${shortLine}`);
        });
      }
      
      console.log(`   ⏳ Please wait for system to come online...`);
    }
    
    // Process information (condensed when starting)
    if (status.processes.length > 0) {
      const sectionTitle = status.startup.isStarting ? '🔧 KEY PROCESSES' : '🔧 RUNNING PROCESSES';
      console.log(`\n${sectionTitle}`);
      console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
      
      const processesToShow = status.startup.isStarting ? 2 : 3;
      status.processes.slice(0, processesToShow).forEach(process => {
        const parts = process.split(/\s+/);
        const pid = parts[1];
        const command = parts.slice(10).join(' ').substring(0, 60);
        console.log(`   🔹 PID ${pid}: ${command}`);
      });
      if (status.processes.length > processesToShow) {
        console.log(`   ... and ${status.processes.length - processesToShow} more processes`);
      }
    }
    
    // Smart recommendations based on system state
    if (status.online) {
      console.log('\n   🎯 Ready for:');
      console.log('      • Browser UI: http://localhost:9002');
      if (status.browser.connected) {
        console.log('      • Browser is connected and working');
      }
      console.log('      • Terminal commands: npx tsx test-server-client.ts');
    } else if (status.startup.isStarting) {
      console.log('\n   ⏰ System is starting up - please wait or:');
      console.log('      • Watch progress: tmux attach -t jtag-test');
      console.log('      • Monitor logs: tail -f .continuum/jtag/system/logs/npm-start.log');
      console.log('      • Check status: Keep this monitor running');
    } else {
      console.log('\n   🚀 System is offline. You can:');
      console.log('      • Press [s] to start system now');
      console.log('      • Run: npm run system:start');
      console.log('      • Or wait - I can start it for you!');
    }
  }

  static showLogs() {
    console.log('\n📄 LOG STATUS');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    
    const logs = [
      '.continuum/jtag/system/logs/npm-start.log',
      'examples/test-bench/.continuum/jtag/system/logs/server-node-output.log'
    ];
    
    logs.forEach(path => {
      const info = SystemStatus.getLogInfo(path);
      if (info.exists) {
        console.log(`   📝 ${path} (${info.size}KB, ${info.ageMinutes}min ago)`);
      }
    });
  }

  static showCommands(status: ReturnType<typeof SystemStatus.getFullStatus>) {
    console.log('\n🚀 QUICK COMMANDS');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    
    if (status.online) {
      console.log('   [t] Test client     [r] Restart        [q] Quit monitor');
      console.log('   [l] Show logs       [b] Build          [s] Status');
    } else if (status.startup.isStarting) {
      console.log('   [l] Show logs       [q] Quit monitor    [r] Restart');
      console.log('   [a] Attach tmux     [k] Kill & restart  [w] Wait');
    } else {
      console.log('   [s] Start system    [q] Quit monitor    [b] Build first');
      console.log('   [l] Show logs       [c] Clean & start   [h] Help');
    }
    
    console.log('   Press any key for command...');
  }
}

// ============================================================================
// REAL-TIME MONITORING ENGINE
// ============================================================================

class RealTimeMonitor {
  private running = false;
  private interval?: NodeJS.Timeout;
  private lastStatus?: ReturnType<typeof SystemStatus.getFullStatus>;

  start() {
    this.running = true;
    console.log('🔄 Starting real-time monitoring...\n');
    
    // Show initial status
    this.updateStatus();
    
    // Start monitoring loop
    this.interval = setInterval(() => {
      this.updateStatus();
    }, 2000); // Update every 2 seconds
    
    // Handle keyboard input (only in TTY mode)
    if (process.stdin.isTTY) {
      process.stdin.setRawMode(true);
      process.stdin.resume();
      process.stdin.on('data', (key) => {
        this.handleKeyPress(key.toString());
      });
    }
    
    // Handle Ctrl+C
    process.on('SIGINT', () => {
      this.stop();
    });
  }

  private updateStatus() {
    const status = SystemStatus.getFullStatus();
    
    // Only refresh display if something changed
    if (!this.lastStatus || this.hasStatusChanged(status, this.lastStatus)) {
      StatusDisplay.showStatus(status);
      StatusDisplay.showLogs();
      StatusDisplay.showCommands();
      
      if (this.lastStatus && status.browser !== this.lastStatus.browser) {
        const msg = status.browser ? '🦊 Browser CONNECTED' : '🦊 Browser DISCONNECTED';
        console.log(`\n⬇️  CHANGE: ${msg}`);
      }
    }
    
    this.lastStatus = status;
  }

  private hasStatusChanged(current: any, last: any): boolean {
    return Object.keys(current).some(key => current[key] !== last[key]);
  }

  private handleKeyPress(key: string) {
    switch (key.toLowerCase()) {
      case 's':
        this.executeCommand('npm run system:start');
        break;
      case 'r':
        this.executeCommand('npm run system:restart');
        break;
      case 't':
        this.executeCommand('npx tsx test-server-client.ts');
        break;
      case 'b':
        this.executeCommand('npm run build');
        break;
      case 'l':
        this.showLiveLogs();
        break;
      case 'q':
      case '\u0003': // Ctrl+C
        this.stop();
        break;
    }
  }

  private executeCommand(cmd: string) {
    console.log(`\n🚀 Executing: ${cmd}`);
    const parts = cmd.split(' ');
    spawn(parts[0], parts.slice(1), { detached: true, stdio: 'ignore' }).unref();
  }

  private showLiveLogs() {
    console.log('\n📋 Live logs: tail -f .continuum/jtag/system/logs/npm-start.log');
  }

  stop() {
    this.running = false;
    if (this.interval) {
      clearInterval(this.interval);
    }
    
    process.stdin.setRawMode(false);
    process.stdin.pause();
    
    console.log('\n👋 Real-time monitoring stopped');
    process.exit(0);
  }
}

// ============================================================================
// MAIN ENTRY POINT
// ============================================================================

async function main() {
  const args = process.argv.slice(2);
  
  if (args.includes('--help') || args.includes('-h')) {
    console.log(`
🤖 JTAG Agent v2 - Real-time Monitoring Dashboard

Usage: 
  npm run agent:live        # Real-time monitoring with live updates
  npm run agent            # Single status check (original)

Features:
  • Real-time browser connection monitoring
  • Live status updates every 2 seconds  
  • Interactive keyboard commands
  • Modular, clean architecture
    `);
    return;
  }
  
  if (args.includes('--live') || args.includes('-l')) {
    const monitor = new RealTimeMonitor();
    monitor.start();
  } else {
    // Single status check (original behavior)
    const status = SystemStatus.getFullStatus();
    StatusDisplay.showStatus(status);
    StatusDisplay.showLogs();
  }
}

// Export modules for testing
export { SystemStatus, StatusDisplay, RealTimeMonitor };

// Run if called directly
if (require.main === module) {
  main().catch(console.error);
}