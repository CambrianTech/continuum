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

// ============================================================================
// WELL-TYPED CONFIGURATION STRUCTURE
// ============================================================================

type Environment = 'browser' | 'server';

interface NameExtractionRule {
  type: 'regex' | 'path-segment' | 'custom';
  pattern?: string;          // For regex type
  pathIndex?: number;        // For path-segment type  
  pathAnchor?: string;       // Anchor point in path (e.g., 'commands', 'daemons')
  segmentRange?: [number, number]; // Range of segments to join
  customExtractor?: (filePath: string, className: string) => string;
}

interface EntryTypeDefinition {
  name: string;               // 'daemon', 'command', 'widget', etc.
  pluralName: string;         // 'daemons', 'commands', 'widgets', etc.
  patterns: string[];         // File glob patterns to match
  nameExtraction: NameExtractionRule;
  registryTemplate: {
    arrayName: string;        // 'BROWSER_DAEMONS', 'SERVER_COMMANDS', etc.
    entryTemplate: string;    // Template for registry entry
  };
}

interface EnvironmentTarget {
  environment: Environment;
  outputFile: string;
  entryTypes: string[];      // Which entry types to include
  typeImports: {             // Additional type imports needed
    [typeName: string]: string; // type name -> import path pattern
  };
}

interface GeneratorConfig {
  rootPatterns: {
    exclude: string[];       // Global exclusions
  };
  entryTypes: EntryTypeDefinition[];
  targets: EnvironmentTarget[];
}

// ============================================================================
// HARDCODED CONFIGURATION (to be refined)
// ============================================================================

const STRUCTURE_CONFIG: GeneratorConfig = {
  rootPatterns: {
    exclude: [
      '**/*.bak',
      '**/*.bak/**/*', 
      '**/node_modules/**/*',
      '**/dist/**/*',
      '**/*.test.ts',
      '**/*.spec.ts'
    ]
  },
  
  entryTypes: [
    {
      name: 'daemon',
      pluralName: 'daemons',
      patterns: ['daemons/*/browser/*Browser.ts', 'daemons/*/server/*Server.ts'],
      nameExtraction: {
        type: 'regex',
        pattern: '^(.+?)(Browser|Server)$'  // Extract base name, removing Browser/Server suffix
      },
      registryTemplate: {
        arrayName: '{ENV}_DAEMONS',
        entryTemplate: `{
    name: '{name}',
    className: '{className}',
    daemonClass: {className}
  }`
      }
    },
    
    {
      name: 'command', 
      pluralName: 'commands',
      patterns: ['commands/**/browser/*Command.ts', 'commands/**/server/*Command.ts'],
      nameExtraction: {
        type: 'path-segment',
        pathAnchor: 'commands',
        segmentRange: [1, -2]  // From after 'commands' to before 'browser/server'
      },
      registryTemplate: {
        arrayName: '{ENV}_COMMANDS',
        entryTemplate: `{
    name: '{name}',
    className: '{className}',
    commandClass: {className}
  }`
      }
    }
  ],
  
  targets: [
    {
      environment: 'browser',
      outputFile: 'browser/generated.ts',
      entryTypes: ['daemon', 'command'],
      typeImports: {
        'DaemonEntry': 'daemons/command-daemon/shared/DaemonBase',
        'CommandEntry': 'daemons/command-daemon/shared/CommandBase'
      }
    },
    
    {
      environment: 'server', 
      outputFile: 'server/generated.ts',
      entryTypes: ['daemon', 'command'],
      typeImports: {
        'DaemonEntry': 'daemons/command-daemon/shared/DaemonBase',
        'CommandEntry': 'daemons/command-daemon/shared/CommandBase'  
      }
    }
  ]
};

// ============================================================================
// LEGACY INTERFACES (for backward compatibility)
// ============================================================================

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

// Generic entry interface
interface GeneratedEntry {
  name: string;
  className: string;
  importPath: string;
  disabled?: boolean;
  reason?: string;
}

class StructureGenerator {
  private config: GeneratorConfig;
  private rootPath: string;

  constructor(rootPath: string, config?: GeneratorConfig) {
    this.rootPath = rootPath;
    this.config = config ?? STRUCTURE_CONFIG;
  }

