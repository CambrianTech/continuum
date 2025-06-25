#!/usr/bin/env npx tsx
/**
 * Command Migration Script
 * Systematically migrate all commands from JavaScript to TypeScript
 */

import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { TypeScriptCommandRegistry } from './TypeScriptCommandRegistry';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

interface MigrationCandidate {
  name: string;
  path: string;
  category: string;
  complexity: 'simple' | 'medium' | 'complex';
  dependencies: string[];
  hasTests: boolean;
}

class CommandMigrationOrchestrator {
  private registry = new TypeScriptCommandRegistry();
  private candidates: MigrationCandidate[] = [];
  private migrated: string[] = [];
  private failed: Array<{ command: string; error: string }> = [];

  /**
   * Discover all JavaScript commands to migrate
   */
  async discoverCommands(): Promise<void> {
    console.log('🔍 Discovering JavaScript commands...');
    
    const commandsDir = path.join(__dirname);
    const commandFiles = this.findCommandFiles(commandsDir);
    
    for (const filePath of commandFiles) {
      try {
        const candidate = await this.analyzeCommand(filePath);
        if (candidate) {
          this.candidates.push(candidate);
        }
      } catch (error) {
        console.warn(`⚠️ Failed to analyze ${filePath}:`, error.message);
      }
    }
    
    console.log(`📋 Found ${this.candidates.length} commands to migrate`);
    this.printMigrationPlan();
  }

  /**
   * Find all command files recursively
   */
  private findCommandFiles(dir: string): string[] {
    const files: string[] = [];
    
    try {
      const entries = fs.readdirSync(dir, { withFileTypes: true });
      
      for (const entry of entries) {
        const fullPath = path.join(dir, entry.name);
        
        if (entry.isDirectory()) {
          files.push(...this.findCommandFiles(fullPath));
        } else if (entry.name.endsWith('Command.cjs')) {
          files.push(fullPath);
        }
      }
    } catch (error) {
      // Skip directories that can't be read
    }
    
    return files;
  }

  /**
   * Analyze a command file for migration complexity
   */
  private async analyzeCommand(filePath: string): Promise<MigrationCandidate | null> {
    try {
      const content = fs.readFileSync(filePath, 'utf-8');
      const relativePath = path.relative(__dirname, filePath);
      const commandName = path.basename(filePath, '.cjs');
      
      // Determine category from path
      const pathParts = relativePath.split(path.sep);
      const category = pathParts.length > 1 ? pathParts[0] : 'core';
      
      // Analyze complexity
      const complexity = this.assessComplexity(content);
      
      // Find dependencies
      const dependencies = this.findDependencies(content);
      
      // Check for existing tests
      const testPath = filePath.replace('.cjs', '.test.js');
      const hasTests = fs.existsSync(testPath);
      
      return {
        name: commandName,
        path: filePath,
        category,
        complexity,
        dependencies,
        hasTests
      };
      
    } catch (error) {
      console.error(`Failed to analyze ${filePath}:`, error);
      return null;
    }
  }

  /**
   * Assess migration complexity
   */
  private assessComplexity(content: string): 'simple' | 'medium' | 'complex' {
    const complexityFactors = [
      content.includes('require('), // External dependencies
      content.includes('async'), // Async operations
      content.includes('WebSocket'), // WebSocket handling
      content.includes('fs.'), // File system operations
      content.includes('spawn'), // Process spawning
      content.includes('setTimeout'), // Timing operations
      content.length > 1000, // Large file
      (content.match(/function|=>/g) || []).length > 10 // Many functions
    ];
    
    const complexityScore = complexityFactors.filter(Boolean).length;
    
    if (complexityScore <= 2) return 'simple';
    if (complexityScore <= 5) return 'medium';
    return 'complex';
  }

