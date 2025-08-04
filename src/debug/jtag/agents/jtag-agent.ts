#!/usr/bin/env npx tsx
/**
 * JTAG Agent - Autonomous debugging and development agent
 * 
 * This agent can:
 * 1. Start/restart JTAG systems autonomously
 * 2. Diagnose WebSocket correlation issues
 * 3. Fix common problems automatically
 * 4. Monitor system health continuously
 * 5. Execute tests and report results
 */

import { execSync, spawn, ChildProcess } from 'child_process';
import { existsSync, readlinkSync, statSync, writeFileSync } from 'fs';
import { join } from 'path';

interface AgentConfig {
  verbose: boolean;
  autoFix: boolean;
  continuous: boolean;
  timeout: number;
}

class JTAGAgent {
  private config: AgentConfig;
  private systemProcess?: ChildProcess;
  private testClientProcess?: ChildProcess;
  private monitoringInterval?: NodeJS.Timeout;

  constructor(config: Partial<AgentConfig> = {}) {
    this.config = {
      verbose: false,
      autoFix: false,  // Thin client doesn't auto-fix by default
      continuous: false,
      timeout: 60000,
      ...config
    };
  }

  /**
   * Main agent execution - MONITORING DASHBOARD
   */
  async run(): Promise<void> {
    console.log('🤖 JTAG Monitoring Dashboard');
    
    if (this.config.continuous) {
      await this.startMonitoringMode();
    } else {
      await this.runSingleCheck();
    }
  }

  private async runSingleCheck(): Promise<void> {
    try {
      // 1. Show current status
      this.showCurrentStatus();
      
      // 2. Show log locations  
      this.showLogLocations();
      
      // 3. Test correlation if system is running
      const port9001 = this.isPortListening(9001);
      const port9002 = this.isPortListening(9002);
      
      if (port9001 && port9002) {
        console.log('\n🔗 TESTING WEBSOCKET CORRELATION');
        console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
        const correlationTest = await this.testWebSocketCorrelation();
        
        if (correlationTest.success) {
          console.log('✅ External client: WORKING');
          console.log('✅ Terminal commands should work');
        } else {
          console.log('❌ External client: FAILED');
          console.log('🔍 Browser works, terminal client broken');
        }
      }
      
      // 4. Show available commands
      this.showAvailableCommands();
      
      // 5. Interactive command mode if requested
      if (process.argv.includes('--interactive') || process.argv.includes('-i')) {
        await this.startInteractiveMode();
      }
      
    } catch (error) {
      console.log(`❌ Check failed: ${error}`);
    }
  }

  private showCurrentStatus(): void {
    const port9001 = this.isPortListening(9001);
    const port9002 = this.isPortListening(9002);
    const tmuxRunning = this.isTmuxSessionRunning();
    const browserConnected = this.checkBrowserConnections();
    
    console.log('\n🔌 JTAG SYSTEM STATUS');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    
    // Tmux session
    console.log(`   🪟 Tmux Session: ${tmuxRunning ? '✅ jtag-test running' : '❌ Not running'}`);
    
    // Port status
    console.log(`   🔌 WebSocket (9001): ${port9001 ? '✅ Listening' : '❌ Not listening'}`);
    console.log(`   🌐 HTTP (9002): ${port9002 ? '✅ Listening' : '❌ Not listening'}`);
    
    // Browser connection status
    if (port9001) {
      console.log(`   🦊 Browser Connected: ${browserConnected ? '✅ Yes' : '❌ No WebSocket connections'}`);
    }
    
    // Overall status - prioritize ports over tmux
    const systemOnline = port9001 && port9002;
    console.log(`   📊 JTAG System: ${systemOnline ? '✅ FULLY ONLINE' : '❌ OFFLINE'}`);
    
    // Show connected clients (like nvidia-smi)
    const clients = this.getAllConnectedClients();
    if (clients.length > 0) {
      console.log('\n📱 CONNECTED CLIENTS');
      console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
      console.log('   Type      PID    User    Process         Connection');
      console.log('   ────────  ─────  ──────  ─────────────  ──────────────────────');
      for (const client of clients) {
        const typeIcon = client.type === 'browser' ? '🌐' : client.type === 'external' ? '🔗' : '⚙️ ';
        console.log(`   ${typeIcon} ${client.type.padEnd(7)} ${client.pid.toString().padEnd(6)} ${client.user.padEnd(7)} ${client.process.padEnd(14)} ${client.connection}`);
      }
    }
    
    if (systemOnline) {
      console.log('\n   🎯 Ready for:');
      console.log('      • Browser UI: http://localhost:9002');
      if (browserConnected) {
        console.log('      • Browser is connected and working');
      }
      console.log('      • Terminal commands: npx tsx test-server-client.ts');
      console.log('      • Screenshot commands from CLI');
      
      if (!tmuxRunning) {
        console.log('   ⚠️  System running but tmux session ended (this is normal after build)');
      }
    } else {
      console.log('\n   🚀 To start: npm run system:start (or use interactive mode)');
      if (tmuxRunning && (!port9001 || !port9002)) {
        console.log('   ⚠️  Tmux running but ports not ready - system may still be starting');
      }
    }
  }

