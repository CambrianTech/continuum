/**
 * Build Efficiency Monitor - Track npm starts, builds, and system events
 * ====================================================================
 * 
 * Monitors build efficiency by tracking:
 * - npm start calls vs expectations
 * - Build redundancy patterns
 * - System startup timing
 * - Git hook efficiency
 * 
 * Helps optimize development workflow and catch inefficiencies.
 */

import * as fs from 'fs';
import * as path from 'path';

export interface BuildEvent {
  timestamp: string;
  type: 'npm_start' | 'npm_build' | 'git_hook' | 'system_startup' | 'browser_launch';
  trigger: string; // What triggered this event
  expected: boolean; // Was this event expected?
  duration_ms?: number;
  context: {
    hook_type?: 'pre-commit' | 'pre-push';
    command?: string;
    session_id?: string;
    process_id: number;
  };
  efficiency_notes?: string[];
}

export interface EfficiencyReport {
  period: { start: string; end: string };
  summary: {
    total_npm_starts: number;
    expected_npm_starts: number;
    redundant_npm_starts: number;
    efficiency_score: number; // 0-100%
  };
  events: BuildEvent[];
  recommendations: string[];
}

export class BuildEfficiencyMonitor {
  private logFile: string;
  private expectedBuilds: Map<string, number> = new Map();

  constructor() {
    this.logFile = path.join(process.cwd(), '.continuum', 'logs', 'build-efficiency.jsonl');
    this.ensureLogDirectory();
    this.initializeExpectations();
  }

  /**
   * Set expectations for build events in different scenarios
   */
  private initializeExpectations(): void {
    // Git hook expectations
    this.expectedBuilds.set('pre-commit', 1); // Should only run npm run jtag once
    this.expectedBuilds.set('pre-push', 0);   // Should reuse pre-commit results
    this.expectedBuilds.set('manual-test', 1); // Manual testing should be efficient
    
    // Development workflow expectations
    this.expectedBuilds.set('development-session', 1); // One npm start per dev session
    this.expectedBuilds.set('hot-reload', 0); // Hot reload shouldn't require full restart
  }

  /**
   * Log a build event with efficiency analysis
   */
  logBuildEvent(event: Omit<BuildEvent, 'timestamp'>): void {
    const fullEvent: BuildEvent = {
      ...event,
      timestamp: new Date().toISOString()
    };

    // Add efficiency analysis
    this.analyzeBuildEfficiency(fullEvent);

    // Log to file
    this.appendToLog(fullEvent);

    // Real-time efficiency alerts
    if (!event.expected) {
      this.alertRedundantBuild(fullEvent);
    }
  }

  /**
   * Analyze build efficiency and add recommendations
   */
  private analyzeBuildEfficiency(event: BuildEvent): void {
    const notes: string[] = [];
    
    // Check for common inefficiency patterns
    if (event.type === 'npm_start' && !event.expected) {
      notes.push('Unexpected npm start - investigate trigger');
    }

    if (event.type === 'browser_launch' && this.getRecentBrowserLaunches() > 1) {
      notes.push('Multiple browser launches detected - possible redundancy');
    }

    if (event.context.hook_type === 'pre-push' && event.type === 'npm_start') {
      notes.push('Pre-push should reuse pre-commit validation - optimization opportunity');
    }

    if (event.duration_ms && event.duration_ms > 30000) {
      notes.push(`Slow ${event.type} (${event.duration_ms}ms) - performance investigation needed`);
    }

    event.efficiency_notes = notes;
  }

  /**
   * Get recent browser launches (last 5 minutes)
   */
  private getRecentBrowserLaunches(): number {
    const fiveMinutesAgo = new Date(Date.now() - 5 * 60 * 1000);
    const recentEvents = this.getRecentEvents(fiveMinutesAgo);
    
    return recentEvents.filter(e => e.type === 'browser_launch').length;
  }

  /**
   * Alert about redundant builds
   */
  private alertRedundantBuild(event: BuildEvent): void {
    console.warn(`⚠️ BUILD EFFICIENCY ALERT: Unexpected ${event.type} triggered by ${event.trigger}`);
    if (event.efficiency_notes?.length) {
      event.efficiency_notes.forEach(note => console.warn(`   💡 ${note}`));
    }
  }

