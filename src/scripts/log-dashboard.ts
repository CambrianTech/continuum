#!/usr/bin/env tsx
/**
 * JTAG Intelligent Log Dashboard
 * 
 * Uses agent detection to provide the right interface:
 * 
 * 🤖 FOR AI AGENTS:
 *   - Structured, filtered log output
 *   - Key events and errors only
 *   - Machine-readable format
 *   - Focused on actionable information
 * 
 * 👤 FOR HUMANS:
 *   - Clean single-pane tmux session
 *   - Contextual log switching (npm -> browser -> server)  
 *   - Visual highlights and colors
 *   - Interactive commands
 * 
 * Usage:
 *   npm run logs:dashboard    # Auto-detect and start appropriate interface
 *   npm run logs:ai          # Force AI-friendly output
 *   npm run logs:human       # Force human-friendly tmux interface
 */

import { spawn, ChildProcess } from 'child_process';
import { promises as fs } from 'fs';
import path from 'path';
import { AgentDetector } from '../system/core/detection/AgentDetector';
import { SystemPaths } from '../system/core/config/SystemPaths';

// Dashboard configuration
interface DashboardConfig {
  readonly sessionName: string;
  readonly logFiles: {
    readonly npm: string;
    readonly browser: string;
    readonly server: string;
    readonly system: string;
  };
  readonly refreshRate: number; // seconds
}

// AI-friendly log event structure
interface LogEvent {
  readonly timestamp: string;
  readonly source: 'npm' | 'browser' | 'server' | 'system';
  readonly level: 'info' | 'warn' | 'error' | 'debug';
  readonly message: string;
  readonly category?: string;
  readonly actionable?: boolean;
}

const CONFIG: DashboardConfig = {
  sessionName: 'jtag-test',  // Use same session as main system
  logFiles: {
    npm: path.join(SystemPaths.logs.system, 'npm-start.log'),
    browser: path.join(SystemPaths.sessions.user, 'logs', 'browser-console-log.log'),
    server: path.join(SystemPaths.sessions.user, 'logs', 'server-console-log.log'),
    system: path.join(SystemPaths.registry.root, 'system-ready.json')
  },
  refreshRate: 2 // 2 second refresh for better readability
} as const;

class LogDashboard {
  private sessionName: string;
  private config: DashboardConfig;

  constructor(config: DashboardConfig = CONFIG) {
    this.config = config;
    this.sessionName = config.sessionName;
  }

  /**
   * Check if log files exist and create them if missing
   */
  private async ensureLogFiles(): Promise<void> {
    for (const [name, filePath] of Object.entries(this.config.logFiles)) {
      try {
        await fs.stat(filePath);
      } catch (error) {
        // Create directory structure if needed
        const dir = path.dirname(filePath);
        await fs.mkdir(dir, { recursive: true });
        
        // Create empty log file with helpful placeholder
        if (name === 'system') {
          await fs.writeFile(filePath, '{"status": "waiting for system startup..."}');
        } else {
          await fs.writeFile(filePath, `# ${name.toUpperCase()} LOGS - Waiting for system startup...\n`);
        }
      }
    }
  }

