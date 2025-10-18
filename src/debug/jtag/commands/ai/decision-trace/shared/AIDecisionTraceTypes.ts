import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

/**
 * Parameters for debug/ai-decision-trace command
 */
export interface AIDecisionTraceParams extends CommandParams {
  /** Persona name (e.g., "Helper AI", "Teacher AI") */
  readonly personaName: string;

  /** Optional: Specific message ID to trace (defaults to most recent) */
  readonly messageId?: UUID;

  /** Optional: Show full RAG context details */
  readonly verbose?: boolean;
}

/**
 * Result from debug/ai-decision-trace command
 */
export interface AIDecisionTraceResult extends CommandResult {
  /** Message being traced */
  readonly messageId?: UUID;
  readonly messageText?: string;
  readonly messageTimestamp?: string;

  /** Step 1: RAG Context */
  readonly ragContext?: {
    systemPromptLength: number;
    messageCount: number;
    tokenEstimate: number;
    messages?: string[]; // Only if verbose=true
  };

  /** Step 2: Gating Decision */
  readonly gatingDecision?: {
    shouldRespond: boolean;
    confidence: number;
    reason: string;
    model: string;
  };

  /** Step 3: Coordination */
  readonly coordination?: {
    allowed: boolean;
    reason: string;
    winningPersona?: string;
  };

  /** Step 4: Generation */
  readonly generation?: {
    responseText?: string;
    durationMs?: number;
    error?: string;
  };

  /** Step 5: Posting */
  readonly posting?: {
    messageId?: UUID;
    timestamp?: string;
    error?: string;
  };
}