  /**
   * Generate efficiency report for a time period
   */
  generateEfficiencyReport(hours: number = 24): EfficiencyReport {
    const startTime = new Date(Date.now() - hours * 60 * 60 * 1000);
    const events = this.getRecentEvents(startTime);

    const npmStarts = events.filter(e => e.type === 'npm_start');
    const expectedStarts = npmStarts.filter(e => e.expected);
    const redundantStarts = npmStarts.filter(e => !e.expected);

    const efficiencyScore = npmStarts.length > 0 
      ? Math.round((expectedStarts.length / npmStarts.length) * 100)
      : 100;

    const recommendations = this.generateRecommendations(events);

    return {
      period: {
        start: startTime.toISOString(),
        end: new Date().toISOString()
      },
      summary: {
        total_npm_starts: npmStarts.length,
        expected_npm_starts: expectedStarts.length,
        redundant_npm_starts: redundantStarts.length,
        efficiency_score: efficiencyScore
      },
      events,
      recommendations
    };
  }

  /**
   * Generate optimization recommendations based on event patterns
   */
  private generateRecommendations(events: BuildEvent[]): string[] {
    const recommendations: string[] = [];

    // Check for multiple npm starts in short periods
    const npmStarts = events.filter(e => e.type === 'npm_start');
    if (npmStarts.length > 3) {
      recommendations.push('Consider implementing build result caching to avoid redundant npm starts');
    }

    // Check for browser launch patterns
    const browserLaunches = events.filter(e => e.type === 'browser_launch');
    if (browserLaunches.length > 2) {
      recommendations.push('Multiple browser launches detected - implement better process cleanup');
    }

    // Check git hook efficiency
    const gitHookEvents = events.filter(e => e.context.hook_type);
    const redundantHookBuilds = gitHookEvents.filter(e => !e.expected);
    if (redundantHookBuilds.length > 0) {
      recommendations.push('Git hooks have redundant builds - optimize pre-push to reuse pre-commit results');
    }

    // Check for slow builds
    const slowBuilds = events.filter(e => e.duration_ms && e.duration_ms > 20000);
    if (slowBuilds.length > 0) {
      recommendations.push('Some builds are slow (>20s) - investigate performance bottlenecks');
    }

    return recommendations;
  }

  /**
   * Get recent events from log file
   */
  private getRecentEvents(since: Date): BuildEvent[] {
    if (!fs.existsSync(this.logFile)) {
      return [];
    }

    const lines = fs.readFileSync(this.logFile, 'utf-8').trim().split('\n');
    const events: BuildEvent[] = [];

    for (const line of lines) {
      if (!line.trim()) continue;
      
      try {
        const event: BuildEvent = JSON.parse(line);
        if (new Date(event.timestamp) >= since) {
          events.push(event);
        }
      } catch (error) {
        console.warn(`Invalid log line: ${line}`);
      }
    }

    return events.sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime());
  }

  /**
   * Append event to log file
   */
  private appendToLog(event: BuildEvent): void {
    const logEntry = JSON.stringify(event) + '\n';
    fs.appendFileSync(this.logFile, logEntry);
  }

  /**
   * Ensure log directory exists
   */
  private ensureLogDirectory(): void {
    const logDir = path.dirname(this.logFile);
    if (!fs.existsSync(logDir)) {
      fs.mkdirSync(logDir, { recursive: true });
    }
  }

  /**
   * Print efficiency summary to console
   */
  printEfficiencySummary(hours: number = 1): void {
    const report = this.generateEfficiencyReport(hours);
    
    console.log('\n📊 BUILD EFFICIENCY REPORT');
    console.log('==========================');
    console.log(`📅 Period: Last ${hours} hour(s)`);
    console.log(`🔧 Total npm starts: ${report.summary.total_npm_starts}`);
    console.log(`✅ Expected: ${report.summary.expected_npm_starts}`);
    console.log(`⚠️ Redundant: ${report.summary.redundant_npm_starts}`);
    console.log(`📈 Efficiency Score: ${report.summary.efficiency_score}%`);
    
    if (report.recommendations.length > 0) {
      console.log('\n💡 OPTIMIZATION RECOMMENDATIONS:');
      report.recommendations.forEach((rec, i) => {
        console.log(`   ${i + 1}. ${rec}`);
      });
    }

    if (report.summary.efficiency_score < 80) {
      console.log('\n⚠️ LOW EFFICIENCY DETECTED - Review build patterns!');
    } else if (report.summary.efficiency_score === 100) {
      console.log('\n🎉 PERFECT EFFICIENCY - No redundant builds detected!');
    }

    console.log('');
  }
}

// Singleton instance for global use
export const buildMonitor = new BuildEfficiencyMonitor();