  /**
   * Parse log line into structured event for AI agents
   */
  private parseLogEvent(line: string, source: 'npm' | 'browser' | 'server' | 'system'): LogEvent | null {
    // Skip empty lines and noise
    if (!line.trim() || line.includes('GET /') || line.includes('Served ../public')) {
      return null;
    }

    const timestamp = new Date().toISOString();
    let level: 'info' | 'warn' | 'error' | 'debug' = 'info';
    let category: string | undefined;
    let actionable = false;

    // Determine level and category
    if (line.includes('❌') || line.includes('ERROR') || line.includes('error')) {
      level = 'error';
      actionable = true;
    } else if (line.includes('⚠️') || line.includes('WARN') || line.includes('warn')) {
      level = 'warn';
      actionable = true;
    } else if (line.includes('✅') || line.includes('SUCCESS')) {
      level = 'info';
      category = 'completion';
    } else if (line.includes('🔨') || line.includes('build')) {
      category = 'build';
    } else if (line.includes('Bootstrap complete')) {
      level = 'info';
      category = 'bootstrap';
      actionable = true;
    }

    // Clean up the message
    const cleanMessage = line
      .replace(/^\s*>\s*/, '') // Remove npm output prefix
      .replace(/\[\d+m/g, '')  // Remove ANSI codes
      .replace(/\x1b\[[0-9;]*m/g, '') // Remove more ANSI codes
      .trim();

    return {
      timestamp,
      source,
      level,
      message: cleanMessage,
      category,
      actionable
    };
  }

  /**
   * Get filtered log events for AI agents (last 50 important events)
   */
  private async getFilteredLogEvents(): Promise<LogEvent[]> {
    const events: LogEvent[] = [];
    
    // Read recent lines from each log file
    for (const [source, filePath] of Object.entries(this.config.logFiles)) {
      try {
        const content = await fs.readFile(filePath, 'utf-8');
        const lines = content.split('\n').slice(-20); // Last 20 lines per source
        
        for (const line of lines) {
          const event = this.parseLogEvent(line, source as any);
          if (event && (event.actionable || event.level === 'error' || event.category)) {
            events.push(event);
          }
        }
      } catch {
        // Log file doesn't exist or can't be read
      }
    }

    // Sort by timestamp and return last 50
    return events.sort((a, b) => a.timestamp.localeCompare(b.timestamp)).slice(-50);
  }

  /**
   * AI-friendly structured log output
   */
  async showAILogs(): Promise<void> {
    console.log('🤖 AI-FRIENDLY LOG STREAM');
    console.log('================================');
    
    // Ensure log files exist
    await this.ensureLogFiles();
    
    const events = await this.getFilteredLogEvents();
    
    if (events.length === 0) {
      console.log('{"status": "no_recent_events", "message": "System starting or no actionable events"}');
      return;
    }

    console.log(`📊 RECENT IMPORTANT EVENTS (${events.length}):`);
    console.log('');
    
    for (const event of events) {
      const indicator = event.level === 'error' ? '❌' : 
                       event.level === 'warn' ? '⚠️' : 
                       event.actionable ? '🎯' : '📋';
      
      console.log(`${indicator} [${event.source.toUpperCase()}] ${event.message}`);
      if (event.category) {
        console.log(`   Category: ${event.category}`);
      }
    }
    
    console.log('');
    console.log('📋 SUMMARY:');
    const errorCount = events.filter(e => e.level === 'error').length;
    const warnCount = events.filter(e => e.level === 'warn').length;
    const actionableCount = events.filter(e => e.actionable).length;
    
    console.log(`   Errors: ${errorCount}, Warnings: ${warnCount}, Actionable: ${actionableCount}`);
    
    if (errorCount > 0) {
      console.log('');
      console.log('🚨 IMMEDIATE ACTIONS NEEDED:');
      events
        .filter(e => e.level === 'error')
        .forEach(e => console.log(`   • Fix: ${e.message} (${e.source})`));
    }
  }

  /**
   * Human-friendly single tmux pane with contextual switching
   */
  async createHumanDashboard(): Promise<void> {
    console.log('👤 Creating Human-Friendly Log Dashboard...');
    
    // Ensure log files exist
    await this.ensureLogFiles();
    
    // Check if main system session exists
    const systemSessionExists = await this.sessionExists();
    
    let cmd: string[];
    if (systemSessionExists) {
      // Add a new window to the existing system session
      console.log('🔗 Adding dashboard window to existing JTAG system session...');
      cmd = [
        'new-window', 
        '-t', this.sessionName,
        '-n', 'logs',  // Window name
        `tail -f ${this.config.logFiles.npm}`
      ];
    } else {
      // Create new session if system isn't running
      console.log('🚀 Creating new JTAG session with dashboard...');
      cmd = [
        'new-session',
        '-d', 
        '-s', this.sessionName,
        `tail -f ${this.config.logFiles.npm}`
      ];
    }
    
    await new Promise<void>((resolve, reject) => {
      const process = spawn('tmux', cmd, { stdio: 'ignore' });
      process.on('close', (code) => {
        if (code === 0) resolve();
        else reject(new Error(`tmux failed with code ${code}`));
      });
    });
    
    console.log('✅ Human dashboard created - launching live log view...');
    console.log('');
    console.log('🎯 LIVE LOG DASHBOARD');
    console.log('┌─────────────────────────┐');
    console.log('│  📦 NPM STARTUP LOGS    │');  
    console.log('│     (Live Updates)      │');
    console.log('└─────────────────────────┘');
    console.log('');
    console.log('💡 EXIT OPTIONS (system keeps running):');
    console.log('   • Ctrl+B then D (tmux detach - SAFE, keeps system running)');
    console.log('   • Type "exit" then Enter (safe exit)');
    console.log('   • Ctrl+D (EOF - safe exit)');
    console.log('   ⚠️  Ctrl+C kills system - avoid!');
    console.log('');
    console.log('💡 TMUX NAVIGATION:');
    console.log('   • Prefix might be Ctrl+B or Ctrl+A (check your config)');
    console.log('   • Window switching: [prefix] then 0/1/2');
    console.log('   • If prefix not working: tmux show-options -g prefix');
    console.log('');
    
    // Auto-launch tmux with safe signal handling
    console.log('🚀 Launching dashboard in tmux...');
    
    // Configure tmux session to not die when detached
    try {
      await new Promise<void>((resolve) => {
        const setOption = spawn('tmux', ['set-option', '-t', `${this.sessionName}:logs`, 'destroy-unattached', 'off'], { stdio: 'ignore' });
        setOption.on('close', () => resolve());
      });
    } catch {
      // Option setting failed, continue anyway
    }
    
    // Launch tmux attach using window index for reliability
    if (logsWindowIndex) {
      const targetWindow = `${this.sessionName}:${logsWindowIndex}`;
      console.log(`🎯 Attaching to window ${logsWindowIndex} (logs)...`);
      
      const attach = spawn('tmux', ['attach-session', '-t', targetWindow], {
        stdio: 'inherit',
        detached: false
      });
      
      // Prevent child signals from propagating to parent npm process
      process.removeAllListeners('SIGINT');
      process.removeAllListeners('SIGTERM');
      
      attach.on('close', (code) => {
        console.log('');
        console.log('👋 Dashboard detached (system still running)');
        console.log(`💡 To reopen: tmux attach-session -t ${targetWindow}`);
        console.log(`💡 Quick reopen: npm run logs:attach`);
        console.log(`💡 System status: ps aux | grep continuum`);
      });
    } else {
      console.log('❌ Could not create or find logs window');
      console.log(`💡 Manual attach: tmux attach-session -t ${this.sessionName}`);
    }
  }

  /**
   * Check if dashboard session already exists
   */
  private async sessionExists(): Promise<boolean> {
    return new Promise((resolve) => {
      const check = spawn('tmux', ['has-session', '-t', this.sessionName], { stdio: 'ignore' });
      check.on('close', (code) => resolve(code === 0));
    });
  }

  /**
   * Kill existing dashboard session
   */
  private async killSession(): Promise<void> {
    return new Promise((resolve) => {
      const kill = spawn('tmux', ['kill-session', '-t', this.sessionName], { stdio: 'ignore' });
      kill.on('close', () => resolve());
    });
  }

  /**
   * Intelligent dashboard creation based on agent detection
   */
  async createDashboard(forceMode?: 'ai' | 'human'): Promise<void> {
    let mode = forceMode;
    
    if (!mode) {
      const agentInfo = AgentDetector.detect();
      mode = agentInfo.type === 'ai' ? 'ai' : 'human';
      
      if (agentInfo.type === 'ai') {
        console.log(`🤖 Detected AI agent: ${agentInfo.name} - Using structured output`);
      } else {
        console.log(`👤 Detected ${agentInfo.type}: ${agentInfo.name} - Auto-launching dashboard`);
      }
    }
    
    if (mode === 'ai') {
      await this.showAILogs();
    } else {
      await this.createHumanDashboard();
    }
  }

  /**
   * Attach to existing dashboard session (human mode only)
   */
  async attachToDashboard(): Promise<void> {
    if (!(await this.sessionExists())) {
      console.log('❌ Dashboard session not found. Creating human dashboard...');
      await this.createHumanDashboard();
    }
    
    console.log(`🔗 Attaching to JTAG session: ${this.sessionName}`);
    console.log('💡 EXIT OPTIONS:');
    console.log('   • Ctrl+B, release, then D (tmux detach)');  
    console.log('   • Ctrl+C (kill process)');
    console.log('💡 NAVIGATION:');
    console.log('   • Ctrl+B, release, then 0/1/2 (switch windows)');
    
    // Spawn tmux attach in foreground, select logs window if it exists
    const attach = spawn('tmux', ['attach-session', '-t', `${this.sessionName}:logs`], {
      stdio: 'inherit'  // Let user interact directly
    });
    
    attach.on('close', (code) => {
      console.log('👋 Detached from dashboard session');
      process.exit(code || 0);
    });
  }

  /**
   * Setup dashboard window and auto-launch for npm start
   */
  async setupDashboard(): Promise<void> {
    const agentInfo = AgentDetector.detect();
    
    if (agentInfo.type === 'ai') {
      // AI agents just get status
      await this.showAILogs();
      return;
    }
    
    // For humans: create the dashboard window AND auto-launch
    console.log('👤 Setting up and launching log dashboard...');
    
    // Ensure log files exist
    await this.ensureLogFiles();
    
    // Check if main system session exists
    const systemSessionExists = await this.sessionExists();
    
    if (systemSessionExists) {
      // Find or create logs window
      const windowsCheck = await new Promise<string>((resolve) => {
        const check = spawn('tmux', ['list-windows', '-t', this.sessionName, '-F', '#{window_index}:#{window_name}'], { stdio: 'pipe' });
        let output = '';
        check.stdout?.on('data', (data) => { output += data.toString(); });
        check.on('close', () => resolve(output));
      });
      
      // Look for existing logs window
      const logsWindowMatch = windowsCheck.match(/(\d+):logs/);
      let logsWindowIndex = logsWindowMatch ? logsWindowMatch[1] : null;
      
      if (!logsWindowIndex) {
        // Create new logs window
        console.log('🔗 Adding logs window to JTAG system session...');
        const cmd = [
          'new-window', 
          '-t', this.sessionName,
          '-n', 'logs',  // Window name
          `tail -f ${this.config.logFiles.npm}`
        ];
        
        await new Promise<void>((resolve, reject) => {
          const process = spawn('tmux', cmd, { stdio: 'ignore' });
          process.on('close', (code) => {
            if (code === 0) resolve();
            else reject(new Error(`tmux failed with code ${code}`));
          });
        });
        
        // Get the new window index
        const newWindowsCheck = await new Promise<string>((resolve) => {
          const check = spawn('tmux', ['list-windows', '-t', this.sessionName, '-F', '#{window_index}:#{window_name}'], { stdio: 'pipe' });
          let output = '';
          check.stdout?.on('data', (data) => { output += data.toString(); });
          check.on('close', () => resolve(output));
        });
        
        const newLogsMatch = newWindowsCheck.match(/(\d+):logs/);
        logsWindowIndex = newLogsMatch ? newLogsMatch[1] : null;
      }
      
      console.log('✅ Log dashboard ready - launching...');
      console.log('');
      console.log('🎯 LIVE LOG DASHBOARD');
      console.log('┌─────────────────────────┐');
      console.log('│  📦 NPM STARTUP LOGS    │');  
      console.log('│     (Live Updates)      │');
      console.log('└─────────────────────────┘');
      console.log('');
      console.log('💡 EXIT OPTIONS:');
      console.log('   • Type "exit" and press Enter (recommended)');
      console.log('   • Ctrl+C (kill tail process)');
      console.log('   • Ctrl+B then D (tmux detach sequence)');
      console.log('');
      console.log('💡 NAVIGATION:');
      console.log('   • Ctrl+B then 0 (main system window)');
      console.log('   • Ctrl+B then 1 (logs window)');
      console.log('');
      console.log('🚀 Launching dashboard...');
      
      // Auto-launch but use better tmux approach - spawn new shell
      const attach = spawn('bash', ['-c', `tmux select-window -t ${this.sessionName}:logs && exec tmux attach-session -t ${this.sessionName}:logs`], {
        stdio: 'inherit'  // Full terminal control
      });
      
      attach.on('close', (code) => {
        console.log('');
        console.log('👋 Dashboard closed');
        console.log(`💡 To reopen: tmux attach-session -t ${this.sessionName}:logs`);
        console.log('💡 Quick reopen: npm run logs:attach');
      });
      
    } else {
      console.log('❌ No JTAG system session found - start the system first');
      console.log('💡 Run: npm run start:no-dashboard');
    }
  }

  /**
   * Show dashboard status
   */
  async showStatus(): Promise<void> {
    const agentInfo = AgentDetector.detect();
    
    if (agentInfo.type === 'ai') {
      // AI agents get current log status
      await this.showAILogs();
      return;
    }

    // Humans get dashboard info
    const exists = await this.sessionExists();
    
    console.log(`🎯 JTAG LOG DASHBOARD STATUS`);
    console.log('');
    console.log(`📋 Agent Detection: ${agentInfo.type === 'ai' ? `🤖 ${agentInfo.name}` : `👤 ${agentInfo.name}`}`);
    console.log(`📋 Session Running: ${exists ? '✅ Yes' : '❌ No'}`);
    console.log('');
    
    if (exists) {
      console.log('💡 COMMANDS:');
      console.log(`   tmux attach-session -t ${this.sessionName}     # View dashboard`);
      console.log('   npm run logs:attach                      # Quick attach');
    } else {
      console.log('💡 COMMANDS:');
      console.log('   npm run logs:dashboard                   # Start intelligent dashboard');
      console.log('   npm run logs:human                       # Force human tmux dashboard');
      console.log('   npm run logs:ai                          # Force AI structured output');
    }
  }
}

// CLI interface
async function main() {
  const dashboard = new LogDashboard();
  const args = process.argv.slice(2);
  
  try {
    if (args.includes('--attach')) {
      await dashboard.attachToDashboard();
    } else if (args.includes('--status')) {
      await dashboard.showStatus();
    } else if (args.includes('--setup')) {
      await dashboard.setupDashboard();
    } else if (args.includes('--ai')) {
      await dashboard.createDashboard('ai');
    } else if (args.includes('--human')) {
      await dashboard.createDashboard('human');
    } else if (args.includes('--stop')) {
      if (await dashboard.sessionExists()) {
        await dashboard.killSession();
        console.log('🛑 Dashboard session stopped');
      } else {
        console.log('📋 Dashboard was not running');
      }
    } else {
      // Default: intelligent detection
      await dashboard.createDashboard();
    }
  } catch (error) {
    console.error('🚨 Dashboard error:', error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

if (require.main === module) {
  main().catch(console.error);
}

export { LogDashboard, type DashboardConfig };