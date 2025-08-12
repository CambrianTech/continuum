// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Universal Module Structure Generator - JTAG/Continuum Core Architecture
 * 
 * Generates type-safe registries for all JTAG modules following the Universal Module Pattern.
 * This is the foundation of JTAG's 85% code reduction through shared abstraction architecture.
 * 
 * ============================================================================
 * UNIVERSAL MODULE ARCHITECTURE PATTERN (Core JTAG Design)
 * ============================================================================
 * 
 * Every module (daemon/command/adapter/protocol/ai-hook) follows this structure:
 * 
 * module-name/
 * ├── shared/              # 85% of logic - Pure TypeScript, no environment deps
 * │   ├── Types.ts         # Universal interfaces, CommandParams/CommandResult
 * │   ├── Base.ts          # Core business logic, validation, processing
 * │   └── Validator.ts     # Shared validation, utilities, formatters
 * ├── browser/             # 7-8% - Browser-specific transport layer
 * │   └── ModuleBrowser.ts # Thin wrapper: DOM APIs, WebSocket client, etc.
 * ├── server/              # 7-8% - Server-specific transport layer  
 * │   └── ModuleServer.ts  # Thin wrapper: Node.js APIs, file system, etc.
 * └── README.md            # Module documentation
 * 
 * IMPORT DIRECTION RULES (Enforced by TypeScript):
 * ✅ ALLOWED:
 *   browser/ → shared/     # Browser imports shared logic
 *   server/  → shared/     # Server imports shared logic  
 *   shared/  → shared/     # Shared imports other shared modules
 * 
 * ❌ FORBIDDEN (Compilation failure):
 *   shared/  → browser/    # Shared cannot import browser-specific
 *   shared/  → server/     # Shared cannot import server-specific
 *   browser/ → server/     # Cross-environment contamination
 *   server/  → browser/    # Cross-environment contamination
 * 
 * CODE REDUCTION THROUGH ABSTRACTION:
 * - Before: 580 lines (300 browser + 280 server) with massive duplication
 * - After:  325 lines (250 shared + 40 browser + 35 server) = 44% reduction
 * - Shared base contains ALL business logic, validation, and processing
 * - Environment wrappers are THIN transport adapters only
 * 
 * SUPPORTED MODULE TYPES:
 * - daemons/        - Background services (health, console, session, etc.)
 * - commands/       - User-invokable actions (screenshot, file operations, etc.)  
 * - channels/       - Communication infrastructure (routing, factories)
 * - adapters/       - Protocol implementations (websocket, http, mesh, etc.)
 * - ai-hooks/       - AI capability interfaces (MPC, federated learning)
 * - protocols/      - Network protocol handlers (P2P, consensus, etc.)
 * 
 * This generator automatically discovers all modules following this pattern
 * and creates environment-specific registries for dependency injection.
 * 
 * ============================================================================
 */

import { readFileSync, writeFileSync, readdirSync, statSync, existsSync } from 'fs';
import { join, relative } from 'path';

// ============================================================================
// UNIVERSAL MODULE STRUCTURE SCHEMA
// ============================================================================

/**
 * TypeScript schema for validating Universal Module Architecture compliance
 */
interface UniversalModuleStructure {
  moduleName: string;
  moduleType: 'daemon' | 'command' | 'adapter' | 'protocol' | 'ai-hook' | 'channel';
  basePath: string;
  directories: {
    shared: UniversalModuleDirectory;
    browser?: UniversalModuleDirectory;  // Optional for shared-only modules
    server?: UniversalModuleDirectory;   // Optional for shared-only modules
  };
  conformance: {
    hasSharedBase: boolean;              // Must have shared/ directory
    hasEnvironmentSeparation: boolean;   // Browser/server separation maintained  
    importDirectionValid: boolean;       // No forbidden imports (shared → env)
    codeReductionRatio: number;         // Percentage of code in shared vs environment
  };
}

interface UniversalModuleDirectory {
  path: string;
  files: UniversalModuleFile[];
  lineCount: number;
  imports: {
    fromShared: string[];      // Imports from ../shared/
    fromExternal: string[];    // Imports from outside module
    crossEnvironment: string[]; // FORBIDDEN: imports from other environments  
  };
}

interface UniversalModuleFile {
  filename: string;
  exports: string[];         // Classes/interfaces exported
  imports: string[];         // All import statements
  lineCount: number;
  role: 'types' | 'base' | 'validator' | 'wrapper' | 'other';
}

// ============================================================================
// WELL-TYPED CONFIGURATION STRUCTURE  
// ============================================================================

type Environment = 'browser' | 'server';

interface NameExtractionRule {
  type: 'regex' | 'path-segment' | 'filename-transport-class' | 'custom';
  pattern?: string;          // For regex type
  pathIndex?: number;        // For path-segment type  
  pathAnchor?: string;       // Anchor point in path (e.g., 'commands', 'daemons')
  segmentRange?: [number, number]; // Range of segments to join
  customExtractor?: (filePath: string, className: string) => string;
}

