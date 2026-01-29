/**
 * AI Report Command Types
 *
 * Analyze AI decision logs to generate actionable insights
 */

import type { CommandParams, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

export interface AIReportParams extends CommandParams {

  // Filtering
  roomId?: string;                    // Analyze specific room
  personaName?: string;                // Analyze specific persona by display name
  personaId?: string;                  // Analyze specific persona by user ID
  personaUniqueId?: string;            // Analyze specific persona by unique ID
  since?: string;                      // Time range start (ISO or relative like "1h", "today")
  until?: string;                      // Time range end

  // Analysis options
  includeTimeline?: boolean;           // Include decision timeline
  includeContextAnalysis?: boolean;    // Analyze RAG context quality
  detectIssues?: boolean;              // Auto-detect problems (low confidence, errors, etc.)
  recreateDecision?: string;           // Recreate specific decision by timestamp

  // Output control
  format?: 'text' | 'json';            // Output format
}

export interface AIReportResult {
  context: JTAGContext;
  sessionId: UUID;

  success: boolean;
  error?: string;

  // Summary statistics
  summary: {
    totalDecisions: number;
    responseDecisions: number;
    silentDecisions: number;
    responseRate: number;              // Percentage (0-100)
    avgConfidence: number;             // Average confidence (0-1)
    timeRange: {
      start: string;
      end: string;
    };
    totalCost: number;                 // Total estimated cost in USD
    costByProvider: {                  // Cost breakdown by provider
      [provider: string]: number;
    };
  };

  // Room breakdown (if filtering by room or requested)
  roomAnalysis?: {
    roomId: string;
    totalDecisions: number;
    responseRate: number;
    avgConfidence: number;
    personaBreakdown: {
      [personaName: string]: {
        decisions: number;
        responses: number;
        silences: number;
        avgConfidence: number;
      };
    };
  };

  // Persona breakdown (if filtering by persona or requested)
  personaAnalysis?: {
    personaName: string;
    totalDecisions: number;
    responseRate: number;
    confidenceDistribution: {
      low: number;      // 0.0-0.4
      medium: number;   // 0.4-0.7
      high: number;     // 0.7-1.0
    };
    roomBreakdown: {
      [roomId: string]: {
        decisions: number;
        responses: number;
        silences: number;
      };
    };
    modelUsage: {
      [model: string]: number;
    };
  };

  // Context quality analysis
  contextAnalysis?: {
    avgMessagesAvailable: number;
    avgMessagesFiltered: number;
    insufficientContextCount: number;  // Times when < 2 messages in RAG
    timeWindowIssues: number;          // Times when filtering removed too many messages
  };

  // Timeline (if requested)
  timeline?: Array<{
    timestamp: string;
    persona: string;
    decision: 'RESPOND' | 'SILENT';
    confidence: number;
    reason: string;
    roomId: string;
  }>;

  // Issues detected (if detectIssues=true)
  issues?: {
    lowConfidenceDecisions: Array<{
      timestamp: string;
      persona: string;
      confidence: number;
      reason: string;
      roomId: string;
    }>;
    insufficientContext: Array<{
      timestamp: string;
      persona: string;
      messagesAvailable: number;
      roomId: string;
    }>;
    errors: Array<{
      timestamp: string;
      persona: string;
      error: string;
      operation: string;
    }>;
    redundancyDiscards: Array<{
      timestamp: string;
      persona: string;
      roomId: string;
      reason: string;
    }>;
  };

  // Decision recreation (if recreateDecision specified)
  recreatedDecision?: {
    timestamp: string;
    persona: string;
    decision: 'RESPOND' | 'SILENT';
    confidence: number;
    reason: string;
    model: string;
    conversationHistory: Array<{
      name: string;
      content: string;
      timestamp?: number;
    }>;
    ragContextSummary: {
      totalMessages: number;
      filteredMessages: number;
      timeWindowMinutes?: number;
    };
    debugPrompt: string;  // Reconstructed prompt to test in Ollama
  };

  // Coordination statistics (quality-boost + recency rotation)
  coordinationStats?: {
    activeStreams: number;
    totalRejections: number;
    rejectionsByReason: {
      no_slots: number;
      low_confidence: number;
      outranked: number;
      deferred: number;
      timeout: number;
    };
    rejectionsByPriority: {
      moderator: number;
      expert: number;
      participant: number;
      observer: number;
      undefined: number;
    };
  };
}

/**
 * AIReport — Type-safe command executor
 *
 * Usage:
 *   import { AIReport } from '...shared/AIReportTypes';
 *   const result = await AIReport.execute({ ... });
 */
export const AIReport = {
  execute(params: CommandInput<AIReportParams>): Promise<AIReportResult> {
    return Commands.execute<AIReportParams, AIReportResult>('ai/report', params as Partial<AIReportParams>);
  },
  commandName: 'ai/report' as const,
} as const;
