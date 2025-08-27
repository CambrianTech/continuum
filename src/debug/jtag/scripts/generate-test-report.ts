#!/usr/bin/env npx tsx

/**
 * Generate Test Report - Standalone Script
 * 
 * Can be called after npm test to generate comprehensive markdown report
 * Usage: npx tsx scripts/generate-test-report.ts [--output=path]
 */

import { generateTestResultsMarkdown } from './test-results/TestResultsMarkdownGenerator';
import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';
import path from 'path';

async function main() {
  const args = process.argv.slice(2);
  const outputArg = args.find(arg => arg.startsWith('--output='));
  const outputPath = outputArg 
    ? outputArg.split('=')[1] 
    : path.join(WorkingDirConfig.getContinuumPath(), 'jtag', 'test-results', 'latest-test-run.md');

  console.log('📊 Generating comprehensive test report...');
  console.log(`📁 Working directory: ${WorkingDirConfig.getWorkingDir()}`);
  
  try {
    // Generate basic test result (will be enhanced by session scanning)
    const basicResult = {
      totalTests: 0, // Will be calculated from session analysis
      passed: 0,     // Will be calculated from session analysis
      failed: 0,     // Will be calculated from session analysis
      successRate: 0, // Will be calculated
      systemInfo: {
        testStartTime: new Date(Date.now() - 5 * 60 * 1000).toISOString(), // 5 min ago estimate
        testEndTime: new Date().toISOString(),
        duration: '~5min (estimated)',
        workingDirectory: WorkingDirConfig.getWorkingDir(),
        sessionRootPath: '',
        totalSessionsCreated: 0
      }
    };

    const markdown = await generateTestResultsMarkdown(basicResult, outputPath);
    
    console.log('✅ Test report generated successfully!');
    console.log(`📄 Report location: ${outputPath}`);
    console.log('');
    console.log('🔍 Report includes:');
    console.log('   • Session-by-session analysis');
    console.log('   • Copy-paste debugging commands');
    console.log('   • Structured data for AI analysis');
    console.log('   • Human-friendly status summary');
    console.log('');
    console.log(`📖 View report: cat "${outputPath}"`);
    
  } catch (error) {
    console.error('❌ Failed to generate test report:', error);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  main().catch(console.error);
}