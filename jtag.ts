#!/usr/bin/env npx tsx

/**
 * JTAG - AI Autonomous Debugging Tool (TypeScript)
 * ================================================
 * Provides browser JTAG probe commands and log analysis
 */

import { spawn } from 'child_process';
import { promises as fs } from 'fs';
import { join } from 'path';
import fetch from 'node-fetch';

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
  probe [method] [options]   Execute JTAG probe directly (returns promise)
  probe widgets              Execute widget analysis and return results
  probe shadowDOM            Execute shadow DOM analysis  
  probe health               Execute health check
  
📋 Log Commands:
  logs                       Show recent browser logs
  errors                     Show recent browser errors
  warnings                   Show recent browser warnings (deprecated API usage)
  logs-live                  Follow browser logs in real-time
  errors-live                Follow browser errors in real-time
  
🛠️ Development Commands:
  session                    Show current session info
  health                     Check system health
  hot-reload                 Rebuild widgets and reload browser (preserves session)
  watch                      Watch for code changes and auto hot-reload
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

  async warnings() {
    console.log('⚠️ Recent Browser Warnings');
    console.log('==========================');
    
    const sessionDir = await findCurrentSession();
    if (!sessionDir) {
      console.log('❌ No active session found');
      return;
    }
    
    const warningFile = join(sessionDir, 'logs/browser.warn.json');
    
    try {
      const tail = spawn('tail', ['-15', warningFile]);
      
      let output = '';
      tail.stdout.on('data', (data) => {
        output += data;
      });
      
      tail.on('close', () => {
        // Parse and format JSON warnings for readability
        const lines = output.trim().split('\n').filter(line => line.trim());
        const warningCounts = new Map();
        
        lines.forEach((line) => {
          try {
            const warning = JSON.parse(line);
            const message = warning.consoleMessage;
            warningCounts.set(message, (warningCounts.get(message) || 0) + 1);
          } catch (e) {
            // Skip malformed lines
          }
        });
        
        // Display unique warnings with counts
        let index = 1;
        for (const [message, count] of warningCounts) {
          console.log(`⚠️ Warning ${index}: ${message}`);
          if (count > 1) {
            console.log(`   Count: ${count} occurrences`);
          }
          console.log('');
          index++;
        }
        
        console.log('💡 Use "./jtag warnings-live" to follow warnings in real-time');
      });
    } catch (error) {
      console.log(`❌ Error reading warning logs: ${error}`);
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
  },

  async probe(method: string = 'widgets', options: any = {}) {
    console.log(`🛸 JTAG Browser Probe: ${method}`);
    console.log('==================================');
    
    try {
      // Execute the probe via js-execute command
      const response = await fetch('http://localhost:9000/api/commands/js-execute', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          args: [`--script=if (window.jtag) { const result = window.jtag.${method}(${JSON.stringify(options)}); console.probe('🛸 JTAG Probe Result', result); return result; } else { console.log('❌ JTAG API not available'); return { error: 'JTAG API not available' }; }`]
        })
      });
      
      const result = await response.json();
      console.log('📊 Probe Results:');
      console.log('=================');
      
      if (result.success && result.data) {
        const probeResult = result.data.result;
        
        // Pretty print the widget analysis
        if (method === 'widgets' && probeResult.data) {
          const summary = probeResult.data.summary;
          console.log(`📦 Total widgets: ${summary.total}`);
          console.log(`✅ Rendered: ${summary.rendered}`);
          console.log(`❌ Broken: ${summary.broken}`);
          console.log(`⚪ Empty: ${summary.empty}`);
          console.log(`🎯 Performance: ${summary.performance}`);
          console.log('');
          
          if (probeResult.data.widgets.length > 0) {
            console.log('📊 Widget Details:');
            probeResult.data.widgets.forEach((widget: any) => {
              const status = widget.isRendered ? '✅' : (widget.hasShadowRoot ? '⚠️' : '❌');
              console.log(`${status} ${widget.name.toUpperCase()}`);
              console.log(`   Shadow: ${widget.hasShadowRoot}, Content: ${widget.shadowContentLength} chars`);
              if (widget.errors.length > 0) {
                console.log(`   ⚠️ Errors: ${widget.errors.join(', ')}`);
              }
            });
          }
        } else {
          console.log(JSON.stringify(probeResult, null, 2));
        }
      } else {
        console.log(JSON.stringify(result, null, 2));
      }
      
      return result;
      
    } catch (error) {
      console.log(`❌ Error executing probe: ${error}`);
      return { error: error.message };
    }
  },

  async hotReload() {
    console.log('🔥 Hot Reload: Rebuilding widgets...');
    console.log('==================================');
    
    try {
      // Rebuild browser bundle without cleaning sessions
      const build = spawn('npm', ['run', 'build:browser-hot'], { stdio: 'pipe' });
      
      let buildOutput = '';
      build.stdout.on('data', (data) => {
        buildOutput += data;
        process.stdout.write(data);
      });
      
      build.stderr.on('data', (data) => {
        process.stderr.write(data);
      });
      
      build.on('close', async (code) => {
        if (code === 0) {
          console.log('\n✅ Build complete, reloading browser...');
          
          // Reload the browser
          try {
            const response = await fetch('http://localhost:9000/api/commands/reload', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ target: 'page' })
            });
            
            const result = await response.json();
            if (result.success) {
              console.log('🔄 Browser reloaded successfully');
              console.log('💡 Check ./jtag warnings to see if issues are fixed');
            } else {
              console.log('❌ Failed to reload browser:', result.error);
            }
          } catch (reloadError) {
            console.log('❌ Failed to reload browser:', reloadError);
          }
        } else {
          console.log(`❌ Build failed with code: ${code}`);
        }
      });
      
    } catch (error) {
      console.log(`❌ Error during hot reload: ${error}`);
    }
  },

  watch() {
    console.log('🔄 Starting hot reload watcher...');
    const watcher = spawn('npx', ['tsx', 'src/hot-reload.ts'], { stdio: 'inherit' });
    
    watcher.on('close', (code) => {
      console.log(`File watcher exited with code: ${code}`);
    });
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
} else if (command === 'probe') {
  // Handle probe execution
  const method = args[0] || 'widgets';
  const options = args[1] ? JSON.parse(args[1]) : {};
  await commands.probe(method, options);
} else if (command === 'hot-reload') {
  // Handle hot reload
  await commands.hotReload();
} else if (command === 'watch') {
  // Handle file watching
  commands.watch();
} else if (commands[command]) {
  await commands[command]();
} else {
  console.log(`❌ Unknown command: ${command}`);
  console.log('Run ./jtag help for available commands');
  process.exit(1);
}