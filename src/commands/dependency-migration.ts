#!/usr/bin/env npx tsx
/**
 * Dependency-Aware Command Migration System
 * Handles complex command ecosystems with fluent APIs and interdependencies
 */

import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { CommandMigrationOrchestrator } from './migrate-commands';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

interface CommandDependency {
  name: string;
  type: 'extends' | 'requires' | 'imports' | 'fluent-api';
  source: string;
  target: string;
  line?: number;
}

interface PackageMetadata {
  name: string;
  capabilities: string[];
  dependencies: string[];
  hasTests: boolean;
  testRunner?: string;
  fluentAPIs?: string[];
}

interface MigrationNode {
  command: string;
  path: string;
  package?: PackageMetadata;
  dependencies: CommandDependency[];
  dependents: string[];
  migrationOrder: number;
  complexity: 'simple' | 'medium' | 'complex';
}

class DependencyAwareMigration extends CommandMigrationOrchestrator {
  private dependencyGraph = new Map<string, MigrationNode>();
  private migrationOrder: string[] = [];

  /**
   * Analyze command ecosystem with dependencies
   */
  async analyzeDependencyGraph(): Promise<void> {
    console.log('🕸️ Analyzing Command Dependency Graph...\n');
    
    // First pass: Discover all commands and their basic info
    await this.discoverCommands();
    
    // Second pass: Analyze dependencies and package metadata
    for (const candidate of this.candidates) {
      const node = await this.analyzeCommandDependencies(candidate);
      this.dependencyGraph.set(candidate.name, node);
    }
    
    // Third pass: Calculate migration order
    this.calculateMigrationOrder();
    
    this.printDependencyAnalysis();
  }

