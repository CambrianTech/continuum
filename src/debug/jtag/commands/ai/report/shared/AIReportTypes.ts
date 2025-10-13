/**
 * AI Report Command Types
 *
 * Analyze AI decision logs to generate actionable insights
 */

import type { CommandParams } from '../../../../system/core/types/JTAGTypes';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface AIReportParams extends CommandParams {

  // Filtering
  roomId?: string;                    // Analyze specific room
  personaName?: string;                // Analyze specific persona
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
}
