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
    const unifiedConfigPath = join(this.rootPath, 'unified-config.json');
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
    const found: string[] = [];
    
    for (const pattern of patterns) {
      // Simple pattern matching - look for files matching the pattern structure
      const parts = pattern.split('/');
      const searchPaths = this.expandPattern(parts, this.rootPath);
      
      for (const searchPath of searchPaths) {
        if (existsSync(searchPath) && !this.isExcluded(searchPath, excludePatterns)) {
          found.push(searchPath);
        }
      }
    }
    
    return found.sort();
  }

  private expandPattern(patternParts: string[], basePath: string, index = 0): string[] {
    if (index >= patternParts.length) {
      return [basePath];
    }
    
    const part = patternParts[index];
    const results: string[] = [];
    
    if (part === '**') {
      // Double wildcard - recursive directory matching
      try {
        // First, try continuing from current directory (zero directories matched)
        results.push(...this.expandPattern(patternParts, basePath, index + 1));
        
        // Then recursively descend into subdirectories
        const entries = readdirSync(basePath);
        for (const entry of entries) {
          const fullPath = join(basePath, entry);
          if (statSync(fullPath).isDirectory()) {
            // Continue with ** (stay at the same pattern index)
            results.push(...this.expandPattern(patternParts, fullPath, index));
          }
        }
      } catch (e) {
        // Directory doesn't exist or can't be read
      }
    } else if (part === '*') {
      // Single wildcard - check all directories at this level
      try {
        const entries = readdirSync(basePath);
        for (const entry of entries) {
          const fullPath = join(basePath, entry);
          if (statSync(fullPath).isDirectory()) {
            results.push(...this.expandPattern(patternParts, fullPath, index + 1));
          }
        }
      } catch (e) {
        // Directory doesn't exist or can't be read
      }
    } else if (part.includes('*')) {
      // Pattern matching (e.g., *Browser.ts)
      try {
        const entries = readdirSync(basePath);
        for (const entry of entries) {
          const fullPath = join(basePath, entry);
          if (this.matchesPattern(entry, part) && statSync(fullPath).isFile()) {
            results.push(fullPath);
          }
        }
      } catch (e) {
        // Directory doesn't exist or can't be read
      }
    } else {
      // Literal path component
      const fullPath = join(basePath, part);
      results.push(...this.expandPattern(patternParts, fullPath, index + 1));
    }
    
    return results;
  }

  private matchesPattern(filename: string, pattern: string): boolean {
    // Simple pattern matching - convert * to regex
    const regexPattern = pattern.replace(/\*/g, '.*');
    const regex = new RegExp(`^${regexPattern}$`);
    return regex.test(filename);
  }

  private extractDaemonInfo(filePath: string): DaemonEntry | null {
    try {
      const content = readFileSync(filePath, 'utf8');
      const relativePath = relative(this.rootPath, filePath);
      
      // Extract class name from filename (e.g., CommandDaemonBrowser.ts -> CommandDaemonBrowser)
      const filename = filePath.split('/').pop()!;
      const className = filename.replace('.ts', '');
      
      // Extract daemon name (e.g., CommandDaemonBrowser -> CommandDaemon)
      const daemonName = className.replace(/(Browser|Server)$/, '');
      
      // Check if file exports the expected class
      if (!content.includes(`export class ${className}`)) {
        return null;
      }
      
      // Generate proper import path
      const importPath = relativePath.replace('.ts', '').replace(/\\/g, '/');
      
      return {
        name: daemonName,
        className,
        importPath: `../${importPath}`
      };
    } catch (e) {
      console.warn(`Ignoring daemon info from ${filePath}:`, e);
      return null;
    }
  }

  private extractCommandInfo(filePath: string, baseDir?: string): CommandEntry | null {
    try {
      const content = readFileSync(filePath, 'utf8');
      const relativePath = relative(this.rootPath, filePath);
      
      // Extract class name from filename
      const filename = filePath.split('/').pop()!;
      const className = filename.replace('.ts', '');
      
      // Extract command name from path structure
      // e.g., daemons/command-daemon/commands/screenshot/browser/ScreenshotBrowserCommand.ts -> screenshot
      const pathParts = relativePath.split('/');
      const commandIndex = pathParts.indexOf('commands');
      let commandName = '';
      
      if (commandIndex !== -1 && commandIndex + 1 < pathParts.length) {
        const commandParts = pathParts.slice(commandIndex + 1);
        // Handle nested commands (e.g., chat/send-message)
        if (commandParts.length > 2) {
          commandName = commandParts.slice(0, -2).join('/');
        } else {
          commandName = commandParts[0];
        }
      }
      
      // Check if file exports the expected class
      if (!content.includes(`export class ${className}`)) {
        return null;
      }
      
      // Generate proper import path relative to the output file
      let importPath: string;
      if (baseDir && relativePath.startsWith('commands/')) {
        // For command daemon structure files, use relative path from daemon directory to commands
        const commandPath = relativePath.replace('commands/', '');
        importPath = `../../../commands/${commandPath.replace('.ts', '')}`;
      } else {
        // For root structure files, use full relative path
        const fullImportPath = relativePath.replace('.ts', '').replace(/\\/g, '/');
        importPath = `../${fullImportPath}`;
      }
      
      return {
        name: commandName,
        className,
        importPath
      };
    } catch (e) {
      return null;
    }
  }

  private generateStructureFile(
    outputPath: string,
    environment: 'browser' | 'server',
    daemons: DaemonEntry[],
    commands: CommandEntry[]
  ): void {
    const envCap = environment.charAt(0).toUpperCase() + environment.slice(1);
    const envUpper = environment.toUpperCase();
    
    // Determine relative path depth for import paths
    const pathDepth = outputPath.split('/').length - 1;
    const relativePath = '../'.repeat(pathDepth);
    
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
    
    // Type imports
    sections.push('\n// Types');
    if (daemons.length > 0) {
      sections.push(`import type { DaemonEntry } from '${relativePath}shared/DaemonBase';`);
    }
    if (commands.length > 0) {
      sections.push(`import type { CommandEntry } from '${relativePath}daemons/command-daemon/shared/CommandBase';`);
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


  public generate(): void {
    console.log('🏭 Intelligent Structure Generator');
    console.log('================================');
    console.log(`📍 Root path: ${this.rootPath}`);
    console.log(`📋 Directories configured: ${Object.keys(this.config.directories).join(', ')}`);
    
    for (const [dirName, dirConfig] of Object.entries(this.config.directories)) {
      console.log(`\n🔍 Processing ${dirName} (${dirConfig.environment})...`);
      
      
      // Find daemon files (if configured)
      let daemons: DaemonEntry[] = [];
      if (dirConfig.daemonPaths) {
        console.log(`   Searching for daemons: ${dirConfig.daemonPaths.join(', ')}`);
        const daemonFiles = this.findFiles(dirConfig.daemonPaths, dirConfig.excludePatterns);
        daemons = daemonFiles
          .map(f => this.extractDaemonInfo(f))
          .filter((d): d is DaemonEntry => d !== null);
        
        console.log(`   Found ${daemons.length} daemons: ${daemons.map(d => d.name).join(', ')}`);
      }
      
      // Find command files (if configured)
      let commands: CommandEntry[] = [];
      if (dirConfig.commandPaths) {
        console.log(`   Searching for commands: ${dirConfig.commandPaths.join(', ')}`);
        const commandFiles = this.findFiles(dirConfig.commandPaths, dirConfig.excludePatterns);
        const isCommandDaemonStructure = dirConfig.outputFile.includes('command-daemon');
        
        // Deduplicate command files by path
        const uniqueCommandFiles = Array.from(new Set(commandFiles));
        
        commands = uniqueCommandFiles
          .map(f => this.extractCommandInfo(f, isCommandDaemonStructure ? 'command-daemon' : undefined))
          .filter((c): c is CommandEntry => c !== null);
        
        console.log(`   Found ${commands.length} commands: ${commands.map(c => c.name).join(', ')}`);
      }
      
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