  /**
   * Create name extractor function from configuration
   */
  private createNameExtractor(rule: NameExtractionRule): (filePath: string, className: string) => string {
    switch (rule.type) {
      case 'regex':
        const regex = new RegExp(rule.pattern!);
        return (_, className) => {
          const match = className.match(regex);
          return match ? match[1] : className;
        };
        
      case 'path-segment':
        return (filePath) => {
          const relativePath = relative(this.rootPath, filePath);
          const pathParts = relativePath.split('/');
          const anchorIndex = pathParts.indexOf(rule.pathAnchor!);
          
          if (anchorIndex !== -1) {
            const [start, end] = rule.segmentRange!;
            const segmentStart = anchorIndex + start;
            const segmentEnd = end < 0 ? pathParts.length + end : anchorIndex + end;
            return pathParts.slice(segmentStart, segmentEnd).join('/');
          }
          return '';
        };
        
      case 'custom':
        return rule.customExtractor!;
        
      default:
        return (_, className) => className;
    }
  }

  /**
   * Get all patterns for a specific environment and entry type
   */
  private getPatternsForEnvironment(entryType: EntryTypeDefinition, environment: Environment): string[] {
    const envSuffix = environment.charAt(0).toUpperCase() + environment.slice(1);
    return entryType.patterns.filter(pattern => 
      pattern.includes(`/${environment}/`) || pattern.includes(`*${envSuffix}.ts`)
    );
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
   * Generate structure file using new configuration system
   */
  private generateStructureFileFromConfig(
    target: EnvironmentTarget,
    allEntries: Map<string, GeneratedEntry[]>
  ): void {
    const envCap = target.environment.charAt(0).toUpperCase() + target.environment.slice(1);
    const envUpper = target.environment.toUpperCase();
    
    const sections: string[] = [];
    
    // Header
    const entryTypeCounts = Array.from(allEntries.entries())
      .map(([type, entries]) => `${entries.length} ${type}${entries.length !== 1 ? 's' : ''}`)
      .join(' and ');
    
    sections.push(`/**
 * ${envCap} Structure Registry - Auto-generated on ${new Date().toISOString()}
 * 
 * Contains ${entryTypeCounts}.
 * Generated by scripts/generate-structure.ts - DO NOT EDIT MANUALLY
 */`);
    
    // Imports by entry type
    Array.from(allEntries.entries()).forEach(([entryTypeName, entries]) => {
      if (entries.length > 0) {
        const entryType = this.config.entryTypes.find(et => et.name === entryTypeName);
        sections.push(`\n// ${envCap} ${entryType?.pluralName || entryTypeName} Imports`);
        sections.push(entries.map(e => `import { ${e.className} } from '${e.importPath}';`).join('\n'));
      }
    });
    
    // Type imports
    sections.push('\n// Types');
    for (const [typeName, importPath] of Object.entries(target.typeImports)) {
      const outputDir = join(this.rootPath, target.outputFile).replace(/[^/]*$/, '');
      const fullImportPath = join(this.rootPath, importPath + '.ts');
      const relativeImportPath = relative(outputDir, fullImportPath).replace('.ts', '').replace(/\\/g, '/');
      sections.push(`import type { ${typeName} } from './${relativeImportPath}';`);
    }
    
    // Exports
    sections.push(`\n/**\n * ${envCap} Environment Registry\n */`);
    
    Array.from(allEntries.entries()).forEach(([entryTypeName, entries], index) => {
      if (entries.length > 0) {
        const entryType = this.config.entryTypes.find(et => et.name === entryTypeName);
        if (entryType) {
          const arrayName = entryType.registryTemplate.arrayName.replace('{ENV}', envUpper);
          sections.push(`export const ${arrayName}: ${this.getEntryTypeName(entryTypeName)}[] = [`);
          
          const entryStrings = entries.map(entry => {
            return entryType.registryTemplate.entryTemplate
              .replace(/\{name\}/g, entry.name)
              .replace(/\{className\}/g, entry.className)
              .replace(/\{importPath\}/g, entry.importPath);
          });
          
          sections.push(entryStrings.join(',\n'));
          sections.push('];');
          
          if (index < allEntries.size - 1) {
            sections.push('');
          }
        }
      }
    });
    
    const content = sections.join('\n') + '\n';
    const fullOutputPath = join(this.rootPath, target.outputFile);
    writeFileSync(fullOutputPath, content, 'utf8');
    
    const totalEntries = Array.from(allEntries.values()).reduce((sum, entries) => sum + entries.length, 0);
    console.log(`✅ Generated ${target.outputFile} with ${totalEntries} entries`);
  }

  /**
   * Get TypeScript type name for entry type
   */
  private getEntryTypeName(entryTypeName: string): string {
    const typeMap: Record<string, string> = {
      'daemon': 'DaemonEntry',
      'command': 'CommandEntry'
    };
    return typeMap[entryTypeName] || `${entryTypeName.charAt(0).toUpperCase() + entryTypeName.slice(1)}Entry`;
  }


  /**
   * Process entries using the new configuration system
   */
  private processEntriesFromConfig(
    target: EnvironmentTarget,
    entryTypeName: string
  ): GeneratedEntry[] {
    const entryType = this.config.entryTypes.find(et => et.name === entryTypeName);
    if (!entryType) {
      console.warn(`⚠️  Entry type '${entryTypeName}' not found in configuration`);
      return [];
    }

    const patterns = this.getPatternsForEnvironment(entryType, target.environment);
    if (patterns.length === 0) {
      console.log(`   No patterns found for ${entryTypeName} in ${target.environment} environment`);
      return [];
    }

    console.log(`   Searching for ${entryType.pluralName}: ${patterns.join(', ')}`);
    
    const files = this.findFiles(patterns, this.config.rootPatterns.exclude);
    const uniqueFiles = Array.from(new Set(files));
    
    const nameExtractor = this.createNameExtractor(entryType.nameExtraction);
    
    const entries = uniqueFiles
      .map(filePath => this.extractEntryFromConfig(filePath, target.outputFile, nameExtractor, entryType.name))
      .filter((entry): entry is GeneratedEntry => entry !== null);
    
    const validatedEntries = this.validateUniqueNames(entries, entryType.name);
    
    console.log(`   Found ${validatedEntries.length} ${entryType.pluralName}: ${validatedEntries.map(e => e.name).join(', ')}`);
    
    return validatedEntries;
  }

  /**
   * Extract entry info using configuration-driven approach
   */
  private extractEntryFromConfig(
    filePath: string,
    outputPath: string,
    nameExtractor: (filePath: string, className: string) => string,
    entryType: string
  ): GeneratedEntry | null {
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
      
      // Extract name using configured strategy  
      const name = nameExtractor(filePath, className);
      
      // Calculate proper relative import path
      const outputDir = join(this.rootPath, outputPath).replace(/[^/]*$/, '');
      const relativeImportPath = relative(outputDir, filePath).replace('.ts', '').replace(/\\/g, '/');
      
      return {
        name,
        className,
        importPath: `./${relativeImportPath}`
      };
    } catch (e) {
      console.warn(`Ignoring ${entryType} info from ${filePath}:`, e);
      return null;
    }
  }

  /**
   * Legacy generic entry processing pipeline (for backward compatibility)
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
    console.log('🏭 Intelligent Structure Generator (Config-Driven)');
    console.log('=================================================');
    console.log(`📍 Root path: ${this.rootPath}`);
    console.log(`📋 Entry types: ${this.config.entryTypes.map(et => et.name).join(', ')}`);
    console.log(`📂 Targets: ${this.config.targets.map(t => `${t.environment} (${t.outputFile})`).join(', ')}`);
    
    for (const target of this.config.targets) {
      console.log(`\n🔍 Processing ${target.environment} target...`);
      
      // Process all configured entry types for this target
      const allEntries = new Map<string, GeneratedEntry[]>();
      
      for (const entryTypeName of target.entryTypes) {
        const entries = this.processEntriesFromConfig(target, entryTypeName);
        allEntries.set(entryTypeName, entries);
      }
      
      // Generate structure file using new configuration-driven approach
      this.generateStructureFileFromConfig(target, allEntries);
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