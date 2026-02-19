/**
 * JTAG System Health Suite - Modular replacement for improved-system-detector
 *
 * Uses the new HealthCheckFramework to provide reusable JTAG system validation
 */

import { HealthCheckRunner, HealthSuite, SystemHealthChecks } from './HealthCheckFramework';
import { HTTP_PORT, WS_PORT } from '../config';

export function createJTAGHealthSuite(): HealthSuite {
  return {
    name: 'JTAG System Health',
    description: 'Comprehensive validation of JTAG debugging infrastructure',
    checks: [
      SystemHealthChecks.httpEndpoint(`http://localhost:${HTTP_PORT}`, 200),
      SystemHealthChecks.portOpen(WS_PORT),  // WebSocket port (from config)
      SystemHealthChecks.portOpen(HTTP_PORT), // HTTP port (from config)
      SystemHealthChecks.processRunning('tmux'),
      SystemHealthChecks.directoryExists('.continuum/jtag'),
      
      // Custom JTAG-specific checks
      {
        name: 'Session Directory Structure',
        description: 'Verify session directory is properly created',
        check: async () => {
          const { execAsync } = await import('../utils/ProcessUtils');
          
          // Check multiple possible session locations
          const possiblePaths = [
            '.continuum/jtag/currentUser/',
            'examples/widget-ui/.continuum/jtag/currentUser/',
            'examples/test-bench/.continuum/jtag/currentUser/'
          ];
          
          let foundPath = '';
          let hasLogs = false;
          let hasScreenshots = false;
          
          for (const path of possiblePaths) {
            const result = await execAsync(`ls -la ${path} 2>/dev/null || echo ""`);
            if (result.success && result.stdout.trim() && !result.stdout.includes('No such file')) {
              foundPath = path;
              hasLogs = result.stdout.includes('logs');
              hasScreenshots = result.stdout.includes('screenshots');
              break;
            }
          }
          
          return {
            name: 'Session Directory Structure',
            success: hasLogs || hasScreenshots || foundPath.length > 0,
            details: foundPath
              ? hasLogs && hasScreenshots 
                ? `Session directory found at ${foundPath} with logs and screenshots`
                : hasLogs 
                  ? `Session directory found at ${foundPath} with logs (screenshots missing)`
                  : hasScreenshots
                    ? `Session directory found at ${foundPath} with screenshots (logs missing)`
                    : `Session directory found at ${foundPath} but no logs/screenshots yet`
              : 'Session directory not found in any expected location',
            metadata: { foundPath, hasLogs, hasScreenshots, checkedPaths: possiblePaths }
          };
        }
      },

      {
        name: 'Command Registration',
        description: 'Verify JTAG commands are properly registered',
        check: async () => {
          const { execAsync } = await import('../utils/ProcessUtils');
          const result = await execAsync('tmux capture-pane -t jtag-test -p 2>/dev/null | grep -E "commands? ready" | tail -1 || echo "no commands"');
          
          if (result.success && result.stdout.includes('commands ready')) {
            const match = result.stdout.match(/(\d+)\s+commands?\s+ready/);
            const commandCount = match ? parseInt(match[1]) : 0;
            
            return {
              name: 'Command Registration',
              success: commandCount > 0,
              details: commandCount > 0 
                ? `${commandCount} commands registered and ready`
                : 'Commands detected but count unknown',
              metadata: { commandCount }
            };
          } else {
            return {
              name: 'Command Registration', 
              success: false,
              details: 'No command registration messages detected',
              metadata: { commandCount: 0 }
            };
          }
        }
      },

      {
        name: 'Widget UI Elements',
        description: 'Verify core widgets are loaded in browser',
        check: async () => {
          const { execAsync } = await import('../utils/ProcessUtils');
          const result = await execAsync(`curl -s http://localhost:${HTTP_PORT}/ | grep -E "chat-widget|screenshot-widget" | wc -l`);
          
          if (result.success) {
            const widgetCount = parseInt(result.stdout.trim());
            return {
              name: 'Widget UI Elements',
              success: widgetCount >= 2,
              details: widgetCount >= 2
                ? `Found ${widgetCount} core widgets (chat-widget, screenshot-widget)`
                : `Only found ${widgetCount} widgets - some may be missing`,
              metadata: { widgetCount, expectedCount: 2 }
            };
          } else {
            return {
              name: 'Widget UI Elements',
              success: false,
              details: 'Failed to check widget elements',
              metadata: { error: result.stderr }
            };
          }
        }
      }
    ],
    onComplete: (results) => {
      const runner = new HealthCheckRunner();
      const summary = runner.getSummary(results);
      
      console.log('📊 JTAG SYSTEM HEALTH SUMMARY');
      console.log('==============================');
      console.log(`✅ Passed: ${summary.passed}/${summary.total} checks`);
      console.log(`📈 Success Rate: ${summary.successRate.toFixed(1)}%`);
      console.log(`⏱️  Total Duration: ${summary.totalDuration}ms`);
      
      if (summary.successRate === 100) {
        console.log();
        console.log('🎉 JTAG SYSTEM FULLY OPERATIONAL!');
        console.log('✅ All systems green - ready for development');
      } else if (summary.successRate >= 75) {
        console.log();
        console.log('⚠️  JTAG SYSTEM MOSTLY OPERATIONAL');
        console.log('🔄 Some non-critical systems may need attention');
      } else {
        console.log();
        console.log('❌ JTAG SYSTEM HAS SIGNIFICANT ISSUES');
        console.log('🚨 Critical systems failing - investigate immediately');
      }
    }
  };
}

/**
 * Quick health check for JTAG system
 */
export async function checkJTAGHealth(): Promise<boolean> {
  const runner = new HealthCheckRunner();
  runner.registerSuite('jtag', createJTAGHealthSuite());
  
  const results = await runner.runSuite('jtag');
  const summary = runner.getSummary(results);
  
  return summary.successRate >= 75; // 75% threshold for "healthy"
}

/**
 * Quick critical-only health check (for startup validation)
 */
export async function checkJTAGCritical(): Promise<boolean> {
  const runner = new HealthCheckRunner();
  const criticalSuite: HealthSuite = {
    name: 'JTAG Critical Systems',
    description: 'Essential systems that must be working',
    checks: [
      SystemHealthChecks.httpEndpoint(`http://localhost:${HTTP_PORT}`, 200),
      SystemHealthChecks.portOpen(WS_PORT),
      SystemHealthChecks.processRunning('tmux')
    ]
  };
  
  runner.registerSuite('critical', criticalSuite);
  const results = await runner.runSuite('critical');
  const summary = runner.getSummary(results);
  
  return summary.successRate === 100; // All critical checks must pass
}