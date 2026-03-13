/**
 * Codebase Index Command Types
 *
 * Index TypeScript and Markdown files with domain-specific embeddings
 * for semantic code search via RAG
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import type { CodeExportType } from '../../../../../system/data/entities/CodeIndexEntity';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Crawl and index TypeScript, Markdown, and other source files into the RAG store with domain-specific embeddings for semantic code search.
 */
export interface CodebaseIndexParams extends CommandParams {
  /** Directories or files to index (relative to repo root) */
  paths: string[];

  /** File types to index (default: ['typescript', 'markdown']) */
  fileTypes?: Array<'typescript' | 'markdown' | 'javascript' | 'json'>;

  /** Recurse into subdirectories (default: true) */
  recursive?: boolean;

  /** Update existing entries (default: true) */
  updateExisting?: boolean;

  /** Generate embeddings (default: true) */
  generateEmbeddings?: boolean;

  /** Embedding model to use (default: auto-select by file type) */
  embeddingModel?: string;

  /** Verbose output (default: false) */
  verbose?: boolean;
}

/**
 * Result from rag/index-codebase command
 */
export interface CodebaseIndexResult extends CommandResult {
  readonly success: boolean;
  readonly error?: string;

  /** Summary statistics */
  readonly summary: {
    filesIndexed: number;
    filesSkipped: number;
    entriesCreated: number;
    entriesUpdated: number;
    embeddingsGenerated: number;
    durationMs: number;
  };

  /** Per-file breakdown */
  readonly files: Array<{
    filePath: string;
    fileType: 'typescript' | 'markdown' | 'javascript' | 'json';
    entriesCreated: number;
    exportsFound?: Array<{
      exportType: CodeExportType;
      exportName: string;
      startLine: number;
      endLine: number;
    }>;
    embeddingModel?: string;
    status: 'indexed' | 'updated' | 'skipped' | 'error';
    error?: string;
  }>;

  /** Errors encountered */
  readonly errors?: Array<{
    file: string;
    error: string;
  }>;

  /** Warnings */
  readonly warnings?: string[];
}

/**
 * CodebaseIndex — Type-safe command executor
 *
 * Usage:
 *   import { CodebaseIndex } from '...shared/CodebaseIndexTypes';
 *   const result = await CodebaseIndex.execute({ ... });
 */
export const CodebaseIndex = {
  execute(params: CommandInput<CodebaseIndexParams>): Promise<CodebaseIndexResult> {
    return Commands.execute<CodebaseIndexParams, CodebaseIndexResult>('ai/rag/index-codebase', params as Partial<CodebaseIndexParams>);
  },
  commandName: 'ai/rag/index-codebase' as const,
} as const;

/**
 * Factory function for creating AiRagIndexCodebaseParams
 */
export const createAiRagIndexCodebaseParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<CodebaseIndexParams, 'context' | 'sessionId' | 'userId'>
): CodebaseIndexParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating AiRagIndexCodebaseResult with defaults
 */
export const createAiRagIndexCodebaseResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<CodebaseIndexResult, 'context' | 'sessionId' | 'userId'>
): CodebaseIndexResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart ai/rag/index-codebase-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiRagIndexCodebaseResultFromParams = (
  params: CodebaseIndexParams,
  differences: Omit<CodebaseIndexResult, 'context' | 'sessionId' | 'userId'>
): CodebaseIndexResult => transformPayload(params, differences);

