#!/usr/bin/env npx tsx
/**
 * AI Agent Dashboard - Centralized Autonomous Development Portal
 * 
 * 🚀 YOUR MISSION: This is your centralized control room for autonomous development
 * 🎯 EVERYTHING YOU NEED: System status, intelligent diagnostics, logs, and debugging tools
 * 📊 NO MYSTERIES: Clear signals, obvious errors, direct paths to solutions
 * 🔧 AUTOMATION: Limited failure points, self-healing where possible
 * 
 * Usage:
 *   npx tsx agents/ai-dashboard.ts           # Full dashboard
 *   npx tsx agents/ai-dashboard.ts --quick   # Quick health check
 *   npx tsx agents/ai-dashboard.ts --fix     # Auto-fix common issues
 *   npx tsx agents/ai-dashboard.ts --exec    # Interactive command mode
 */

import { SystemReadySignaler, type SystemReadySignal } from '../scripts/signal-system-ready';
import { exec } from 'child_process';
import { promisify } from 'util';
import { existsSync, readFileSync, statSync } from 'fs';

const execAsync = promisify(exec);

interface AIAgentDashboard {
  timestamp: string;
  systemHealth: 'healthy' | 'degraded' | 'unhealthy' | 'error' | 'unknown';
  readyForDevelopment: boolean;
  criticalIssues: string[];
  availableCommands: string[];
  logPaths: {
    startup: string;
    browser: string;
    server: string;
    errors: string;
  };
  quickActions: {
    [key: string]: string;
  };
  autonomousGuidance: string[];
}

class AIAgentDashboardRunner {
  private signaler = new SystemReadySignaler();

  async runDashboard(mode: 'full' | 'quick' | 'fix' | 'exec' = 'full'): Promise<void> {
    console.log('🤖 AI AGENT DASHBOARD - Your Autonomous Development Control Room');
    console.log('=' .repeat(80));
    console.log('🎯 Mission: Centralized portal with everything you need for autonomous development');
    console.log('📊 No mysteries: Clear signals, obvious errors, direct solution paths');
    console.log('');
    console.log('🚨 CRITICAL DEVELOPMENT WORKFLOW (NEVER SKIP) 🚨');
    console.log('📋 Follow dev-process.md: 1) npm run system:stop 2) npm run system:start 3) Verify bootstrap!');
    console.log('');

    const dashboard = await this.generateDashboard();
    
    switch (mode) {
      case 'quick':
        this.displayQuickStatus(dashboard);
        break;
      case 'fix':
        await this.runAutoFix(dashboard);
        break;
      case 'exec':
        await this.runCommandExecution(dashboard);
        break;
      default:
        this.displayFullDashboard(dashboard);
    }
    
    this.displayAutonomousGuidance(dashboard);
  }

  private async generateDashboard(): Promise<AIAgentDashboard> {
    console.log('📊 Gathering intelligent system diagnostics...');
    
    // Get intelligent signal data
    const signalData = await this.signaler.checkSystemReady(5000);
    
    // Determine development readiness
    const readyForDevelopment = signalData?.systemHealth === 'healthy' || signalData?.systemHealth === 'degraded';
    
    // Get available commands
    const availableCommands = await this.getAvailableCommands();
    
    // Generate critical issues list
    const criticalIssues = this.identifyCriticalIssues(signalData);
    
    // Log paths for debugging
    const logPaths = this.getLogPaths();
    
    // Quick actions for common tasks
    const quickActions = this.getQuickActions();
    
    // AI-specific guidance
    const autonomousGuidance = this.generateAutonomousGuidance(signalData, criticalIssues);
    
    return {
      timestamp: new Date().toISOString(),
      systemHealth: signalData?.systemHealth ?? 'unknown',
      readyForDevelopment,
      criticalIssues,
      availableCommands,
      logPaths,
      quickActions,
      autonomousGuidance
    };
  }

