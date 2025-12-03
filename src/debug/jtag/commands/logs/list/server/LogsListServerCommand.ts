/**
 * logs/list Server Command
 */

import { LogsListCommand } from '../shared/LogsListCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LogsListParams, LogsListResult, LogInfo } from '../shared/LogsListTypes';
import { LogFileRegistry } from '../../../../system/core/logging/LogFileRegistry';
import { pathToLogName, resolvePersonaId } from '../../shared/LogsShared';

export class LogsListServerCommand extends LogsListCommand {
  private registry: LogFileRegistry;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('logs/list', context, subpath, commander);
    this.registry = new LogFileRegistry();
  }

  async execute(params: LogsListParams): Promise<LogsListResult> {
    const personaFilter = await resolvePersonaId(params.personaId, params.personaUniqueId);

    const logs = await this.registry.filter({
      category: params.category,
      component: params.component,
      personaId: personaFilter || undefined,
      logType: params.logType
    });

    const logInfos: LogInfo[] = logs.map(log => ({
      name: pathToLogName(log.filePath),
      category: log.category,
      component: log.component,
      personaName: log.personaName,
      logType: log.logType,
      sizeMB: log.sizeBytes / (1024 * 1024),
      lineCount: log.lineCountEstimate,
      lastModified: log.lastModified.toISOString(),
      isActive: log.isActive
    }));

    const totalSizeMB = logInfos.reduce((sum, log) => sum + log.sizeMB, 0);
    const categories: Record<string, number> = {};
    for (const log of logInfos) {
      categories[log.category] = (categories[log.category] || 0) + 1;
    }

    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      logs: logInfos,
      summary: { totalFiles: logInfos.length, totalSizeMB, categories }
    };
  }
}
