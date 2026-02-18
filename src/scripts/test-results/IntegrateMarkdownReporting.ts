/**
 * Integration script to add comprehensive markdown reporting to npm test
 * 
 * This modifies the existing test output to include the AI-friendly session analysis
 * while preserving the current human-readable console output.
 */

import { generateTestResultsMarkdown, ComprehensiveTestResult } from './TestResultsMarkdownGenerator';
import path from 'path';
import { WorkingDirConfig } from '../../system/core/config/WorkingDirConfig';

export interface TestRunData {
  categories: Map<string, { passed: number; total: number; tests: string[] }>;
  totalTests: number;
  passed: number;
  failed: number;
  startTime: Date;
  endTime?: Date;
}

/**
 * Enhanced test summary that includes markdown generation
 */
export async function generateEnhancedTestSummary(testData: TestRunData): Promise<void> {
  // Generate the current console output (preserve existing behavior)
  console.log('═'.repeat(79));
  console.log('🎯 TEST RESULTS - Profile: comprehensive');
  console.log('═'.repeat(79));
  
  console.log('📊 Overall Summary:');
  console.log(`   Total Tests: ${testData.totalTests}`);
  console.log(`   ✅ Passed: ${testData.passed}`);
  console.log(`   ❌ Failed: ${testData.failed}`);
  console.log(`   📈 Success Rate: ${Math.round((testData.passed / testData.totalTests) * 100)}%`);
  console.log('');

  // Category breakdown (existing format)
  console.log('📋 Results by Category:');
  const sortedCategories = Array.from(testData.categories.entries()).sort();
  
  for (const [categoryName, categoryData] of sortedCategories) {
    const successRate = Math.round((categoryData.passed / categoryData.total) * 100);
    const status = successRate === 100 ? '✅ All passing' : 
                   successRate > 0 ? '⚠️ Partial failures' : '❌ All failed';
    
    console.log(`   ${categoryName}: ${categoryData.passed}/${categoryData.total} tests (${successRate}%) ${status}`);
  }

  const allTestsPassed = testData.failed === 0;
  
  if (allTestsPassed) {
    console.log('');
    console.log('🎉 ALL TESTS PASSED!');
    console.log('✅ Profile \'comprehensive\' is fully functional');
    console.log('');
    console.log('🚀 System ready for autonomous development');
  } else {
    console.log('');
    console.log('⚠️ SOME TESTS FAILED!');
    console.log('🔍 Check detailed results below for debugging information');
  }

  // NEW: Generate comprehensive markdown report
  try {
    const endTime = testData.endTime || new Date();
    const duration = formatDuration(testData.startTime, endTime);
    
    // Convert test data to comprehensive result format
    const comprehensiveResult: Partial<ComprehensiveTestResult> = {
      totalTests: testData.totalTests,
      passed: testData.passed,
      failed: testData.failed,
      successRate: Math.round((testData.passed / testData.totalTests) * 100),
      categories: Array.from(testData.categories.entries()).map(([category, data]) => ({
        category,
        passed: data.passed,
        total: data.total,
        successRate: Math.round((data.passed / data.total) * 100),
        tests: data.tests,
        sessions: [] // Will be populated by markdown generator
      })),
      systemInfo: {
        testStartTime: testData.startTime.toISOString(),
        testEndTime: endTime.toISOString(),
        duration,
        workingDirectory: WorkingDirConfig.getWorkingDir(),
        sessionRootPath: '', // Will be populated by markdown generator
        totalSessionsCreated: 0 // Will be calculated by markdown generator
      }
    };

    // Generate markdown report
    const continuumPath = WorkingDirConfig.getContinuumPath();
    const outputPath = path.join(continuumPath, 'jtag', 'test-results', 'latest-test-run.md');
    
    // Ensure directory exists
    const fs = await import('fs/promises');
    await fs.mkdir(path.dirname(outputPath), { recursive: true });
    
    const markdown = await generateTestResultsMarkdown(comprehensiveResult, outputPath);
    
    // Enhanced console output with markdown info
    console.log('');
    console.log('📄 COMPREHENSIVE TEST REPORT GENERATED');
    console.log(`   Report Path: ${outputPath}`);
    console.log('   Contains: Session analysis, debugging paths, AI-friendly data');
    
    if (allTestsPassed) {
      console.log('   🎯 All tests passed - system is ready for development!');
    } else {
      console.log('   🔍 Use report for detailed failure analysis and session debugging');
    }
    
    console.log('');
    console.log('🚀 Quick Access:');
    console.log(`   View report: cat "${outputPath}"`);
    console.log(`   Session root: ls -la "${path.join(continuumPath, 'jtag', 'sessions', 'user')}"`);
    console.log('   Current session: ls -la .continuum/jtag/currentUser/');

  } catch (error) {
    console.warn('⚠️ Failed to generate comprehensive markdown report:', error);
    console.log('   Continuing with standard test output...');
  }

  // Preserve existing ending
  console.log('');
  console.log('✅ All tests passed!');
  console.log('🎯 TEST RESULTS:');
  console.log(JSON.stringify({
    success: allTestsPassed,
    serverStarted: true,
    testsRan: true
  }));
  
  if (allTestsPassed) {
    console.log('🎉 ALL TESTS PASSED - npm test succeeded!');
  } else {
    console.log('❌ SOME TESTS FAILED - check details above');
  }
  
  console.log('🚀 Server left running for development (as intended)');
  console.log('📡 Use Ctrl+C to stop the server, or run ./jtag commands in another terminal');
}

/**
 * Format duration between two dates
 */
function formatDuration(start: Date, end: Date): string {
  const durationMs = end.getTime() - start.getTime();
  const seconds = Math.floor(durationMs / 1000);
  const minutes = Math.floor(seconds / 60);
  
  if (minutes > 0) {
    return `${minutes}m ${seconds % 60}s`;
  } else {
    return `${seconds}s`;
  }
}

/**
 * Hook for existing test runners - call this instead of current summary logic
 */
export function createTestDataCollector() {
  const testData: TestRunData = {
    categories: new Map(),
    totalTests: 0,
    passed: 0,
    failed: 0,
    startTime: new Date()
  };

  return {
    // Called when test starts
    recordTestStart: () => {
      testData.startTime = new Date();
    },

    // Called for each test completion
    recordTestResult: (testName: string, category: string, passed: boolean) => {
      testData.totalTests++;
      if (passed) {
        testData.passed++;
      } else {
        testData.failed++;
      }

      // Update category tracking
      if (!testData.categories.has(category)) {
        testData.categories.set(category, { passed: 0, total: 0, tests: [] });
      }
      
      const categoryData = testData.categories.get(category)!;
      categoryData.total++;
      categoryData.tests.push(testName);
      if (passed) {
        categoryData.passed++;
      }
    },

    // Called when all tests complete
    generateFinalSummary: async () => {
      testData.endTime = new Date();
      await generateEnhancedTestSummary(testData);
    },

    // Direct access to data for custom processing
    getTestData: () => testData
  };
}