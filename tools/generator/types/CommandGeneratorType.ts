/**
 * CommandGeneratorType — IGeneratorType implementation for commands
 *
 * Wraps CommandGenerator, CommandAuditor, and HelpFormatter into
 * the unified GeneratorSDK interface.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { CommandSpec } from '../CommandNaming';
import type {
  IGeneratorType,
  GeneratorAuditSummary,
  GeneratorAuditEntry,
  FixSummary,
  FixResult,
  GeneratorHelp
} from '../GeneratorSDK';
import type { GenerateOptions } from '../ModuleGenerator';
import { CommandGenerator } from '../CommandGenerator';
import { CommandAuditor } from '../CommandAuditor';
import { HelpFormatter } from '../HelpFormatter';
import { TypesFilePatcher } from '../core/TypesFilePatcher';
import { extractTypeInfo, generateFactoryPatches, generateAccessorPatches } from '../core/CommandFixerStrategies';

export class CommandGeneratorType implements IGeneratorType<CommandSpec> {
  readonly typeName = 'command';
  readonly description = 'Command modules (shared types, browser/server implementations, README, tests)';

  private readonly rootPath: string;
  private readonly generator: CommandGenerator;
  private readonly auditor: CommandAuditor;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
    this.generator = new CommandGenerator(rootPath);
    this.auditor = new CommandAuditor(rootPath);
  }

  // ── Generate ────────────────────────────────────────────────────

  generate(spec: CommandSpec, outputDir?: string, options?: GenerateOptions): void {
    this.generator.generate(spec, outputDir, options);
  }

  generateFromFile(specFilePath: string, outputDir?: string, options?: GenerateOptions): void {
    const specJson = fs.readFileSync(specFilePath, 'utf-8');
    const spec: CommandSpec = JSON.parse(specJson);
    this.generator.generate(spec, outputDir, options);
  }

  // ── Audit ───────────────────────────────────────────────────────

  audit(): GeneratorAuditSummary {
    const raw = this.auditor.audit();

    const checkSummary: Record<string, { passing: number; failing: number }> = {
      'has-spec': { passing: 0, failing: 0 },
      'static-accessor': { passing: 0, failing: 0 },
      'factory-functions': { passing: 0, failing: 0 },
      'no-any-casts': { passing: 0, failing: 0 },
      'has-types-file': { passing: 0, failing: 0 },
      'has-server': { passing: 0, failing: 0 },
      'has-browser': { passing: 0, failing: 0 },
      'has-readme': { passing: 0, failing: 0 },
    };

    const entries: GeneratorAuditEntry[] = raw.entries.map(e => {
      const checks: Record<string, boolean> = {
        'has-spec': e.hasSpec,
        'static-accessor': e.hasStaticAccessor,
        'factory-functions': e.hasFactoryFunctions,
        'no-any-casts': e.anyCastCount === 0,
        'has-types-file': e.hasTypesFile,
        'has-server': e.hasServerFile,
        'has-browser': e.hasBrowserFile,
        'has-readme': e.hasReadme,
      };

      // Update summary counts
      for (const [checkName, passing] of Object.entries(checks)) {
        if (passing) {
          checkSummary[checkName].passing++;
        } else {
          checkSummary[checkName].failing++;
        }
      }

      // Determine fixable issues
      const fixableIssues: string[] = [];
      // Surgical patches (accessor/factory) work with just the Types file — no spec needed
      if (!e.hasStaticAccessor && e.hasTypesFile) {
        fixableIssues.push('Missing static accessor (can inject into existing Types)');
      }
      if (!e.hasFactoryFunctions && e.hasTypesFile) {
        fixableIssues.push('Missing factory functions (can inject into existing Types)');
      }
      // Creating files from scratch requires a spec
      if (!e.hasReadme && e.hasSpec) {
        fixableIssues.push('Missing README (can generate from spec)');
      }

      return {
        name: e.commandName,
        path: e.dirPath,
        hasSpec: e.hasSpec,
        specPath: e.specPath,
        checks,
        issues: e.issues,
        fixableIssues,
      };
    });

    return {
      type: this.typeName,
      entries,
      total: raw.totalCommands,
      withSpecs: raw.withSpecs,
      totalIssues: raw.entries.reduce((sum, e) => sum + e.issues.length, 0),
      totalFixable: entries.reduce((sum, e) => sum + e.fixableIssues.length, 0),
      orphanedSpecs: raw.orphanedSpecs,
      checkSummary,
    };
  }

  auditOne(modulePath: string): GeneratorAuditEntry {
    const summary = this.audit();
    // Find by path or name
    const entry = summary.entries.find(e =>
      e.path === modulePath || e.name === modulePath
    );
    if (!entry) {
      return {
        name: modulePath,
        path: modulePath,
        hasSpec: false,
        checks: {},
        issues: [`Module not found: ${modulePath}`],
        fixableIssues: [],
      };
    }
    return entry;
  }

  // ── Fix ─────────────────────────────────────────────────────────

  fixAll(): FixSummary {
    const audit = this.audit();
    const results: FixResult[] = [];

    for (const entry of audit.entries) {
      if (entry.fixableIssues.length === 0) continue;
      const result = this.fixOne(entry.path);
      if (result.issuesFixed.length > 0) {
        results.push(result);
      }
    }

    return {
      type: this.typeName,
      results,
      totalFixed: results.reduce((sum, r) => sum + r.issuesFixed.length, 0),
      totalRemaining: results.reduce((sum, r) => sum + r.issuesRemaining.length, 0),
    };
  }

  fixOne(modulePath: string): FixResult {
    const entry = this.auditOne(modulePath);
    const filesModified: string[] = [];
    const filesCreated: string[] = [];
    const issuesFixed: string[] = [];
    const issuesRemaining: string[] = [];

    // ── Surgical patches (no spec required) ────────────────────────
    // When a Types file exists but is missing accessor/factory, we can
    // extract type info directly from the file and inject code.

    if (entry.checks['has-types-file'] &&
        (!entry.checks['static-accessor'] || !entry.checks['factory-functions'])) {
      const sharedDir = path.join(entry.path, 'shared');
      const typesFiles = fs.readdirSync(sharedDir).filter((f: string) => f.endsWith('Types.ts'));

      if (typesFiles.length > 0) {
        const typesPath = path.join(sharedDir, typesFiles[0]);
        const typesContent = fs.readFileSync(typesPath, 'utf-8');
        const typeInfo = extractTypeInfo(typesContent, entry.name);

        if (typeInfo) {
          const patches: import('../core/TypesFilePatcher').PatchOperation[] = [];

          if (!entry.checks['factory-functions']) {
            patches.push(...generateFactoryPatches(typeInfo, typesContent));
          }
          if (!entry.checks['static-accessor']) {
            patches.push(...generateAccessorPatches(typeInfo, typesContent));
          }

          if (patches.length > 0) {
            const patchResult = TypesFilePatcher.patchFile(typesPath, patches);

            if (patchResult.applied.length > 0) {
              filesModified.push(typesPath);
              for (const applied of patchResult.applied) {
                issuesFixed.push(applied);
              }
            }
            for (const error of patchResult.errors) {
              issuesRemaining.push(error);
            }
          }
        } else {
          if (!entry.checks['static-accessor']) {
            issuesRemaining.push('Missing static accessor — non-standard Types file, add manually');
          }
          if (!entry.checks['factory-functions']) {
            issuesRemaining.push('Missing factory functions — non-standard Types file, add manually');
          }
        }
      }
    }

    // ── Spec-dependent fixes ───────────────────────────────────────
    // Creating files from scratch requires a spec to generate from.

    let spec: CommandSpec | undefined;
    if (entry.hasSpec && entry.specPath) {
      try {
        spec = JSON.parse(fs.readFileSync(entry.specPath, 'utf-8'));
      } catch {
        issuesRemaining.push(`Failed to load spec: ${entry.specPath}`);
      }
    }

    // Create Types file from spec if none exists
    if (!entry.checks['has-types-file'] && spec) {
      try {
        const { TemplateLoader } = require('../TemplateLoader');
        const rendered = TemplateLoader.renderCommand(spec);
        const className = spec.name
          .split(/[\/\-_]/)
          .map((part: string) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
          .join('');

        const typesPath = path.join(entry.path, 'shared', `${className}Types.ts`);
        const dir = path.dirname(typesPath);
        if (!fs.existsSync(dir)) {
          fs.mkdirSync(dir, { recursive: true });
        }
        fs.writeFileSync(typesPath, rendered.sharedTypes, 'utf-8');
        issuesFixed.push('Created missing Types file from spec');
        filesCreated.push(typesPath);
      } catch (err) {
        issuesRemaining.push(`Failed to create Types: ${err instanceof Error ? err.message : String(err)}`);
      }
    }

    // Generate README from spec
    if (!entry.checks['has-readme'] && spec) {
      try {
        const { TemplateLoader } = require('../TemplateLoader');
        const rendered = TemplateLoader.renderCommand(spec);
        const readmePath = path.join(entry.path, 'README.md');
        fs.writeFileSync(readmePath, rendered.readme, 'utf-8');
        issuesFixed.push('Generated README from spec');
        filesCreated.push(readmePath);
      } catch (err) {
        issuesRemaining.push(`Failed to generate README: ${err instanceof Error ? err.message : String(err)}`);
      }
    }

    // Generate browser command from spec
    if (!entry.checks['has-browser'] && spec) {
      try {
        const { TemplateLoader } = require('../TemplateLoader');
        const rendered = TemplateLoader.renderCommand(spec);
        const className = spec.name
          .split(/[\/\-_]/)
          .map((part: string) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
          .join('');
        const browserPath = path.join(entry.path, 'browser', `${className}BrowserCommand.ts`);
        const dir = path.dirname(browserPath);
        if (!fs.existsSync(dir)) {
          fs.mkdirSync(dir, { recursive: true });
        }
        fs.writeFileSync(browserPath, rendered.browser, 'utf-8');
        issuesFixed.push('Generated browser command from spec');
        filesCreated.push(browserPath);
      } catch (err) {
        issuesRemaining.push(`Failed to generate browser command: ${err instanceof Error ? err.message : String(err)}`);
      }
    }

    // Any original issues not addressed by fixes or already in remaining
    const handledKeywords = issuesFixed.concat(issuesRemaining).join(' ').toLowerCase();
    for (const issue of entry.issues) {
      const key = issue.toLowerCase();
      const alreadyTracked = issuesFixed.some(f => f.toLowerCase().includes(key.split(' ')[0])) ||
        issuesRemaining.includes(issue);
      if (!alreadyTracked && !handledKeywords.includes(key.split(' ')[0])) {
        issuesRemaining.push(issue);
      }
    }

    return {
      name: entry.name,
      filesModified,
      filesCreated,
      issuesFixed,
      issuesRemaining,
    };
  }

  // ── Reverse Engineer ────────────────────────────────────────────

  reverseEngineer(modulePath: string): CommandSpec | null {
    return this.auditor.reverseEngineer(modulePath) as CommandSpec | null;
  }

  // ── Help & Templates ────────────────────────────────────────────

  help(): GeneratorHelp {
    return {
      full: HelpFormatter.fullHelp(),
      short: HelpFormatter.shortHelp(),
      topics: {
        spec: HelpFormatter.topicHelp('spec'),
        types: HelpFormatter.topicHelp('types'),
        examples: HelpFormatter.topicHelp('examples'),
        audit: HelpFormatter.topicHelp('audit'),
        workflow: HelpFormatter.topicHelp('workflow'),
      },
      availableTopics: ['spec', 'types', 'examples', 'audit', 'workflow'],
    };
  }

  templateSpec(variant: string = 'standard'): CommandSpec {
    return HelpFormatter.templateSpec(variant) as CommandSpec;
  }

  templateVariants(): string[] {
    return ['minimal', 'standard', 'rust-ipc', 'browser-only'];
  }
}
