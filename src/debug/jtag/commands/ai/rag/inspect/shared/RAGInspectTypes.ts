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
  };

  /** Validation warnings */
  readonly warnings?: string[];
}
