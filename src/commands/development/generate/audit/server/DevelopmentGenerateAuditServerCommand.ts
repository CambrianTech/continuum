/**
 * Development Generate Audit Command - Server Implementation
 *
 * Audit all commands for generator conformance. Scans every command directory
 * and reports spec coverage, accessor patterns, factory functions, and type safety.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DevelopmentGenerateAuditParams, DevelopmentGenerateAuditResult } from '../shared/DevelopmentGenerateAuditTypes';
import { createDevelopmentGenerateAuditResultFromParams } from '../shared/DevelopmentGenerateAuditTypes';
import { CommandAuditor } from '@generator/CommandAuditor';

export class DevelopmentGenerateAuditServerCommand extends CommandBase<DevelopmentGenerateAuditParams, DevelopmentGenerateAuditResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/generate/audit', context, subpath, commander);
  }

  async execute(params: DevelopmentGenerateAuditParams): Promise<DevelopmentGenerateAuditResult> {
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

    // Format-dependent: summary mode omits entries
    const includeEntries = params.format !== 'summary';

    // Also print to console for CLI visibility
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
}