  /**
   * Analyze a single command's dependencies
   */
  private async analyzeCommandDependencies(candidate: any): Promise<MigrationNode> {
    const dependencies: CommandDependency[] = [];
    const dependents: string[] = [];
    let packageMetadata: PackageMetadata | undefined;
    
    try {
      // Read the command file
      const content = fs.readFileSync(candidate.path, 'utf-8');
      
      // Analyze extends relationships
      const extendsMatch = content.match(/class\s+\w+\s+extends\s+(\w+)/);
      if (extendsMatch) {
        dependencies.push({
          name: 'extends',
          type: 'extends',
          source: candidate.name,
          target: extendsMatch[1],
          line: this.findLineNumber(content, extendsMatch[0])
        });
      }
      
      // Analyze require/import dependencies
      const requireMatches = content.matchAll(/require\(['"`]([^'"`]+)['"`]\)/g);
      for (const match of requireMatches) {
        if (match[1].includes('Command')) {
          dependencies.push({
            name: 'requires',
            type: 'requires',
            source: candidate.name,
            target: match[1],
            line: this.findLineNumber(content, match[0])
          });
        }
      }
      
      // Analyze fluent API usage (method chaining)
      const fluentMatches = content.matchAll(/(\w+)\.(\w+)\(\)\.(\w+)\(/g);
      for (const match of fluentMatches) {
        dependencies.push({
          name: 'fluent-api',
          type: 'fluent-api',
          source: candidate.name,
          target: match[1],
          line: this.findLineNumber(content, match[0])
        });
      }
      
      // Read package.json if exists
      const packagePath = path.join(path.dirname(candidate.path), 'package.json');
      
      if (fs.existsSync(packagePath)) {
        const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf-8'));
        packageMetadata = {
          name: packageJson.name,
          capabilities: packageJson.continuum?.capabilities || [],
          dependencies: packageJson.continuum?.dependencies || [],
          hasTests: packageJson.continuum?.hasTests || false,
          testRunner: packageJson.scripts?.test || 'jest',
          fluentAPIs: this.extractFluentAPIs(content)
        };
      }
      
    } catch (error) {
      console.warn(`⚠️ Failed to analyze dependencies for ${candidate.name}:`, error.message);
    }
    
    return {
      command: candidate.name,
      path: candidate.path,
      package: packageMetadata,
      dependencies,
      dependents,
      migrationOrder: 0,
      complexity: candidate.complexity
    };
  }

  /**
   * Extract fluent API patterns from command code
   */
  private extractFluentAPIs(content: string): string[] {
    const apis: string[] = [];
    
    // Look for method chaining patterns
    const chainMatches = content.matchAll(/(\w+)\s*\.\s*(\w+)\s*\(\s*[^)]*\s*\)\s*\.\s*(\w+)/g);
    for (const match of chainMatches) {
      apis.push(`${match[1]}.${match[2]}().${match[3]}()`);
    }
    
    // Look for builder patterns
    const builderMatches = content.matchAll(/new\s+(\w+Builder|\w+Factory)\(\)/g);
    for (const match of builderMatches) {
      apis.push(match[1]);
    }
    
    return [...new Set(apis)];
  }

  /**
   * Calculate optimal migration order based on dependencies
   */
  private calculateMigrationOrder(): void {
    const visited = new Set<string>();
    const visiting = new Set<string>();
    const order: string[] = [];
    
    // Topological sort with dependency resolution
    for (const [commandName] of this.dependencyGraph) {
      this.topologicalSort(commandName, visited, visiting, order);
    }
    
    this.migrationOrder = order;
    
    // Update migration order in nodes
    order.forEach((command, index) => {
      const node = this.dependencyGraph.get(command);
      if (node) {
        node.migrationOrder = index;
      }
    });
  }

  /**
   * Topological sort for dependency resolution
   */
  private topologicalSort(
    command: string,
    visited: Set<string>,
    visiting: Set<string>,
    order: string[]
  ): void {
    if (visiting.has(command)) {
      console.warn(`⚠️ Circular dependency detected involving: ${command}`);
      return;
    }
    
    if (visited.has(command)) {
      return;
    }
    
    visiting.add(command);
    
    const node = this.dependencyGraph.get(command);
    if (node) {
      // Process dependencies first
      for (const dep of node.dependencies) {
        if (dep.type === 'extends' || dep.type === 'requires') {
          const targetCommand = this.findCommandByName(dep.target);
          if (targetCommand) {
            this.topologicalSort(targetCommand, visited, visiting, order);
          }
        }
      }
    }
    
    visiting.delete(command);
    visited.add(command);
    order.push(command);
  }

  /**
   * Find command name from class/module name
   */
  private findCommandByName(target: string): string | null {
    for (const [name, node] of this.dependencyGraph) {
      if (node.command.includes(target) || target.includes(node.command)) {
        return name;
      }
    }
    return null;
  }

  /**
   * Find line number of a match in content
   */
  private findLineNumber(content: string, match: string): number {
    const lines = content.substring(0, content.indexOf(match)).split('\n');
    return lines.length;
  }

  /**
   * Print comprehensive dependency analysis
   */
  private printDependencyAnalysis(): void {
    console.log('🕸️ DEPENDENCY GRAPH ANALYSIS');
    console.log('============================\n');
    
    // Print migration order
    console.log('📊 OPTIMAL MIGRATION ORDER:');
    this.migrationOrder.forEach((command, index) => {
      const node = this.dependencyGraph.get(command);
      const complexity = node?.complexity || 'unknown';
      const depCount = node?.dependencies.length || 0;
      console.log(`  ${index + 1}. ${command} (${complexity}, ${depCount} deps)`);
    });
    
    console.log('\n🔗 DEPENDENCY RELATIONSHIPS:');
    for (const [command, node] of this.dependencyGraph) {
      if (node.dependencies.length > 0) {
        console.log(`\n  ${command}:`);
        for (const dep of node.dependencies) {
          console.log(`    ${dep.type}: ${dep.target} (line ${dep.line})`);
        }
      }
    }
    
    console.log('\n📦 PACKAGE CAPABILITIES:');
    for (const [command, node] of this.dependencyGraph) {
      if (node.package) {
        console.log(`\n  ${command}:`);
        console.log(`    Capabilities: ${node.package.capabilities.join(', ')}`);
        console.log(`    Dependencies: ${node.package.dependencies.join(', ')}`);
        console.log(`    Has Tests: ${node.package.hasTests}`);
        if (node.package.fluentAPIs && node.package.fluentAPIs.length > 0) {
          console.log(`    Fluent APIs: ${node.package.fluentAPIs.join(', ')}`);
        }
      }
    }
    
    console.log('\n🔄 FLUENT API PATTERNS:');
    const fluentCommands = Array.from(this.dependencyGraph.values())
      .filter(node => node.package?.fluentAPIs && node.package.fluentAPIs.length > 0);
      
    if (fluentCommands.length > 0) {
      for (const node of fluentCommands) {
        console.log(`  ${node.command}: ${node.package?.fluentAPIs?.join(', ')}`);
      }
    } else {
      console.log('  No fluent API patterns detected yet');
    }
  }

  /**
   * Generate npm-like dependency tree
   */
  generateDependencyTree(): string {
    let tree = 'continuum-commands/\n';
    
    for (const command of this.migrationOrder) {
      const node = this.dependencyGraph.get(command);
      if (node) {
        tree += `├── ${command}@1.0.0\n`;
        
        for (const dep of node.dependencies) {
          if (dep.type === 'extends' || dep.type === 'requires') {
            tree += `│   ├── ${dep.target} (${dep.type})\n`;
          }
        }
        
        if (node.package?.capabilities) {
          tree += `│   └── capabilities: ${node.package.capabilities.join(', ')}\n`;
        }
      }
    }
    
    return tree;
  }

  /**
   * Migrate commands in dependency order
   */
  async migrateInDependencyOrder(): Promise<void> {
    console.log('\n🚀 DEPENDENCY-AWARE MIGRATION');
    console.log('=============================\n');
    
    for (const command of this.migrationOrder) {
      const node = this.dependencyGraph.get(command);
      if (node) {
        console.log(`🔄 Migrating ${command} (order ${node.migrationOrder + 1})...`);
        
        // Check if dependencies are satisfied
        const unsatisfiedDeps = this.checkDependencies(node);
        if (unsatisfiedDeps.length > 0) {
          console.log(`  ⚠️ Waiting for dependencies: ${unsatisfiedDeps.join(', ')}`);
          continue;
        }
        
        try {
          // Run package-specific tests if they exist
          if (node.package?.hasTests) {
            console.log(`  🧪 Running ${node.package.testRunner} tests...`);
            // await this.runCommandTests(node);
          }
          
          // Perform actual migration
          // await this.migrateCommand(candidate);
          console.log(`  ✅ ${command} migration simulated`);
          
        } catch (error) {
          console.log(`  ❌ Failed to migrate ${command}: ${error.message}`);
        }
      }
    }
  }

  /**
   * Check if all dependencies are satisfied
   */
  private checkDependencies(node: MigrationNode): string[] {
    const unsatisfied: string[] = [];
    
    for (const dep of node.dependencies) {
      if (dep.type === 'extends' || dep.type === 'requires') {
        const targetCommand = this.findCommandByName(dep.target);
        if (targetCommand && !this.migrated.includes(targetCommand)) {
          unsatisfied.push(dep.target);
        }
      }
    }
    
    return unsatisfied;
  }
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const migration = new DependencyAwareMigration();
  
  migration.analyzeDependencyGraph()
    .then(() => {
      console.log('\n📈 NEXT STEPS:');
      console.log('1. Review dependency order');
      console.log('2. Start with foundation commands (no dependencies)');
      console.log('3. Use npm-like test patterns per command');
      console.log('4. Maintain fluent API compatibility');
      
      console.log('\n🌳 DEPENDENCY TREE:');
      console.log(migration.generateDependencyTree());
    })
    .catch(error => {
      console.error('❌ Dependency analysis failed:', error);
      process.exit(1);
    });
}

export { DependencyAwareMigration };