  private isTmuxSessionRunning(): boolean {
    try {
      execSync('tmux list-sessions | grep -E "(jtag-test|jtag-debug)"', { encoding: 'utf8' });
      return true;
    } catch {
      return false;
    }
  }

  private checkBrowserConnections(): boolean {
    try {
      // Check for established connections to port 9001
      const result = execSync('lsof -i :9001 | grep ESTABLISHED', { encoding: 'utf8' });
      return result.trim().length > 0;
    } catch {
      return false;
    }
  }
  
  private getAllConnectedClients(): Array<{type: string, pid: number, user: string, process: string, connection: string}> {
    try {
      const result = execSync('lsof -i :9001 -i :9002 | grep ESTABLISHED', { encoding: 'utf8' });
      const lines = result.trim().split('\n').filter(line => line.length > 0);
      
      return lines.map(line => {
        const parts = line.trim().split(/\s+/);
        const processName = parts[0] || 'unknown';
        const pid = parseInt(parts[1]) || 0;
        const user = parts[2] || 'unknown';
        const connection = parts[parts.length - 1] || 'unknown';
        
        // Enhanced client detection with meaningful names
        let type = 'unknown';
        let displayName = processName;
        
        if (processName.toLowerCase().includes('opera') || processName.toLowerCase().includes('chrome') || processName.toLowerCase().includes('safari')) {
          type = 'browser';
          displayName = processName.replace('\\x20', ' '); // Clean up encoded spaces
        } else if (processName === 'node') {
          // For node processes, check if it's the JTAG server or external client
          try {
            const cmdLine = execSync(`ps -p ${pid} -o args=`, { encoding: 'utf8', timeout: 1000 });
            if (cmdLine.includes('test-server-client') || cmdLine.includes('JTAGClient')) {
              type = 'external';
              displayName = 'JTAGClient';
            } else if (cmdLine.includes('tsx') && cmdLine.includes('test-')) {
              type = 'external';
              displayName = 'external-test';
            } else if (cmdLine.includes('minimal-server') || cmdLine.includes('test-bench')) {
              type = 'server';
              displayName = 'JTAG-server';
            } else {
              type = 'server';
              displayName = 'node-server';
            }
          } catch {
            // Fallback: assume server if we can't determine
            type = 'server';
            displayName = 'node-process';
          }
        } else if (processName.includes('tsx')) {
          type = 'external';
          displayName = 'tsx-client';
        }
        
        return { type, pid, user, process: displayName, connection };
      });
    } catch {
      return [];
    }
  }

  private showLogLocations(): void {
    console.log('\n📄 LOG LOCATIONS');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    
    const logPaths = [
      '.continuum/jtag/system/logs/npm-start.log',  // NEW: npm start output
      'examples/test-bench/.continuum/jtag/system/logs/server-node-output.log',
      '.continuum/jtag/system/logs/server-console-log.log'
    ];
    
    logPaths.forEach(path => {
      if (existsSync(path)) {
        const stats = statSync(path);
        const ageMinutes = Math.round((Date.now() - stats.mtime.getTime()) / (1000 * 60));
        const sizeKB = Math.round(stats.size / 1024);
        console.log(`   📝 ${path} (${sizeKB}KB, ${ageMinutes}min ago)`);
      } else {
        console.log(`   📝 ${path} (not found)`);
      }
    });
    
    console.log('\n📋 Log Commands:');
    console.log('   tail -f .continuum/jtag/system/logs/npm-start.log    # Main startup log');
    console.log('   tail -f examples/test-bench/.continuum/jtag/system/logs/server-node-output.log');
    console.log('   tmux attach -t jtag-test      # Attach to tmux session');
    console.log('   tmux capture-pane -t jtag-test -p    # Capture tmux output');
  }

