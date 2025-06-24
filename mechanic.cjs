#!/usr/bin/env node
/**
 * Mechanic - Universal diagnostic tool for Continuum
 * 
 * "God mode" accessibility - works even when Continuum is catastrophically broken
 * The equivalent of having physical access to the server hardware
 * 
 * Usage:
 *   node mechanic.cjs                    # Dashboard
 *   node mechanic.cjs --emergency        # Emergency diagnostics
 *   node mechanic.cjs --run git abc123   # Inspect specific run
 *   node mechanic.cjs --watch            # Watch for new runs
 *   node mechanic.cjs --report           # Generate report
 */

const RunDiagnostics = require('./src/components/RunDiagnostics.cjs');
const RunArtifact = require('./src/core/RunArtifact.cjs');
const fs = require('fs').promises;
const path = require('path');

async function main() {
    const args = process.argv.slice(2);
    const baseDir = '.continuum';
    
    console.log('🔧 CONTINUUM MECHANIC - Universal Diagnostic Tool');
    console.log('===================================================');
    
    const diagnostics = new RunDiagnostics(baseDir);
    
    try {
        if (args.includes('--emergency')) {
            console.log('🚨 ENTERING EMERGENCY MODE');
            const result = await diagnostics.emergencyDiagnostics();
            console.log('\n🚨 Emergency Status:', result.status);
            
        } else if (args.includes('--watch')) {
            console.log('👀 WATCHING FOR NEW RUNS (Ctrl+C to stop)');
            diagnostics.onUpdate((type, data) => {
                if (type === 'newRun') {
                    const status = data.status === 'PASS' ? '✅' : '❌';
                    console.log(`🆕 ${status} ${data.runType}/run_${data.runId} (${data.status})`);
                }
            });
            await diagnostics.startWatching();
            
        } else if (args.includes('--run')) {
            const runTypeIdx = args.indexOf('--run') + 1;
            const runIdIdx = runTypeIdx + 1;
            
            if (args[runTypeIdx] && args[runIdIdx]) {
                const runType = args[runTypeIdx];
                const runId = args[runIdIdx];
                
                console.log(`🔍 INSPECTING RUN: ${runType}/run_${runId}`);
                const details = await diagnostics.getRunDetails(runType, runId);
                
                if (details.error) {
                    console.log('❌ Error:', details.error);
                } else {
                    console.log('\nSTATUS:', details.status.status);
                    console.log('DURATION:', details.status.duration ? `${(details.status.duration / 1000).toFixed(1)}s` : 'N/A');
                    console.log('\nSUMMARY:');
                    console.log(details.keyContent.summary);
                    
                    console.log('\nARTIFACTS:');
                    for (const artifact of details.artifacts) {
                        console.log(`  📄 ${artifact.name} (${artifact.size} bytes)`);
                    }
                    
                    if (details.keyContent.errors !== 'No errors recorded') {
                        console.log('\n🚨 ERRORS:');
                        console.log(details.keyContent.errors);
                    }
                }
            } else {
                console.log('❌ Usage: --run <runType> <runId>');
            }
            
        } else if (args.includes('--report')) {
            console.log('📊 GENERATING DIAGNOSTIC REPORT');
            const report = await diagnostics.generateReport('text');
            console.log('\n' + report);
            
            // Also save to file
            const reportPath = path.join(baseDir, 'diagnostic-report.txt');
            await fs.writeFile(reportPath, report);
            console.log(`\n💾 Report saved: ${reportPath}`);
            
        } else {
            // Default: Dashboard
            console.log('📊 SYSTEM DASHBOARD');
            const dashboard = await diagnostics.getDashboard();
            
            if (dashboard.error) {
                console.log('❌ Dashboard Error:', dashboard.error);
                console.log('\n🚨 Trying emergency diagnostics...');
                await diagnostics.emergencyDiagnostics();
                return;
            }
            
            console.log('\n🏥 SYSTEM HEALTH:', dashboard.health.status);
            console.log('📈 Pass Rate:', dashboard.health.passRate + '%');
            console.log('🔢 Total Runs:', dashboard.summary.totalRuns);
            console.log('📂 Run Types:', dashboard.summary.runTypes.join(', '));
            
            console.log('\n📋 RECENT ACTIVITY:');
            for (const run of dashboard.summary.recentActivity.slice(0, 10)) {
                const status = run.status === 'PASS' ? '✅' : '❌';
                const duration = run.duration ? `${(run.duration / 1000).toFixed(1)}s` : 'N/A';
                console.log(`  ${status} ${run.runType}/run_${run.runId} (${duration})`);
            }
            
            if (dashboard.health.trends) {
                console.log('\n📈 TRENDS (Last 24h):');
                console.log(`  Runs: ${dashboard.health.trends.last24h}`);
                console.log(`  Peak Hour: ${dashboard.health.trends.peakHour}:00`);
                console.log(`  Avg Duration: ${(dashboard.health.trends.averageDuration / 1000).toFixed(1)}s`);
            }
            
            console.log('\n🔧 MECHANIC OPTIONS:');
            console.log('  node mechanic.cjs --emergency     # Emergency diagnostics');
            console.log('  node mechanic.cjs --watch         # Watch for new runs');
            console.log('  node mechanic.cjs --run git abc   # Inspect specific run');
            console.log('  node mechanic.cjs --report        # Generate full report');
        }
        
    } catch (error) {
        console.error('🚨 MECHANIC TOOL FAILURE:', error);
        console.log('\n🔥 LAST RESORT: Emergency file system check...');
        
        try {
            const exists = await fs.access(baseDir).then(() => true).catch(() => false);
            console.log(`📁 Base directory: ${exists ? '✅ EXISTS' : '❌ MISSING'}`);
            
            if (exists) {
                const files = await fs.readdir(baseDir);
                console.log(`📂 Contents: ${files.join(', ')}`);
            }
        } catch (lastResortError) {
            console.error('💀 COMPLETE SYSTEM FAILURE - Manual intervention required');
            process.exit(1);
        }
    }
}

// Handle graceful shutdown
process.on('SIGINT', () => {
    console.log('\n👋 Mechanic shutting down gracefully...');
    process.exit(0);
});

if (require.main === module) {
    main().catch(error => {
        console.error('💥 Unexpected error:', error);
        process.exit(1);
    });
}