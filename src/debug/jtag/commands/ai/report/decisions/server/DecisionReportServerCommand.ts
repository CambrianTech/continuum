/**
 * DecisionReportServerCommand - Macro command for generating decision reports
 *
 * Orchestrates:
 * 1. data/list (query coordination_decisions)
 * 2. DecisionReportFormatter (markdown generation)
 * 3. file/save (write report to disk)
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import { Commands } from '../../../../../system/core/shared/Commands';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { DecisionReportParams, DecisionReportResult, DecisionForReport } from '../shared/DecisionReportTypes';
import { createDecisionReportResult } from '../shared/DecisionReportTypes';
import { COLLECTIONS } from '../../../../../system/data/config/DatabaseConfig';
import { DecisionReportFormatter } from './DecisionReportFormatter';
import type { CoordinationDecisionEntity } from '../../../../../system/data/entities/CoordinationDecisionEntity';
import type { DataListParams, DataListResult } from '../../../../data/list/shared/DataListTypes';
import type { FileSaveParams, FileSaveResult } from '../../../../file/save/shared/FileSaveTypes';
import * as path from 'path';
import * as fs from 'fs';

export class DecisionReportServerCommand extends CommandBase<DecisionReportParams, DecisionReportResult> {
  static readonly commandName = 'ai/report/decisions';

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai-report-decisions', context, subpath, commander);
  }

  /**
   * Execute the report generation command
   * NO try/catch - let errors throw to be caught by infrastructure
   */
  async execute(params: DecisionReportParams): Promise<DecisionReportResult> {
    console.log(`📊 SERVER: DecisionReport (macro command orchestration)`);

    // Build data/list query
    const filter: Record<string, unknown> = {};

    // Date range filter - NOTE: BaseEntity fields (createdAt/updatedAt) don't exist in this collection!
    // Data has NO timestamp fields at top level. Would need to filter on ragContext.metadata.timestamp
    // but that requires JSON path queries which data/list doesn't support yet.
    // For now, skip date filtering and do it in post-processing.
    // TODO: Fix Phase 5C to properly save BaseEntity fields OR add JSON path query support

    // Actor name filter
    if (params.actors && params.actors.length > 0) {
      filter.actorName = { in: params.actors };
    }

    // Action filter
    if (params.actions && params.actions.length > 0) {
      filter['decision.action'] = { in: params.actions };
    }

    // Confidence filter
    if (params.minConfidence !== undefined) {
      filter['decision.confidence'] = { gte: params.minConfidence };
    }

    console.log(`🔍 SERVER: Querying decisions with filter:`, filter);

    // Execute data/list command
    const listResult = await Commands.execute<DataListParams<CoordinationDecisionEntity>, DataListResult<CoordinationDecisionEntity>>(
      'data/list',
      {
        collection: COLLECTIONS.COORDINATION_DECISIONS,
        filter: Object.keys(filter).length > 0 ? filter : undefined,
        orderBy: [{ field: 'createdAt', direction: 'desc' }],  // ✅ FIX: Order by newest first
        limit: params.limit ?? 100,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!listResult.success) {
      throw new Error(`Failed to query decisions: ${listResult.error}`);
    }

    console.log(`✅ SERVER: Found ${listResult.items.length} decisions`);

    // Convert entities to DecisionForReport format
    // NOTE: Use ragContext.metadata.timestamp since createdAt doesn't exist (Phase 5C bug)
    const decisions: DecisionForReport[] = listResult.items.map(entity => ({
      id: entity.id,
      timestamp: new Date(entity.ragContext?.metadata?.timestamp || Date.now()),
      actorId: entity.actorId,
      actorName: entity.actorName,
      actorType: entity.actorType,
      action: entity.decision.action,
      confidence: entity.decision.confidence,
      reasoning: entity.decision.reasoning,
      responseContent: entity.decision.responseContent,
      modelUsed: entity.decision.modelUsed,
      ragContext: entity.ragContext,
      coordinationSnapshot: entity.coordinationSnapshot,
      ambientState: entity.ambientState,
      metadata: entity.metadata
    }));

    // Sort by timestamp (since DB doesn't have createdAt to sort on)
    decisions.sort((a, b) => b.timestamp.getTime() - a.timestamp.getTime());

    // Apply date range filter (post-processing since DB can't filter on JSON fields)
    let filteredDecisions = decisions;
    if (params.startDate || params.endDate) {
      const startMs = params.startDate ? new Date(params.startDate).getTime() : 0;
      const endMs = params.endDate ? new Date(params.endDate).getTime() : Date.now();
      filteredDecisions = decisions.filter(d => {
        const ts = d.timestamp.getTime();
        return ts >= startMs && ts <= endMs;
      });
    }

    // Generate markdown report (use filtered decisions)
    const markdown = DecisionReportFormatter.formatReport(filteredDecisions, {
      verbose: params.verbose,
      groupByActor: params.groupByActor,
      startDate: params.startDate,
      endDate: params.endDate
    });

    // Determine output path
    let outputPath: string;
    if (params.output) {
      outputPath = path.resolve(params.output);
    } else {
      // Default: .continuum/reports/decisions-{timestamp}.md
      const reportsDir = path.resolve('.continuum/reports');

      // Ensure reports directory exists
      if (!fs.existsSync(reportsDir)) {
        fs.mkdirSync(reportsDir, { recursive: true });
      }

      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      outputPath = path.join(reportsDir, `decisions-${timestamp}.md`);
    }

    console.log(`💾 SERVER: Saving report to ${outputPath}`);

    // Save to disk using file/save command
    const saveResult = await Commands.execute<FileSaveParams, FileSaveResult>(
      'file/save',
      {
        filepath: outputPath,
        content: markdown,
        encoding: 'utf-8',
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!saveResult.success) {
      throw new Error(`Failed to save report: ${saveResult.error}`);
    }

    console.log(`✅ SERVER: Report saved successfully`);

    // Calculate statistics (use filtered decisions)
    const stats = this.calculateStats(filteredDecisions);

    // Determine date range (use filtered decisions)
    const timestamps = filteredDecisions.map(d => d.timestamp.getTime());
    const minTimestamp = timestamps.length > 0 ? Math.min(...timestamps) : Date.now();
    const maxTimestamp = timestamps.length > 0 ? Math.max(...timestamps) : Date.now();

    return createDecisionReportResult(params.context, params.sessionId, {
      success: true,
      reportPath: outputPath,
      decisionCount: filteredDecisions.length,
      actorCount: new Set(filteredDecisions.map(d => d.actorName)).size,
      dateRange: {
        start: new Date(minTimestamp).toISOString(),
        end: new Date(maxTimestamp).toISOString()
      },
      stats
    });
  }

  /**
   * Calculate summary statistics
   */
  private calculateStats(decisions: DecisionForReport[]): {
    totalDecisions: number;
    posted: number;
    silent: number;
    errors: number;
    avgConfidence: number;
  } {
    const total = decisions.length;
    const posted = decisions.filter(d => d.action === 'POSTED').length;
    const silent = decisions.filter(d => d.action === 'SILENT').length;
    const errors = decisions.filter(d => d.action === 'ERROR' || d.action === 'TIMEOUT').length;
    const avgConfidence = total > 0
      ? decisions.reduce((sum, d) => sum + d.confidence, 0) / total
      : 0;

    return {
      totalDecisions: total,
      posted,
      silent,
      errors,
      avgConfidence
    };
  }
}
