/**
 * GeneratorSDK - Universal interface for all generator types
 *
 * Every generator type (command, entity, daemon, widget) implements IGeneratorType<TSpec>.
 * This gives every type: generate, audit, fix, reverse-engineer, help, and template for free.
 *
 * The GeneratorRegistry maps type names to instances, enabling:
 *   ./jtag development/generate/audit --type=command
 *   ./jtag development/generate/audit --type=entity
 *   ./jtag development/generate/audit --type=daemon
 *
 * Pattern: cargo clippy --fix for code generation patterns.
 * Each generator knows what "correct" looks like (from spec + template),
 * can detect deviation, and apply surgical fixes.
 */

import type { GenerateOptions } from './ModuleGenerator';

// ── Audit Types ─────────────────────────────────────────────────────

export interface GeneratorAuditEntry {
  /** Module name (e.g., 'ping', 'data-daemon', 'ChatMessage') */
  name: string;
  /** Absolute path to module directory or file */
  path: string;
  /** Whether a matching spec exists */
  hasSpec: boolean;
  /** Path to spec file if it exists */
  specPath?: string;
  /** Per-check conformance flags */
  checks: Record<string, boolean>;
  /** Human-readable issue descriptions */
  issues: string[];
  /** Issues that can be auto-fixed */
  fixableIssues: string[];
}

export interface GeneratorAuditSummary {
  /** Generator type that produced this audit */
  type: string;
  /** All audited entries */
  entries: GeneratorAuditEntry[];
  /** Total modules found */
  total: number;
  /** Modules with matching specs */
  withSpecs: number;
  /** Total issues across all modules */
  totalIssues: number;
  /** Issues that can be auto-fixed */
  totalFixable: number;
  /** Spec files with no matching module */
  orphanedSpecs: string[];
  /** Per-check summary counts */
  checkSummary: Record<string, { passing: number; failing: number }>;
}

// ── Fix Types ───────────────────────────────────────────────────────

export interface FixResult {
  /** Module that was fixed */
  name: string;
  /** Files that were modified */
  filesModified: string[];
  /** Files that were created */
  filesCreated: string[];
  /** Issues that were fixed */
  issuesFixed: string[];
  /** Issues that could not be auto-fixed */
  issuesRemaining: string[];
}

export interface FixSummary {
  type: string;
  results: FixResult[];
  totalFixed: number;
  totalRemaining: number;
}

// ── Help Types ──────────────────────────────────────────────────────

export interface GeneratorHelp {
  /** Full comprehensive help text */
  full: string;
  /** Short help for error cases */
  short: string;
  /** Topic-specific help */
  topics: Record<string, string>;
  /** Available topic names */
  availableTopics: string[];
}

// ── The Core Interface ──────────────────────────────────────────────

/**
 * IGeneratorType<TSpec> — the universal generator interface.
 *
 * Every generator type implements this. The registry provides
 * uniform access to all capabilities across all types.
 */
export interface IGeneratorType<TSpec = unknown> {
  /** Generator type identifier: 'command' | 'entity' | 'daemon' | 'widget' */
  readonly typeName: string;

  /** Human-readable description */
  readonly description: string;

  // ── Generate ────────────────────────────────────────────────────

  /** Generate module from spec */
  generate(spec: TSpec, outputDir?: string, options?: GenerateOptions): void;

  /** Generate module from spec file path */
  generateFromFile(specFilePath: string, outputDir?: string, options?: GenerateOptions): void;

  // ── Audit ───────────────────────────────────────────────────────

  /** Audit all modules of this type for conformance */
  audit(): GeneratorAuditSummary;

  /** Audit a single module */
  auditOne(modulePath: string): GeneratorAuditEntry;

  // ── Fix ─────────────────────────────────────────────────────────

  /** Fix all fixable issues across all modules */
  fixAll(): FixSummary;

  /** Fix a single module */
  fixOne(modulePath: string): FixResult;

  // ── Reverse Engineer ────────────────────────────────────────────

  /** Reverse-engineer a spec from an existing module */
  reverseEngineer(modulePath: string): TSpec | null;

  // ── Help & Templates ────────────────────────────────────────────

  /** Get help documentation */
  help(): GeneratorHelp;

