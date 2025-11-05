#!/usr/bin/env tsx
/**
 * ESLint Graduation Tracker - Progressive Quality Enforcement
 * 
 * Tracks directories as they graduate from "dirty" to "clean" ESLint status.
 * Once graduated, directories must maintain PERFECT code quality forever.
 */

import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

interface ESLintResult {
  directory: string;
  status: 'graduated' | 'candidate' | 'dirty';
  issueCount: number;
  issues: string[];
  canGraduate: boolean;
}

interface ESLintGraduationConfig {
  graduated: string[];
  candidates: string[];
  dirty: string[];
  graduationThreshold: number; // Max issues allowed for graduation
}

class ESLintGraduationTracker {
  private config: ESLintGraduationConfig;
  private rootDir: string;

  constructor() {
    this.rootDir = process.cwd();
    
    // Current configuration - sync with .husky/pre-commit
    this.config = {
      graduated: [
        'src/daemons/base',                // Core daemon infrastructure ✅
        'src/daemons/daemon-manager',      // Master daemon controller ✅
        'src/daemons/mesh-coordinator',    // Mesh coordination daemon ✅
        'src/commands/core/base-command'   // BaseCommand infrastructure ✅
      ],
      candidates: [
        'src/daemons/static-file',         // 8 issues - close to graduation
        'src/daemons/mesh',               // 12 issues - needs work
        'src/daemons/renderer'            // 13 issues - needs work
      ],
      dirty: [
        'src/test/integration',           // 50 issues (imports with .js extensions)
        'src/types',                      // 37 issues (any types)
        'src/commands',                   // Many 'any' types
        'src/daemons',                    // 605 issues total
        'src/integrations',               // After daemons are clean
        'src/ui'                          // After core is clean
      ],
      graduationThreshold: 0 // Zero tolerance for graduation
    };
  }

  /**
   * Check graduation status of all directories
   */
  async checkGraduationStatus(): Promise<void> {
    console.log('🎓 ESLINT GRADUATION TRACKER');
    console.log('============================');
    console.log('💡 Progressive quality enforcement for ESLint compliance');
    console.log('');

    const allDirectories = [
      ...this.config.graduated,
      ...this.config.candidates,
      ...this.config.dirty
    ];

    let graduationCandidates = 0;
    let regressions = 0;

    for (const directory of allDirectories) {
      const result = await this.analyzeDirectory(directory);
      
      console.log(`🔍 ${directory}:`);
      console.log(`   Status: ${this.getStatusIcon(result.status)} ${result.status.toUpperCase()}`);
      console.log(`   Issues: ${result.issueCount}`);
      
      if (result.status === 'graduated' && result.issueCount > 0) {
        console.log(`   🚨 REGRESSION DETECTED! Graduated directory has ${result.issueCount} new issues`);
        regressions++;
      } else if (result.canGraduate && result.status !== 'graduated') {
        console.log(`   🎯 READY FOR GRADUATION! Zero issues found`);
        graduationCandidates++;
      } else if (result.status === 'candidate') {
        console.log(`   📈 Progress: ${this.config.graduationThreshold - result.issueCount} issues from graduation`);
      }
      
      if (result.issueCount > 0 && result.issueCount <= 5) {
        console.log(`   Top issues:`);
        result.issues.slice(0, 3).forEach(issue => {
          console.log(`     → ${issue}`);
        });
      }
      
      console.log('');
    }

    this.showGraduationSummary(graduationCandidates, regressions);
  }

  /**
   * Analyze a specific directory for ESLint compliance
   */
  private async analyzeDirectory(directory: string): Promise<ESLintResult> {
    const fullPath = path.join(this.rootDir, directory);
    
    if (!fs.existsSync(fullPath)) {
      return {
        directory,
        status: 'dirty',
        issueCount: 999,
        issues: ['Directory does not exist'],
        canGraduate: false
      };
    }

    try {
      // Run ESLint on the directory
      const command = `npx eslint "${directory}" --ext .ts,.js --ignore-pattern "**/*.test.ts" --ignore-pattern "**/test/**" --format json`;
      const output = execSync(command, { encoding: 'utf8', stdio: 'pipe' });
      
      // Parse ESLint JSON output
      const results = JSON.parse(output);
      const issues: string[] = [];
      let totalIssues = 0;

      results.forEach((result: any) => {
        result.messages.forEach((message: any) => {
          totalIssues++;
          issues.push(`${path.relative(this.rootDir, result.filePath)}:${message.line} - ${message.message}`);
        });
      });

      const currentStatus = this.getCurrentStatus(directory);
      const canGraduate = totalIssues <= this.config.graduationThreshold && currentStatus !== 'graduated';

      return {
        directory,
        status: currentStatus,
        issueCount: totalIssues,
        issues,
        canGraduate
      };

    } catch (error) {
      // ESLint failed - assume many issues
      return {
        directory,
        status: this.getCurrentStatus(directory),
        issueCount: 999,
        issues: ['ESLint execution failed'],
        canGraduate: false
      };
    }
  }

