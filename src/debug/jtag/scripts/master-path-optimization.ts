#!/usr/bin/env npx tsx

/**
 * Master Path Optimization Script
 * 
 * Complete end-to-end path management system that:
 * 1. Generates unified configuration from directory structure
 * 2. Updates all config files (tsconfig, package.json, etc.)
 * 3. Automatically replaces relative paths with clean aliases
 * 4. Provides comprehensive reporting and validation
 */

import * as path from 'path';
import type { ProcessingStats } from '@scripts/shared/ScriptTypes';
import { Logger, StatsUtils, ConfigLoader } from '@scripts/shared/ScriptUtils';
import { RelativePathReplacer } from '@scripts/auto-replace-relative-paths';
import { UnifiedConfigGenerator, ConfigFilesGenerator } from '@scripts/update-all-configs';

// ============================================================================
// Master Orchestrator - Coordinates All Path Optimization
// ============================================================================

class MasterPathOptimizer {
  private rootPath: string;
  private totalStats: ProcessingStats;

  constructor(rootPath: string = process.cwd()) {
    this.rootPath = rootPath;
    this.totalStats = StatsUtils.createEmptyStats();
  }

  async optimize(): Promise<ProcessingStats> {
    Logger.info('🚀 Master Path Optimization System');
    Logger.info('==================================');
    Logger.info(`📍 Root path: ${this.rootPath}`);
    console.log('');

    try {
      // Phase 1: Generate unified configuration
      await this.runConfigurationGeneration();
      
      // Phase 2: Replace relative paths with aliases
      await this.runRelativePathReplacement();
      
      // Phase 3: Validation and reporting
      await this.runValidation();
      
      this.printFinalSummary();
      return StatsUtils.finalizeStats(this.totalStats);
      
    } catch (error) {
      Logger.error(`Path optimization failed: ${error}`);
      throw error;
    }
  }

  private async runConfigurationGeneration(): Promise<void> {
    Logger.info('📋 Phase 1: Configuration Generation');
    Logger.info('=====================================');
    
    try {
      // Generate unified config from directory structure
      const configGenerator = new UnifiedConfigGenerator();
      await configGenerator.generateConfig();
      
      // Generate all config files from unified config
      const filesGenerator = new ConfigFilesGenerator();
      await filesGenerator.generateAllConfigFiles();
      
      Logger.success('Configuration generation complete');
      console.log('');
      
    } catch (error) {
      Logger.error(`Configuration generation failed: ${error}`);
      throw error;
    }
  }

  private async runRelativePathReplacement(): Promise<void> {
    Logger.info('🔄 Phase 2: Relative Path Replacement');
    Logger.info('=====================================');
    
    try {
      const pathReplacer = new RelativePathReplacer(this.rootPath);
      const stats = await pathReplacer.run();
      
      // Merge stats
      this.totalStats.filesProcessed += stats.filesProcessed;
      this.totalStats.filesModified += stats.filesModified;
      this.totalStats.totalReplacements += stats.totalReplacements;
      
      Logger.success(`Path replacement complete: ${stats.totalReplacements} replacements`);
      console.log('');
      
    } catch (error) {
      Logger.error(`Path replacement failed: ${error}`);
      throw error;
    }
  }

  private async runValidation(): Promise<void> {
    Logger.info('✅ Phase 3: Validation & Quality Checks');
    Logger.info('=======================================');
    
    try {
      // Load and validate the unified configuration
      const config = await ConfigLoader.loadUnifiedConfig(this.rootPath);
      const isValid = ConfigLoader.validateConfig(config);
      
      if (!isValid) {
        throw new Error('Generated configuration is invalid');
      }
      
      Logger.success(`Configuration validated: ${Object.keys(config.pathMappings).length} path mappings`);
      
      // Check for potential issues
      await this.checkForIssues(config);
      
    } catch (error) {
      Logger.error(`Validation failed: ${error}`);
      throw error;
    }
  }