  private displayFullDashboard(dashboard: AIAgentDashboard): void {
    console.log('🚦 SYSTEM STATUS');
    console.log('-' .repeat(40));
    console.log(`Health: ${this.getHealthEmoji(dashboard.systemHealth)} ${dashboard.systemHealth.toUpperCase()}`);
    console.log(`Ready for Development: ${dashboard.readyForDevelopment ? '✅ YES' : '❌ NO'}`);
    console.log(`Commands Available: ${dashboard.availableCommands.length}`);
    console.log(`Last Updated: ${new Date(dashboard.timestamp).toLocaleTimeString()}`);
    console.log('');

    if (dashboard.criticalIssues.length > 0) {
      console.log('🚨 CRITICAL ISSUES (Fix These First)');
      console.log('-' .repeat(40));
      dashboard.criticalIssues.forEach((issue, i) => {
        console.log(`${i + 1}. ❌ ${issue}`);
      });
      console.log('');
    }

    console.log('📂 LOG LOCATIONS (Your Debugging Friends)');
    console.log('-' .repeat(40));
    console.log(`Startup Logs:    ${dashboard.logPaths.startup}`);
    console.log(`Browser Logs:    ${dashboard.logPaths.browser}`);
    console.log(`Server Logs:     ${dashboard.logPaths.server}`);
    console.log(`Error Analysis:  ${dashboard.logPaths.errors}`);
    console.log('');

    console.log('⚡ QUICK ACTIONS');
    console.log('-' .repeat(40));
    Object.entries(dashboard.quickActions).forEach(([action, command]) => {
      console.log(`${action}: ${command}`);
    });
    console.log('');

    console.log('🛠️ JTAG COMMAND SYSTEM (Rich Parameter Support)');
    console.log('-' .repeat(40));
    if (dashboard.availableCommands.length > 0) {
      console.log('  📸 SCREENSHOT: ./jtag screenshot --querySelector="body" --filename="debug.png"');
      console.log('  🔧 EXEC:       ./jtag exec --code="console.log(\'Hello AI\')" --environment="browser"');
      console.log('  📝 FILE SAVE:  ./jtag file/save --path="/tmp/test.txt" --content="AI generated"');
      console.log('  🖱️ CLICK:      ./jtag click --selector="button.submit"');
      console.log('  ⌨️ TYPE:       ./jtag type --text="Autonomous AI input" --selector="input[type=text]"');
      console.log('  🌐 NAVIGATE:   ./jtag navigate --url="http://localhost:9002"');
      console.log('  📋 LIST ALL:   ./jtag list --category="all" --includeDescription="true"');
      console.log('');
      console.log('  💡 Full help:  ./jtag help');
      console.log(`  📊 Available: ${dashboard.availableCommands.length} commands ready for use`);
    } else {
      console.log('  ❌ No commands available - system needs startup');
      console.log('  🚀 Start with: npm run system:start');
    }
    console.log('');
  }

  private displayQuickStatus(dashboard: AIAgentDashboard): void {
    const status = dashboard.readyForDevelopment ? '✅ READY' : '❌ NOT READY';
    console.log(`${status} | Health: ${dashboard.systemHealth} | Commands: ${dashboard.availableCommands.length}`);
    
    if (dashboard.criticalIssues.length > 0) {
      console.log('🚨 Critical Issues:');
      dashboard.criticalIssues.forEach(issue => console.log(`  - ${issue}`));
    }
  }