interface EntryTypeDefinition {
  name: string;               // 'daemon', 'command', 'widget', etc.
  pluralName: string;         // 'daemons', 'commands', 'widgets', etc.
  typeScriptTypeName: string; // 'DaemonEntry', 'CommandEntry', etc.
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
      typeScriptTypeName: 'DaemonEntry',
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
      typeScriptTypeName: 'CommandEntry',
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
    },

    {
      name: 'adapter',
      pluralName: 'adapters',
      typeScriptTypeName: 'AdapterEntry', 
      patterns: ['system/transports/*/browser/*Transport*.ts', 'system/transports/*/server/*Transport*.ts'],
      nameExtraction: {
        type: 'filename-transport-class',  // Custom extraction for transport classes
        pathAnchor: 'transports',
        segmentRange: [1, -2]  // From transport type to before browser/server
      },
      registryTemplate: {
        arrayName: '{ENV}_ADAPTERS',
        entryTemplate: `{
    name: '{name}',
    className: '{className}',
    adapterClass: {className},
    protocol: 'websocket', // TODO: Extract from transport type
    supportedRoles: ['client', 'server'], // TODO: Extract from class metadata
    supportedEnvironments: ['{env}'] // Current environment
  }`
      }
    }
  ],
  
  targets: [
    {
      environment: 'browser',
      outputFile: 'browser/generated.ts',
      entryTypes: ['daemon', 'command', 'adapter'],
      typeImports: {
        'DaemonEntry': 'daemons/command-daemon/shared/DaemonBase',
        'CommandEntry': 'daemons/command-daemon/shared/CommandBase',
        'AdapterEntry': 'system/transports/shared/TransportBase'
      }
    },
    
    {
      environment: 'server', 
      outputFile: 'server/generated.ts',
      entryTypes: ['daemon', 'command', 'adapter'],
      typeImports: {
        'DaemonEntry': 'daemons/command-daemon/shared/DaemonBase',
        'CommandEntry': 'daemons/command-daemon/shared/CommandBase',
        'AdapterEntry': 'system/transports/shared/TransportBase'
      }
    }
  ]
};

// ============================================================================
// ENTRY INTERFACES
// ============================================================================

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
      case 'regex': {
        const regex = new RegExp(rule.pattern!);
        return (_, className) => {
          const match = className.match(regex);
          return match ? match[1] : className;
        };
      }
        
      case 'path-segment': {
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
      }

      case 'filename-transport-class': {
        return (filePath, className) => {
          // For transport classes, use the full class name to avoid duplicates
          // e.g., WebSocketTransportServer -> websocket-transport-server
          //       WebSocketTransportClientServer -> websocket-transport-server-client
          return className
            .replace(/Transport/g, '-transport-')
            .replace(/([A-Z])/g, (match, p1, offset) => offset > 0 ? '-' + p1.toLowerCase() : p1.toLowerCase())
            .replace(/-+/g, '-')
            .replace(/^-|-$/g, '');
        };
      }
        
      case 'custom': {
        return rule.customExtractor!;
      }
        
      default: {
        return (_, className) => className;
      }
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
    const relativePath = relative(this.rootPath, filePath).replace(/\\/g, '/');
    return excludePatterns.some(pattern => this.matchesGlobPattern(relativePath, pattern));
  }

  /**
   * Efficient glob pattern matching
   */
  private matchesGlobPattern(filePath: string, pattern: string): boolean {
    // Convert glob pattern to regex - be more careful about order
    const regexPattern = pattern
      .replace(/\./g, '\\.')            // Escape dots first
      .replace(/\*\*/g, '§DOUBLESTAR§')  // Temporarily replace ** to avoid conflicts
      .replace(/\*/g, '[^/]*')          // * matches within a segment  
      .replace(/§DOUBLESTAR§/g, '.*');  // ** matches any path segments
    
    const regex = new RegExp(`^${regexPattern}$`);
    return regex.test(filePath);
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
    
    // Handle different pattern types
    if (part === '**') {
      return [
        // Zero directories matched - continue from current directory
        ...this.expandPattern(patternParts, basePath, index + 1),
        // Recursively descend into subdirectories
        ...safeReadDir(basePath)
          .map(entry => join(basePath, entry))
          .filter(safeIsDirectory)
          .flatMap(fullPath => this.expandPattern(patternParts, fullPath, index))
      ];
    }
    
    if (part === '*') {
      return safeReadDir(basePath)
        .map(entry => join(basePath, entry))
        .filter(safeIsDirectory)
        .flatMap(fullPath => this.expandPattern(patternParts, fullPath, index + 1));
    }
    
    if (part.includes('*')) {
      return safeReadDir(basePath)
        .filter(entry => this.matchesPattern(entry, part))
        .map(entry => join(basePath, entry))
        .filter(safeIsFile);
    }
    
    // Literal path component
    return this.expandPattern(patternParts, join(basePath, part), index + 1);
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
   * Get TypeScript type name for entry type from configuration
   */
  private getEntryTypeName(entryTypeName: string): string {
    const entryType = this.config.entryTypes.find(et => et.name === entryTypeName);
    return entryType?.typeScriptTypeName ?? `${entryTypeName.charAt(0).toUpperCase() + entryTypeName.slice(1)}Entry`;
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