  /**
   * Find command dependencies
   */
  private findDependencies(content: string): string[] {
    const dependencies: string[] = [];
    const requireMatches = content.match(/require\\(['"`]([^'"`]+)['"`]\\)/g);
    
    if (requireMatches) {
      for (const match of requireMatches) {
        const dep = match.match(/require\\(['"`]([^'"`]+)['"`]\\)/)?.[1];
        if (dep && !dep.startsWith('.')) {
          dependencies.push(dep);
        }
      }
    }
    
    return [...new Set(dependencies)];
  }

  /**
   * Print migration plan
   */
  private printMigrationPlan(): void {
    console.log('\n📋 MIGRATION PLAN');
    console.log('=================');
    
    const byComplexity = {
      simple: this.candidates.filter(c => c.complexity === 'simple'),
      medium: this.candidates.filter(c => c.complexity === 'medium'),
      complex: this.candidates.filter(c => c.complexity === 'complex')
    };
    
    console.log(`✅ Simple (${byComplexity.simple.length}): ${byComplexity.simple.map(c => c.name).join(', ')}`);
    console.log(`🔶 Medium (${byComplexity.medium.length}): ${byComplexity.medium.map(c => c.name).join(', ')}`);
    console.log(`🔴 Complex (${byComplexity.complex.length}): ${byComplexity.complex.map(c => c.name).join(', ')}`);
    
    console.log('\n🎯 RECOMMENDED ORDER:');
    console.log('1. Simple commands (foundation)');
    console.log('2. Medium commands (bulk migration)');
    console.log('3. Complex commands (careful handling)');
  }

  /**
   * Migrate commands in optimal order
   */
  async migrateAll(): Promise<void> {
    console.log('\n🚀 Starting systematic migration...');
    
    // Sort by complexity (simple first)
    const ordered = this.candidates.sort((a, b) => {
      const complexityOrder = { simple: 1, medium: 2, complex: 3 };
      return complexityOrder[a.complexity] - complexityOrder[b.complexity];
    });
    
    for (const candidate of ordered) {
      try {
        await this.migrateCommand(candidate);
        this.migrated.push(candidate.name);
        console.log(`✅ Migrated: ${candidate.name} (${candidate.complexity})`);
      } catch (error) {
        this.failed.push({ command: candidate.name, error: error.message });
        console.error(`❌ Failed: ${candidate.name} - ${error.message}`);
      }
    }
    
    this.printMigrationResults();
  }

  /**
   * Migrate a single command
   */
  private async migrateCommand(candidate: MigrationCandidate): Promise<void> {
    console.log(`🔄 Migrating ${candidate.name}...`);
    
    // For now, we'll use the EmotionCommand as a template
    // In a real implementation, this would:
    // 1. Parse the JavaScript file
    // 2. Generate TypeScript interfaces
    // 3. Convert the class
    // 4. Generate unit tests
    // 5. Update imports/exports
    
    // This is a placeholder for the actual migration logic
    throw new Error('Migration logic not yet implemented - using EmotionCommand as template');
  }

  /**
   * Print final migration results
   */
  private printMigrationResults(): void {
    console.log('\n🎯 MIGRATION RESULTS');
    console.log('===================');
    console.log(`✅ Migrated: ${this.migrated.length}`);
    console.log(`❌ Failed: ${this.failed.length}`);
    
    if (this.failed.length > 0) {
      console.log('\n❌ FAILED MIGRATIONS:');
      for (const failure of this.failed) {
        console.log(`  - ${failure.command}: ${failure.error}`);
      }
    }
    
    console.log(`\n📊 Progress: ${this.migrated.length}/${this.candidates.length} commands migrated`);
  }

  /**
   * Generate migration status report
   */
  generateStatusReport(): any {
    return {
      total: this.candidates.length,
      migrated: this.migrated.length,
      failed: this.failed.length,
      remaining: this.candidates.length - this.migrated.length - this.failed.length,
      byComplexity: {
        simple: this.candidates.filter(c => c.complexity === 'simple').length,
        medium: this.candidates.filter(c => c.complexity === 'medium').length,
        complex: this.candidates.filter(c => c.complexity === 'complex').length
      },
      registry: this.registry.getMigrationStatus()
    };
  }
}

// Run migration if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const orchestrator = new CommandMigrationOrchestrator();
  
  orchestrator.discoverCommands()
    .then(() => {
      console.log('\n📋 Discovery complete. Ready for migration.');
      console.log('💡 Next: Implement individual command migration logic');
      console.log('📖 Template: EmotionCommand.ts shows the TypeScript pattern');
      
      const report = orchestrator.generateStatusReport();
      console.log('\n📊 STATUS:', JSON.stringify(report, null, 2));
    })
    .catch(error => {
      console.error('❌ Migration discovery failed:', error);
      process.exit(1);
    });
}

export { CommandMigrationOrchestrator };