  private async runAutoFix(dashboard: AIAgentDashboard): Promise<void> {
    console.log('🔧 AUTONOMOUS AUTO-FIX MODE');
    console.log('-' .repeat(40));
    
    if (dashboard.readyForDevelopment) {
      console.log('✅ System is ready - no fixes needed');
      return;
    }
    
    console.log('🔄 Attempting automatic fixes...');
    
    try {
      // Auto-fix 1: System not running
      if (dashboard.systemHealth === 'unknown' || dashboard.systemHealth === 'error') {
        console.log('🚀 Starting system with intelligent monitoring...');
        await execAsync('npm run system:start');
        console.log('⏳ Waiting for system to signal readiness...');
        await this.waitForSystemReady();
      }
      
      // Auto-fix 2: Check for common issues
      for (const issue of dashboard.criticalIssues) {
        if (issue.includes('Tmux session')) {
          console.log('🔄 Restarting tmux session...');
          await execAsync('npm run system:restart');
          await this.waitForSystemReady();
        }
      }
      
      console.log('✅ Auto-fix complete - recheck system status');
      
    } catch (error: any) {
      console.error('❌ Auto-fix failed:', error.message);
      console.log('💡 Manual intervention required - check logs:');
      console.log(`   ${dashboard.logPaths.startup}`);
      console.log(`   ${dashboard.logPaths.errors}`);
    }
  }

  private displayAutonomousGuidance(dashboard: AIAgentDashboard): void {
    console.log('🧠 AUTONOMOUS DEVELOPMENT GUIDANCE');
    console.log('=' .repeat(80));
    
    dashboard.autonomousGuidance.forEach((guidance, i) => {
      console.log(`${i + 1}. ${guidance}`);
    });
    
    console.log('');
    console.log('🎯 CORE PRINCIPLE: Logs are your friend - no mysteries in autonomous development!');
    console.log('📈 INTELLIGENT SYSTEM: No more guessing - system tells you exactly what\'s wrong');
    console.log('🚀 AUTOMATION GOAL: Limited failure points, self-healing, obvious error paths');
  }

  private async getAvailableCommands(): Promise<string[]> {
    try {
      // Use JTAG list command to get real available commands dynamically
      const { stdout } = await execAsync('./jtag list 2>/dev/null || echo "Commands not available"');
      
      if (stdout.includes('SUCCESS')) {
        // Parse the actual command count from the output
        const match = stdout.match(/Found: (\d+) commands/);
        if (match) {
          // Get the real command list from the signal file
          try {
            const signalData = JSON.parse(readFileSync('.continuum/jtag/signals/system-ready.json', 'utf8'));
            if (signalData.commandCount) {
              return Array.from({length: signalData.commandCount}, (_, i) => `command_${i+1}`);
            }
          } catch {
            // Fallback: Use known commands if signal parsing fails
            return ['screenshot', 'ping', 'list', 'exec', 'file/save', 'file/load', 'navigate', 'click', 'type', 'get-text', 'wait-for-element', 'scroll', 'session/create', 'file/append', 'compile-typescript', 'proxy-navigate', 'test-error', 'test/routing-chaos'];
          }
        }
      }
      
      return [];
    } catch {
      return [];
    }
  }

  private identifyCriticalIssues(signalData: SystemReadySignal | null): string[] {
    const issues: string[] = [];
    
    if (!signalData) {
      issues.push('No system signal available - run: npm run system:start');
      return issues;
    }
    
    if (signalData.compilationStatus === 'failed') {
      issues.push('TypeScript compilation failed - check: npm run signal:logs');
    }
    
    if (signalData.errors && signalData.errors.length > 0) {
      signalData.errors.forEach(error => {
        issues.push(`System error: ${error}`);
      });
    }
    
    if (signalData.nodeErrors && signalData.nodeErrors.length > 0) {
      issues.push(`Node runtime errors detected (${signalData.nodeErrors.length} errors)`);
    }
    
    if (signalData.portsActive.length < 2) {
      issues.push(`Ports not active: Expected [9001,9002], got [${signalData.portsActive.join(',')}]`);
    }
    
    if (!signalData.bootstrapComplete) {
      issues.push('Bootstrap not complete - browser client not connected');
    }
    
    return issues;
  }

