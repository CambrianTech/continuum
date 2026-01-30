/**
 * Decision Report Command Types
 *
 * Macro command that orchestrates:
 * 1. data/list (query coordination_decisions)
 * 2. Markdown formatting
 * 3. file/save (write report to disk)
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import type { DecisionAction } from '../../../../../system/data/entities/CoordinationDecisionEntity';
import type { JTAGError } from '../../../../../system/core/types/ErrorTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Parameters for generating decision report
 */
export interface DecisionReportParams extends CommandParams {
  /**
   * Start date for filtering decisions (ISO 8601 format)
   * Example: "2025-11-11T00:00:00Z"
   */
  readonly startDate?: string;

  /**
   * End date for filtering decisions (ISO 8601 format)
   * Example: "2025-11-11T23:59:59Z"
   */
  readonly endDate?: string;

  /**
   * Filter by actor names (persona names)
   * Example: ["Grok", "Sentinel", "Helper AI"]
   */
  readonly actors?: string[];

  /**
   * Filter by specific action types
   * Example: ["POSTED", "SILENT"]
   */
  readonly actions?: DecisionAction[];

  /**
   * Minimum confidence level (0-1)
   * Example: 0.5 (only show decisions >= 50% confidence)
   */
  readonly minConfidence?: number;

  /**
   * Maximum number of decisions to include
   * Default: 100
   */
  readonly limit?: number;

  /**
   * Output file path (relative to project root)
   * Default: ".continuum/reports/decisions-{timestamp}.md"
   */
  readonly output?: string;

  /**
   * Show complete RAG context (full system prompts, conversation history)
   * Alias: --verbose or -v (Unix convention)
   * Default: false (only show summary)
   *
   * CLI Usage:
   *   --verbose=true  (explicit boolean)
   *   --verbose       (flag style - TODO: needs arg parser enhancement)
   */
  readonly verbose?: boolean;

  /**
   * Group decisions by actor (vs chronological)
   * Default: false (chronological timeline)
   */
  readonly groupByActor?: boolean;
}

/**
 * Helper function to create DecisionReportParams with proper typing
 */
export const createDecisionReportParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    startDate?: string;
    endDate?: string;
    actors?: string[];
    actions?: DecisionAction[];
    minConfidence?: number;
    limit?: number;
    output?: string;
    verbose?: boolean;
    groupByActor?: boolean;
  }
): DecisionReportParams => ({
  context,
  sessionId,
  startDate: data.startDate,
  endDate: data.endDate,
  actors: data.actors,
  actions: data.actions,
  minConfidence: data.minConfidence,
  limit: data.limit ?? 100,
  output: data.output,
  verbose: data.verbose ?? false,
  groupByActor: data.groupByActor ?? false
});

/**
 * Result from generating decision report
 */
export interface DecisionReportResult extends CommandResult {
  /**
   * Whether the report was successfully generated
   */
  readonly success: boolean;

  /**
   * Absolute path to generated report file
   */
  readonly reportPath: string;

  /**
   * Number of decisions included in report
   */
  readonly decisionCount: number;

  /**
   * Number of actors represented
   */
  readonly actorCount: number;

  /**
   * Date range covered (ISO 8601)
   */
  readonly dateRange: {
    readonly start: string;
    readonly end: string;
  };

  /**
   * Summary statistics
   */
  readonly stats: {
    readonly totalDecisions: number;
    readonly posted: number;
    readonly silent: number;
    readonly errors: number;
    readonly avgConfidence: number;
  };

  /**
   * Error if operation failed
   */
  readonly error?: JTAGError;
}

/**
 * Helper function to create DecisionReportResult with proper typing
 */
export const createDecisionReportResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    reportPath: string;
    decisionCount: number;
    actorCount: number;
    dateRange: {
      start: string;
      end: string;
    };
    stats: {
      totalDecisions: number;
      posted: number;
      silent: number;
      errors: number;
      avgConfidence: number;
    };
    error?: JTAGError;
  }
): DecisionReportResult => ({
  context,
  sessionId,
  success: data.success,
  reportPath: data.reportPath,
  decisionCount: data.decisionCount,
  actorCount: data.actorCount,
  dateRange: data.dateRange,
  stats: data.stats,
  error: data.error
});

/**
 * Internal: Decision data for formatting
 */
export interface DecisionForReport {
  id: UUID;
  timestamp: Date;
  actorId: UUID;
  actorName: string;
  actorType: 'human' | 'ai-persona';
  action: DecisionAction;
  confidence: number;
  reasoning?: string;
  responseContent?: string;
  modelUsed?: string;
  /** RAG context stored inline (for small contexts) */
  ragContext?: {
    identity: {
      systemPrompt: string;
      bio: string;
      role: string;
    };
    conversationHistory: Array<{
      role: 'system' | 'user' | 'assistant';
      content: string;
      timestamp: number;
    }>;
    metadata: {
      tokenCount: number;
      contextWindow: number;
    };
  };
  /** Reference to RAG context in blob storage (for large contexts) */
  ragContextRef?: string;
  coordinationSnapshot: {
    phase?: 'gathering' | 'deciding' | 'closed';
    availableSlots?: number;
    othersConsideringCount: number;
    othersConsideringNames: string[];
  };
  ambientState: {
    temperature: number;
    userPresent: boolean;
    timeSinceLastResponse: number;
    mentionedByName: boolean;
  };
  metadata: {
    sessionId: UUID;
    contextId: UUID;
    sequenceNumber: number;
    tags?: string[];
  };
}

/**
 * DecisionReport — Type-safe command executor
 *
 * Usage:
 *   import { DecisionReport } from '...shared/DecisionReportTypes';
 *   const result = await DecisionReport.execute({ ... });
 */
export const DecisionReport = {
  execute(params: CommandInput<DecisionReportParams>): Promise<DecisionReportResult> {
    return Commands.execute<DecisionReportParams, DecisionReportResult>('ai/report/decisions', params as Partial<DecisionReportParams>);
  },
  commandName: 'ai/report/decisions' as const,
} as const;