  private async checkForIssues(config: any): Promise<void> {
    const issues: string[] = [];
    
    // Check for duplicate aliases (shouldn't happen with our duplicate detection)
    const aliases = Object.keys(config.pathMappings);
    const uniqueAliases = new Set(aliases);
    if (aliases.length !== uniqueAliases.size) {
      issues.push('Duplicate aliases detected');
    }
    
    // Check for very long alias names
    const longAliases = aliases.filter(alias => alias.length > 50);
    if (longAliases.length > 0) {
      issues.push(`${longAliases.length} very long alias names`);
    }
    
    // Check for potentially confusing similar aliases
    const similarPairs = this.findSimilarAliases(aliases);
    if (similarPairs.length > 0) {
      issues.push(`${similarPairs.length} pairs of similar aliases`);
    }
    
    if (issues.length > 0) {
      Logger.warn('Potential issues detected:');
      issues.forEach(issue => Logger.warn(`  • ${issue}`));
    } else {
      Logger.success('No issues detected');
    }
    
    console.log('');
  }

  private findSimilarAliases(aliases: string[]): string[] {
    const similar: string[] = [];
    
    for (let i = 0; i < aliases.length; i++) {
      for (let j = i + 1; j < aliases.length; j++) {
        const alias1 = aliases[i];
        const alias2 = aliases[j];
        
        // Simple similarity check - same prefix with only minor differences
        if (this.areAliasesSimilar(alias1, alias2)) {
          similar.push(`${alias1} ↔ ${alias2}`);
        }
      }
    }
    
    return similar;
  }

  private areAliasesSimilar(alias1: string, alias2: string): boolean {
    // Check if they share a long common prefix
    let commonPrefix = 0;
    const minLength = Math.min(alias1.length, alias2.length);
    
    for (let i = 0; i < minLength; i++) {
      if (alias1[i] === alias2[i]) {
        commonPrefix++;
      } else {
        break;
      }
    }
    
    // Consider similar if they share >80% common prefix and differ by <5 chars
    const maxLength = Math.max(alias1.length, alias2.length);
    const similarity = commonPrefix / maxLength;
    const lengthDiff = Math.abs(alias1.length - alias2.length);
    
    return similarity > 0.8 && lengthDiff < 5;
  }

  private printFinalSummary(): void {
    const finalStats = StatsUtils.finalizeStats(this.totalStats);
    
    console.log('🎉 Master Path Optimization Complete!');
    console.log('====================================');
    console.log(`📊 Final Statistics:`);
    console.log(`   Files Processed: ${finalStats.filesProcessed}`);
    console.log(`   Files Modified: ${finalStats.filesModified}`);
    console.log(`   Total Replacements: ${finalStats.totalReplacements}`);
    
    if (finalStats.duration) {
      console.log(`   Duration: ${StatsUtils.formatDuration(finalStats.duration)}`);
    }
    
    console.log('');
    console.log('✨ Path Management System Benefits:');
    console.log('   • Clean, readable import statements');
    console.log('   • Automatic duplicate detection');
    console.log('   • Self-maintaining configuration');
    console.log('   • Zero relative path maintenance');
    console.log('');
    console.log('💡 Next steps:');
    console.log('   • Run TypeScript compilation to verify');
    console.log('   • Test your application functionality');
    console.log('   • Commit your optimized codebase');
  }
}

// ============================================================================
// CLI Integration
// ============================================================================

async function main(): Promise<void> {
  try {
    const optimizer = new MasterPathOptimizer();
    const stats = await optimizer.optimize();
    
    // Exit with success code
    process.exit(0);
    
  } catch (error) {
    Logger.error(`Master path optimization failed: ${error}`);
    process.exit(1);
  }
}

// ============================================================================
// Export for Testing and Integration
// ============================================================================

export { MasterPathOptimizer };

// Run if called directly
if (require.main === module) {
  main();
}