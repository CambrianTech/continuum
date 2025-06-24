/**
 * RunDiagnostics - Universal "mechanic's view" widget for all run types
 * 
 * The diagnostic component that works everywhere:
 * - Portal sessions (interactive debugging)
 * - Web console (animated run discovery) 
 * - Git hook runs (Saint Peter verification)
 * - Emergency mode (god mode accessibility)
 * - Persona debugging (agent introspection)
 * 
 * "Always working tools" - functions even when Continuum is catastrophically broken
 */

const RunArtifact = require('../core/RunArtifact.cjs');
const fs = require('fs').promises;
const path = require('path');

class RunDiagnostics {
    constructor(baseDir = '.continuum') {
        this.baseDir = baseDir;
        this.watchedRuns = new Map(); // Track runs for animation
        this.updateCallbacks = []; // UI update callbacks
    }

    /**
     * Get real-time run dashboard - all runs across all types
     */
    async getDashboard() {
        try {
            const allRuns = await RunArtifact.discoverAllRuns(this.baseDir);
            
            // Group by run type for organized display
            const dashboard = {
                summary: {
                    totalRuns: allRuns.length,
                    runTypes: [...new Set(allRuns.map(r => r.runType))],
                    recentActivity: allRuns.slice(0, 10)
                },
                byType: {},
                health: await this.getSystemHealth(allRuns)
            };
            
            // Organize runs by type
            for (const run of allRuns) {
                if (!dashboard.byType[run.runType]) {
                    dashboard.byType[run.runType] = [];
                }
                dashboard.byType[run.runType].push(run);
            }
            
            return dashboard;
            
        } catch (error) {
            console.error('⚠️ Dashboard generation failed:', error);
            return { error: error.message, summary: { totalRuns: 0 } };
        }
    }

    /**
     * System health analysis from run patterns
     */
    async getSystemHealth(runs) {
        const recent = runs.slice(0, 20); // Last 20 runs
        const passing = recent.filter(r => r.status === 'PASS').length;
        const failing = recent.filter(r => r.status === 'FAIL').length;
        const errors = recent.filter(r => r.status === 'ERROR').length;
        
        let healthStatus = 'HEALTHY';
        if (failing > passing) healthStatus = 'DEGRADED';
        if (errors > 5) healthStatus = 'CRITICAL';
        if (recent.length === 0) healthStatus = 'UNKNOWN';
        
        return {
            status: healthStatus,
            passRate: recent.length ? (passing / recent.length * 100).toFixed(1) : 0,
            totalRuns: recent.length,
            passing,
            failing,
            errors,
            trends: await this.analyzeTrends(runs)
        };
    }

    /**
     * Analyze trends and patterns in run data
     */
    async analyzeTrends(runs) {
        try {
            const last24h = runs.filter(r => {
                const runTime = new Date(r.timestamp);
                const now = new Date();
                return (now - runTime) < 24 * 60 * 60 * 1000;
            });
            
            const runsByHour = {};
            for (const run of last24h) {
                const hour = new Date(run.timestamp).getHours();
                if (!runsByHour[hour]) runsByHour[hour] = 0;
                runsByHour[hour]++;
            }
            
            return {
                last24h: last24h.length,
                peakHour: Object.keys(runsByHour).reduce((a, b) => 
                    runsByHour[a] > runsByHour[b] ? a : b, 0),
                averageDuration: runs.length ? 
                    runs.reduce((sum, r) => sum + (r.duration || 0), 0) / runs.length : 0
            };
            
        } catch (error) {
            return { error: error.message };
        }
    }

    /**
     * Get detailed run information with all artifacts
     */
    async getRunDetails(runType, runId) {
        try {
            const artifact = new RunArtifact(this.baseDir, runType, runId);
            const status = await artifact.getStatus();
            const artifacts = await artifact.listArtifacts();
            
            // Read key artifacts for quick display
            const keyContent = {};
            
            try {
                keyContent.summary = await artifact.readArtifact('summary.txt');
            } catch (e) { keyContent.summary = 'No summary available'; }
            
            try {
                const recentLogs = await artifact.readArtifact('client-logs.txt');
                keyContent.recentLogs = recentLogs.split('\n').slice(-20).join('\n');
            } catch (e) { keyContent.recentLogs = 'No recent logs'; }
            
            try {
                const errors = await artifact.readArtifact('error-logs.txt');
                keyContent.errors = errors.trim() || 'No errors recorded';
            } catch (e) { keyContent.errors = 'No error log available'; }
            
            return {
                status,
                artifacts,
                keyContent,
                path: artifact.runDir
            };
            
        } catch (error) {
            console.error('⚠️ Failed to get run details:', error);
            return { error: error.message };
        }
    }

