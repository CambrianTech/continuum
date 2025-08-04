#!/usr/bin/env npx tsx
/**
 * JTAG Agent Dashboard - Complete autonomous development console
 * 
 * Provides everything an AI agent needs to know for autonomous development:
 * - System status, ports, tmux sessions
 * - Log locations (via symlinks)
 * - Screenshot paths
 * - Current user session info
 * - Real-time test client status
 */

import { execSync, spawn } from 'child_process';
import { existsSync, readlinkSync, statSync } from 'fs';
import { join } from 'path';

class JTAGAgentDashboard {
  
  private checkPort(port: number): string {
    try {
      const result = execSync(`lsof -ti:${port} 2>/dev/null || echo ""`, { encoding: 'utf8' });
      const pid = result.trim();
      return pid ? `✅ Port ${port}: PID ${pid}` : `❌ Port ${port}: Not listening`;
    } catch {
      return `❌ Port ${port}: Check failed`;
    }
  }

  private checkTmuxSession(sessionName: string): string {
    try {
      execSync(`tmux has-session -t ${sessionName} 2>/dev/null`);
      return `✅ tmux '${sessionName}': Running`;
    } catch {
      return `❌ tmux '${sessionName}': Not found`;
    }
  }

  private getSymlinkInfo(path: string): string {
    try {
      if (!existsSync(path)) return `❌ ${path}: Does not exist`;
      
      const stats = statSync(path);
      if (stats.isSymbolicLink()) {
        const target = readlinkSync(path);
        return `🔗 ${path} → ${target}`;
      } else if (stats.isDirectory()) {
        return `📁 ${path}: Directory`;
      } else {
        return `📄 ${path}: File`;
      }
    } catch (error) {
      return `❌ ${path}: Error - ${error}`;
    }
  }

  private getLogStatus(logPath: string): string {
    try {
      if (!existsSync(logPath)) return `❌ Missing`;
      
      const stats = statSync(logPath);
      const ageMinutes = Math.floor((Date.now() - stats.mtime.getTime()) / (1000 * 60));
      const sizeMB = (stats.size / (1024 * 1024)).toFixed(2);
      
      return `✅ ${sizeMB}MB (${ageMinutes}m old)`;
    } catch {
      return `❌ Error`;
    }
  }

  async startTestClient(): Promise<void> {
    console.log(`🚀 Starting test client in background...`);
    
    // Clean previous log (use relative path)
    try {
      execSync('rm -f .continuum/jtag/system/logs/server-node-output.log 2>/dev/null');
    } catch {}
    
    // Start test client in background from correct directory
    const child = spawn('npx', ['tsx', 'test-server-client.ts'], {
      cwd: process.cwd(), // Use current working directory
      stdio: ['ignore', 'pipe', 'pipe'],
      detached: true
    });
    
    // Redirect output to log file (relative path)
    const logStream = require('fs').createWriteStream('.continuum/jtag/system/logs/server-node-output.log');
    child.stdout.pipe(logStream);
    child.stderr.pipe(logStream);
    
    // Detach so it keeps running
    child.unref();
    
    console.log(`📝 Test client PID: ${child.pid} (detached, logging to server-node-output.log)`);
  }

  displayDashboard(): void {
    console.log(`\n🤖 === AI AGENT AUTONOMOUS DEVELOPMENT MANUAL ===`);
    console.log(`📅 ${new Date().toISOString()}\n`);

    console.log(`🤖 HEY, I AM AN AI AGENT. I NEED THESE THINGS TO BE RUNNING:`);
    console.log(`   🚨 JTAG System on ports 9001 (WebSocket) + 9002 (HTTP)`);
    console.log(`   🚨 Build process completed (takes 45+ seconds)`);
    console.log(`   🚨 Fresh logs with recent timestamps`);
    console.log(`   🚨 Browser client connected and working`);
    console.log(``);

    console.log(`📊 CURRENT SYSTEM STATUS:`);
    console.log(`   ${this.checkPort(9001)} - JTAG WebSocket Server`);
    console.log(`   ${this.checkPort(9002)} - JTAG HTTP Server`);
    console.log(`   ${this.checkTmuxSession('jtag-test')} - Background processes`);
    console.log(``);

    console.log(`🚨 IF THINGS GO WRONG, I NEED TO KNOW:`);
    console.log(`\n📋 WHERE TO FIND ERROR INFORMATION:`);
    console.log(`   🖥️  Server Logs: .continuum/jtag/system/logs/server-console-log.log ${this.getLogStatus('.continuum/jtag/system/logs/server-console-log.log')}`);
    console.log(`   🌐 Browser Logs: .continuum/jtag/currentUser/logs/browser-console-log.log ${this.getLogStatus('.continuum/jtag/currentUser/logs/browser-console-log.log')}`);
    console.log(`   🧪 My Test Client: .continuum/jtag/system/logs/server-node-output.log ${this.getLogStatus('.continuum/jtag/system/logs/server-node-output.log')}`);
    console.log(`   ⚠️  Server Errors: .continuum/jtag/system/logs/server-console-error.log ${this.getLogStatus('.continuum/jtag/system/logs/server-console-error.log')}`);
    console.log(``);

    console.log(`📸 WHERE SCREENSHOTS ARE STORED:`);
    console.log(`   📂 .continuum/jtag/currentUser/screenshots/ (user session)`);
    console.log(`   📂 .continuum/jtag/system/screenshots/ (system)`);
    console.log(``);

    console.log(`🔧 WHAT I CAN DO TO FIX PROBLEMS:`);
    console.log(`   🔄 Start System: cd examples/test-bench && npm start (wait 45+ seconds!)`);
    console.log(`   📊 Watch My Logs: npm run agent:logs`);
    console.log(`   💥 Force Restart: npm run system:restart`);
    console.log(`   🧪 Test Manually: npx tsx test-server-client.ts`);
    console.log(`   📈 Check Ports: lsof -i :9001 -i :9002`);
    console.log(``);

    console.log(`✅ AUTONOMOUS DEVELOPMENT CHECKLIST FOR AI AGENTS:`);
    console.log(`   1. Are ports 9001/9002 listening? ${this.checkPort(9001).includes('✅') && this.checkPort(9002).includes('✅') ? '✅' : '❌'}`);
    console.log(`   2. Are logs fresh (< 5 minutes old)? Check timestamps above`);
    console.log(`   3. Can I connect to WebSocket? Check server-node-output.log`);
    console.log(`   4. Is server processing messages? Check server-console-log.log`);
    console.log(`   5. Are screenshots being created? Check screenshot folders`);
    console.log(`\n🤖 I AM READY TO DEBUG AUTONOMOUSLY WITH THIS INFORMATION!\n`);
  }