  private showAvailableCommands(): void {
    console.log('\n🚀 AVAILABLE COMMANDS');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('   npm run agent -- -i          # Interactive mode (easy start/stop/rebuild)');
    console.log('   npm run system:start         # Start system in tmux (non-blocking)');
    console.log('   npm run system:stop          # Stop tmux system');
    console.log('   npm run system:restart       # Restart system');
    console.log('   npm run build                # Build project');
    console.log('   npm run agent -- -c          # Continuous monitoring');
    console.log('   npx tsx test-server-client.ts # Test external client');
    console.log('');
    console.log('📋 LOGGING COMMANDS:');
    console.log('   npm run logs:show            # Show log file status');
    console.log('   npm run logs:tail            # Tail system logs');
    console.log('   tmux attach -t jtag-test     # Attach to tmux session (see live output)');
    console.log('   tmux capture-pane -t jtag-test -p  # Capture tmux output');
    console.log('');
    console.log('⚠️  Never run "npm start" directly - it blocks! Use system:start instead.');
  }

  async executeCommand(command: string): Promise<void> {
    console.log(`\n🚀 Executing: ${command}`);
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    
    try {
      // Execute detached - don't capture output, let it go to tmux/background
      const child = spawn('npm', ['run', ...command.split(' ').slice(2)], {
        detached: true,
        stdio: 'ignore'
      });
      
      child.unref(); // Don't wait for it
      
      console.log(`✅ Command launched: ${command}`);
      console.log('📋 Check tmux session or logs for output');
      
    } catch (error) {
      console.log(`❌ Failed to launch: ${command} - ${error}`);
    }
  }

  private async startMonitoringMode(): Promise<void> {
    console.log('🔄 Starting continuous monitoring...');
    console.log('Press Ctrl+C to exit\n');
    
    let lastPort9001 = false;
    let lastPort9002 = false;
    
    const monitor = setInterval(() => {
      const port9001 = this.isPortListening(9001);
      const port9002 = this.isPortListening(9002);
      
      if (port9001 !== lastPort9001 || port9002 !== lastPort9002) {
        const timestamp = new Date().toLocaleTimeString();
        console.log(`[${timestamp}] Status change:`);
        console.log(`   WebSocket (9001): ${port9001 ? '✅ UP' : '❌ DOWN'}`);
        console.log(`   HTTP (9002): ${port9002 ? '✅ UP' : '❌ DOWN'}`);
        
        lastPort9001 = port9001;
        lastPort9002 = port9002;
      }
    }, 2000);
    
    // Keep alive and handle Ctrl+C
    process.on('SIGINT', () => {
      clearInterval(monitor);
      console.log('\n🛑 Monitoring stopped');
      process.exit(0);
    });
    
    // Keep process alive
    await new Promise(() => {}); // Never resolves
  }

