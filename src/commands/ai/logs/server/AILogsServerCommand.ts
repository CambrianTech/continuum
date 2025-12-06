/**
 * AI Logs Server Command
 *
 * Read and analyze AI decision log with rich filtering and statistics
 */

import * as fs from 'fs';
import * as path from 'path';
import { AILogsCommand } from '../shared/AILogsCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AILogsParams, AILogsResult } from '../shared/AILogsTypes';

export class AILogsServerCommand extends AILogsCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/logs', context, subpath, commander);
  }

  async execute(params: AILogsParams): Promise<AILogsResult> {
    try {
      // Get AI decision log path
      const logPath = path.join(
        process.cwd(),
        '.continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/ai-decisions.log'
      );

      if (!fs.existsSync(logPath)) {
        return {
          context: params.context || this.context,
          sessionId: params.sessionId || '00000000-0000-0000-0000-000000000000',
          success: false,
          error: 'AI decision log not found',
          logPath,
          lines: [],
          totalLines: 0,
          filteredLines: 0
        };
      }

      // Read log file
      const content = fs.readFileSync(logPath, 'utf-8');
      const allLines = content.split('\n').filter(line => line.trim().length > 0);

      // Apply filters
      let filteredLines = allLines;

      // Filter by persona
      if (params.personaName) {
        filteredLines = filteredLines.filter(line =>
          line.includes(params.personaName!)
        );
      }

      // Filter by decision type
      if (params.decisionType && params.decisionType !== 'ALL') {
        filteredLines = filteredLines.filter(line => {
          if (params.decisionType === 'RESPOND') return line.includes('→ RESPOND');
          if (params.decisionType === 'SILENT') return line.includes('→ SILENT');
          if (params.decisionType === 'POSTED') return line.includes('→ POSTED');
          if (params.decisionType === 'REDUNDANCY-CHECK') return line.includes('→ REDUNDANCY-CHECK');
          if (params.decisionType === 'ERROR') return line.includes('→ ERROR');
          return true;
        });
      }

      // Filter by roomId
      if (params.roomId) {
        filteredLines = filteredLines.filter(line =>
          line.includes(`Room: ${params.roomId}`)
        );
      }

      // Apply tail limit
      const tailLines = params.tailLines ?? 50;
      const displayLines = filteredLines.slice(-tailLines);

      // Calculate statistics if requested
      let stats = undefined;
      if (params.includeStats) {
        stats = this.calculateStats(filteredLines);
      }

      return {
        context: params.context || this.context,
        sessionId: params.sessionId || '00000000-0000-0000-0000-000000000000',
        success: true,
        logPath,
        lines: displayLines,
        totalLines: allLines.length,
        filteredLines: filteredLines.length,
        stats
      };

    } catch (error) {
      return {
        context: params.context || this.context,
        sessionId: params.sessionId || '00000000-0000-0000-0000-000000000000',
        success: false,
        error: error instanceof Error ? error.message : String(error),
        logPath: '',
        lines: [],
        totalLines: 0,
        filteredLines: 0
      };
    }
  }

  private calculateStats(lines: string[]): AILogsResult['stats'] {
    const stats = {
      totalDecisions: 0,
      responseCount: 0,
      silentCount: 0,
      postedCount: 0,
      redundancyChecks: 0,
      redundancyDiscards: 0,
      errors: 0,
      personaBreakdown: {} as { [name: string]: { respond: number; silent: number; posted: number } }
    };

    for (const line of lines) {
      // Skip header lines
      if (line.includes('===') || line.includes('Session Started')) {
        continue;
      }

      // Extract persona name
      const personaMatch = line.match(/\] ([^→]+) →/);
      const personaName = personaMatch ? personaMatch[1].trim() : 'Unknown';

      // Initialize persona stats
      if (!stats.personaBreakdown[personaName]) {
        stats.personaBreakdown[personaName] = { respond: 0, silent: 0, posted: 0 };
      }

      // Count by type
      if (line.includes('→ RESPOND')) {
        stats.responseCount++;
        stats.totalDecisions++;
        stats.personaBreakdown[personaName].respond++;
      } else if (line.includes('→ SILENT')) {
        stats.silentCount++;
        stats.totalDecisions++;
        stats.personaBreakdown[personaName].silent++;
      } else if (line.includes('→ POSTED')) {
        stats.postedCount++;
        stats.personaBreakdown[personaName].posted++;
      } else if (line.includes('→ REDUNDANCY-CHECK')) {
        stats.redundancyChecks++;
        if (line.includes('DISCARD')) {
          stats.redundancyDiscards++;
        }
      } else if (line.includes('→ ERROR')) {
        stats.errors++;
      }
    }

    return stats;
  }
}
