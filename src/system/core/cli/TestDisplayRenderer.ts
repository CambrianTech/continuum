/**
 * CLI Test Display Renderer
 * 
 * Universal display function that transforms structured TestSummary data
 * into human-readable CLI output with multiple format options.
 */

import { TestSummary, TestFailure, TestDisplayOptions, TestMetrics } from '../types/TestSummaryTypes';

export class TestDisplayRenderer {
  
  /**
   * Main display function - transforms TestSummary into CLI output
   */
  static display(summary: TestSummary, options: TestDisplayOptions = this.defaultOptions()): string {
    switch (options.format) {
      case 'json':
        return this.renderJSON(summary);
      case 'compact':
        return this.renderCompact(summary, options);
      case 'ai-friendly':
        return this.renderAIFriendly(summary, options);
      case 'human':
      default:
        return this.renderHuman(summary, options);
    }
  }

  /**
   * Human-readable format with colors and icons
   */
  static renderHuman(summary: TestSummary, options: TestDisplayOptions): string {
    const lines: string[] = [];
    const { colorOutput } = options;
    
    // Header
    const status = summary.machineReadable.status;
    const statusIcon = status === 'passed' ? '✅' : status === 'failed' ? '❌' : '⚠️';
    const statusColor = colorOutput ? (status === 'passed' ? '\x1b[32m' : status === 'failed' ? '\x1b[31m' : '\x1b[33m') : '';
    const resetColor = colorOutput ? '\x1b[0m' : '';
    
    lines.push(`${statusIcon} ${statusColor}${summary.testSuite.toUpperCase()} TEST RESULTS${resetColor}`);
    lines.push(`📊 Tests: ${summary.passedTests}/${summary.totalTests} passed (${summary.duration}ms)`);
    
    if (summary.failedTests > 0) {
      lines.push('');
      lines.push('📋 Failed Tests:');
      
      // Show failures with rich context
      summary.failures.slice(0, options.maxFailureDetail).forEach((failure, index) => {
        const icons = this.getFailureIcons(failure);
        const errorDisplay = options.showStackTraces ? failure.stackTrace || failure.error : failure.error;
        
        lines.push(`${colorOutput ? '\x1b[91m' : ''}❌ Test ${index + 1}: ${failure.name}${resetColor}`);
        lines.push(`   ${icons.category} Category: ${failure.category.replace('-', ' ')}`);
        lines.push(`   ${icons.testType} Type: ${failure.testType} | ${icons.environment} Environment: ${failure.environment}`);
        lines.push(`   📝 Error: ${colorOutput ? '\x1b[93m' : ''}${errorDisplay}${resetColor}`);
        
        if (failure.suggestedFix) {
          lines.push(`   💡 Fix: ${colorOutput ? '\x1b[92m' : ''}${failure.suggestedFix}${resetColor}`);
        }
        
        if (failure.logPath) {
          lines.push(`   📁 Logs: ${colorOutput ? '\x1b[94m' : ''}${failure.logPath}${resetColor}`);
        }
        
        lines.push(''); // Add spacing between failures
      });
      
      // Compact summary
      lines.push('');
      lines.push('🔍 Quick Summary:');
      const typesSummary = Object.entries(summary.categories.testTypes)
        .map(([type, count]) => `${type}:${count}`).join(' | ');
      const envsSummary = Object.entries(summary.categories.environments)
        .map(([env, count]) => `${env}:${count}`).join(' | ');
      lines.push(`   ${typesSummary} | ${envsSummary}`);
      
      // Guidance section
      if (options.showGuidance && summary.guidance.actionItems.length > 0) {
        lines.push('');
        lines.push('💡 What to do next:');
        summary.guidance.actionItems.forEach(item => {
          lines.push(`   ${item}`);
        });
        
        if (summary.guidance.debugCommands.length > 0) {
          lines.push('');
          lines.push('🔍 Debug commands:');
          summary.guidance.debugCommands.slice(0, 3).forEach(cmd => {
            lines.push(`   ${colorOutput ? '\x1b[36m' : ''}${cmd}${resetColor}`);
          });
        }
      }
    }
    
    return lines.join('\n');
  }

