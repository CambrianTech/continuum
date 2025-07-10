#!/usr/bin/env npx tsx

/**
 * JTAG - AI Autonomous Debugging Tool (TypeScript)
 * ================================================
 * Provides browser JTAG probe commands and log analysis
 */

import { spawn } from 'child_process';
import { promises as fs } from 'fs';
import { join } from 'path';

interface JtagCommand {
  (): void | Promise<void>;
}

const commands: Record<string, JtagCommand> = {
  help() {
    console.log(`
🛸 JTAG - AI Autonomous Debugging Tool

Usage: ./jtag <command>

🔍 Analysis Commands:
  widgets                    Show browser JTAG command for widget analysis
  probe                      Show browser JTAG command for custom probing
  
📋 Log Commands:
  logs                       Show recent browser logs
  errors                     Show recent browser errors  
  logs-live                  Follow browser logs in real-time
  errors-live                Follow browser errors in real-time
  
🛠️ Development Commands:
  session                    Show current session info
  health                     Check system health
  help                       Show this help

🚀 Command Execution:
  ./jtag command <cmd> [args] Execute any continuum command
  ./jtag command personas list Execute personas command
  ./jtag command health        Execute health command

🚀 Quick Start:
  npm jtag                   Start system + get JTAG commands
  ./jtag widgets             Get widget analysis command
  ./jtag logs                See what widgets are doing
  ./jtag errors              Find widget problems

📱 Browser Commands (paste in DevTools):
  fetch("/src/ui/jtag-probe.js").then(r=>r.text()).then(eval)
  window.jtag.widgets()      Direct widget analysis
`);
  },

  widgets() {
    console.log('🛸 JTAG Widget Analysis');
    console.log('=======================');
    console.log('');
    console.log('1. Ensure system is running (npm start or npm jtag)');
    console.log('2. Open browser DevTools (F12)');
    console.log('3. Paste this command:');
    console.log('');
    console.log('   fetch("/src/ui/jtag-probe.js").then(r=>r.text()).then(eval)');
    console.log('');
    console.log('This will analyze all widget shadow DOM states.');
  },

  probe() {
    console.log('🛸 JTAG Custom Probe');
    console.log('====================');
    console.log('');
    console.log('Available browser commands:');
    console.log('');
    console.log('  window.jtag.widgets()     - Widget analysis');
    console.log('  window.jtag.shadowDOM()   - Shadow DOM analysis');
    console.log('  window.jtag.health()      - System health check');
    console.log('  window.jtag.network()     - Network status');
    console.log('  window.jtag.performance() - Performance metrics');
    console.log('');
    console.log('Example: window.jtag.widgets({ autoLog: true })');
  },

  async logs() {
    console.log('📋 Recent Browser Logs');
    console.log('=====================');
    
    const sessionDir = await findCurrentSession();
    if (!sessionDir) {
      console.log('❌ No active session found');
      return;
    }
    
    const logFile = join(sessionDir, 'logs/browser.log');
    
    try {
      const tail = spawn('tail', ['-20', logFile]);
      tail.stdout.pipe(process.stdout);
      tail.on('close', () => {
        console.log('\n💡 Use "./jtag logs-live" to follow logs in real-time');
      });
    } catch (error) {
      console.log(`❌ Error reading logs: ${error}`);
    }
  },

  async errors() {
    console.log('🚨 Recent Browser Errors');
    console.log('========================');
    
    const sessionDir = await findCurrentSession();
    if (!sessionDir) {
      console.log('❌ No active session found');
      return;
    }
    
    const errorFile = join(sessionDir, 'logs/browser.error.json');
    
    try {
      const tail = spawn('tail', ['-10', errorFile]);
      
      let output = '';
      tail.stdout.on('data', (data) => {
        output += data;
      });
      
      tail.on('close', () => {
        // Parse and format JSON errors for readability
        const lines = output.trim().split('\n').filter(line => line.trim());
        lines.forEach((line, index) => {
          try {
            const error = JSON.parse(line);
            console.log(`❌ Error ${index + 1}: ${error.consoleMessage}`);
            if (error.consoleArguments && error.consoleArguments.length > 0) {
              console.log(`   Args: ${error.consoleArguments.map((arg: any) => arg.argumentValue).join(', ')}`);
            }
            console.log(`   Time: ${error.serverTimestamp}`);
            console.log('');
          } catch (e) {
            console.log(`Raw: ${line.substring(0, 100)}...`);
          }
        });
        
        console.log('💡 Use "./jtag errors-live" to follow errors in real-time');
      });
    } catch (error) {
      console.log(`❌ Error reading error logs: ${error}`);
    }
  },

  async session() {
    const sessionDir = await findCurrentSession();
    if (!sessionDir) {
      console.log('❌ No active session found');
      return;
    }
    
    console.log('📱 Current Session Info');
    console.log('======================');
    console.log(`Session: ${sessionDir.split('/').pop()}`);
    console.log(`Path: ${sessionDir}`);
    console.log(`URL: http://localhost:9000`);
    console.log('');
    console.log('Available logs:');
    console.log('  📋 browser.log     - All browser activity');
    console.log('  🚨 browser.error.json - Errors only');
    console.log('  🖥️  server.log     - Server activity');
  },

  health() {
    console.log('🏥 System Health Check');
    console.log('======================');
    
    // Check if server is responding
    const curl = spawn('curl', ['-s', 'http://localhost:9000']);
    
    curl.on('close', async (code) => {
      if (code === 0) {
        console.log('✅ Server: Running (localhost:9000)');
        
        // Check session
        const sessionDir = await findCurrentSession();
        if (sessionDir) {
          console.log('✅ Session: Active');
          console.log(`📱 Session: ${sessionDir.split('/').pop()}`);
        } else {
          console.log('❌ Session: No active session');
        }
        
        console.log('');
        console.log('🛸 Ready for JTAG analysis!');
        console.log('Run: ./jtag widgets');
        
      } else {
        console.log('❌ Server: Not responding');
        console.log('💡 Run: npm jtag');
      }
    });
  },

  async command(commandName: string, ...args: string[]) {
    console.log(`🛸 JTAG Command: ${commandName}`);
    console.log('==========================');
    
    try {
      // Build command with arguments
      const fullCommand = [commandName, ...args].join(' ');
      console.log(`📤 Executing: ${fullCommand}`);
      
      const continuum = spawn('./continuum', [commandName, ...args]);
      
      continuum.stdout.on('data', (data) => {
        process.stdout.write(data);
      });
      
      continuum.stderr.on('data', (data) => {
        process.stderr.write(data);
      });
      
      continuum.on('close', (code) => {
        console.log(`\n📊 Command completed with code: ${code}`);
      });
      
    } catch (error) {
      console.log(`❌ Error executing command: ${error}`);
    }
  }
};

async function findCurrentSession(): Promise<string | null> {
  const sessionsBase = '.continuum/sessions/user/shared';
  try {
    const sessions = (await fs.readdir(sessionsBase))
      .filter(d => d.startsWith('development-shared-'));
    
    if (sessions.length === 0) return null;
    
    // Get most recent session
    let latest = sessions[0];
    let latestTime = (await fs.stat(join(sessionsBase, latest))).mtime;
    
    for (const current of sessions.slice(1)) {
      const currentTime = (await fs.stat(join(sessionsBase, current))).mtime;
      if (currentTime > latestTime) {
        latest = current;
        latestTime = currentTime;
      }
    }
    
    return join(sessionsBase, latest);
  } catch (e) {
    return null;
  }
}

// Parse command line
const [,, command, ...args] = process.argv;

if (!command || command === 'help') {
  commands.help();
} else if (command === 'command' && args.length > 0) {
  // Handle command execution
  await commands.command(args[0], ...args.slice(1));
} else if (commands[command]) {
  await commands[command]();
} else {
  console.log(`❌ Unknown command: ${command}`);
  console.log('Run ./jtag help for available commands');
  process.exit(1);
}