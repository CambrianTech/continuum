#!/usr/bin/env tsx
/**
 * Incremental Compliance Tracker - Development-friendly module compliance
 * 
 * Designed for slow, methodical development:
 * - Shows progress over time
 * - Suggests next logical fixes
 * - Tracks improvements incrementally
 * - Provides small, focused tasks
 */

import { IntelligentModularTestRunner } from './IntelligentModularTestRunner';
import * as fs from 'fs';
import * as path from 'path';

interface ComplianceHistory {
  timestamp: Date;
  totalModules: number;
  compliantModules: number;
  complianceRate: number;
  moduleTypes: {
    [key: string]: {
      compliant: number;
      total: number;
      rate: number;
    };
  };
}

interface FixSuggestion {
  module: string;
  type: 'widget' | 'daemon' | 'command' | 'integration';
  priority: 'high' | 'medium' | 'low';
  effort: 'quick' | 'moderate' | 'complex';
  description: string;
  steps: string[];
}

class IncrementalComplianceTracker {
  private runner: IntelligentModularTestRunner;
  private historyFile: string;

  constructor() {
    this.runner = new IntelligentModularTestRunner();
    this.historyFile = path.join(process.cwd(), '.continuum', 'compliance-history.json');
    this.ensureHistoryDirectory();
  }

  private ensureHistoryDirectory(): void {
    const dir = path.dirname(this.historyFile);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
  }

  /**
   * Run incremental compliance check - development friendly
   */
  async runIncrementalCheck(options: {
    showSuggestions?: boolean;
    trackHistory?: boolean;
    focusType?: 'widget' | 'daemon' | 'command' | 'integration';
  } = {}): Promise<void> {
    console.log('🔄 INCREMENTAL COMPLIANCE TRACKER');
    console.log('==================================');
    console.log('💡 Development-friendly modular architecture progress');
    console.log('');

    // Get current state
    const moduleTypes: ('daemon' | 'widget' | 'command' | 'integration')[] = 
      options.focusType ? [options.focusType] : ['daemon', 'widget', 'command', 'integration'];

    let totalModules = 0;
    let totalCompliant = 0;
    const typeResults: { [key: string]: { compliant: number; total: number; rate: number } } = {};

    console.log('📊 Current Compliance Status:');
    console.log('------------------------------');

    for (const type of moduleTypes) {
      try {
        // Run silently to avoid spam
        const originalLog = console.log;
        console.log = () => {}; // Suppress output temporarily
        
        const results = await this.runner.runModuleTests(type);
        
        console.log = originalLog; // Restore logging
        
        const compliant = results.summary.compliantCount;
        const total = results.summary.totalModules;
        const rate = results.summary.complianceRate;

        typeResults[type] = { compliant, total, rate };
        totalModules += total;
        totalCompliant += compliant;

        const statusIcon = rate >= 90 ? '✅' : rate >= 70 ? '⚠️' : '❌';
        const trend = this.getComplianceTrend(type, rate);
        
        console.log(`${statusIcon} ${type.toUpperCase()}: ${rate.toFixed(1)}% (${compliant}/${total}) ${trend}`);
        
        if (rate < 100 && options.showSuggestions) {
          const suggestions = this.generateFixSuggestions(results.nonCompliantModules, type as any);
          if (suggestions.length > 0) {
            const quickFix = suggestions.find(s => s.effort === 'quick');
            if (quickFix) {
              console.log(`   💡 Quick fix: ${quickFix.description}`);
            }
          }
        }
      } catch (error) {
        console.error(`❌ Failed to check ${type} modules:`, error);
      }
    }

    const overallRate = totalModules > 0 ? (totalCompliant / totalModules) * 100 : 0;
    
    console.log('');
    console.log('📈 Overall Progress:');
    console.log(`   Total Modules: ${totalModules}`);
    console.log(`   Compliant: ${totalCompliant} (${overallRate.toFixed(1)}%)`);
    
    if (options.trackHistory) {
      this.recordProgress({
        timestamp: new Date(),
        totalModules,
        compliantModules: totalCompliant,
        complianceRate: overallRate,
        moduleTypes: typeResults
      });
      
      this.showProgressHistory();
    }

    if (options.showSuggestions && !options.focusType) {
      console.log('');
      this.showNextSteps();
    }
  }

  /**
   * Generate actionable fix suggestions for non-compliant modules
   */
  private generateFixSuggestions(nonCompliantModules: any[], type: 'widget' | 'daemon' | 'command' | 'integration'): FixSuggestion[] {
    const suggestions: FixSuggestion[] = [];

    for (const module of nonCompliantModules.slice(0, 3)) { // Top 3 suggestions
      if (!module.hasPackageJson) {
        suggestions.push({
          module: module.name,
          type,
          priority: 'high',
          effort: 'quick',
          description: `Add package.json to ${module.name}`,
          steps: [
            'Create package.json with name, version, main fields',
            `Add continuum.type: "${type}" field`,
            'Define exports if needed'
          ]
        });
      } else if (!module.hasMainFile) {
        suggestions.push({
          module: module.name,
          type,
          priority: 'medium',
          effort: 'moderate',
          description: `Add main implementation file to ${module.name}`,
          steps: [
            `Create ${module.name}${type === 'widget' ? 'Widget' : type === 'daemon' ? 'Daemon' : 'Command'}.ts`,
            'Implement required interfaces',
            'Update package.json main field'
          ]
        });
      } else if (!module.hasTestDir) {
        suggestions.push({
          module: module.name,
          type,
          priority: 'low',
          effort: 'moderate',
          description: `Add test directory to ${module.name}`,
          steps: [
            'Create test/unit/ directory',
            'Add basic unit tests',
            'Consider integration tests'
          ]
        });
      }
    }

    return suggestions;
  }

