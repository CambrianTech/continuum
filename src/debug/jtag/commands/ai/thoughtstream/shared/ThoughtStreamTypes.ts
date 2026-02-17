/**
 * AI ThoughtStream Command Types
 *
 * Inspect ThoughtStream coordinator activity for a specific message
 * Shows thought broadcasts, rankings, and final decisions
 */

import type { CommandParams, JTAGContext, UUID, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';

export interface ThoughtStreamParams extends CommandParams {
  messageId?: string;                  // Specific message to inspect
  roomId?: string;                     // Show all thoughts in room (last N minutes)
  personaId?: string;                  // Filter to specific persona's thoughts
  since?: string;                      // Time range (e.g., "5m", "1h")
  limit?: number;                      // Max number of streams to show (default 10)

  // Display options
  showContent?: boolean;               // Show actual message content (not "Unknown")
  showRagContext?: boolean;            // Include RAG context for each thought
  showPrompts?: boolean;               // Include evaluation prompts
  verbose?: boolean;                   // Full details
}

export interface Thought {
  personaId: UUID;
  personaName: string;
  type: 'claiming' | 'deferring' | 'observing';
  confidence?: number;
  reasoning?: string;
  timestamp: Date;
  ragContext?: {
    totalMessages: number;
    filteredMessages: number;
    conversationHistory?: Array<{
      name: string;
      content: string;
      timestamp?: number;
    }>;
  };
}

export interface ThoughtStreamDecision {
  messageId: string;
  streamId: string;
  messageContent: string;
  messageSender: string;
  messageTimestamp: Date;

  // Thought collection phase
  thoughts: Thought[];
  evaluationDuration: number;          // How long AIs took to evaluate

  // Coordinator decision
  decision: {
    granted: UUID[];                   // PersonaIds granted permission
    denied: UUID[];                    // PersonaIds denied
    reasoning: string;                 // Why this decision was made
    decisionTime: number;              // Timestamp of decision
    waitDuration: number;              // How long coordinator waited
  };

  // Actual outcomes
  outcomes: Array<{
    personaId: UUID;
    personaName: string;
    action: 'POSTED' | 'TIMEOUT' | 'ERROR' | 'REDUNDANT' | 'SILENT';
    responseText?: string;
    responseTimeMs?: number;
    error?: string;
  }>;

  // Moderator statistics
  moderatorDecision?: {
    name: string;                      // Which moderator made the decision
    confidenceThreshold: number;       // Threshold used
    maxResponders: number;             // Max allowed
    conversationHealth: {
      consecutiveSilence: number;
      recentMessageCount: number;
      avgResponseTime: number;
      activeParticipants: number;
    };
  };
}

export interface ThoughtStreamResult {
  context: JTAGContext;
  sessionId: UUID;
  success: boolean;
  error?: string;

  // Stream details
  streams: ThoughtStreamDecision[];

  // Summary stats
  summary: {
    totalStreams: number;
    totalThoughts: number;
    avgThoughtsPerStream: number;
    avgEvaluationTime: number;
    avgDecisionTime: number;

    // Outcome breakdown
    posted: number;
    timeouts: number;
    errors: number;
    redundant: number;
    silent: number;

    // Moderator effectiveness
    avgConfidenceThreshold: number;
    coordinationEfficiency: number;    // % of time single AI granted (ideal)
  };

  // Issues detected
  issues?: {
    slowEvaluations: Array<{
      personaName: string;
      duration: number;
      messageId: string;
    }>;
    coordinationDeadlocks: Array<{
      messageId: string;
      grantedPersona: string;
      outcome: string;
      reason: string;
    }>;
    lowConfidenceDecisions: Array<{
      personaName: string;
      confidence: number;
      messageId: string;
    }>;
  };
}

/**
 * ThoughtStream — Type-safe command executor
 *
 * Usage:
 *   import { ThoughtStream } from '...shared/ThoughtStreamTypes';
 *   const result = await ThoughtStream.execute({ ... });
 */
export const ThoughtStream = {
  execute(params: CommandInput<ThoughtStreamParams>): Promise<ThoughtStreamResult> {
    return Commands.execute<ThoughtStreamParams, ThoughtStreamResult>('ai/thoughtstream', params as Partial<ThoughtStreamParams>);
  },
  commandName: 'ai/thoughtstream' as const,
} as const;
