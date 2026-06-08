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
import fs from 'fs/promises';

async function main() {
  const args = process.argv.slice(2);
  const outputArg = args.find(arg => arg.startsWith('--output='));
  const outputPath = outputArg 
    ? outputArg.split('=')[1] 
    : path.join(WorkingDirConfig.getContinuumPath(), 'jtag', 'test-results', 'latest-test-run.md');

  console.log('📊 Generating comprehensive test report...');
  console.log(`📁 Working directory: ${WorkingDirConfig.getWorkingDir()}`);
  
  try {
    // Parse actual test results from test_results.tmp file
    const testResultsPath = path.join(WorkingDirConfig.getContinuumPath(), 'tests', 'test_results.tmp');
    let basicResult = {
      totalTests: 0,
      passed: 0,
      failed: 0,
      successRate: 0,
      categories: [] as Array<{category: string, passed: number, total: number, successRate: number, tests: string[], sessions: any[]}>,
      systemInfo: {
        testStartTime: new Date(Date.now() - 5 * 60 * 1000).toISOString(), // 5 min ago estimate
        testEndTime: new Date().toISOString(),
        duration: '~5min (estimated)',
        workingDirectory: WorkingDirConfig.getWorkingDir(),
        sessionRootPath: '',
        totalSessionsCreated: 0
      }
    };

    // Parse test results file if it exists
    try {
      const testResultsContent = await fs.readFile(testResultsPath, 'utf-8');
      const lines = testResultsContent.trim().split('\n').filter(line => line.trim());
      
      // Count totals
      let totalTests = 0;
      let passed = 0;
      let failed = 0;
      const categoryStats = new Map<string, {passed: number, total: number, tests: string[]}>();
      
      for (const line of lines) {
        const parts = line.split('|');
        if (parts.length >= 3) {
          const category = parts[0].trim();
          const status = parts[1].trim();
          const testName = parts[2].trim();
          
          totalTests++;
          if (status === 'PASS') {
            passed++;
          } else if (status === 'FAIL') {
            failed++;
          }
          
          // Track category stats
          if (!categoryStats.has(category)) {
            categoryStats.set(category, {passed: 0, total: 0, tests: []});
          }
          const catStats = categoryStats.get(category)!;
          catStats.total++;
          catStats.tests.push(testName);
          if (status === 'PASS') {
            catStats.passed++;
          }
        }
      }
      
      // Build category results
      const categories = Array.from(categoryStats.entries()).map(([category, stats]) => ({
        category,
        passed: stats.passed,
        total: stats.total,
        successRate: stats.total > 0 ? Math.round((stats.passed / stats.total) * 100) : 0,
        tests: stats.tests,
        sessions: []
      }));
      
      const successRate = totalTests > 0 ? Math.round((passed / totalTests) * 100) : 0;
      
      basicResult = {
        ...basicResult,
        totalTests,
        passed,
        failed,
        successRate,
        categories
      };
      
      console.log(`📊 Parsed test results: ${totalTests} tests, ${passed} passed, ${failed} failed (${successRate}% success)`);
      
    } catch (parseError) {
      console.warn(`Warning: Could not parse test results from ${testResultsPath}, using defaults`);
    }

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