  /**
   * Show suggested next steps for incremental improvement
   */
  private showNextSteps(): void {
    console.log('🎯 SUGGESTED NEXT STEPS (Small, Focused Commits):');
    console.log('================================================');
    console.log('');
    
    const suggestions = [
      {
        title: '1. Quick Wins - Missing package.json files',
        description: 'Add package.json files to modules missing them',
        benefit: 'Makes modules discoverable',
        effort: '5-10 minutes per module'
      },
      {
        title: '2. Fix Widget compliance issues',
        description: 'ActiveProjects, SavedPersonas, UserSelector need main files',
        benefit: 'Improves widget compliance rate',
        effort: '15-30 minutes per widget'
      },
      {
        title: '3. Add continuum.type fields',
        description: 'Add missing continuum.type fields to package.json files',
        benefit: 'Improves module classification',
        effort: '2-5 minutes per module'
      },
      {
        title: '4. Whitelist intentionally non-compliant modules',
        description: 'Update compliance whitelist for modules that should stay non-compliant',
        benefit: 'Reduces noise in compliance reports',
        effort: '10-15 minutes'
      }
    ];

    for (const suggestion of suggestions) {
      console.log(`${suggestion.title}`);
      console.log(`   📝 ${suggestion.description}`);
      console.log(`   💪 Benefit: ${suggestion.benefit}`);
      console.log(`   ⏱️ Effort: ${suggestion.effort}`);
      console.log('');
    }

    console.log('💡 Pro tip: Make small commits after each module fix');
    console.log('   This keeps the git hook happy and tracks progress incrementally!');
  }

  /**
   * Get compliance trend for a module type
   */
  private getComplianceTrend(type: string, currentRate: number): string {
    try {
      const history = this.loadHistory();
      if (history.length < 2) return '';
      
      const lastEntry = history[history.length - 1];
      const lastRate = lastEntry.moduleTypes[type]?.rate || 0;
      
      if (currentRate > lastRate) return '📈';
      if (currentRate < lastRate) return '📉';
      return '→';
    } catch {
      return '';
    }
  }

  /**
   * Record compliance progress to history
   */
  private recordProgress(entry: ComplianceHistory): void {
    try {
      const history = this.loadHistory();
      history.push(entry);
      
      // Keep last 10 entries
      if (history.length > 10) {
        history.splice(0, history.length - 10);
      }
      
      fs.writeFileSync(this.historyFile, JSON.stringify(history, null, 2));
    } catch (error) {
      console.warn('⚠️ Failed to record compliance history:', error);
    }
  }

  /**
   * Load compliance history
   */
  private loadHistory(): ComplianceHistory[] {
    try {
      if (fs.existsSync(this.historyFile)) {
        return JSON.parse(fs.readFileSync(this.historyFile, 'utf8'));
      }
    } catch (error) {
      console.warn('⚠️ Failed to load compliance history:', error);
    }
    return [];
  }

  /**
   * Show progress over time
   */
  private showProgressHistory(): void {
    const history = this.loadHistory();
    if (history.length < 2) return;

    console.log('');
    console.log('📊 COMPLIANCE PROGRESS HISTORY:');
    console.log('--------------------------------');
    
    const recent = history.slice(-5); // Last 5 entries
    for (const entry of recent) {
      const date = new Date(entry.timestamp).toLocaleDateString();
      const time = new Date(entry.timestamp).toLocaleTimeString();
      console.log(`📅 ${date} ${time}: ${entry.complianceRate.toFixed(1)}% (${entry.compliantModules}/${entry.totalModules})`);
    }

    // Show trend
    const first = recent[0];
    const last = recent[recent.length - 1];
    const change = last.complianceRate - first.complianceRate;
    const moduleChange = last.compliantModules - first.compliantModules;
    
    if (change > 0) {
      console.log(`📈 Improvement: +${change.toFixed(1)}% (+${moduleChange} modules)`);
    } else if (change < 0) {
      console.log(`📉 Regression: ${change.toFixed(1)}% (${moduleChange} modules)`);
    } else {
      console.log(`→ Stable: No change in compliance`);
    }
  }
}

// Main execution
if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  const options = {
    showSuggestions: !args.includes('--no-suggestions'),
    trackHistory: args.includes('--track-history'),
    focusType: args.find(arg => ['widget', 'daemon', 'command', 'integration'].includes(arg)) as any
  };

  const tracker = new IncrementalComplianceTracker();
  tracker.runIncrementalCheck(options).catch(error => {
    console.error('❌ Incremental compliance check failed:', error);
    process.exit(1);
  });
}

export { IncrementalComplianceTracker };