  /**
   * Compact one-line format for CI/automation
   */
  static renderCompact(summary: TestSummary, options: TestDisplayOptions): string {
    const status = summary.machineReadable.status.toUpperCase();
    const ratio = `${summary.passedTests}/${summary.totalTests}`;
    const duration = `${summary.duration}ms`;
    const critical = summary.machineReadable.criticalFailures ? ' CRITICAL' : '';
    
    if (summary.failedTests === 0) {
      return `✅ ${status} ${ratio} ${duration}`;
    }
    
    const topCategory = Object.entries(summary.categories.rootCauses)
      .sort(([,a], [,b]) => b - a)[0]?.[0] || 'unknown';
    
    return `❌ ${status} ${ratio} ${duration} [${topCategory}${critical}]`;
  }

  /**
   * AI-friendly structured format with actionable intelligence
   */
  static renderAIFriendly(summary: TestSummary, options: TestDisplayOptions): string {
    const aiData = {
      status: summary.machineReadable.status,
      metrics: this.calculateMetrics(summary),
      actionable: {
        autoFixable: summary.guidance.autoFixable,
        aiActionable: summary.machineReadable.aiActionable,
        suggestedActions: summary.guidance.actionItems,
        debugPaths: summary.guidance.debugCommands
      },
      failures: summary.failures.map(f => ({
        name: f.name,
        category: f.category,
        environment: f.environment,
        severity: f.severity,
        fixable: !!f.suggestedFix,
        actionRequired: f.severity === 'critical'
      })),
      summary: {
        canProceed: summary.machineReadable.canProceed,
        blocksDeployment: summary.machineReadable.blocksDeployment,
        requiresAttention: summary.failedTests > 0
      }
    };
    
    return JSON.stringify(aiData, null, 2);
  }

  /**
   * Raw JSON format for machine consumption
   */
  static renderJSON(summary: TestSummary): string {
    return JSON.stringify(summary, null, 2);
  }

  /**
   * Get contextual icons for failure display
   */
  private static getFailureIcons(failure: TestFailure) {
    const categoryIcons = {
      'database': '🗄️',
      'event-system': '📡',
      'module-resolution': '📦',
      'network': '🌐',
      'timeout': '⏰',
      'unknown': '❓'
    };
    
    const typeIcons = {
      'unit': '🧪',
      'integration': '🔗',
      'e2e': '🎯'
    };
    
    const envIcons = {
      'browser': '🌐',
      'server': '🖥️',
      'cross-context': '↔️'
    };
    
    return {
      category: categoryIcons[failure.category] || '❓',
      testType: typeIcons[failure.testType] || '🧪',
      environment: envIcons[failure.environment] || '🖥️'
    };
  }

  /**
   * Calculate additional metrics for display
   */
  private static calculateMetrics(summary: TestSummary): TestMetrics {
    return {
      passRate: summary.totalTests > 0 ? (summary.passedTests / summary.totalTests) * 100 : 0,
      environmentHealth: summary.categories.environments,
      categoryDistribution: summary.categories.rootCauses,
      trends: {
        improving: false, // Would need historical data
        regression: false, // Would need historical data
        newFailures: summary.failures.filter(f => f.severity === 'critical').map(f => f.name)
      }
    };
  }

  /**
   * Default display options
   */
  private static defaultOptions(): TestDisplayOptions {
    return {
      format: 'human',
      showStackTraces: false,
      showGuidance: true,
      maxFailureDetail: 10,
      colorOutput: true
    };
  }
}

// Export convenience functions
export const displayTestResults = (summary: TestSummary, format: TestDisplayOptions['format'] = 'human') => {
  return TestDisplayRenderer.display(summary, { ...TestDisplayRenderer['defaultOptions'](), format });
};

export const displayCompactResults = (summary: TestSummary) => {
  return TestDisplayRenderer.display(summary, { 
    ...TestDisplayRenderer['defaultOptions'](), 
    format: 'compact',
    showGuidance: false,
    maxFailureDetail: 3
  });
};

export const displayAIResults = (summary: TestSummary) => {
  return TestDisplayRenderer.display(summary, {
    ...TestDisplayRenderer['defaultOptions'](),
    format: 'ai-friendly',
    showStackTraces: true,
    showGuidance: true
  });
};