  /** Get a template spec for creating new modules */
  templateSpec(variant?: string): TSpec;

  /** List available template variants */
  templateVariants(): string[];
}

// ── Registry ────────────────────────────────────────────────────────

/**
 * GeneratorRegistry — single access point for all generator types.
 *
 * Usage:
 *   const registry = GeneratorRegistry.instance;
 *   const cmd = registry.get('command');
 *   const summary = cmd.audit();
 *   cmd.fixAll();
 */
export class GeneratorRegistry {
  private static _instance: GeneratorRegistry | null = null;
  private readonly generators = new Map<string, IGeneratorType>();

  private constructor() {}

  static get instance(): GeneratorRegistry {
    if (!GeneratorRegistry._instance) {
      GeneratorRegistry._instance = new GeneratorRegistry();
    }
    return GeneratorRegistry._instance;
  }

  /** Register a generator type */
  register(generator: IGeneratorType): void {
    if (this.generators.has(generator.typeName)) {
      throw new Error(`Generator type '${generator.typeName}' already registered`);
    }
    this.generators.set(generator.typeName, generator);
  }

  /** Get a generator by type name */
  get(typeName: string): IGeneratorType {
    const gen = this.generators.get(typeName);
    if (!gen) {
      const available = this.typeNames.join(', ');
      throw new Error(`Unknown generator type '${typeName}'. Available: ${available}`);
    }
    return gen;
  }

  /** Check if a type is registered */
  has(typeName: string): boolean {
    return this.generators.has(typeName);
  }

  /** Get all registered type names */
  get typeNames(): string[] {
    return Array.from(this.generators.keys());
  }

  /** Get all registered generators */
  get all(): IGeneratorType[] {
    return Array.from(this.generators.values());
  }

  /** Audit ALL types, returns combined results */
  auditAll(): Map<string, GeneratorAuditSummary> {
    const results = new Map<string, GeneratorAuditSummary>();
    for (const [name, gen] of this.generators) {
      results.set(name, gen.audit());
    }
    return results;
  }

  /** Fix ALL types */
  fixAll(): Map<string, FixSummary> {
    const results = new Map<string, FixSummary>();
    for (const [name, gen] of this.generators) {
      results.set(name, gen.fixAll());
    }
    return results;
  }

  /** Print combined audit report */
  printAuditReport(summaries: Map<string, GeneratorAuditSummary>): void {
    console.log('\n╔══════════════════════════════════════════════════════════════╗');
    console.log('║                 GENERATOR SDK AUDIT REPORT                   ║');
    console.log('╚══════════════════════════════════════════════════════════════╝\n');

    let grandTotalModules = 0;
    let grandTotalIssues = 0;
    let grandTotalFixable = 0;

    for (const [typeName, summary] of summaries) {
      console.log(`\n  ${typeName.toUpperCase()} (${summary.total} modules)`);
      console.log('  ' + '─'.repeat(50));

      // Check summary
      for (const [checkName, counts] of Object.entries(summary.checkSummary)) {
        const pct = summary.total > 0 ? Math.round(counts.passing / summary.total * 100) : 0;
        const icon = counts.failing === 0 ? '  ' : '  ';
        console.log(`  ${icon}${checkName.padEnd(25)} ${counts.passing}/${summary.total} (${pct}%)`);
      }

      if (summary.orphanedSpecs.length > 0) {
        console.log(`    Orphaned specs: ${summary.orphanedSpecs.join(', ')}`);
      }

      grandTotalModules += summary.total;
      grandTotalIssues += summary.totalIssues;
      grandTotalFixable += summary.totalFixable;
    }

    console.log('\n  TOTALS');
    console.log('  ' + '─'.repeat(50));
    console.log(`    Modules:  ${grandTotalModules}`);
    console.log(`    Issues:   ${grandTotalIssues}`);
    console.log(`    Fixable:  ${grandTotalFixable}`);

    if (grandTotalFixable > 0) {
      console.log(`\n  Run with --fix to auto-fix ${grandTotalFixable} issues`);
    }
    console.log();
  }

  /** Reset the singleton (for testing) */
  static reset(): void {
    GeneratorRegistry._instance = null;
  }
}
