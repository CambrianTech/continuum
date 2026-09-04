/**
 * DaemonGeneratorType — IGeneratorType implementation for daemons
 *
 * Provides audit, fix, reverse-engineer, and help for daemon modules.
 * Daemons have shared/browser/server layers like commands.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { DaemonSpec, DaemonJob } from '../DaemonTypes';
import type {
  IGeneratorType,
  GeneratorAuditSummary,
  GeneratorAuditEntry,
  FixSummary,
  FixResult,
  GeneratorHelp
} from '../GeneratorSDK';
import type { GenerateOptions } from '../ModuleGenerator';
import { DaemonGenerator } from '../DaemonGenerator';

export class DaemonGeneratorType implements IGeneratorType<DaemonSpec> {
  readonly typeName = 'daemon';
  readonly description = 'Daemon services with jobs, events, and lifecycle hooks';

  private readonly rootPath: string;
  private readonly generator: DaemonGenerator;
  private readonly daemonsDir: string;
  private readonly specsDir: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
    this.generator = new DaemonGenerator(rootPath);
    this.daemonsDir = path.join(rootPath, 'daemons');
    this.specsDir = path.join(rootPath, 'generator', 'specs', 'daemons');
  }

  // ── Generate ────────────────────────────────────────────────────

  generate(spec: DaemonSpec, outputDir?: string, options?: GenerateOptions): void {
    const dir = outputDir || path.join(this.daemonsDir, spec.name);
    this.generator.generate(spec, dir, options);
  }

  generateFromFile(specFilePath: string, outputDir?: string, options?: GenerateOptions): void {
    const spec: DaemonSpec = JSON.parse(fs.readFileSync(specFilePath, 'utf-8'));
    this.generate(spec, outputDir, options);
  }

  // ── Audit ───────────────────────────────────────────────────────

  audit(): GeneratorAuditSummary {
    const daemonDirs = this.discoverDaemons();
    const specMap = this.loadSpecMap();
    const entries: GeneratorAuditEntry[] = [];

    const checkSummary: Record<string, { passing: number; failing: number }> = {
      'has-spec': { passing: 0, failing: 0 },
      'has-shared': { passing: 0, failing: 0 },
      'has-browser': { passing: 0, failing: 0 },
      'has-server': { passing: 0, failing: 0 },
      'extends-daemon-base': { passing: 0, failing: 0 },
      'has-handle-message': { passing: 0, failing: 0 },
      'no-any-casts': { passing: 0, failing: 0 },
    };

    for (const { name, dirPath } of daemonDirs) {
      const hasSpec = name in specMap;
      const sharedFile = this.findSharedFile(dirPath);
      const sharedContent = sharedFile ? fs.readFileSync(sharedFile, 'utf-8') : '';

      const checks: Record<string, boolean> = {
        'has-spec': hasSpec,
        'has-shared': !!sharedFile,
        'has-browser': fs.existsSync(path.join(dirPath, 'browser')) &&
          fs.readdirSync(path.join(dirPath, 'browser')).some(f => f.endsWith('.ts')),
        'has-server': fs.existsSync(path.join(dirPath, 'server')) &&
          fs.readdirSync(path.join(dirPath, 'server')).some(f => f.endsWith('.ts')),
        'extends-daemon-base': /extends\s+DaemonBase/.test(sharedContent),
        'has-handle-message': /handleMessage/.test(sharedContent),
        'no-any-casts': !/(:\s*any\b|as\s+any\b)/.test(sharedContent),
      };

      const issues: string[] = [];
      const fixableIssues: string[] = [];

      if (!checks['has-spec']) issues.push('No matching daemon spec');
      if (!checks['has-shared']) issues.push('Missing shared base class');
      if (!checks['has-browser']) issues.push('Missing browser implementation');
      if (!checks['has-server']) issues.push('Missing server implementation');
      if (!checks['extends-daemon-base']) issues.push('Does not extend DaemonBase');
      if (!checks['has-handle-message']) issues.push('Missing handleMessage method');
      if (!checks['no-any-casts']) issues.push('Contains any casts');

      for (const [checkName, passing] of Object.entries(checks)) {
        if (passing) checkSummary[checkName].passing++;
        else checkSummary[checkName].failing++;
      }

      entries.push({
        name,
        path: dirPath,
        hasSpec,
        specPath: hasSpec ? specMap[name] : undefined,
        checks,
        issues,
        fixableIssues,
      });
    }

    const daemonNames = new Set(daemonDirs.map(d => d.name));
    const orphanedSpecs = Object.keys(specMap).filter(name => !daemonNames.has(name));

    return {
      type: this.typeName,
      entries,
      total: daemonDirs.length,
      withSpecs: entries.filter(e => e.hasSpec).length,
      totalIssues: entries.reduce((sum, e) => sum + e.issues.length, 0),
      totalFixable: entries.reduce((sum, e) => sum + e.fixableIssues.length, 0),
      orphanedSpecs,
      checkSummary,
    };
  }

  auditOne(modulePath: string): GeneratorAuditEntry {
    const summary = this.audit();
    const entry = summary.entries.find(e =>
      e.path === modulePath || e.name === modulePath
    );
    if (!entry) {
      return {
        name: modulePath, path: modulePath, hasSpec: false,
        checks: {}, issues: [`Daemon not found: ${modulePath}`], fixableIssues: [],
      };
    }
    return entry;
  }

  // ── Fix ─────────────────────────────────────────────────────────

  fixAll(): FixSummary {
    // Daemons have too much custom logic to auto-fix safely
    // Return empty — manual review required
    return {
      type: this.typeName,
      results: [],
      totalFixed: 0,
      totalRemaining: 0,
    };
  }

  fixOne(_modulePath: string): FixResult {
    return {
      name: _modulePath,
      filesModified: [],
      filesCreated: [],
      issuesFixed: [],
      issuesRemaining: ['Daemon auto-fix not yet implemented — daemons have too much custom logic'],
    };
  }

  // ── Reverse Engineer ────────────────────────────────────────────

  reverseEngineer(modulePath: string): DaemonSpec | null {
    const absDir = path.isAbsolute(modulePath) ? modulePath :
      path.join(this.daemonsDir, modulePath);

    const sharedFile = this.findSharedFile(absDir);
    if (!sharedFile) return null;

    const content = fs.readFileSync(sharedFile, 'utf-8');

    // Extract daemon name from subpath
    const subpathMatch = content.match(/subpath.*?=\s*'([^']+)'/);
    const name = subpathMatch?.[1] || path.basename(absDir);

    // Extract description from file header
    const descMatch = content.match(/\*\s+(\w.+?)\s*\n/);
    const description = descMatch?.[1]?.replace(/Daemon\s*-?\s*/i, '').trim() || 'TODO';

    // Extract job methods
    const jobs: DaemonJob[] = [];
    const jobRegex = /(?:async\s+)?(\w+)\s*\(([^)]*)\)\s*:\s*(Promise<)?(\w+)/g;
    let jobMatch: RegExpExecArray | null;

    while ((jobMatch = jobRegex.exec(content)) !== null) {
      const [, methodName, paramsStr, isAsync, returnType] = jobMatch;
      if (['constructor', 'initialize', 'handleMessage', 'shutdown', 'onStart', 'onStop'].includes(methodName)) continue;
      if (methodName.startsWith('_')) continue;

      const params = paramsStr.split(',').filter(p => p.trim()).map(p => {
        const [pName, pType] = p.split(':').map(s => s.trim());
        return { name: pName, type: pType || 'unknown' };
      });

      jobs.push({
        name: methodName,
        params,
        returns: returnType,
        async: !!isAsync,
      });
    }

    return { name, description, jobs };
  }

  // ── Help & Templates ────────────────────────────────────────────

  help(): GeneratorHelp {
    const full = `
DAEMON GENERATOR
================

Generates daemon services with the three-layer pattern: shared/browser/server.

SPEC FORMAT (DaemonSpec):
  {
    "name": "my-service",                     // kebab-case
    "description": "Manages something",
    "jobs": [
      {
        "name": "processItem",
        "params": [{ "name": "itemId", "type": "string" }],
        "returns": "boolean",
        "async": true,
        "description": "Process a single item"
      }
    ],
    "events": [
      {
        "name": "item-processed",
        "payload": { "itemId": "string", "success": "boolean" },
        "description": "Fired after processing"
      }
    ],
    "lifecycle": {
      "onStart": "Initialize connections",
      "onStop": "Close connections"
    }
  }

GENERATED OUTPUT:
  daemons/{name}/
    shared/{ClassName}.ts    - Abstract base with job routing
    browser/{ClassName}Browser.ts  - Browser implementation
    server/{ClassName}Server.ts    - Server implementation

WORKFLOW:
  1. Create spec: generator/specs/daemons/my-service.json
  2. Generate: npx tsx generator/DaemonGenerator.ts specs/daemons/my-service.json
  3. Implement job methods in server class
  4. Register daemon in DaemonRegistry
`;

    return {
      full,
      short: 'Usage: npx tsx generator/DaemonGenerator.ts <spec.json>',
      topics: { spec: full },
      availableTopics: ['spec'],
    };
  }

  templateSpec(_variant: string = 'standard'): DaemonSpec {
    return {
      name: 'example-service',
      description: 'Example daemon service',
      jobs: [
        {
          name: 'processItem',
          params: [{ name: 'itemId', type: 'string' }],
          returns: 'boolean',
          async: true,
          description: 'Process a single item',
        },
      ],
      events: [
        {
          name: 'item-processed',
          payload: { itemId: 'string', success: 'boolean' },
          description: 'Fired after processing',
        },
      ],
    };
  }

  templateVariants(): string[] {
    return ['standard'];
  }

  // ── Private Helpers ─────────────────────────────────────────────

  private discoverDaemons(): Array<{ name: string; dirPath: string }> {
    const results: Array<{ name: string; dirPath: string }> = [];
    if (!fs.existsSync(this.daemonsDir)) return results;

    const entries = fs.readdirSync(this.daemonsDir, { withFileTypes: true });
    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      if (entry.name === 'node_modules') continue;

      const dirPath = path.join(this.daemonsDir, entry.name);
      // A daemon directory has shared/ or server/
      const hasShared = fs.existsSync(path.join(dirPath, 'shared'));
      const hasServer = fs.existsSync(path.join(dirPath, 'server'));
      if (hasShared || hasServer) {
        results.push({ name: entry.name, dirPath });
      }
    }

    return results;
  }

  private loadSpecMap(): Record<string, string> {
    const map: Record<string, string> = {};
    if (!fs.existsSync(this.specsDir)) return map;

    const files = fs.readdirSync(this.specsDir).filter(f => f.endsWith('.json'));
    for (const file of files) {
      const fullPath = path.join(this.specsDir, file);
      try {
        const spec: DaemonSpec = JSON.parse(fs.readFileSync(fullPath, 'utf-8'));
        if (spec.name) map[spec.name] = fullPath;
      } catch { /* skip */ }
    }

    return map;
  }

  private findSharedFile(dirPath: string): string | null {
    const sharedDir = path.join(dirPath, 'shared');
    if (!fs.existsSync(sharedDir)) return null;

    const files = fs.readdirSync(sharedDir).filter(f =>
      f.endsWith('.ts') && !f.includes('.test.') && !f.includes('Types')
    );

    // Prefer the main class file (not Events or Types)
    const mainFile = files.find(f => !f.includes('Events'));
    return mainFile ? path.join(sharedDir, mainFile) : files[0] ? path.join(sharedDir, files[0]) : null;
  }
}
