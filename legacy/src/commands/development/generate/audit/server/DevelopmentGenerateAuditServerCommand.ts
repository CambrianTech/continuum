/**
 * Development Generate Audit Command - Server Implementation
 *
 * Audit modules for generator conformance. Supports all generator types
 * via the GeneratorSDK: command, entity, daemon, widget.
 *
 * When --type is omitted, audits commands (backward-compatible).
 * When --type=all, audits all types and produces a combined report.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DevelopmentGenerateAuditParams, DevelopmentGenerateAuditResult } from '../shared/DevelopmentGenerateAuditTypes';
import { createDevelopmentGenerateAuditResultFromParams } from '../shared/DevelopmentGenerateAuditTypes';
import { CommandAuditor } from '@generator/CommandAuditor';
import { createGeneratorRegistry } from '@generator/GeneratorSDKFactory';

export class DevelopmentGenerateAuditServerCommand extends CommandBase<DevelopmentGenerateAuditParams, DevelopmentGenerateAuditResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/generate/audit', context, subpath, commander);
  }

  async execute(params: DevelopmentGenerateAuditParams): Promise<DevelopmentGenerateAuditResult> {
    const auditType = (params as unknown as Record<string, unknown>).type as string | undefined;

    // If a non-command type is specified, use the SDK
    if (auditType && auditType !== 'command') {
      return this.executeSDKAudit(params, auditType);
    }

    // Default: command audit (backward-compatible)
    return this.executeCommandAudit(params);
  }

  private executeCommandAudit(params: DevelopmentGenerateAuditParams): DevelopmentGenerateAuditResult {
    const auditor = new CommandAuditor(process.cwd());
    const summary = auditor.audit();

    // Apply filter if specified
    let entries = summary.entries;
    if (params.filter) {
      switch (params.filter) {
        case 'missing-spec':
          entries = entries.filter(e => !e.hasSpec);
          break;
        case 'missing-accessor':
          entries = entries.filter(e => !e.hasStaticAccessor);
          break;
        case 'has-any':
          entries = entries.filter(e => e.anyCastCount > 0);
          break;
        case 'conformant':
          entries = entries.filter(e => e.issues.length === 0);
          break;
      }
    }

    const includeEntries = params.format !== 'summary';

    if (params.format !== 'json') {
      auditor.printAudit();
    }

    return createDevelopmentGenerateAuditResultFromParams(params, {
      success: true,
      totalCommands: summary.totalCommands,
      withSpecs: summary.withSpecs,
      missingAccessors: summary.missingAccessors,
      missingFactories: summary.missingFactories,
      totalAnyCasts: summary.totalAnyCasts,
      commandsWithAny: summary.commandsWithAny,
      orphanedSpecs: summary.orphanedSpecs,
      entries: includeEntries ? entries.map(e => ({
        commandName: e.commandName,
        hasSpec: e.hasSpec,
        hasStaticAccessor: e.hasStaticAccessor,
        hasFactoryFunctions: e.hasFactoryFunctions,
        anyCastCount: e.anyCastCount,
        issues: e.issues
      })) : []
    });
  }

  private executeSDKAudit(params: DevelopmentGenerateAuditParams, auditType: string): DevelopmentGenerateAuditResult {
    const registry = createGeneratorRegistry(process.cwd());

    if (auditType === 'all') {
      // Audit all types
      const summaries = registry.auditAll();
      if (params.format !== 'json') {
        registry.printAuditReport(summaries);
      }

      // Aggregate totals for the result
      let totalModules = 0;
      let totalWithSpecs = 0;
      let totalIssues = 0;

      for (const summary of summaries.values()) {
        totalModules += summary.total;
        totalWithSpecs += summary.withSpecs;
        totalIssues += summary.totalIssues;
      }

      return createDevelopmentGenerateAuditResultFromParams(params, {
        success: true,
        totalCommands: totalModules,
        withSpecs: totalWithSpecs,
        missingAccessors: 0,
        missingFactories: 0,
        totalAnyCasts: 0,
        commandsWithAny: 0,
        orphanedSpecs: [],
        entries: [],
      });
    }

    // Single type audit via SDK
    const gen = registry.get(auditType);
    const summary = gen.audit();

    if (params.format !== 'json') {
      const map = new Map([[auditType, summary]]);
      registry.printAuditReport(map);
    }

    return createDevelopmentGenerateAuditResultFromParams(params, {
      success: true,
      totalCommands: summary.total,
      withSpecs: summary.withSpecs,
      missingAccessors: 0,
      missingFactories: 0,
      totalAnyCasts: 0,
      commandsWithAny: 0,
      orphanedSpecs: summary.orphanedSpecs,
      entries: summary.entries.map(e => ({
        commandName: e.name,
        hasSpec: e.hasSpec,
        hasStaticAccessor: e.checks['static-accessor'] ?? true,
        hasFactoryFunctions: e.checks['factory-functions'] ?? true,
        anyCastCount: e.checks['no-any-casts'] === false ? 1 : 0,
        issues: e.issues,
      })),
    });
  }
}
