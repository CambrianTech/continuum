/**
 * Simple Build Event Logger - Track npm starts, builds, and redundancy
 * ====================================================================
 * 
 * Just logs build events for now - can be enhanced later with analysis.
 * Easy to integrate into package.json scripts and git hooks.
 */

import * as fs from 'fs';
import * as path from 'path';

interface BuildLogEntry {
  timestamp: string;
  event: string;
  trigger: string;
  expected: boolean;
  process_id: number;
  context?: Record<string, any>;
}

class BuildLogger {
  private logFile: string;

  constructor() {
    this.logFile = path.join(process.cwd(), '.continuum', 'logs', 'build-events.log');
    this.ensureLogDirectory();
  }

  /**
   * Log a build event (simple format for easy reading)
   */
  log(event: string, trigger: string, expected: boolean = true, context?: Record<string, any>): void {
    const entry: BuildLogEntry = {
      timestamp: new Date().toISOString(),
      event,
      trigger,
      expected,
      process_id: process.pid,
      ...(context && { context })
    };

    const logLine = this.formatLogEntry(entry);
    fs.appendFileSync(this.logFile, logLine + '\n');

    // Console output for immediate feedback
    const status = expected ? '✅' : '⚠️';
    console.log(`${status} BUILD EVENT: ${event} (${trigger}) - PID: ${process.pid}`);
  }

  /**
   * Format log entry for human readability
   */
  private formatLogEntry(entry: BuildLogEntry): string {
    const time = entry.timestamp.split('T')[1].split('.')[0]; // Just HH:MM:SS
    const status = entry.expected ? 'EXPECTED' : 'UNEXPECTED';
    const contextStr = entry.context ? ` | ${JSON.stringify(entry.context)}` : '';
    
    return `[${time}] ${status} | ${entry.event} | ${entry.trigger} | PID:${entry.process_id}${contextStr}`;
  }

  /**
   * Show recent build events (for debugging)
   */
  showRecent(lines: number = 10): void {
    if (!fs.existsSync(this.logFile)) {
      console.log('📋 No build events logged yet');
      return;
    }

    const content = fs.readFileSync(this.logFile, 'utf-8');
    const recentLines = content.trim().split('\n').slice(-lines);
    
    console.log('\n📋 RECENT BUILD EVENTS:');
    console.log('========================');
    recentLines.forEach(line => console.log(line));
    console.log('');
  }

  /**
   * Clear old logs (keep last N days)
   */
  cleanup(_daysToKeep: number = 7): void {
    // TODO: Implement log rotation if needed
  }

  private ensureLogDirectory(): void {
    const logDir = path.dirname(this.logFile);
    if (!fs.existsSync(logDir)) {
      fs.mkdirSync(logDir, { recursive: true });
    }
  }
}

// Export singleton for easy use
export const buildLogger = new BuildLogger();

// CLI usage functions
export function logNpmStart(trigger: string, expected: boolean = true): void {
  buildLogger.log('npm_start', trigger, expected);
}

export function logBuild(trigger: string, expected: boolean = true): void {
  buildLogger.log('npm_build', trigger, expected);
}

export function logGitHook(hookType: 'pre-commit' | 'pre-push', expected: boolean = true): void {
  buildLogger.log('git_hook', hookType, expected, { hook_type: hookType });
}

export function logBrowserLaunch(trigger: string, expected: boolean = true): void {
  buildLogger.log('browser_launch', trigger, expected);
}

export function showRecentEvents(): void {
  buildLogger.showRecent();
}

// CLI interface for package.json scripts
if (import.meta.url === `file://${process.argv[1]}`) {
  const [,, command, ...args] = process.argv;
  
  switch (command) {
    case 'npm-start':
      logNpmStart(args[0] || 'manual', args[1] !== 'false');
      break;
    case 'build':
      logBuild(args[0] || 'manual', args[1] !== 'false');
      break;
    case 'git-hook':
      logGitHook(args[0] as any, args[1] !== 'false');
      break;
    case 'browser':
      logBrowserLaunch(args[0] || 'manual', args[1] !== 'false');
      break;
    case 'show':
      showRecentEvents();
      break;
    default:
      console.log(`
🔧 Build Logger Usage:
=====================
npx tsx src/monitoring/build-logger.ts npm-start <trigger> [expected]
npx tsx src/monitoring/build-logger.ts build <trigger> [expected]  
npx tsx src/monitoring/build-logger.ts git-hook <pre-commit|pre-push> [expected]
npx tsx src/monitoring/build-logger.ts browser <trigger> [expected]
npx tsx src/monitoring/build-logger.ts show

Examples:
npx tsx src/monitoring/build-logger.ts npm-start "pre-commit-hook" true
npx tsx src/monitoring/build-logger.ts browser "jtag-validation" true
npx tsx src/monitoring/build-logger.ts show
`);
  }
}