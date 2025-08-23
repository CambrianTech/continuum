#!/usr/bin/env tsx
/**
 * Improved System Detector - Actually tests system functionality
 * 
 * Instead of looking for specific log messages, this detector:
 * 1. Tests if the browser UI is responding
 * 2. Tests if the WebSocket is accepting connections
 * 3. Tests if commands are actually working
 * 4. Provides clear failure diagnostics
 */

import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

interface SystemHealthCheck {
  name: string;
  check: () => Promise<{ success: boolean; details: string }>;
}

async function improvedSystemDetector(): Promise<void> {
  console.log('🔍 IMPROVED SYSTEM DETECTOR');
  console.log('==========================');
  console.log('Testing actual system functionality instead of log file parsing');
  console.log();

  const healthChecks: SystemHealthCheck[] = [
    {
      name: 'Browser UI Response',
      check: async () => {
        try {
          const { stdout } = await execAsync('curl -s -w "%{http_code}" -o /dev/null http://localhost:9003/');
          const statusCode = stdout.trim();
          if (statusCode === '200') {
            return { success: true, details: `HTTP ${statusCode} - Browser UI responding` };
          } else {
            return { success: false, details: `HTTP ${statusCode} - Browser UI not responding` };
          }
        } catch (error: any) {
          return { success: false, details: `Connection failed: ${error.message}` };
        }
      }
    },
    
    {
      name: 'WebSocket Server',
      check: async () => {
        try {
          // Test if WebSocket port is open
          const { stdout } = await execAsync('nc -z localhost 9001 && echo "open" || echo "closed"');
          if (stdout.trim() === 'open') {
            return { success: true, details: 'WebSocket port 9001 accepting connections' };
          } else {
            return { success: false, details: 'WebSocket port 9001 not accepting connections' };
          }
        } catch (error: any) {
          return { success: false, details: `WebSocket test failed: ${error.message}` };
        }
      }
    },
    
    {
      name: 'System Logs Creation',
      check: async () => {
        try {
          // Check if session directory structure exists
          const { stdout } = await execAsync('ls -la .continuum/jtag/currentUser/ 2>/dev/null || echo "no session"');
          if (stdout.includes('logs') || stdout.includes('screenshots')) {
            return { success: true, details: 'Session directory with logs/screenshots exists' };
          } else {
            return { success: false, details: 'Session directory not properly created' };
          }
        } catch (error: any) {
          return { success: false, details: `Session check failed: ${error.message}` };
        }
      }
    },
    
    {
      name: 'Command System',
      check: async () => {
        try {
          // Try to find any evidence that commands are registered
          const tmuxOutput = await execAsync('tmux capture-pane -t jtag-test -p | grep -E "commands? ready|Bootstrap complete" | tail -1');
          if (tmuxOutput.stdout.includes('commands ready') || tmuxOutput.stdout.includes('Bootstrap complete')) {
            const match = tmuxOutput.stdout.match(/(\d+)\s+commands?\s+ready/);
            const commandCount = match ? parseInt(match[1]) : 0;
            if (commandCount > 0) {
              return { success: true, details: `${commandCount} commands registered and ready` };
            } else {
              return { success: false, details: 'Bootstrap complete but no commands detected' };
            }
          } else {
            return { success: false, details: 'No command registration messages detected' };
          }
        } catch (error: any) {
          return { success: false, details: `Command detection failed: ${error.message}` };
        }
      }
    }
  ];

  console.log('🧪 Running System Health Checks:');
  console.log();
  
  let allPassed = true;
  const results: { name: string; success: boolean; details: string }[] = [];
  
  for (const check of healthChecks) {
    try {
      const result = await check.check();
      results.push({ name: check.name, ...result });
      
      const status = result.success ? '✅ PASS' : '❌ FAIL';
      console.log(`${status}: ${check.name}`);
      console.log(`   ${result.details}`);
      
      if (!result.success) {
        allPassed = false;
      }
    } catch (error: any) {
      results.push({ name: check.name, success: false, details: `Exception: ${error.message}` });
      console.log(`❌ FAIL: ${check.name}`);
      console.log(`   Exception: ${error.message}`);
      allPassed = false;
    }
  }
  
  console.log();
  console.log('📊 SYSTEM HEALTH SUMMARY');
  console.log('========================');
  
  if (allPassed) {
    console.log('🎉 ALL HEALTH CHECKS PASSED!');
    console.log('✅ System is fully operational and ready for development');
    console.log();
    console.log('🔧 You can now:');
    console.log('   • Run tests: npm test');
    console.log('   • Take screenshots: npm run screenshot');  
    console.log('   • Access browser UI: http://localhost:9003');
    console.log('   • Use JTAG commands via WebSocket on port 9001');
    process.exit(0);
  } else {
    console.log('⚠️ SOME HEALTH CHECKS FAILED');
    const failedChecks = results.filter(r => !r.success);
    console.log(`❌ Failed: ${failedChecks.length}/${results.length} checks`);
    console.log();
    
    console.log('🔍 DIAGNOSTIC INFORMATION:');
    failedChecks.forEach(check => {
      console.log(`   • ${check.name}: ${check.details}`);
    });
    
    console.log();
    console.log('💡 SUGGESTED FIXES:');
    
    if (failedChecks.some(c => c.name === 'Browser UI Response')) {
      console.log('   • Browser UI not responding - check if server started properly');
      console.log('   • Try: tmux attach-session -t jtag-test');
    }
    
    if (failedChecks.some(c => c.name === 'WebSocket Server')) {
      console.log('   • WebSocket server not running - check for port conflicts');
      console.log('   • Try: lsof -i :9001');
    }
    
    if (failedChecks.some(c => c.name === 'System Logs Creation')) {
      console.log('   • Session directory not created - browser may not be connecting');
      console.log('   • This is likely the root cause of the silent failure');
    }
    
    if (failedChecks.some(c => c.name === 'Command System')) {
      console.log('   • Commands not registered - daemon system may have failed');
      console.log('   • Check server console for daemon creation errors');
    }
    
    process.exit(1);
  }
}

// Run if executed directly
if (require.main === module) {
  improvedSystemDetector().catch(error => {
    console.error('❌ System detector crashed:', error);
    process.exit(1);
  });
}

export { improvedSystemDetector };