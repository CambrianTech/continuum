/**
 * AI Logs Command - Shared Base
 *
 * View and analyze AI decision-making logs
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { AILogsParams, AILogsResult } from './AILogsTypes';

export abstract class AILogsCommand extends CommandBase<CommandParams, CommandResult> {
  static readonly commandName = 'ai/logs';

  getDescription(): string {
    return 'View and analyze AI decision-making logs';
  }

  getParameters(): string[] {
    return [
      '--personaName=<name>      Filter by persona (e.g., "Helper AI")',
      '--decisionType=<type>     Filter by type (RESPOND|SILENT|POSTED|REDUNDANCY-CHECK|ERROR|ALL)',
      '--roomId=<id>             Filter by room ID',
      '--since=<time>            Time range start (ISO or relative like "5m", "1h")',
      '--until=<time>            Time range end (ISO)',
      '--tailLines=<n>           Show last N lines (default: 50)',
      '--includeStats            Include statistics summary',
      '--format=<fmt>            Output format (text|json, default: text)'
    ];
  }

  abstract execute(params: AILogsParams): Promise<AILogsResult>;
}