  async startJTAGSystem(): Promise<void> {
    console.log(`🚀 Starting JTAG system in background (takes 45+ seconds)...`);
    
    // Start JTAG system in background with output capture
    const jtagProcess = spawn('npm', ['start'], {
      cwd: './examples/test-bench',
      stdio: ['ignore', 'pipe', 'pipe'],
      detached: true
    });
    
    // Capture all output to system logs
    const systemLogStream = require('fs').createWriteStream('.continuum/jtag/system/logs/jtag-startup.log');
    jtagProcess.stdout.pipe(systemLogStream);
    jtagProcess.stderr.pipe(systemLogStream);
    
    // Detach so it keeps running
    jtagProcess.unref();
    
    console.log(`📝 JTAG system PID: ${jtagProcess.pid} (detached, logging to jtag-startup.log)`);
    console.log(`🔄 System starting in background - check ports in ~45 seconds`);
  }

  async run(): Promise<void> {
    // Display dashboard first so I know what's happening
    this.displayDashboard();
    
    // Check if system is already running
    const port9001Running = this.checkPort(9001).includes('✅');
    const port9002Running = this.checkPort(9002).includes('✅');
    
    if (!port9001Running || !port9002Running) {
      console.log(`🚨 JTAG system not running - starting it now...`);
      await this.startJTAGSystem();
      
      console.log(`⏳ Waiting for JTAG system to be ready...`);
      let attempts = 0;
      const maxAttempts = 30; // 30 attempts = ~60 seconds
      
      while (attempts < maxAttempts) {
        await new Promise(resolve => setTimeout(resolve, 2000)); // Wait 2 seconds
        const port9001Ready = this.checkPort(9001).includes('✅');
        const port9002Ready = this.checkPort(9002).includes('✅');
        
        attempts++;
        process.stdout.write(`⏳ Attempt ${attempts}/${maxAttempts} - Port 9001: ${port9001Ready ? '✅' : '❌'}, Port 9002: ${port9002Ready ? '✅' : '❌'}\r`);
        
        if (port9001Ready && port9002Ready) {
          console.log(`\n✅ JTAG system is ready!`);
          break;
        }
      }
      
      if (attempts >= maxAttempts) {
        console.log(`\n❌ JTAG system failed to start after ${maxAttempts * 2} seconds`);
        console.log(`📄 Check startup logs: cat .continuum/jtag/system/logs/jtag-startup.log`);
        return;
      }
    } else {
      console.log(`✅ JTAG system already running`);
    }

    // Now start test client since system is ready
    console.log(`🧪 System ready - starting test client...`);
    await this.startTestClient();
    
    // Give test client a moment to try connecting
    await new Promise(resolve => setTimeout(resolve, 5000));
    
    // Show final instructions
    console.log(`\n🤖 AUTONOMOUS AGENT READY:`);
    console.log(`   📊 Check system status: lsof -i :9001 -i :9002`);
    console.log(`   📄 Check my test results: cat .continuum/jtag/system/logs/server-node-output.log`);
    console.log(`   📄 Check system startup: cat .continuum/jtag/system/logs/jtag-startup.log`);
    console.log(`   📄 Check server activity: cat .continuum/jtag/system/logs/server-console-log.log`);
    console.log(`\n✅ Dashboard complete - I can now debug autonomously!`);
  }
}

async function main() {
  const dashboard = new JTAGAgentDashboard();
  await dashboard.run();
}

main().catch(console.error);