    /**
     * Watch for new runs and trigger animations/notifications
     */
    async startWatching() {
        setInterval(async () => {
            try {
                const currentRuns = await RunArtifact.discoverAllRuns(this.baseDir);
                
                // Check for new runs
                for (const run of currentRuns) {
                    const runKey = `${run.runType}_${run.runId}`;
                    
                    if (!this.watchedRuns.has(runKey)) {
                        this.watchedRuns.set(runKey, run);
                        
                        // Trigger animation/notification for new run
                        this.notifyNewRun(run);
                    }
                }
                
                // Clean up old runs from watch list (keep last 100)
                if (this.watchedRuns.size > 100) {
                    const oldKeys = Array.from(this.watchedRuns.keys()).slice(0, -100);
                    oldKeys.forEach(key => this.watchedRuns.delete(key));
                }
                
            } catch (error) {
                console.error('⚠️ Watch error:', error);
            }
        }, 2000); // Check every 2 seconds
    }

    /**
     * Notify about new run (for UI animation)
     */
    notifyNewRun(run) {
        console.log(`🆕 New run detected: ${run.runType}/run_${run.runId} (${run.status})`);
        
        // Trigger all registered callbacks
        for (const callback of this.updateCallbacks) {
            try {
                callback('newRun', run);
            } catch (error) {
                console.error('⚠️ Update callback failed:', error);
            }
        }
    }

    /**
     * Register callback for UI updates/animations
     */
    onUpdate(callback) {
        this.updateCallbacks.push(callback);
    }

    /**
     * Emergency diagnostic mode - works even when everything is broken
     */
    async emergencyDiagnostics() {
        console.log('🚨 EMERGENCY DIAGNOSTICS MODE');
        console.log('=====================================');
        
        try {
            // Basic file system check
            const exists = await fs.access(this.baseDir).then(() => true).catch(() => false);
            console.log(`📁 Base directory (${this.baseDir}): ${exists ? '✅ EXISTS' : '❌ MISSING'}`);
            
            if (!exists) {
                console.log('🚨 CRITICAL: Base directory missing - system may be corrupted');
                return { status: 'CRITICAL', error: 'Base directory missing' };
            }
            
            // Get basic run counts
            const runs = await RunArtifact.discoverAllRuns(this.baseDir);
            console.log(`🔍 Total runs found: ${runs.length}`);
            
            // Show recent runs
            const recent = runs.slice(0, 5);
            console.log('\n📋 RECENT RUNS:');
            for (const run of recent) {
                const status = run.status === 'PASS' ? '✅' : '❌';
                console.log(`   ${status} ${run.runType}/run_${run.runId} (${run.status})`);
            }
            
            // Check for git hook functionality
            const gitRuns = runs.filter(r => r.runType === 'verification');
            console.log(`\n🔒 Git hook runs: ${gitRuns.length}`);
            if (gitRuns.length > 0) {
                const lastGit = gitRuns[0];
                console.log(`   Last: ${lastGit.status} at ${lastGit.timestamp}`);
            }
            
            return {
                status: 'OPERATIONAL',
                totalRuns: runs.length,
                recentRuns: recent,
                gitHookStatus: gitRuns.length > 0 ? 'WORKING' : 'NO_RECENT_ACTIVITY'
            };
            
        } catch (error) {
            console.error('🚨 EMERGENCY DIAGNOSTIC FAILED:', error);
            return { status: 'CRITICAL', error: error.message };
        }
    }

    /**
     * Generate diagnostic report for external tools
     */
    async generateReport(format = 'text') {
        const dashboard = await this.getDashboard();
        const timestamp = new Date().toISOString();
        
        if (format === 'json') {
            return JSON.stringify(dashboard, null, 2);
        }
        
        // Text format report
        let report = `Continuum Diagnostic Report
Generated: ${timestamp}

SYSTEM HEALTH: ${dashboard.health.status}
=======================================
Total Runs: ${dashboard.summary.totalRuns}
Pass Rate: ${dashboard.health.passRate}%
Run Types: ${dashboard.summary.runTypes.join(', ')}

RECENT ACTIVITY:
================
`;
        
        for (const run of dashboard.summary.recentActivity) {
            const status = run.status === 'PASS' ? '✅' : '❌';
            report += `${status} ${run.runType}/run_${run.runId} (${run.status})\n`;
        }
        
        report += `
TRENDS (Last 24h):
==================
Runs: ${dashboard.health.trends.last24h}
Peak Hour: ${dashboard.health.trends.peakHour}:00
Avg Duration: ${(dashboard.health.trends.averageDuration / 1000).toFixed(1)}s
`;
        
        return report;
    }

    /**
     * Static helper: Quick system check from anywhere
     */
    static async quickCheck(baseDir = '.continuum') {
        const diagnostics = new RunDiagnostics(baseDir);
        return await diagnostics.emergencyDiagnostics();
    }
}

module.exports = RunDiagnostics;