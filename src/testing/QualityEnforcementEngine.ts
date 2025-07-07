#!/usr/bin/env tsx
/**
 * Quality Enforcement Engine - Per-Module Quality Standards
 * 
 * Reads quality standards from each module's package.json and enforces them.
 * Modules can declare their own graduation status and requirements.
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';
import { glob } from 'glob';

interface ModuleQualityConfig {
  continuum?: {
    quality?: {
      status: 'graduated' | 'candidate' | 'whitelisted';
      eslint?: {
        enforce: boolean;
        level: 'strict' | 'warn' | 'off';
      };
      typescript?: {
        noAny: boolean;
        strict: boolean;
      };
      tests?: {
        required: boolean;
        coverage?: number;
      };
      compliance?: {
        required: boolean;
        minimumScore?: number;
      };
    };
  };
}

interface QualityResult {
  module: string;
  path: string;
  status: 'graduated' | 'candidate' | 'whitelisted';
  passed: boolean;
  issues: string[];
  config: ModuleQualityConfig['continuum']['quality'];
}

class QualityEnforcementEngine {
  private rootDir: string;
  private enforcementMode: 'commit' | 'push';

  constructor(enforcementMode: 'commit' | 'push' = 'commit') {
    this.rootDir = process.cwd();
    this.enforcementMode = enforcementMode;
  }

  /**
   * Discover all modules with package.json files
   */
  async discoverModules(): Promise<string[]> {
    const packageJsonFiles = await glob('**/package.json', {
      cwd: this.rootDir,
      ignore: [
        'node_modules/**', 
        '**/node_modules/**', 
        '.continuum/**',
        'package.json', // Exclude root package.json
        'packages/*/package.json' // Exclude workspace packages for now
      ]
    });

    return packageJsonFiles
      .map(file => path.dirname(file))
      .filter(dir => dir !== '.' && dir !== ''); // Exclude root directory
  }

  /**
   * Load quality configuration for a module
   */
  loadModuleQualityConfig(modulePath: string): ModuleQualityConfig['continuum']['quality'] | null {
    const packageJsonPath = path.join(this.rootDir, modulePath, 'package.json');
    
    if (!fs.existsSync(packageJsonPath)) {
      return null;
    }

    try {
      const packageJson: ModuleQualityConfig = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
      return packageJson.continuum?.quality || null;
    } catch (error) {
      console.warn(`⚠️ Failed to load quality config for ${modulePath}:`, error);
      return null;
    }
  }

  /**
   * Enforce quality standards for all modules
   */
  async enforceQualityStandards(): Promise<{ passed: boolean; results: QualityResult[] }> {
    console.log('🎯 QUALITY ENFORCEMENT ENGINE');
    console.log('==============================');
    console.log(`📊 Mode: ${this.enforcementMode.toUpperCase()}`);
    console.log('🔍 Discovering modules with quality configurations...');
    console.log('');

    const modules = await this.discoverModules();
    const results: QualityResult[] = [];
    let overallPassed = true;

    for (const modulePath of modules) {
      const config = this.loadModuleQualityConfig(modulePath);
      
      if (!config) {
        // No quality config = default to whitelisted
        continue;
      }

      console.log(`🔍 Checking module: ${modulePath}`);
      console.log(`   Status: ${this.getStatusIcon(config.status)} ${config.status?.toUpperCase() || 'UNKNOWN'}`);

      const result = await this.checkModuleQuality(modulePath, config);
      results.push(result);

      if (!result.passed) {
        overallPassed = false;
        
        // Determine if this should block based on enforcement mode and graduation status
        const shouldBlock = this.shouldBlockCommitOrPush(config.status, result.issues.length);
        
        if (shouldBlock) {
          console.log(`   ❌ BLOCKED: Quality standards not met`);
        } else {
          console.log(`   ⚠️ WARNING: Quality issues found (not blocking ${this.enforcementMode})`);
        }
        
        result.issues.forEach(issue => {
          console.log(`      → ${issue}`);
        });
      } else {
        console.log(`   ✅ PASSED: All quality standards met`);
      }
      
      console.log('');
    }

    return { passed: overallPassed, results };
  }

  /**
   * Check quality standards for a specific module
   */
  private async checkModuleQuality(modulePath: string, config: ModuleQualityConfig['continuum']['quality']): Promise<QualityResult> {
    const issues: string[] = [];
    const fullPath = path.join(this.rootDir, modulePath);

    // ESLint enforcement
    if (config.eslint?.enforce) {
      try {
        const eslintLevel = config.eslint.level || 'warn';
        const command = `npx eslint "${modulePath}" --ext .ts,.js --ignore-pattern "**/*.test.ts" --ignore-pattern "**/test/**" --format json`;
        const output = execSync(command, { encoding: 'utf8', stdio: 'pipe' });
        const eslintResults = JSON.parse(output);
        
        let totalIssues = 0;
        eslintResults.forEach((result: any) => {
          totalIssues += result.messages.length;
        });

        if (totalIssues > 0) {
          if (eslintLevel === 'strict' || (eslintLevel === 'warn' && this.enforcementMode === 'push')) {
            issues.push(`ESLint: ${totalIssues} issues found (${eslintLevel} enforcement)`);
          }
        }
      } catch (error) {
        if (config.eslint.level === 'strict') {
          issues.push('ESLint: Execution failed (strict enforcement)');
        }
      }
    }

    // TypeScript 'any' type checking
    if (config.typescript?.noAny) {
      try {
        const anyTypeCount = this.countAnyTypes(fullPath);
        if (anyTypeCount > 0) {
          issues.push(`TypeScript: ${anyTypeCount} 'any' types found (strict type safety required)`);
        }
      } catch (error) {
        issues.push('TypeScript: Failed to check for any types');
      }
    }

    // Test requirements
    if (config.tests?.required) {
      const testDir = path.join(fullPath, 'test');
      if (!fs.existsSync(testDir)) {
        issues.push('Tests: Test directory required but not found');
      }
    }

    // Module compliance requirements
    if (config.compliance?.required) {
      // This would integrate with our existing module compliance system
      // For now, just check if it's a discoverable module
      const packageJsonPath = path.join(fullPath, 'package.json');
      if (!fs.existsSync(packageJsonPath)) {
        issues.push('Compliance: package.json required for module compliance');
      }
    }

    return {
      module: path.basename(modulePath),
      path: modulePath,
      status: config.status,
      passed: issues.length === 0,
      issues,
      config
    };
  }

  /**
   * Count 'any' types in a directory
   */
  private countAnyTypes(dirPath: string): number {
    try {
      const result = execSync(`grep -r "\\bany\\b" "${dirPath}" --include="*.ts" --exclude-dir=test --exclude-dir=node_modules | wc -l`, 
        { encoding: 'utf8' });
      return parseInt(result.trim()) || 0;
    } catch (error) {
      return 0;
    }
  }

  /**
   * Determine if commit/push should be blocked based on graduation status
   */
  private shouldBlockCommitOrPush(status: string, issueCount: number): boolean {
    if (issueCount === 0) return false; // No issues = never block

    switch (this.enforcementMode) {
      case 'commit':
        // Commits: Only block graduated modules with issues
        return status === 'graduated';
        
      case 'push':
        // Pushes: Block graduated modules and candidates with issues
        return status === 'graduated' || status === 'candidate';
        
      default:
        return false;
    }
  }

  /**
   * Get status icon for display
   */
  private getStatusIcon(status: string): string {
    switch (status) {
      case 'graduated': return '🎓';
      case 'candidate': return '📈';
      case 'whitelisted': return '📋';
      default: return '❓';
    }
  }

  /**
   * Show summary and graduation suggestions
   */
  showSummary(results: QualityResult[]): void {
    const graduated = results.filter(r => r.status === 'graduated');
    const candidates = results.filter(r => r.status === 'candidate');
    const whitelisted = results.filter(r => r.status === 'whitelisted');
    
    console.log('📊 QUALITY ENFORCEMENT SUMMARY');
    console.log('===============================');
    console.log(`🎓 Graduated modules: ${graduated.length} (${graduated.filter(r => r.passed).length} passing)`);
    console.log(`📈 Candidate modules: ${candidates.length} (${candidates.filter(r => r.passed).length} passing)`);
    console.log(`📋 Whitelisted modules: ${whitelisted.length} (${whitelisted.filter(r => r.passed).length} passing)`);
    console.log('');

    // Show graduation candidates
    const readyToGraduate = candidates.filter(r => r.passed);
    if (readyToGraduate.length > 0) {
      console.log('🎯 READY FOR GRADUATION:');
      readyToGraduate.forEach(r => {
        console.log(`   🎓 ${r.path} - Update status to "graduated" in package.json`);
      });
      console.log('');
    }

    // Show regressions in graduated modules
    const regressions = graduated.filter(r => !r.passed);
    if (regressions.length > 0) {
      console.log('🚨 GRADUATED MODULE REGRESSIONS:');
      regressions.forEach(r => {
        console.log(`   ❌ ${r.path} - Must maintain perfect quality!`);
      });
    }
  }
}

// Main execution
if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  const mode = args.includes('--push') ? 'push' : 'commit';
  const engine = new QualityEnforcementEngine(mode);

  engine.enforceQualityStandards().then(({ passed, results }) => {
    engine.showSummary(results);
    
    if (!passed && mode === 'push') {
      console.log('❌ PUSH BLOCKED: Quality standards not met for graduated/candidate modules');
      process.exit(1);
    } else if (!passed && mode === 'commit') {
      console.log('⚠️ Quality issues found - commit allowed for incremental development');
      console.log('💡 Fix issues before pushing to production');
    } else {
      console.log('✅ All quality standards met!');
    }
  }).catch(error => {
    console.error('❌ Quality enforcement failed:', error);
    process.exit(1);
  });
}

export { QualityEnforcementEngine };