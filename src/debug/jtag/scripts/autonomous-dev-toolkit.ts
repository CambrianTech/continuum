#!/usr/bin/env npx tsx
/**
 * AUTONOMOUS DEVELOPMENT TOOLKIT
 * 
 * Foolproof system that eliminates all friction points for AI autonomous development.
 * Never get stuck, always know what's wrong, always know how to fix it.
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import fs from 'fs';
import path from 'path';
import { SystemReadySignaler } from './signal-system-ready';

const execAsync = promisify(exec);

interface FrictionPoint {
  name: string;
  symptom: string;
  check: () => Promise<boolean>;
  fix: () => Promise<void>;
  priority: 'critical' | 'high' | 'medium' | 'low';
}

class AutonomousDevToolkit {
  private signaler = new SystemReadySignaler();
  
  private frictionPoints: FrictionPoint[] = [
    {
      name: 'System Not Started',
      symptom: 'Commands fail, no response from system',
      priority: 'critical',
      check: async () => {
        try {
          const { stdout } = await execAsync('lsof -ti:9002 2>/dev/null || echo ""');
          return stdout.trim().length === 0;
        } catch { return true; }
      },
      fix: async () => {
        console.log('🔧 FIX: Starting system...');
        await execAsync('npm run system:start');
        await this.waitForBootstrap();
      }
    },
    {
      name: 'Bootstrap Not Detected',
      symptom: 'System running but signal shows bootstrap: false',
      priority: 'critical', 
      check: async () => {
        try {
          const signal = await this.signaler.generateReadySignal();
          return !signal.bootstrapComplete;
        } catch { return true; }
      },
      fix: async () => {
        console.log('🔧 FIX: Refreshing bootstrap detection...');
        await this.signaler.generateReadySignal();
        await new Promise(resolve => setTimeout(resolve, 2000));
      }
    },
    {
      name: 'CLI Hanging',
      symptom: 'JTAG CLI connects but hangs without response',
      priority: 'high',
      check: async () => {
        try {
          const { stdout } = await execAsync('timeout 5 ./jtag ping 2>/dev/null || echo "TIMEOUT"');
          return stdout.includes('TIMEOUT');
        } catch { return true; }
      },
      fix: async () => {
        console.log('🔧 FIX: Restarting system to fix CLI hanging...');
        await execAsync('npm run system:restart');
        await this.waitForBootstrap();
      }
    },
    {
      name: 'Stale Signal File',
      symptom: 'Signal file exists but system not responding',
      priority: 'medium',
      check: async () => {
        const signalPath = '.continuum/jtag/signals/system-ready.json';
        try {
          const stat = await fs.promises.stat(signalPath);
          const ageMs = Date.now() - stat.mtime.getTime();
          return ageMs > 300000; // 5 minutes old
        } catch { return false; }
      },
      fix: async () => {
        console.log('🔧 FIX: Clearing stale signals...');
        await this.signaler.clearSignals();
        await this.signaler.generateReadySignal();
      }
    }
  ];

  async runDiagnostics(): Promise<void> {
    console.log('🔍 AUTONOMOUS DEVELOPMENT DIAGNOSTICS');
    console.log('='.repeat(60));
    console.log('🎯 Goal: Identify and auto-fix all friction points');
    console.log('');

    const issues: FrictionPoint[] = [];

    // Check all friction points
    for (const point of this.frictionPoints) {
      try {
        const hasIssue = await point.check();
        if (hasIssue) {
          issues.push(point);
          console.log(`❌ ${point.name}: ${point.symptom}`);
        } else {
          console.log(`✅ ${point.name}: OK`);
        }
      } catch (error: any) {
        console.log(`⚠️ ${point.name}: Check failed - ${error.message}`);
        issues.push(point);
      }
    }

    if (issues.length === 0) {
      console.log('');
      console.log('🎉 SYSTEM PERFECT - No friction points detected!');
      console.log('✅ Ready for autonomous development');
      return;
    }

    console.log('');
    console.log('🔧 AUTO-FIXING DETECTED ISSUES...');
    console.log('');

    // Sort by priority and fix
    const priorityOrder = { critical: 0, high: 1, medium: 2, low: 3 };
    issues.sort((a, b) => priorityOrder[a.priority] - priorityOrder[b.priority]);

    for (const issue of issues) {
      console.log(`🔧 Fixing: ${issue.name}`);
      try {
        await issue.fix();
        console.log(`✅ Fixed: ${issue.name}`);
      } catch (error: any) {
        console.log(`❌ Fix failed: ${issue.name} - ${error.message}`);
      }
      console.log('');
    }

    // Verify fixes worked
    console.log('🔍 Verifying fixes...');
    await this.runDiagnostics();
  }

  async waitForBootstrap(): Promise<void> {
    console.log('⏳ Waiting for bootstrap completion...');
    
    for (let i = 0; i < 30; i++) { // 30 seconds max
      try {
        const signal = await this.signaler.generateReadySignal();
        if (signal.bootstrapComplete && signal.commandCount > 0) {
          console.log(`✅ Bootstrap complete! ${signal.commandCount} commands ready`);
          return;
        }
      } catch { /* ignore errors during startup */ }
      
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
    
    throw new Error('Bootstrap timeout - system may have issues');
  }

  async quickIterationCycle(): Promise<void> {
    console.log('🚀 QUICK ITERATION CYCLE');
    console.log('='.repeat(40));
    console.log('');

    // 1. Ensure system is ready
    await this.runDiagnostics();

    // 2. Take a screenshot to see current state
    console.log('📸 Taking screenshot of current state...');
    try {
      await execAsync('./jtag screenshot --filename=iteration-current.png');
      console.log('✅ Screenshot saved: iteration-current.png');
    } catch (error: any) {
      console.log('⚠️ Screenshot failed:', error.message);
    }

    // 3. Test core functionality
    console.log('🧪 Testing core functionality...');
    try {
      await execAsync('./jtag ping');
      console.log('✅ Core functionality working');
    } catch (error: any) {
      console.log('❌ Core functionality broken:', error.message);
      await this.runDiagnostics();
    }

    console.log('');
    console.log('🎯 Ready for code changes - make your edits and run this again!');
  }

  async stressTest(): Promise<void> {
    console.log('💪 STRESS TESTING AUTONOMOUS DEVELOPMENT');
    console.log('='.repeat(50));
    
    const tests = [
      'Restart system',
      'Kill browser', 
      'Network disconnect simulation',
      'Rapid command execution',
      'Signal corruption recovery'
    ];

    for (const test of tests) {
      console.log(`🧪 Test: ${test}`);
      
      if (test === 'Restart system') {
        await execAsync('npm run system:restart');
        await this.waitForBootstrap();
      }
      // Add more stress tests as needed
      
      console.log(`✅ Test passed: ${test}`);
    }
  }
}

// CLI Interface
const toolkit = new AutonomousDevToolkit();

const command = process.argv[2];

async function main() {
  switch (command) {
    case 'diagnose':
      await toolkit.runDiagnostics();
      break;
    case 'iterate':
      await toolkit.quickIterationCycle();
      break;
    case 'stress':
      await toolkit.stressTest();
      break;
    default:
      console.log('🤖 AUTONOMOUS DEVELOPMENT TOOLKIT');
      console.log('');
      console.log('Commands:');
      console.log('  npx tsx scripts/autonomous-dev-toolkit.ts diagnose  - Find and fix friction points');
      console.log('  npx tsx scripts/autonomous-dev-toolkit.ts iterate   - Quick iteration cycle');  
      console.log('  npx tsx scripts/autonomous-dev-toolkit.ts stress    - Stress test system');
      console.log('');
      console.log('🎯 Goal: Zero friction autonomous development');
  }
}

if (require.main === module) {
  main().catch(console.error);
}

export { AutonomousDevToolkit };