  private async startInteractiveMode(): Promise<void> {
    const readline = require('readline');
    const rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout
    });

    console.log('\n🎮 INTERACTIVE MODE');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('Commands: start, stop, restart, build, rebuild, clean, status, logs, test, quit');
    console.log('');

    const promptUser = () => {
      rl.question('jtag> ', async (input) => {
        const cmd = input.trim().toLowerCase();
        
        try {
          switch (cmd) {
            case 'start':
              await this.executeCommand('npm run system:start');
              console.log('📋 Starting system... check tmux session for progress');
              break;
              
            case 'stop':
              await this.executeCommand('npm run system:stop');
              console.log('🛑 System stopped');
              break;
              
            case 'restart':
              console.log('🔄 Restarting system...');
              await this.executeCommand('npm run system:stop');
              await new Promise(resolve => setTimeout(resolve, 2000));
              await this.executeCommand('npm run system:start');
              console.log('📋 Restarting... check tmux session for progress');
              break;
              
            case 'build':
              await this.executeCommand('npm run build');
              console.log('🔨 Building... check tmux session for progress');
              break;
              
            case 'rebuild':
              console.log('🧹 Clean rebuild...');
              await this.executeCommand('npm run clean:all');
              await new Promise(resolve => setTimeout(resolve, 1000));
              await this.executeCommand('npm run build');
              console.log('📋 Rebuilding... check tmux session for progress');
              break;
              
            case 'clean':
              await this.executeCommand('npm run clean:all');
              console.log('🧹 Cleaned dist and temp files');
              break;
              
            case 'status':
              this.showCurrentStatus();
              break;
              
            case 'logs':
              await this.executeCommand('npm run logs:show');
              console.log('\nTo tail logs: npm run logs:tail');
              console.log('To see tmux: tmux attach -t jtag-test');
              break;
              
            case 'test':
              console.log('🧪 Testing external client connection...');
              const correlationTest = await this.testWebSocketCorrelation();
              if (correlationTest.success) {
                console.log('✅ External client working');
              } else {
                console.log('❌ External client broken - correlation issue');
              }
              break;
              
            case 'quit':
            case 'exit':
            case 'q':
              console.log('👋 Goodbye!');
              rl.close();
              return;
              
            case 'help':
            case '?':
              console.log('\nAvailable commands:');
              console.log('  start    - Start system in tmux');
              console.log('  stop     - Stop system');
              console.log('  restart  - Stop then start');
              console.log('  build    - Build project');
              console.log('  rebuild  - Clean then build');
              console.log('  clean    - Clean dist/temp files');
              console.log('  status   - Show current status');
              console.log('  logs     - Show log locations');
              console.log('  test     - Test external client');
              console.log('  quit     - Exit interactive mode');
              break;
              
            case '':
              // Empty input, just reprompt
              break;
              
            default:
              console.log(`❓ Unknown command: ${cmd}. Type 'help' for available commands.`);
              break;
          }
        } catch (error) {
          console.log(`❌ Error executing command: ${error}`);
        }
        
        if (cmd !== 'quit' && cmd !== 'exit' && cmd !== 'q') {
          promptUser();
        }
      });
    };

    promptUser();
  }

  private log(message: string): void {
    const timestamp = new Date().toISOString();
    const logMessage = `[${timestamp}] ${message}`;
    console.log(logMessage);
    
    if (this.config.verbose) {
      // Also log to file
      try {
        const logFile = '.continuum/jtag/system/logs/agent.log';
        const logEntry = `${logMessage}\n`;
        require('fs').appendFileSync(logFile, logEntry);
      } catch {}
    }
  }


  private isPortListening(port: number): boolean {
    try {
      const result = execSync(`lsof -ti:${port}`, { encoding: 'utf8' });
      return result.trim().length > 0;
    } catch {
      return false;
    }
  }


  private async testWebSocketCorrelation(): Promise<{ success: boolean; error?: string; correlationId?: string }> {
    return new Promise((resolve) => {
      const testProcess = spawn('npx', ['tsx', './test-server-client.ts'], {
        stdio: ['ignore', 'pipe', 'pipe']
      });
      
      let output = '';
      let correlationId = '';
      
      testProcess.stdout?.on('data', (data) => {
        output += data.toString();
        const match = data.toString().match(/Created request (\w+)/);
        if (match) {
          correlationId = match[1];
        }
      });
      
      testProcess.stderr?.on('data', (data) => {
        output += data.toString();
      });
      
      const timeout = setTimeout(() => {
        testProcess.kill();
        resolve({
          success: false,
          error: 'Test timeout',
          correlationId
        });
      }, 35000);
      
      testProcess.on('close', (code) => {
        clearTimeout(timeout);
        resolve({
          success: code === 0,
          error: code !== 0 ? output : undefined,
          correlationId
        });
      });
    });
  }

  private async fixWebSocketCorrelation(): Promise<void> {
    this.log('🔧 Attempting to fix WebSocket correlation...');
    
    // Check server logs for the correlation ID
    try {
      const serverLogs = require('fs').readFileSync('.continuum/jtag/system/logs/server-console-log.log', 'utf8');
      
      // Look for recent correlation patterns
      const correlationMatches = serverLogs.match(/req:(\w+)/g);
      const responseMatches = serverLogs.match(/res:(\w+)/g);
      
      if (correlationMatches && correlationMatches.length > responseMatches?.length) {
        this.log('🔍 Found unmatched requests - server receiving but not responding properly');
        
        // The issue is likely in the WebSocket transport response routing
        // For now, restart the system as that usually fixes correlation issues
        await this.restartJTAGSystem();
      }
    } catch (error) {
      this.log(`⚠️ Could not analyze correlation logs: ${error}`);
    }
  }

  private async testScreenshotCommand(): Promise<{ success: boolean; screenshotPath?: string; error?: string }> {
    // This would be implemented to test actual screenshot functionality
    // For now, return a placeholder
    return { success: false, error: 'Screenshot test not implemented yet' };
  }

  private generateReport(data: any): any {
    return {
      timestamp: new Date().toISOString(),
      agent: 'JTAG Agent v1.0',
      summary: {
        systemHealth: data.systemHealth.status,
        correlationWorking: data.correlationTest.success,
        screenshotWorking: data.screenshotTest.success
      },
      details: data,
      recommendations: this.generateRecommendations(data)
    };
  }

  private generateRecommendations(data: any): string[] {
    const recommendations: string[] = [];
    
    if (data.systemHealth.status !== 'healthy') {
      recommendations.push('System health is degraded - consider restarting JTAG system');
    }
    
    if (!data.correlationTest.success) {
      recommendations.push('WebSocket correlation failing - check transport layer configuration');
    }
    
    if (!data.screenshotTest.success) {
      recommendations.push('Screenshot functionality not working - verify browser automation');
    }
    
    return recommendations;
  }

  private saveReport(report: any): void {
    const reportPath = '.continuum/jtag/system/logs/agent-report.json';
    writeFileSync(reportPath, JSON.stringify(report, null, 2));
  }

  private async startContinuousMonitoring(): Promise<void> {
    this.log('🔄 Starting continuous monitoring...');
    
    this.monitoringInterval = setInterval(async () => {
      const health = await this.checkSystemHealth();
      if (health.status !== 'healthy' && this.config.autoFix) {
        await this.autoFixIssues(health.issues);
      }
    }, 60000); // Check every minute
    
    // Keep process alive
    process.on('SIGINT', () => {
      if (this.monitoringInterval) {
        clearInterval(this.monitoringInterval);
      }
      process.exit(0);
    });
  }
}

