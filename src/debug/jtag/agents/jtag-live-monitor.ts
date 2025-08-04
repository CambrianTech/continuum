#!/usr/bin/env npx tsx
/**
 * JTAG Live Monitor - One-screen stateful real-time dashboard
 * Maintains system state and shows transitions live
 */

import { execSync } from 'child_process';
import { createInterface } from 'readline';

interface SystemState {
  tmux: boolean;
  port9001: boolean;
  port9002: boolean;
  browser: boolean;
  clients: Array<{type: string, pid: number, process: string}>;
  timestamp: Date;
}

interface StateChange {
  field: keyof SystemState;
  from: any;
  to: any;
  timestamp: Date;
}

class JTAGLiveMonitor {
  private currentState: SystemState;
  private lastState: SystemState;
  private changes: StateChange[] = [];
  private startTime = new Date();
  private updateCount = 0;

  constructor() {
    this.currentState = this.getSystemState();
    this.lastState = { ...this.currentState };
  }

  private getSystemState(): SystemState {
    const tmux = this.checkTmux();
    const port9001 = this.checkPort(9001);
    const port9002 = this.checkPort(9002);
    const browser = this.checkBrowser();
    const clients = this.getClients();

    return { tmux, port9001, port9002, browser, clients, timestamp: new Date() };
  }

  private checkTmux(): boolean {
    try {
      execSync('tmux list-sessions | grep -E "(jtag-test|jtag-debug)"', { encoding: 'utf8' });
      return true;
    } catch { return false; }
  }

  private checkPort(port: number): boolean {
    try {
      const result = execSync(`lsof -ti:${port}`, { encoding: 'utf8' });
      return result.trim().length > 0;
    } catch { return false; }
  }

  private checkBrowser(): boolean {
    try {
      const result = execSync('lsof -i :9001 | grep ESTABLISHED', { encoding: 'utf8' });
      return result.trim().length > 0;
    } catch { return false; }
  }

  private getClients(): Array<{type: string, pid: number, process: string}> {
    try {
      const result = execSync('lsof -i :9001 -i :9002 | grep ESTABLISHED', { encoding: 'utf8' });
      const lines = result.trim().split('\n').filter(line => line.length > 0);
      
      return lines.map(line => {
        const parts = line.trim().split(/\s+/);
        const processName = parts[0] || 'unknown';
        const pid = parseInt(parts[1]) || 0;
        
        let type = 'server';
        let process = processName;
        
        if (processName.toLowerCase().includes('opera') || processName.toLowerCase().includes('chrome')) {
          type = 'browser';
          process = processName;
        } else if (processName === 'node') {
          try {
            const cmdLine = execSync(`ps -p ${pid} -o args=`, { encoding: 'utf8', timeout: 500 });
            if (cmdLine.includes('test-server-client') || cmdLine.includes('JTAGClient')) {
              type = 'external';
              process = 'JTAGClient';
            } else if (cmdLine.includes('test-bench')) {
              type = 'server';
              process = 'JTAG-server';
            }
          } catch {}
        }
        
        return { type, pid, process };
      });
    } catch {
      return [];
    }
  }

  private detectChanges(): StateChange[] {
    const changes: StateChange[] = [];
    const now = new Date();

    // Check simple boolean fields
    (['tmux', 'port9001', 'port9002', 'browser'] as const).forEach(field => {
      if (this.currentState[field] !== this.lastState[field]) {
        changes.push({
          field,
          from: this.lastState[field],
          to: this.currentState[field],
          timestamp: now
        });
      }
    });

    // Check client changes
    const oldClients = this.lastState.clients.length;
    const newClients = this.currentState.clients.length;
    if (oldClients !== newClients) {
      changes.push({
        field: 'clients',
        from: oldClients,
        to: newClients,
        timestamp: now
      });
    }

    return changes;
  }

  private getLogStatus(): {browser: boolean, server: boolean, system: boolean} {
    const basePath = '.continuum/jtag';
    try {
      const browserLog = `${basePath}/currentUser/logs/browser-console-log.log`;
      const serverLog = `${basePath}/system/logs/server-console-log.log`;
      const systemLog = `${basePath}/system/logs/npm-start.log`;
      
      return {
        browser: require('fs').existsSync(browserLog),
        server: require('fs').existsSync(serverLog),
        system: require('fs').existsSync(systemLog)
      };
    } catch {
      return { browser: false, server: false, system: false };
    }
  }

