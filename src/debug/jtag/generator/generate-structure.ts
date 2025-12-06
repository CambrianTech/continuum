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
 * - widgets/        - Web component UI elements (chat, sidebar, training, etc.)
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

import { readFileSync } from 'fs';
import { relative } from 'path';
import { FileScanner } from './core/FileScanner';
import { EntryExtractor } from './core/EntryExtractor';
import type { EntryInfo, NameExtractor } from './core/EntryExtractor';
import { RegistryBuilder } from './core/RegistryBuilder';
import type { RegistryConfig, EntryTypeConfig } from './core/RegistryBuilder';

// ============================================================================
// UNIVERSAL MODULE STRUCTURE SCHEMA
// ============================================================================

/**
 * TypeScript schema for validating Universal Module Architecture compliance
 */
/* interface UniversalModuleStructure { // Currently unused
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
} */

/* interface UniversalModuleDirectory {
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
} */

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
    },
    
    {
      name: 'widget',
      pluralName: 'widgets',
      typeScriptTypeName: 'WidgetEntry',
      patterns: ['widgets/**/*Widget.ts'],
      nameExtraction: {
        type: 'regex',
        pattern: '^(.+?)Widget$'  // Extract widget name, removing Widget suffix
      },
      registryTemplate: {
        arrayName: '{ENV}_WIDGETS',
        entryTemplate: `{
    name: '{name}',
    className: '{className}',
    widgetClass: {className},
    tagName: '{name}'.replace(/([A-Z])/g, (match, p1, offset) => offset > 0 ? '-' + p1.toLowerCase() : p1.toLowerCase()) + '-widget'
  }`
      }
    }
  ],
  
  targets: [
    {
      environment: 'browser',
      outputFile: 'browser/generated.ts',
      entryTypes: ['daemon', 'command', 'adapter', 'widget'],
      typeImports: {
        'DaemonEntry': 'daemons/command-daemon/shared/DaemonBase',
        'CommandEntry': 'daemons/command-daemon/shared/CommandBase',
        'AdapterEntry': 'system/transports/shared/TransportBase',
        'WidgetEntry': 'widgets/shared/WidgetBase'
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
  private fileScanner: FileScanner;
  private entryExtractor: EntryExtractor;
  private registryBuilder: RegistryBuilder;

  constructor(rootPath: string, config?: GeneratorConfig) {
    this.rootPath = rootPath;
    this.config = config ?? STRUCTURE_CONFIG;
    this.fileScanner = new FileScanner(rootPath);
    this.entryExtractor = new EntryExtractor(rootPath);
    this.registryBuilder = new RegistryBuilder(rootPath);
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
    
    // Special case: widgets are browser-only components
    if (entryType.name === 'widget' && environment === 'browser') {
      return entryType.patterns; // Return all widget patterns for browser
    }
    
    return entryType.patterns.filter(pattern => 
      pattern.includes(`/${environment}/`) || pattern.includes(`*${envSuffix}.ts`)
    );
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
  ): EntryInfo[] {
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

    // Use FileScanner for file finding
    const files = this.fileScanner.findFiles(patterns, this.config.rootPatterns.exclude);
    const uniqueFiles = Array.from(new Set(files));

    // Create name extractor function
    const nameExtractor = this.createNameExtractor(entryType.nameExtraction);

    // Use EntryExtractor for entry extraction
    const entries = this.entryExtractor.extractMultiple(uniqueFiles, target.outputFile, nameExtractor, entryType.name);

    const validatedEntries = this.validateUniqueNames(entries, entryType.name);

    console.log(`   Found ${validatedEntries.length} ${entryType.pluralName}: ${validatedEntries.map(e => e.name).join(', ')}`);

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
      const allEntries = new Map<string, EntryInfo[]>();

      for (const entryTypeName of target.entryTypes) {
        const entries = this.processEntriesFromConfig(target, entryTypeName);
        allEntries.set(entryTypeName, entries);
      }

      // Build RegistryConfig for RegistryBuilder
      const registryConfig: RegistryConfig = {
        environment: target.environment,
        outputFile: target.outputFile,
        typeImports: target.typeImports,
        entryTypes: this.config.entryTypes.map(et => ({
          name: et.name,
          pluralName: et.pluralName,
          typeScriptTypeName: et.typeScriptTypeName,
          arrayName: et.registryTemplate.arrayName,
          entryTemplate: et.registryTemplate.entryTemplate
        }))
      };

      // Use RegistryBuilder to generate the registry file
      this.registryBuilder.generate(registryConfig, allEntries);
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