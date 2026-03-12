/**
 * Development Generate Audit Command - Browser Implementation
 *
 * Audit all commands for generator conformance. Scans every command directory and checks for: matching generator spec, static accessor (Name.execute pattern), factory functions, any casts in Types files. Reports conformance status and summary statistics.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DevelopmentGenerateAuditParams, DevelopmentGenerateAuditResult } from '../shared/DevelopmentGenerateAuditTypes';

export class DevelopmentGenerateAuditBrowserCommand extends CommandBase<DevelopmentGenerateAuditParams, DevelopmentGenerateAuditResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/generate/audit', context, subpath, commander);
  }

  async execute(params: DevelopmentGenerateAuditParams): Promise<DevelopmentGenerateAuditResult> {
    console.log('🌐 BROWSER: Delegating Development Generate Audit to server');
    return await this.remoteExecute(params);
  }
}
