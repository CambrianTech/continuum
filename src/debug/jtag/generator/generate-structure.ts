// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Intelligent Structure Generator - Automated generated.ts file creation
 * 
 * Recursively discovers daemons and commands, generates proper generated.ts files
 * with environment-specific imports and type-safe registries.
 * 
 * Configuration via package.json "structureGenerator" field allows easy expansion.
 * Follows modular command architecture patterns with ~50 line modules.
 */

import { readFileSync, writeFileSync, readdirSync, statSync, existsSync } from 'fs';
import { join, relative } from 'path';

interface StructureConfig {
  directories: {
    [key: string]: {
      outputFile: string;
      environment: 'browser' | 'server';
      daemonPaths?: string[];
      commandPaths?: string[];
      excludePatterns: string[];
    }
  };
}

interface DaemonEntry {
  name: string;
  className: string;
  importPath: string;
  disabled?: boolean;
  reason?: string;
}

interface CommandEntry {
  name: string;
  className: string;
  importPath: string;
  disabled?: boolean;
  reason?: string;
}

class StructureGenerator {
  private config: StructureConfig;
  private rootPath: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
    this.config = this.loadConfig();
  }

  private loadConfig(): StructureConfig {
    const unifiedConfigPath = join(this.rootPath, '.continuum/generator/unified-config.json');
    const unifiedConfig = JSON.parse(readFileSync(unifiedConfigPath, 'utf8'));
    
    // Return the structureGeneration configuration from unified-config.json
    return unifiedConfig.structureGeneration ?? {
      directories: {
        browser: {
          outputFile: 'browser/generated.ts',
          environment: 'browser',
          daemonPaths: ['daemons/*/browser/*Browser.ts'],
          excludePatterns: ['**/*.bak', '**/*.bak/**/*', '**/node_modules/**/*']
        },
        server: {
          outputFile: 'server/generated.ts',
          environment: 'server', 
          daemonPaths: ['daemons/*/server/*Server.ts'],
          excludePatterns: ['**/*.bak', '**/*.bak/**/*', '**/node_modules/**/*']
        }
      }
    };
  }

  private isExcluded(filePath: string, excludePatterns: string[]): boolean {
    return excludePatterns.some(pattern => {
      // Simple glob pattern matching for .bak files and directories
      if (pattern === '**/*.bak' && filePath.includes('.bak')) return true;
      if (pattern === '**/*.bak/**/*' && filePath.includes('.bak/')) return true;
      if (pattern === '**/node_modules/**/*' && filePath.includes('node_modules/')) return true;
      return false;
    });
  }

  private findFiles(patterns: string[], excludePatterns: string[]): string[] {
    return patterns
      .flatMap(pattern => this.expandPattern(pattern.split('/'), this.rootPath))
      .filter(path => existsSync(path) && !this.isExcluded(path, excludePatterns))
      .sort();
  }

  private expandPattern(patternParts: string[], basePath: string, index = 0): string[] {
    if (index >= patternParts.length) return [basePath];
    
    const part = patternParts[index];
    
    const safeReadDir = (path: string): string[] => {
      try { return readdirSync(path); } catch { return []; }
    };
    
    const safeIsDirectory = (path: string): boolean => {
      try { return statSync(path).isDirectory(); } catch { return false; }
    };
    
    const safeIsFile = (path: string): boolean => {
      try { return statSync(path).isFile(); } catch { return false; }
    };
    
    const handlers = {
      '**': () => [
        // Zero directories matched - continue from current directory
        ...this.expandPattern(patternParts, basePath, index + 1),
        // Recursively descend into subdirectories
        ...safeReadDir(basePath)
          .map(entry => join(basePath, entry))
          .filter(safeIsDirectory)
          .flatMap(fullPath => this.expandPattern(patternParts, fullPath, index))
      ],
      
      '*': () => safeReadDir(basePath)
        .map(entry => join(basePath, entry))
        .filter(safeIsDirectory)
        .flatMap(fullPath => this.expandPattern(patternParts, fullPath, index + 1)),
      
      'pattern': () => safeReadDir(basePath)
        .filter(entry => this.matchesPattern(entry, part))
        .map(entry => join(basePath, entry))
        .filter(safeIsFile),
      
      'literal': () => this.expandPattern(patternParts, join(basePath, part), index + 1)
    };
    
    if (part === '**') return handlers['**']();
    if (part === '*') return handlers['*']();
    if (part.includes('*')) return handlers['pattern']();
    return handlers['literal']();
  }

  private matchesPattern(filename: string, pattern: string): boolean {
    // Simple pattern matching - convert * to regex
    const regexPattern = pattern.replace(/\*/g, '.*');
    const regex = new RegExp(`^${regexPattern}$`);
    return regex.test(filename);
  }

  /**
   * Check for duplicate names in entries and warn
   */
  private validateUniqueNames<T extends { name: string; className: string; importPath: string }>(
    entries: T[], 
    type: string
  ): T[] {
    const duplicates = Object.entries(
      entries.reduce((acc, entry) => {
        (acc[entry.name] ??= []).push(entry);
        return acc;
      }, {} as Record<string, T[]>)
    ).filter(([_, entries]) => entries.length > 1);
    
    if (duplicates.length > 0) {
      console.warn(`⚠️  Found duplicate ${type} names:`);
      duplicates.forEach(([name, entries]) => {
        console.warn(`   ${name}:`);
        entries.forEach(entry => {
          console.warn(`     - ${entry.className} (${entry.importPath})`);
        });
      });
    }
    
    return entries;
  }

  /**
   * Generic entry extraction with type-specific name extraction
   */
  private extractEntryInfo<T extends { name: string; className: string; importPath: string }>(
    filePath: string, 
    outputPath: string,
    nameExtractor: (filePath: string, className: string) => string,
    type: string
  ): T | null {
    try {
      const content = readFileSync(filePath, 'utf8');
      const filename = filePath.split('/').pop()!;
      const className = filename.replace('.ts', '');
      
      // Check if file exports the expected class
      const exportPattern = new RegExp(`export\\s+class\\s+${className}\\b`);
      if (!exportPattern.test(content)) {
        console.warn(`⚠️  Skipping ${filePath}: No export for class ${className}`);
        return null;
      }
      
      // Extract name using provided strategy
      const name = nameExtractor(filePath, className);
      
      // Calculate proper relative import path
      const outputDir = join(this.rootPath, outputPath).replace(/[^/]*$/, '');
      const relativeImportPath = relative(outputDir, filePath).replace('.ts', '').replace(/\\/g, '/');
      
      return {
        name,
        className,
        importPath: `./${relativeImportPath}`
      } as T;
    } catch (e) {
      console.warn(`Ignoring ${type} info from ${filePath}:`, e);
      return null;
    }
  }

  private extractDaemonInfo(filePath: string, outputPath: string): DaemonEntry | null {
    return this.extractEntryInfo<DaemonEntry>(
      filePath, 
      outputPath,
      (_, className) => className.replace(/(Browser|Server)$/, ''),
      'daemon'
    );
  }

  private extractCommandInfo(filePath: string, outputPath: string): CommandEntry | null {
    return this.extractEntryInfo<CommandEntry>(
      filePath,
      outputPath,
      (filePath) => {
        const relativePath = relative(this.rootPath, filePath);
        const pathParts = relativePath.split('/');
        const commandIndex = pathParts.indexOf('commands');
        
        if (commandIndex !== -1 && commandIndex + 1 < pathParts.length) {
          const commandParts = pathParts.slice(commandIndex + 1);
          return commandParts.length > 2 
            ? commandParts.slice(0, -2).join('/')
            : commandParts[0];
        }
        return '';
      },
      'command'
    );
  }

  private generateStructureFile(
    outputPath: string,
    environment: 'browser' | 'server',
    daemons: DaemonEntry[],
    commands: CommandEntry[]
  ): void {
    const envCap = environment.charAt(0).toUpperCase() + environment.slice(1);
    const envUpper = environment.toUpperCase();
    
    // Note: Import paths are calculated using Node.js relative() function 
    // in extractDaemonInfo/extractCommandInfo methods for accuracy
    
    // Build content sections
    const sections: string[] = [];
    
    // Header
    sections.push(`/**
 * ${envCap} Structure Registry - Auto-generated on ${new Date().toISOString()}
 * 
 * Contains ${daemons.length > 0 ? environment + '-side daemon' : ''}${daemons.length > 0 && commands.length > 0 ? ' and ' : ''}${commands.length > 0 ? 'command' : ''} imports.
 * Generated by scripts/generate-structure.ts - DO NOT EDIT MANUALLY
 */`);
    
    // Daemon imports
    if (daemons.length > 0) {
      sections.push(`\n// ${envCap} Daemon Imports`);
      sections.push(daemons.map(d => `import { ${d.className} } from '${d.importPath}';`).join('\n'));
    }
    
    // Command imports  
    if (commands.length > 0) {
      sections.push(`\n// ${envCap} Command Imports`);
      sections.push(commands.map(c => `import { ${c.className} } from '${c.importPath}';`).join('\n'));
    }
    
    // Type imports - calculate proper relative paths
    sections.push('\n// Types');
    if (daemons.length > 0 || commands.length > 0) {
      const outputDir = join(this.rootPath, outputPath).replace(/[^/]*$/, ''); // Get directory of output file
      const daemonBasePath = join(this.rootPath, 'daemons/command-daemon/shared/DaemonBase.ts');
      const commandBasePath = join(this.rootPath, 'daemons/command-daemon/shared/CommandBase.ts');
      
      if (daemons.length > 0) {
        const daemonBaseImport = relative(outputDir, daemonBasePath).replace('.ts', '').replace(/\\/g, '/');
        sections.push(`import type { DaemonEntry } from './${daemonBaseImport}';`);
      }
      if (commands.length > 0) {
        const commandBaseImport = relative(outputDir, commandBasePath).replace('.ts', '').replace(/\\/g, '/');
        sections.push(`import type { CommandEntry } from './${commandBaseImport}';`);
      }
    }
    
    // Exports
    sections.push(`\n/**\n * ${envCap} Environment Registry\n */`);
    
    if (daemons.length > 0) {
      sections.push(`export const ${envUpper}_DAEMONS: DaemonEntry[] = [`);
      sections.push(daemons.map(d => `  {\n    name: '${d.name}',\n    className: '${d.className}',\n    daemonClass: ${d.className}\n  }`).join(',\n'));
      sections.push('];');
    }
    
    if (commands.length > 0) {
      if (daemons.length > 0) sections.push('');
      sections.push(`export const ${envUpper}_COMMANDS: CommandEntry[] = [`);
      sections.push(commands.map(c => `  {\n    name: '${c.name}',\n    className: '${c.className}',\n    commandClass: ${c.className}\n  }`).join(',\n'));
      sections.push('];');
    }
    
    const content = sections.join('\n') + '\n';
    writeFileSync(join(this.rootPath, outputPath), content, 'utf8');
    console.log(`✅ Generated ${outputPath} with ${daemons.length} daemons and ${commands.length} commands`);
  }


  /**
   * Generic entry processing pipeline
   */
  private processEntries<T extends { name: string; className: string; importPath: string }>(
    config: { paths?: string[]; excludePatterns: string[]; outputFile: string },
    extractor: (filePath: string, outputPath: string) => T | null,
    type: string
  ): T[] {
    if (!config.paths) return [];
    
    console.log(`   Searching for ${type}s: ${config.paths.join(', ')}`);
    
    const files = this.findFiles(config.paths, config.excludePatterns);
    const uniqueFiles = Array.from(new Set(files));
    
    const entries = uniqueFiles
      .map(f => extractor(f, config.outputFile))
      .filter((entry): entry is T => entry !== null);
    
    const validatedEntries = this.validateUniqueNames(entries, type);
    
    console.log(`   Found ${validatedEntries.length} ${type}s: ${validatedEntries.map(e => e.name).join(', ')}`);
    
    return validatedEntries;
  }

  public generate(): void {
    console.log('🏭 Intelligent Structure Generator');
    console.log('================================');
    console.log(`📍 Root path: ${this.rootPath}`);
    console.log(`📋 Directories configured: ${Object.keys(this.config.directories).join(', ')}`);
    
    for (const [dirName, dirConfig] of Object.entries(this.config.directories)) {
      console.log(`\n🔍 Processing ${dirName} (${dirConfig.environment})...`);
      
      // Process daemons and commands using unified pipeline
      const daemons = this.processEntries(
        { paths: dirConfig.daemonPaths, excludePatterns: dirConfig.excludePatterns, outputFile: dirConfig.outputFile },
        this.extractDaemonInfo.bind(this),
        'daemon'
      );
      
      const commands = this.processEntries(
        { paths: dirConfig.commandPaths, excludePatterns: dirConfig.excludePatterns, outputFile: dirConfig.outputFile },
        this.extractCommandInfo.bind(this),
        'command'
      );
      
      // Generate structure file
      this.generateStructureFile(
        dirConfig.outputFile,
        dirConfig.environment,
        daemons,
        commands
      );
    }
    
    console.log('\n🎉 Structure generation complete!');
  }
}

// CLI execution
if (require.main === module) {
  const rootPath = process.argv[2] || process.cwd();
  const generator = new StructureGenerator(rootPath);
  generator.generate();
}

export { StructureGenerator };