  private render(): void {
    // Clear and position cursor
    process.stdout.write('\x1b[2J\x1b[H');
    
    const now = new Date();
    const uptime = Math.floor((now.getTime() - this.startTime.getTime()) / 1000);
    const online = this.currentState.port9001 && this.currentState.port9002;
    const logs = this.getLogStatus();
    
    // Header line
    console.log(`🚀 JTAG Live Monitor │ ${online ? '🟢 ONLINE' : '🔴 OFFLINE'} │ ↻${this.updateCount} │ ⏱${uptime}s`);
    console.log('━'.repeat(72));
    
    // System status line
    const tmuxIcon = this.currentState.tmux ? '🪟✅' : '🪟❌';
    const wsIcon = this.currentState.port9001 ? '🔌✅' : '🔌❌';
    const httpIcon = this.currentState.port9002 ? '🌐✅' : '🌐❌';
    const browserIcon = this.currentState.browser ? '🦊✅' : '🦊❌';
    
    console.log(`${tmuxIcon} tmux │ ${wsIcon} :9001 │ ${httpIcon} :9002 │ ${browserIcon} browser`);
    
    // Log status line
    const browserLogIcon = logs.browser ? '📋✅' : '📋❌';
    const serverLogIcon = logs.server ? '📊✅' : '📊❌';  
    const systemLogIcon = logs.system ? '📝✅' : '📝❌';
    
    console.log(`${browserLogIcon} browser │ ${serverLogIcon} server │ ${systemLogIcon} system │ 📂 logs`);
    
    // Clients section
    if (this.currentState.clients.length > 0) {
      console.log('━'.repeat(72));
      console.log('📱 CONNECTED CLIENTS');
      this.currentState.clients.forEach(client => {
        const icon = client.type === 'browser' ? '🌐' : client.type === 'external' ? '🔗' : '⚙️';
        console.log(`   ${icon} ${client.process.padEnd(12)} │ PID ${client.pid}`);
      });
    }
    
    // Recent changes
    if (this.changes.length > 0) {
      console.log('━'.repeat(72));
      console.log('📈 RECENT CHANGES');
      this.changes.slice(-3).forEach(change => {
        const time = change.timestamp.toLocaleTimeString();
        const arrow = change.to ? '🟢' : '🔴';
        console.log(`   ${time} │ ${arrow} ${change.field}: ${change.from} → ${change.to}`);
      });
    }
    
    // Commands with log commands
    console.log('━'.repeat(72));
    console.log('⌨️  [s]tart [r]estart [q]uit [t]est [c]lear [l]ogs │ Updates: 2s');
  }

  private handleCommand(key: string): void {
    switch(key.toLowerCase()) {
      case 's':
        console.log('🚀 Starting system...');
        execSync('npm run system:start', { stdio: 'ignore' });
        break;
      case 'r':
        console.log('🔄 Restarting system...');
        execSync('npm run system:restart', { stdio: 'ignore' });
        break;
      case 't':
        console.log('🧪 Testing correlation...');
        execSync('npx tsx test-server-client.ts', { stdio: 'ignore' });
        break;
      case 'c':
        this.changes = [];
        break;
      case 'l':
        console.log('📂 Available log files via symlinks:');
        console.log('   Browser: tail -f .continuum/jtag/currentUser/logs/browser-console-log.log');
        console.log('   Server:  tail -f .continuum/jtag/system/logs/server-console-log.log');
        console.log('   System:  tail -f .continuum/jtag/system/logs/npm-start.log');
        console.log('   All:     tail -f .continuum/jtag/*/logs/*.log');
        console.log('Press any key to continue...');
        break;
      case 'q':
        process.exit(0);
        break;
    }
  }

  async start(): Promise<void> {
    // Setup keyboard input - check if stdin supports raw mode
    if (process.stdin.isTTY && process.stdin.setRawMode) {
      process.stdin.setRawMode(true);
      process.stdin.resume();
      process.stdin.on('data', (key) => {
        this.handleCommand(key.toString());
      });
    } else {
      // Fallback to readline for non-TTY environments
      const rl = createInterface({
        input: process.stdin,
        output: process.stdout
      });
      
      rl.on('line', (input) => {
        if (input.length > 0) {
          this.handleCommand(input[0]);
        }
      });
    }

    // Main monitoring loop
    setInterval(() => {
      this.lastState = { ...this.currentState };
      this.currentState = this.getSystemState();
      
      const newChanges = this.detectChanges();
      this.changes.push(...newChanges);
      
      // Keep only last 10 changes
      if (this.changes.length > 10) {
        this.changes = this.changes.slice(-10);
      }
      
      this.updateCount++;
      this.render();
    }, 2000);

    // Initial render
    this.render();
  }
}

// Run if called directly
if (require.main === module) {
  const monitor = new JTAGLiveMonitor();
  monitor.start();
}