// CLI interface
async function main() {
  const args = process.argv.slice(2);
  
  const config: Partial<AgentConfig> = {
    verbose: args.includes('--verbose') || args.includes('-v'),
    autoFix: args.includes('--autofix'),  // Only auto-fix if explicitly requested
    continuous: args.includes('--continuous') || args.includes('-c'),
    timeout: args.includes('--timeout') ? parseInt(args[args.indexOf('--timeout') + 1]) : 60000
  };
  
  if (args.includes('--help') || args.includes('-h')) {
    console.log(`
🤖 JTAG Agent - Autonomous debugging and development

Usage: npx tsx agents/jtag-agent.ts [options]

Options:
  -v, --verbose      Enable verbose logging
  -c, --continuous   Run in continuous monitoring mode
  --autofix          Enable automatic issue fixing (DANGEROUS - kills processes)
  --timeout <ms>     Set timeout for operations (default: 60000)
  -h, --help         Show this help message

Examples:
  npx tsx agents/jtag-agent.ts                    # Safe diagnostic mode (default)
  npx tsx agents/jtag-agent.ts -c -v              # Continuous monitoring with verbose logs
  npx tsx agents/jtag-agent.ts --autofix          # DANGEROUS: Will kill and restart node processes
    `);
    return;
  }
  
  const agent = new JTAGAgent(config);
  await agent.run();
}

// Export for use as module
export { JTAGAgent };

// Run if called directly
if (require.main === module) {
  main().catch((error) => {
    console.error('❌ Agent failed:', error);
    process.exit(1);
  });
}