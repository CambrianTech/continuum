/**
 * CommandAuditor - Scan all commands and report conformance
 *
 * Checks every command directory for:
 * - Matching generator spec
 * - Static accessor (Name.execute pattern)
 * - Factory functions (createParams, createResult)
 * - Type violations (any casts, untyped params)
 */

import * as fs from 'fs';
import * as path from 'path';

export interface AuditEntry {
  commandName: string;
  dirPath: string;
  hasSpec: boolean;
  specPath?: string;
  hasStaticAccessor: boolean;
  hasFactoryFunctions: boolean;
  anyCastCount: number;
  hasTypesFile: boolean;
  hasServerFile: boolean;
  hasBrowserFile: boolean;
  hasReadme: boolean;
  issues: string[];
}

export interface AuditSummary {
  entries: AuditEntry[];
  totalCommands: number;
  withSpecs: number;
  missingAccessors: number;
  missingFactories: number;
  totalAnyCasts: number;
  commandsWithAny: number;
  orphanedSpecs: string[];
}

export class CommandAuditor {
  private readonly rootPath: string;
  private readonly commandsDir: string;
  private readonly specsDir: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
    this.commandsDir = path.join(rootPath, 'commands');
    this.specsDir = path.join(rootPath, 'generator', 'specs');
  }

  /**
   * Run full audit and return structured results
   */
  audit(): AuditSummary {
    const commandDirs = this.discoverCommandDirs();
    const specMap = this.loadSpecMap();
    const entries: AuditEntry[] = [];

    for (const { commandName, dirPath } of commandDirs) {
      const entry = this.auditCommand(commandName, dirPath, specMap);
      entries.push(entry);
    }

    // Find orphaned specs (spec exists but no command dir)
    const commandNames = new Set(entries.map(e => e.commandName));
    const orphanedSpecs = Object.keys(specMap).filter(name => !commandNames.has(name));

    // Sort: issues first, then alphabetical
    entries.sort((a, b) => {
      const aIssues = a.issues.length;
      const bIssues = b.issues.length;
      if (aIssues !== bIssues) return bIssues - aIssues;
      return a.commandName.localeCompare(b.commandName);
    });

    return {
      entries,
      totalCommands: entries.length,
      withSpecs: entries.filter(e => e.hasSpec).length,
      missingAccessors: entries.filter(e => !e.hasStaticAccessor).length,
      missingFactories: entries.filter(e => !e.hasFactoryFunctions).length,
      totalAnyCasts: entries.reduce((sum, e) => sum + e.anyCastCount, 0),
      commandsWithAny: entries.filter(e => e.anyCastCount > 0).length,
      orphanedSpecs
    };
  }

  /**
   * Print audit results to console
   */
  printAudit(): void {
    const summary = this.audit();

    console.log('\n╔══════════════════════════════════════════════════════════════╗');
    console.log('║                   COMMAND AUDIT REPORT                       ║');
    console.log('╚══════════════════════════════════════════════════════════════╝\n');

    // Print each entry
    for (const entry of summary.entries) {
      const icon = entry.issues.length === 0 ? '  ' :
                   entry.anyCastCount > 0 ? '  ' : '  ';

      const specStr = entry.hasSpec ? 'spec' : 'NO spec';
      const accessorStr = entry.hasStaticAccessor ? 'accessor' : 'NO accessor';
      const factoryStr = entry.hasFactoryFunctions ? 'factories' : 'NO factories';
      const anyStr = entry.anyCastCount > 0 ? `, any: ${entry.anyCastCount}` : '';

      console.log(`${icon}${entry.commandName.padEnd(35)} ${specStr.padEnd(10)} ${accessorStr.padEnd(14)} ${factoryStr}${anyStr}`);
    }

    // Orphaned specs
    if (summary.orphanedSpecs.length > 0) {
      console.log('\n  Orphaned specs (no command directory):');
      for (const spec of summary.orphanedSpecs) {
        console.log(`    ${spec}`);
      }
    }

    // Summary
    console.log('\n────────────────────────────────────────────────────');
    console.log(`  Commands:           ${summary.totalCommands}`);
    console.log(`  With specs:         ${summary.withSpecs} (${Math.round(summary.withSpecs / summary.totalCommands * 100)}%)`);
    console.log(`  Missing accessors:  ${summary.missingAccessors}`);
    console.log(`  Missing factories:  ${summary.missingFactories}`);
    console.log(`  any casts in Types: ${summary.totalAnyCasts} across ${summary.commandsWithAny} commands`);
    if (summary.orphanedSpecs.length > 0) {
      console.log(`  Orphaned specs:     ${summary.orphanedSpecs.length}`);
    }
    console.log('────────────────────────────────────────────────────\n');
  }

  /**
   * Reverse-engineer a spec from an existing command directory.
   * Reads the Types file, extracts params/results, outputs JSON.
   */
  reverseEngineer(commandDir: string): object | null {
    const absDir = path.isAbsolute(commandDir) ? commandDir : path.join(this.rootPath, commandDir);

    if (!fs.existsSync(absDir)) {
      console.error(`Directory not found: ${absDir}`);
      return null;
    }

    // Find shared Types file
    const sharedDir = path.join(absDir, 'shared');
    if (!fs.existsSync(sharedDir)) {
      console.error(`No shared/ directory in: ${absDir}`);
      return null;
    }

    const typesFiles = fs.readdirSync(sharedDir).filter(f => f.endsWith('Types.ts'));
    if (typesFiles.length === 0) {
      console.error(`No *Types.ts file in: ${sharedDir}`);
      return null;
    }

    const typesContent = fs.readFileSync(path.join(sharedDir, typesFiles[0]), 'utf-8');

    // Extract command name from commandName constant
    const commandNameMatch = typesContent.match(/commandName:\s*'([^']+)'/);
    const name = commandNameMatch?.[1] || this.dirToCommandName(absDir);

    // Extract params from interface
    const params = this.extractInterfaceFields(typesContent, 'Params');

    // Extract results from interface
    const results = this.extractInterfaceFields(typesContent, 'Result');

    // Extract description from file header comment (skip pure asterisk lines)
    const descMatch = typesContent.match(/\*\s+(\w.+?)\s*\n/);
    let description = descMatch?.[1]?.replace(/Command\s*-?\s*Shared\s*Types\s*-?\s*/i, '').trim() || '';
    // Second pass: look for the description line after the command name
    if (!description || description === '*') {
      const descLine = typesContent.match(/\*\s+\w+.*?-\s*(.+)/);
      description = descLine?.[1]?.trim() || 'TODO: Add description';
    }

    // Check for access level
    const accessLevel = typesContent.includes('ai-safe') ? 'ai-safe' :
                        typesContent.includes('dangerous') ? 'dangerous' :
                        typesContent.includes('system') ? 'system' : 'internal';

    return {
      name,
      description,
      params,
      results,
      examples: [
        {
          description: 'Basic usage',
          command: `./jtag ${name}`,
          expectedResult: 'TODO: Add expected result'
        }
      ],
      accessLevel
    };
  }

  // ── Private helpers ──────────────────────────────────────────────

  /**
   * Discover all command directories recursively.
   * A command dir has a shared/ or server/ subdirectory.
   */
  private discoverCommandDirs(): Array<{ commandName: string; dirPath: string }> {
    const results: Array<{ commandName: string; dirPath: string }> = [];

    const walk = (dir: string, prefix: string) => {
      if (!fs.existsSync(dir)) return;
      const entries = fs.readdirSync(dir, { withFileTypes: true });

      for (const entry of entries) {
        if (!entry.isDirectory()) continue;
        if (entry.name === 'node_modules' || entry.name === 'test' || entry.name === '.backup') continue;

        const fullPath = path.join(dir, entry.name);
        const commandName = prefix ? `${prefix}/${entry.name}` : entry.name;

        // Is this a command directory? (has shared/ or server/)
        const hasShared = fs.existsSync(path.join(fullPath, 'shared'));
        const hasServer = fs.existsSync(path.join(fullPath, 'server'));

        if (hasShared || hasServer) {
          results.push({ commandName, dirPath: fullPath });
        }
        // Always recurse — nested commands (development/generate/audit) live
        // inside parent commands (development/generate) that also have shared/
        if (entry.name !== 'shared' && entry.name !== 'server' && entry.name !== 'browser') {
          walk(fullPath, commandName);
        }
      }
    };

    walk(this.commandsDir, '');
    return results;
  }

  /**
   * Load all spec files and map them by command name
   */
  private loadSpecMap(): Record<string, string> {
    const map: Record<string, string> = {};
    if (!fs.existsSync(this.specsDir)) return map;

    const files = fs.readdirSync(this.specsDir).filter(f => f.endsWith('.json'));
    for (const file of files) {
      const fullPath = path.join(this.specsDir, file);
      try {
        const content = fs.readFileSync(fullPath, 'utf-8');
        const spec = JSON.parse(content);
        if (spec.name) {
          map[spec.name] = fullPath;
        }
      } catch {
        // Skip malformed specs
      }
    }

    return map;
  }

  /**
   * Audit a single command directory
   */
  private auditCommand(commandName: string, dirPath: string, specMap: Record<string, string>): AuditEntry {
    const issues: string[] = [];
    const hasSpec = commandName in specMap;

    // Find Types file
    const sharedDir = path.join(dirPath, 'shared');
    let typesContent = '';
    let hasTypesFile = false;
    if (fs.existsSync(sharedDir)) {
      const typesFiles = fs.readdirSync(sharedDir).filter(f => f.endsWith('Types.ts'));
      if (typesFiles.length > 0) {
        hasTypesFile = true;
        typesContent = fs.readFileSync(path.join(sharedDir, typesFiles[0]), 'utf-8');
      }
    }

    // Check static accessor
    const hasStaticAccessor = typesContent.includes('.execute(') && typesContent.includes('commandName:');

    // Check factory functions
    const hasFactoryFunctions = typesContent.includes('createPayload') || typesContent.includes('create') && typesContent.includes('Params');

    // Count any casts
    const anyMatches = typesContent.match(/:\s*any\b|as\s+any\b/g);
    const anyCastCount = anyMatches?.length || 0;

    // Check other files
    const hasServerFile = fs.existsSync(path.join(dirPath, 'server')) &&
      fs.readdirSync(path.join(dirPath, 'server')).some(f => f.endsWith('.ts'));
    const hasBrowserFile = fs.existsSync(path.join(dirPath, 'browser')) &&
      fs.readdirSync(path.join(dirPath, 'browser')).some(f => f.endsWith('.ts'));
    const hasReadme = fs.existsSync(path.join(dirPath, 'README.md'));

    // Record issues
    if (!hasSpec) issues.push('Missing generator spec');
    if (!hasStaticAccessor && hasTypesFile) issues.push('Missing static accessor');
    if (!hasFactoryFunctions && hasTypesFile) issues.push('Missing factory functions');
    if (anyCastCount > 0) issues.push(`${anyCastCount} any cast(s) in Types`);
    if (!hasTypesFile) issues.push('No Types file');

    return {
      commandName,
      dirPath,
      hasSpec,
      specPath: specMap[commandName],
      hasStaticAccessor,
      hasFactoryFunctions,
      anyCastCount,
      hasTypesFile,
      hasServerFile,
      hasBrowserFile,
      hasReadme,
      issues
    };
  }

  /**
   * Extract fields from a TypeScript interface in file content
   */
  private extractInterfaceFields(content: string, interfaceSuffix: string): Array<{ name: string; type: string; optional?: boolean; description?: string }> {
    const fields: Array<{ name: string; type: string; optional?: boolean; description?: string }> = [];

    // Find the interface block
    const interfaceRegex = new RegExp(`interface\\s+\\w+${interfaceSuffix}[^{]*\\{([^}]+)\\}`, 's');
    const match = content.match(interfaceRegex);
    if (!match) return fields;

    const body = match[1];
    // Match field lines: name?: type; with optional comment
    const fieldRegex = /(?:\/\/\s*(.+)\n\s*)?(\w+)(\?)?\s*:\s*([^;\n]+);/g;
    let fieldMatch: RegExpExecArray | null;

    while ((fieldMatch = fieldRegex.exec(body)) !== null) {
      const [, comment, name, optional, type] = fieldMatch;
      // Skip inherited fields
      if (['context', 'sessionId', 'userId', 'success', 'error', '_noParams'].includes(name)) continue;

      fields.push({
        name,
        type: type.trim(),
        ...(optional ? { optional: true } : {}),
        ...(comment ? { description: comment.trim() } : {})
      });
    }

    return fields;
  }

  private dirToCommandName(absDir: string): string {
    const relative = path.relative(this.commandsDir, absDir);
    return relative.replace(/\\/g, '/');
  }
}
