#!/usr/bin/env tsx
/**
 * Display Format Demo Test
 * 
 * Demonstrates the new structured TestSummary display formats
 * Shows how different output formats work for AI, CLI, and UI consumption
 */

import { TestDisplayRenderer } from '../system/core/cli/TestDisplayRenderer';
import type { TestSummary, TestFailure } from '../system/core/types/TestSummaryTypes';

/**
 * Create a realistic test summary with various failure types
 */
function createDemoTestSummary(): TestSummary {
  const failures: TestFailure[] = [
    {
      name: 'databaseRoomCreation',
      error: 'Cannot resolve module \'./daemons/chat-daemon/data/ChatDataService\'',
      category: 'module-resolution',
      testType: 'integration',
      environment: 'cross-context',
      severity: 'major',
      logPath: 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log',
      suggestedFix: 'Fix SQLite module imports in browser context'
    },
    {
      name: 'eventSystemIntegration', 
      error: 'EventManager is not defined in WebSocket execution context',
      category: 'event-system',
      testType: 'integration',
      environment: 'cross-context', 
      severity: 'major',
      logPath: 'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log',
      suggestedFix: 'Fix event system module resolution'
    },
    {
      name: 'networkConnectionTimeout',
      error: 'Connection timeout after 5000ms',
      category: 'network',
      testType: 'integration',
      environment: 'server',
      severity: 'minor',
      logPath: 'examples/test-bench/.continuum/jtag/system/logs/server.log',
      suggestedFix: 'Increase timeout or check network connectivity'
    }
  ];

  const summary: TestSummary = {
    totalTests: 8,
    passedTests: 5,
    failedTests: 3,
    duration: 12450,
    timestamp: new Date().toISOString(),
    testSuite: 'chat-daemon-integration',
    failures,
    categories: {
      testTypes: { integration: 3 },
      environments: { 'cross-context': 2, server: 1 },
      rootCauses: { 'module-resolution': 1, 'event-system': 1, network: 1 },
      severity: { major: 2, minor: 1 }
    },
    guidance: {
      actionItems: [
        '📦 Module issues: Check import paths in WebSocket execution context',
        '↔️ Cross-context failures: Browser trying to load server modules',
        '🌐 Network timeouts: Review connection stability'
      ],
      debugCommands: [
        'tail -f examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log',
        'grep "AUTOMATED CHAT TEST" examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log',
        'netstat -an | grep 9001'
      ],
      logPaths: [
        'examples/test-bench/.continuum/jtag/currentUser/logs/browser-console-log.log',
        'examples/test-bench/.continuum/jtag/system/logs/server.log'
      ],
      autoFixable: true
    },
    machineReadable: {
      status: 'failed',
      criticalFailures: false,
      canProceed: true,
      blocksDeployment: false,
      aiActionable: true
    }
  };

  return summary;
}

/**
 * Demo all display formats
 */
function demonstrateFormats() {
  console.log('🎯 ============= DISPLAY FORMAT DEMONSTRATION =============');
  console.log('');
  
  const testSummary = createDemoTestSummary();
  
  // 1. Human-friendly format (default CLI output)
  console.log('👤 HUMAN FORMAT (Default CLI):');
  console.log('━'.repeat(60));
  console.log(TestDisplayRenderer.display(testSummary, {
    format: 'human',
    showStackTraces: false,
    showGuidance: true,
    maxFailureDetail: 10,
    colorOutput: true
  }));
  
  console.log('\n\n');
  
  // 2. Compact format (CI/automation)
  console.log('🤖 COMPACT FORMAT (CI/Automation):');
  console.log('━'.repeat(60));
  console.log(TestDisplayRenderer.display(testSummary, {
    format: 'compact',
    showStackTraces: false,
    showGuidance: false,
    maxFailureDetail: 3,
    colorOutput: false
  }));
  
  console.log('\n\n');
  
  // 3. AI-friendly format (structured intelligence)
  console.log('🧠 AI-FRIENDLY FORMAT (Structured Intelligence):');
  console.log('━'.repeat(60));
  const aiOutput = TestDisplayRenderer.display(testSummary, {
    format: 'ai-friendly',
    showStackTraces: false,
    showGuidance: true,
    maxFailureDetail: 5,
    colorOutput: false
  });
  console.log(aiOutput);
  
  console.log('\n\n');
  
  // 4. Raw JSON format (machine consumption)
  console.log('🔧 RAW JSON FORMAT (Machine Consumption):');
  console.log('━'.repeat(60));
  console.log('(Truncated for display - first 10 lines):');
  const jsonOutput = TestDisplayRenderer.display(testSummary, {
    format: 'json',
    showStackTraces: false,
    showGuidance: false,
    maxFailureDetail: 10,
    colorOutput: false
  });
  console.log(jsonOutput.split('\n').slice(0, 15).join('\n'));
  console.log('  ... (truncated)');
  
  console.log('\n\n');
  
  // Usage examples
  console.log('📋 INTEGRATION EXAMPLES:');
  console.log('━'.repeat(60));
  console.log('// CLI Integration:');
  console.log('const output = TestDisplayRenderer.display(summary, { format: "human" });');
  console.log('console.log(output);');
  console.log('');
  console.log('// CI Integration:'); 
  console.log('const status = TestDisplayRenderer.display(summary, { format: "compact" });');
  console.log('slackNotify(status); // ❌ FAILED 5/8 12450ms [module-resolution]');
  console.log('');
  console.log('// AI Agent Integration:');
  console.log('const aiData = JSON.parse(TestDisplayRenderer.display(summary, { format: "ai-friendly" }));');
  console.log('if (aiData.actionable.autoFixable) { await autoFixModules(); }');
  console.log('');
  console.log('// Dashboard API Integration:');
  console.log('const apiData = JSON.parse(TestDisplayRenderer.display(summary, { format: "json" }));');
  console.log('updateTestDashboard(apiData.failures);');
  
  console.log('\n');
  console.log('✅ Display format demonstration complete!');
  console.log('💡 This shows how the same TestSummary data can be consumed by:');
  console.log('   👤 Humans (rich CLI output with colors and guidance)');
  console.log('   🤖 CI/CD (compact one-line status for notifications)');
  console.log('   🧠 AI Agents (structured intelligence for autonomous fixes)');
  console.log('   🔧 APIs (raw JSON for dashboards and integrations)');
}

// Run the demonstration
demonstrateFormats();