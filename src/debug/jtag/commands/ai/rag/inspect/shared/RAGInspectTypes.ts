/**
 * RAG Inspect Command Types
 *
 * Utility command to inspect RAG context building for a persona in a room
 * Useful for debugging and validating RAG system behavior
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import type { RAGContext } from '../../../../../system/rag/shared/RAGTypes';

/**
 * Parameters for rag/inspect command
 */
export interface RAGInspectParams extends CommandParams {
  /** Room/context ID to build RAG for */
  contextId: UUID;

  /** Persona ID requesting context */
  personaId: UUID;

  /** Optional: Limit number of messages */
  maxMessages?: number;

  /** Optional: Include artifacts (images, files) */
  includeArtifacts?: boolean;

  /** Optional: Include private memories */
  includeMemories?: boolean;

  /** Optional: Show full RAG content (like ping --verbose) */
  verbose?: boolean;

  /** Optional: Message ID that triggered evaluation (for decision-point analysis) */
  triggerMessageId?: UUID;
}

/**
 * Result from rag/inspect command
 */
export interface RAGInspectResult extends CommandResult {
  readonly success: boolean;
  readonly error?: string;

  /** Built RAG context */
  readonly ragContext?: RAGContext;

  /** Summary stats */
  readonly summary?: {
    messageCount: number;
    artifactCount: number;
    memoryCount: number;
    conversationTimespan?: {
      oldest: string;
      newest: string;
    };
    systemPromptLength: number;
    totalTokensEstimate: number;

    // Phase 2: Learning mode fields (explicitly surfaced)
    learningMode?: 'fine-tuning' | 'inference-only';
    genomeId?: UUID;
    participantRole?: string;
    learningModeStatus: 'enabled' | 'disabled' | 'not-configured';
  };

  /** Decision-point analysis (items 4-6) */
  readonly decisionPoint?: {
    /** The message that triggered this evaluation */
    triggerMessage?: {
      id: UUID;
      content: string;
      senderName: string;
      timestamp: number;
    };

    /** Decision made by this persona */
    decision?: {
      shouldRespond: boolean;
      confidence?: number;
      reasoning?: string;
      action: 'POSTED' | 'SILENT' | 'ERROR' | 'TIMEOUT';
    };

    /** Learning mode context at decision time */
    learningContext: {
      mode: 'fine-tuning' | 'inference-only' | 'not-configured';
      genomeActive: boolean;
      participantRole?: string;
      adaptiveDataAvailable: boolean;
    };
  };

  /** Validation warnings */
  readonly warnings?: string[];
}