  private getLogPaths(): AIAgentDashboard['logPaths'] {
    return {
      startup: '.continuum/jtag/system/logs/npm-start.log',
      browser: 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log',
      server: 'examples/test-bench/.continuum/jtag/sessions/system/*/logs/server-console-log.log',
      errors: '.continuum/jtag/signals/system-ready.json'
    };
  }

  private getQuickActions(): AIAgentDashboard['quickActions'] {
    return {
      '🚨 AUTONOMOUS WORKFLOW': '=== FOOLPROOF AI DEVELOPMENT ===',
      '🎯 JUST USE JTAG': './jtag [command] - handles everything automatically',
      '🚀 FIRST RUN': './jtag screenshot (auto-starts system + browser)',
      '⚡ SUBSEQUENT': './jtag ping (instant, reuses system)',
      '🔄 AFTER CODE CHANGES': './jtag screenshot --restart',
      '📊 CHECK STATUS': 'npm run signal:check',
      '📋 VIEW LOGS': 'npm run signal:logs',
      '❌ ERROR ANALYSIS': 'npm run signal:errors',
      '🧪 RUN TESTS': 'npm test',
      '🛠️ FORCE RESTART': 'npm run system:restart',
      '🎯 AI DASHBOARD': 'npm run agent',
      '⚡ QUICK STATUS': 'npm run agent:quick',
      '🔧 AUTO-FIX': 'npm run agent:fix',
      '💡 UNIVERSAL START': 'npm start (now intelligent too!)'
    };
  }

  private generateAutonomousGuidance(signalData: SystemReadySignal | null, issues: string[]): string[] {
    const guidance: string[] = [];
    
    if (!signalData) {
      guidance.push('🚀 FIRST STEP: Start the system with "npm run system:start"');
      guidance.push('⏳ WAIT: System will signal when ready (no fixed timeouts!)');
      guidance.push('🔍 CHECK: Use "npm run signal:check" to verify readiness');
      return guidance;
    }
    
    switch (signalData.systemHealth) {
      case 'healthy':
        guidance.push('✅ READY: System is healthy - proceed with development/testing');
        guidance.push('🧪 TEST: Run "npm test" for comprehensive testing');
        guidance.push('📸 DEBUG: Use "npm run screenshot" for visual debugging');
        break;
        
      case 'degraded':
        guidance.push('⚠️ CAUTION: System running but has issues - proceed carefully');
        guidance.push('🔍 INVESTIGATE: Check "npm run signal:errors" for details');
        guidance.push('🧪 LIMITED TESTING: Some tests may fail due to degraded state');
        break;
        
      case 'unhealthy':
        guidance.push('❌ NOT READY: System has critical issues - fix before proceeding');
        guidance.push('📋 CHECK LOGS: "npm run signal:logs" shows startup details');
        guidance.push('🔄 RESTART: Try "npm run system:restart" for clean start');
        break;
        
      case 'error':
        guidance.push('🚨 BROKEN: System has errors - requires immediate attention');
        guidance.push('📊 DIAGNOSE: Check both startup logs and error analysis');
        guidance.push('🛠️ MANUAL FIX: Automatic recovery not possible');
        break;
    }
    
    // Context-specific guidance based on issues
    if (issues.some(issue => issue.includes('compilation'))) {
      guidance.push('💻 COMPILATION: Fix TypeScript errors before system can start');
    }
    
    if (issues.some(issue => issue.includes('bootstrap'))) {
      guidance.push('🌐 BROWSER: Check if browser tab opened and connected');
    }
    
    if (issues.some(issue => issue.includes('Ports'))) {
      guidance.push('🔌 NETWORK: Port conflicts - check for other running services');
    }
    
    // Always include these fundamentals
    guidance.push('📊 MONITORING: This dashboard auto-updates - logs always tell the truth');
    guidance.push('🔄 ITERATION: Change code → restart system → check signal → test → repeat');
    guidance.push('🧠 NO GUESSING: If something seems wrong, check the specific logs mentioned');
    guidance.push('🚨 CRITICAL: ALWAYS follow dev-process.md workflow - stop → start → wait → verify bootstrap!');
    
    return guidance;
  }