  /**
   * Get current status of a directory
   */
  private getCurrentStatus(directory: string): 'graduated' | 'candidate' | 'dirty' {
    if (this.config.graduated.includes(directory)) return 'graduated';
    if (this.config.candidates.includes(directory)) return 'candidate';
    return 'dirty';
  }

  /**
   * Get status icon for display
   */
  private getStatusIcon(status: string): string {
    switch (status) {
      case 'graduated': return '🎓';
      case 'candidate': return '📈';
      case 'dirty': return '🔧';
      default: return '❓';
    }
  }

  /**
   * Show graduation summary and next steps
   */
  private showGraduationSummary(graduationCandidates: number, regressions: number): void {
    console.log('🎯 GRADUATION SUMMARY');
    console.log('====================');
    
    if (regressions > 0) {
      console.log(`🚨 CRITICAL: ${regressions} graduated directories have regressions!`);
      console.log(`   Graduated directories must maintain PERFECT code quality`);
      console.log(`   Fix these issues immediately before any push`);
      console.log('');
    }
    
    if (graduationCandidates > 0) {
      console.log(`🎓 READY TO GRADUATE: ${graduationCandidates} directories!`);
      console.log(`   These directories have achieved zero ESLint issues`);
      console.log(`   Add them to GRADUATED_ESLINT_DIRS in .husky/pre-push`);
      console.log('');
    } else {
      console.log(`📚 In Progress: Continue fixing ESLint issues in candidate directories`);
      console.log('');
    }

    console.log('🎯 GRADUATION STRATEGY:');
    console.log('========================');
    console.log('1. 🔧 Fix issues in candidate directories (lowest issue count first)');
    console.log('2. 🎓 Graduate directories with zero issues to strict enforcement');
    console.log('3. 🚨 Never allow regressions in graduated directories');
    console.log('4. 📈 Gradually expand enforcement to all src/ directories');
    console.log('');
    console.log('💡 Goal: Eventually all src/ directories under strict enforcement');
  }

  /**
   * Suggest next graduation target
   */
  async suggestNextTarget(): Promise<void> {
    console.log('🎯 NEXT GRADUATION TARGET');
    console.log('=========================');
    
    // Find candidate with lowest issue count
    let bestCandidate: { dir: string; issues: number } | null = null;
    
    for (const directory of this.config.candidates) {
      const result = await this.analyzeDirectory(directory);
      if (!bestCandidate || result.issueCount < bestCandidate.issues) {
        bestCandidate = { dir: directory, issues: result.issueCount };
      }
    }

    if (bestCandidate) {
      console.log(`💡 RECOMMENDED TARGET: ${bestCandidate.dir}`);
      console.log(`   📊 Current issues: ${bestCandidate.issues}`);
      console.log(`   🎯 Issues to fix: ${bestCandidate.issues - this.config.graduationThreshold}`);
      console.log(`   ⏱️ Estimated effort: ${this.estimateEffort(bestCandidate.issues)}`);
      console.log('');
      console.log('   Next steps:');
      console.log(`   1. Run: npx eslint "${bestCandidate.dir}" --ext .ts,.js --fix`);
      console.log(`   2. Fix remaining issues manually`);
      console.log(`   3. Add to GRADUATED_ESLINT_DIRS in .husky/pre-push`);
      console.log(`   4. Remove from candidates, never allow regressions`);
    } else {
      console.log('🎉 No obvious candidates - focus on dirty directories');
    }
  }

  private estimateEffort(issues: number): string {
    if (issues <= 5) return '15-30 minutes';
    if (issues <= 15) return '1-2 hours';
    if (issues <= 50) return '4-8 hours';
    return '1+ days';
  }
}

// Main execution
if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  const tracker = new ESLintGraduationTracker();

  if (args.includes('--suggest')) {
    tracker.suggestNextTarget().catch(error => {
      console.error('❌ Target suggestion failed:', error);
      process.exit(1);
    });
  } else {
    tracker.checkGraduationStatus().catch(error => {
      console.error('❌ Graduation check failed:', error);
      process.exit(1);
    });
  }
}

export { ESLintGraduationTracker };