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
      if (!e.hasStaticAccessor && e.hasTypesFile && e.hasSpec) {
        fixableIssues.push('Missing static accessor (can regenerate Types from spec)');
      }
      if (!e.hasFactoryFunctions && e.hasTypesFile && e.hasSpec) {
        fixableIssues.push('Missing factory functions (can regenerate Types from spec)');
      }
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

    // Can only fix if we have a spec to regenerate from
    if (!entry.hasSpec || !entry.specPath) {
      return {
        name: entry.name,
        filesModified,
        filesCreated,
        issuesFixed,
        issuesRemaining: entry.issues,
      };
    }

    // Load the spec
    let spec: CommandSpec;
    try {
      spec = JSON.parse(fs.readFileSync(entry.specPath, 'utf-8'));
    } catch {
      return {
        name: entry.name,
        filesModified,
        filesCreated,
        issuesFixed,
        issuesRemaining: [`Failed to load spec: ${entry.specPath}`],
      };
    }

    // Fix: Missing or broken Types file (regenerate shared types)
    if (!entry.checks['static-accessor'] || !entry.checks['factory-functions']) {
      try {
        const { TemplateLoader } = require('../TemplateLoader');
        const rendered = TemplateLoader.renderCommand(spec);
        const className = spec.name.split('/').map((part: string) =>
          part.charAt(0).toUpperCase() + part.slice(1).toLowerCase()
        ).join('');

        const typesPath = path.join(entry.path, 'shared', `${className}Types.ts`);
        const dir = path.dirname(typesPath);
        if (!fs.existsSync(dir)) {
          fs.mkdirSync(dir, { recursive: true });
        }
        fs.writeFileSync(typesPath, rendered.sharedTypes, 'utf-8');

        if (!entry.checks['static-accessor']) {
          issuesFixed.push('Added static accessor pattern');
        }
        if (!entry.checks['factory-functions']) {
          issuesFixed.push('Added factory functions');
        }
        filesModified.push(typesPath);
      } catch (err) {
        issuesRemaining.push(`Failed to fix Types: ${err instanceof Error ? err.message : String(err)}`);
      }
    }

    // Fix: Missing README
    if (!entry.checks['has-readme']) {
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

    // Fix: Missing browser command
    if (!entry.checks['has-browser']) {
      try {
        const { TemplateLoader } = require('../TemplateLoader');
        const rendered = TemplateLoader.renderCommand(spec);
        const className = spec.name.split('/').map((part: string) =>
          part.charAt(0).toUpperCase() + part.slice(1).toLowerCase()
        ).join('');
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

    // Remaining issues that can't be auto-fixed
    for (const issue of entry.issues) {
      if (!issuesFixed.some(f => issue.toLowerCase().includes(f.toLowerCase().split(' ')[1]))) {
        if (!issuesRemaining.includes(issue) && !issuesFixed.some(f => f.includes(issue))) {
          // Check if this issue was already handled
          const handled = issuesFixed.some(fixed =>
            (issue.includes('accessor') && fixed.includes('accessor')) ||
            (issue.includes('factory') && fixed.includes('factory')) ||
            (issue.includes('README') && fixed.includes('README'))
          );
          if (!handled) {
            issuesRemaining.push(issue);
          }
        }
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