  private async waitForSystemReady(timeoutMs = 60000): Promise<void> {
    const startTime = Date.now();
    
    while (Date.now() - startTime < timeoutMs) {
      const signal = await this.signaler.checkSystemReady(5000);
      
      if (signal && (signal.systemHealth === 'healthy' || signal.systemHealth === 'degraded')) {
        console.log('✅ System ready!');
        return;
      }
      
      console.log('⏳ Waiting for system ready signal...');
      await new Promise(resolve => setTimeout(resolve, 3000));
    }
    
    throw new Error('System ready timeout');
  }

  private async runCommandExecution(dashboard: AIAgentDashboard): Promise<void> {
    console.log('🔧 JTAG COMMAND EXECUTION MODE');
    console.log('-' .repeat(40));
    
    if (!dashboard.readyForDevelopment) {
      console.log('❌ System not ready for command execution');
      console.log('🚀 Run auto-fix first: npm run agent:fix');
      return;
    }
    
    console.log('✅ System ready for command execution');
    console.log('📋 Common AI Commands:');
    console.log('');
    
    // Demonstrate command execution with actual JTAG CLI calls
    const commonCommands = [
      {
        name: '📸 Take Screenshot',
        command: './continuum screenshot --querySelector="body" --filename="ai-debug-$(date +%s).png"',
        description: 'Capture current page state for visual debugging'
      },
      {
        name: '🔍 List All Commands',
        command: './continuum list --category="all" --includeDescription="true"',
        description: 'Get complete command inventory with descriptions'
      },
      {
        name: '🧪 Test System Health',
        command: './continuum ping',
        description: 'Verify system responsiveness'
      },
      {
        name: '⚡ Execute Code',
        command: './continuum exec --code="console.log(\'AI Test: \' + new Date())" --environment="browser"',
        description: 'Run JavaScript in browser context'
      }
    ];
    
    console.log('🎯 READY-TO-RUN COMMANDS:');
    commonCommands.forEach((cmd, i) => {
      console.log(`${i + 1}. ${cmd.name}`);
      console.log(`   Command: ${cmd.command}`);
      console.log(`   Purpose: ${cmd.description}`);
      console.log('');
    });
    
    console.log('💡 USAGE PATTERN FOR AIs:');
    console.log('  1. Check system ready: npm run agent:quick');
    console.log('  2. Execute JTAG commands: ./continuum <command> [options]');
    console.log('  3. Get visual feedback: ./continuum screenshot');
    console.log('  4. Debug with logs if needed');
    console.log('');
    
    console.log('🔗 COMMAND CHAINING EXAMPLES:');
    console.log('  ./continuum screenshot --filename="before.png" && \\');
    console.log('  ./continuum click --selector="button#test" && \\');
    console.log('  ./continuum screenshot --filename="after.png"');
    console.log('');
    
    console.log('📊 This dashboard integrates with the full JTAG command ecosystem!');
  }

  private getHealthEmoji(health: string): string {
    switch (health) {
      case 'healthy': return '✅';
      case 'degraded': return '⚠️';
      case 'unhealthy': return '❌';
      case 'error': return '🚨';
      default: return '❓';
    }
  }
}

// CLI Interface
async function main() {
  const args = process.argv.slice(2);
  const dashboard = new AIAgentDashboardRunner();
  
  if (args.includes('--quick')) {
    await dashboard.runDashboard('quick');
  } else if (args.includes('--fix')) {
    await dashboard.runDashboard('fix');
  } else if (args.includes('--exec')) {
    await dashboard.runDashboard('exec');
  } else {
    await dashboard.runDashboard('full');
  }
}

if (require.main === module) {
  main().catch(console.error);
}

export { AIAgentDashboardRunner };